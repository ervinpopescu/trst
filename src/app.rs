use std::collections::BTreeSet;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::client::TransmissionClient;
use crate::config::{Bindings, Config, ThemeConfig};
use crate::credentials;
use crate::protocol::*;
use crate::ui;
use crate::util;

enum RefreshMsg {
    Torrents(Result<Vec<Torrent>, String>),
    Detail(Box<Result<Option<Torrent>, String>>),
    Stats {
        stats: Option<SessionStats>,
        free: Option<FreeSpace>,
        default_dir: Option<String>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    TorrentList,
    Files,
    Details,
    #[cfg(feature = "rsync")]
    Rsync,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    Remove,
    DeleteFiles,
    DeleteFileFromDisk,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthField {
    Username,
    Password,
}

pub enum Modal {
    AddUrl(String),
    AddLocation {
        url: String,
        location: String,
    },
    ChangeLocation(String),
    Filter,
    Confirm(Confirm),
    Auth {
        username: String,
        password: String,
        focused: AuthField,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortColumn {
    Name,
    Size,
    Progress,
    Down,
    Up,
    Eta,
    Ratio,
    Status,
    Queue,
}

impl SortColumn {
    pub fn next(self) -> Self {
        match self {
            Self::Queue => Self::Name,
            Self::Name => Self::Size,
            Self::Size => Self::Progress,
            Self::Progress => Self::Down,
            Self::Down => Self::Up,
            Self::Up => Self::Eta,
            Self::Eta => Self::Ratio,
            Self::Ratio => Self::Status,
            Self::Status => Self::Queue,
        }
    }

    /// Column index in the torrent list header, if visible.
    pub fn column_index(self) -> Option<usize> {
        match self {
            Self::Status => Some(0),
            Self::Name => Some(1),
            Self::Size => Some(2),
            Self::Progress => Some(3),
            Self::Down => Some(4),
            Self::Up => Some(5),
            Self::Eta => Some(6),
            Self::Ratio => Some(7),
            Self::Queue => None, // no visible column
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Size => "size",
            Self::Progress => "progress",
            Self::Down => "down",
            Self::Up => "up",
            Self::Eta => "eta",
            Self::Ratio => "ratio",
            Self::Status => "status",
            Self::Queue => "queue",
        }
    }
}

pub struct App {
    pub client: Arc<TransmissionClient>,
    pub bindings: Bindings,
    pub theme: ThemeConfig,
    pub view: View,
    pub running: bool,
    // Some(scroll) when help overlay is open; None when closed
    pub help: Option<u16>,

    // torrent list
    pub torrents: Vec<Torrent>,
    pub cursor: usize,
    pub selected: BTreeSet<usize>,
    pub sort_column: SortColumn,
    pub sort_ascending: bool,

    // label editing
    pub label_editing: bool,
    pub label_input: String,

    // active modal overlay (add, filter editing, confirm)
    pub modal: Option<Modal>,
    // persistent filter string (survives closing the filter modal)
    pub filter_input: String,
    // cached indices into self.torrents matching the current filter
    filtered_indices: Vec<usize>,

    // file view
    pub detail_torrent: Option<Torrent>,
    pub file_cursor: usize,
    pub file_selected: BTreeSet<usize>,

    #[cfg(feature = "rsync")]
    pub rsync_state: crate::rsync::RsyncState,

    // status bar
    pub stats: Option<SessionStats>,
    pub free: Option<FreeSpace>,
    pub last_error: Option<String>,
    error_since: Option<Instant>,
    pub default_download_dir: Option<String>,
    free_space_tick: u8,

    // background refresh
    refresh_tx: mpsc::SyncSender<RefreshMsg>,
    refresh_rx: mpsc::Receiver<RefreshMsg>,
    refresh_in_flight: bool,

    // written to config on first successful connection; None once saved or not needed
    pending_url_save: Option<String>,

    // SSH directory listing cache for the location modal: (listed_dir, subdirs).
    // Populated on Tab press; avoids redundant SSH round-trips when completing
    // within the same parent directory.
    pub location_dir_cache: Option<(String, Vec<String>)>,
}

impl App {
    pub fn new(client: TransmissionClient, config: Config) -> Self {
        let bindings = Bindings::from_config(&config.keys);
        let (refresh_tx, refresh_rx) = mpsc::sync_channel(8);
        Self {
            client: Arc::new(client),
            bindings,
            theme: config.theme,
            view: View::TorrentList,
            running: true,
            help: None,
            torrents: Vec::new(),
            cursor: 0,
            selected: BTreeSet::new(),
            sort_column: SortColumn::Queue,
            sort_ascending: true,
            label_editing: false,
            label_input: String::new(),
            modal: None,
            filter_input: String::new(),
            filtered_indices: Vec::new(),
            detail_torrent: None,
            file_cursor: 0,
            file_selected: BTreeSet::new(),
            #[cfg(feature = "rsync")]
            rsync_state: crate::rsync::RsyncState::default(),
            stats: None,
            free: None,
            last_error: None,
            error_since: None,
            default_download_dir: None,
            free_space_tick: 0,
            refresh_tx,
            refresh_rx,
            refresh_in_flight: false,
            pending_url_save: None,
            location_dir_cache: None,
        }
    }

    pub fn with_pending_url_save(mut self, url: Option<String>) -> Self {
        self.pending_url_save = url;
        self
    }

    /// Returns the remote hostname for SSH if the connection is not to localhost.
    fn ssh_host(&self) -> Option<String> {
        let url = &self.client.url;
        let after_scheme = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))?;
        let host_port = after_scheme.split('/').next()?;
        // IPv6 literals are bracketed: [::1]:9091 — strip brackets before comparing.
        let host = if host_port.starts_with('[') {
            host_port.split(']').next()?.trim_start_matches('[')
        } else {
            host_port.split(':').next()?.trim()
        };
        match host {
            "" | "localhost" | "127.0.0.1" | "::1" => None,
            // Reject flag-shaped hostnames to prevent argv flag smuggling in ssh invocations.
            h if h.starts_with('-') => None,
            h => Some(h.to_string()),
        }
    }

    /// Tab-completes a location input, using SSH for remote daemons and the local
    /// filesystem for local ones.  Results are cached by parent directory so that
    /// repeated Tab presses in the same directory don't re-run SSH.
    ///
    /// In both cases, known torrent download directories are used as a fallback
    /// when neither SSH nor the local filesystem produces a completion — this
    /// ensures Tab is useful even with an empty input or after an SSH failure.
    fn complete_location(&mut self, input: &str) -> Option<String> {
        // Pre-collect known torrent dirs for the fallback; used in both branches.
        let known_dirs: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            self.torrents
                .iter()
                .filter(|t| !t.download_dir.is_empty())
                .filter_map(|t| {
                    seen.insert(t.download_dir.clone())
                        .then(|| t.download_dir.clone())
                })
                .collect()
        };

        match self.ssh_host() {
            Some(host) => {
                let dir = util::location_parent_dir(input);
                let listing = if self.location_dir_cache.as_ref().map(|(d, _)| d.as_str())
                    == Some(dir.as_str())
                {
                    self.location_dir_cache
                        .as_ref()
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                } else {
                    match util::list_remote_dirs(&host, &dir) {
                        Ok(dirs) => {
                            self.location_dir_cache = Some((dir, dirs.clone()));
                            dirs
                        }
                        Err(e) => {
                            // Cache empty so we don't retry on every Tab press.
                            self.location_dir_cache = Some((dir, vec![]));
                            self.last_error = Some(format!("SSH directory listing failed: {e}"));
                            self.error_since = Some(std::time::Instant::now());
                            vec![]
                        }
                    }
                };
                // Fall back to torrent dirs only when SSH returned nothing at all
                // (e.g. auth failure). If the listing is non-empty but has no
                // unique prefix, the user simply needs to type more.
                util::autocomplete_remote_path(input, &listing).or_else(|| {
                    if listing.is_empty() {
                        util::autocomplete_remote_path(input, &known_dirs)
                    } else {
                        None
                    }
                })
            }
            None => {
                // Fall back to torrent dirs only when the filesystem has no
                // candidates at all — not merely when they share no common prefix.
                let fs_matches = util::get_path_suggestions(input);
                util::autocomplete_path(input).or_else(|| {
                    if fs_matches.is_empty() {
                        util::autocomplete_remote_path(input, &known_dirs)
                    } else {
                        None
                    }
                })
            }
        }
    }

    pub fn filtered_torrents(&self) -> Vec<&Torrent> {
        self.filtered_indices
            .iter()
            .filter_map(|&i| self.torrents.get(i))
            .collect()
    }

    pub fn rebuild_filter(&mut self) {
        let raw = self.filter_input.trim().to_lowercase();
        self.filtered_indices = if raw.is_empty() {
            (0..self.torrents.len()).collect()
        } else if let Some(status) = raw.strip_prefix("status:") {
            let status = status.trim();
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| t.status_str().to_lowercase() == status)
                .map(|(i, _)| i)
                .collect()
        } else if let Some(tracker) = raw.strip_prefix("tracker:") {
            let tracker = tracker.trim().to_string();
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| {
                    t.tracker_stats.iter().any(|ts| {
                        ts.host.to_lowercase().contains(&tracker)
                            || ts.announce.to_lowercase().contains(&tracker)
                    })
                })
                .map(|(i, _)| i)
                .collect()
        } else if let Some(lbl) = raw.strip_prefix("label:") {
            let lbl = lbl.trim().to_lowercase();
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| t.labels.iter().any(|l| l.to_lowercase().contains(&lbl)))
                .map(|(i, _)| i)
                .collect()
        } else {
            self.torrents
                .iter()
                .enumerate()
                .filter(|(_, t)| t.name.to_lowercase().contains(&raw))
                .map(|(i, _)| i)
                .collect()
        };
    }

    pub fn target_ids(&self) -> Vec<i64> {
        let visible = self.filtered_torrents();
        if self.selected.is_empty() {
            visible
                .get(self.cursor)
                .map(|t| vec![t.id])
                .unwrap_or_default()
        } else {
            self.selected
                .iter()
                .filter_map(|&i| visible.get(i).map(|t| t.id))
                .collect()
        }
    }

    fn file_target_indices(&self) -> Vec<usize> {
        if self.file_selected.is_empty() {
            vec![self.file_cursor]
        } else {
            self.file_selected.iter().copied().collect()
        }
    }

    fn clamp_cursor(&mut self) {
        let len = self.filtered_torrents().len();
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    fn clamp_file_cursor(&mut self) {
        let len = self
            .detail_torrent
            .as_ref()
            .map(|t| t.files.len())
            .unwrap_or(0);
        if len == 0 {
            self.file_cursor = 0;
        } else if self.file_cursor >= len {
            self.file_cursor = len - 1;
        }
    }

    fn tick_autoclear(&mut self) {
        if self
            .error_since
            .map(|t| t.elapsed() >= Duration::from_secs(10))
            .unwrap_or(false)
        {
            self.last_error = None;
            self.error_since = None;
        }
    }

    fn handle_tick(&mut self) {
        if self.help.is_none() && !matches!(self.modal, Some(Modal::Auth { .. })) {
            self.trigger_refresh();
            #[cfg(feature = "rsync")]
            if self.view == View::Rsync {
                self.refresh_rsync();
            }
        }
        self.tick_autoclear();
    }

    fn set_error(&mut self, e: impl Into<String>) {
        let e = e.into();
        if e == "HTTP 401 Unauthorized" && !matches!(self.modal, Some(Modal::Auth { .. })) {
            self.modal = Some(Modal::Auth {
                username: String::new(),
                password: String::new(),
                focused: AuthField::Username,
            });
        } else {
            self.last_error = Some(e);
            self.error_since = Some(Instant::now());
        }
    }

    fn refresh_torrents(&mut self) {
        match self.client.get_torrents(TORRENT_LIST_FIELDS) {
            Ok(mut list) => {
                self.sort_torrents(&mut list);
                self.torrents = list;
                self.rebuild_filter();
                self.clamp_cursor();
                self.last_error = None;
                self.error_since = None;
            }
            Err(e) => self.set_error(e),
        }
    }

    fn refresh_detail(&mut self) {
        let Some(tid) = self.detail_torrent.as_ref().map(|t| t.id) else {
            return;
        };
        match self.client.get_torrent(tid, TORRENT_DETAIL_FIELDS) {
            Ok(Some(t)) => {
                self.detail_torrent = Some(t);
                self.clamp_file_cursor();
            }
            Ok(None) => {
                self.detail_torrent = None;
                self.file_cursor = 0;
                self.file_selected.clear();
                self.view = View::TorrentList;
            }
            Err(e) => self.set_error(e),
        }
    }

    /// Spawn a background thread to refresh data for the current tick.
    /// No-ops if a refresh is already in flight.
    fn trigger_refresh(&mut self) {
        if self.refresh_in_flight {
            return;
        }
        self.refresh_in_flight = true;
        self.free_space_tick = self.free_space_tick.wrapping_add(1);

        let client = Arc::clone(&self.client);
        let tx = self.refresh_tx.clone();
        let view = self.view;
        let detail_id = self.detail_torrent.as_ref().map(|t| t.id);
        let need_dir = self.default_download_dir.is_none();
        let free_tick = self.free_space_tick;
        let default_dir = self.default_download_dir.clone();

        std::thread::spawn(move || {
            match view {
                View::TorrentList => {
                    let r = client.get_torrents(TORRENT_LIST_FIELDS);
                    let _ = tx.send(RefreshMsg::Torrents(r));
                }
                View::Files | View::Details => {
                    if let Some(id) = detail_id {
                        let r = client.get_torrent(id, TORRENT_DETAIL_FIELDS);
                        let _ = tx.send(RefreshMsg::Detail(Box::new(r)));
                    }
                }
                #[cfg(feature = "rsync")]
                View::Rsync => {
                    let r = client.get_torrents(TORRENT_LIST_FIELDS);
                    let _ = tx.send(RefreshMsg::Torrents(r));
                }
            }

            let stats = client.session_stats().ok();
            let new_dir = if need_dir {
                client
                    .get_torrents(&["id", "downloadDir"])
                    .ok()
                    .and_then(|v| v.into_iter().find(|t| !t.download_dir.is_empty()))
                    .map(|t| t.download_dir)
            } else {
                None
            };
            let dir = new_dir.as_deref().or(default_dir.as_deref());
            let free = if free_tick % 5 == 1 {
                dir.and_then(|d| client.free_space(d).ok())
            } else {
                None
            };
            let _ = tx.send(RefreshMsg::Stats {
                stats,
                free,
                default_dir: new_dir,
            });
        });
    }

    /// Apply any pending results from the background refresh thread.
    fn drain_results(&mut self) {
        while let Ok(msg) = self.refresh_rx.try_recv() {
            match msg {
                RefreshMsg::Torrents(result) => match result {
                    Ok(mut list) => {
                        self.sort_torrents(&mut list);
                        self.torrents = list;
                        self.rebuild_filter();
                        self.clamp_cursor();
                        self.last_error = None;
                        self.error_since = None;
                        if let Some(url) = self.pending_url_save.take() {
                            let mut cfg = crate::config::Config::load();
                            cfg.connection.url = Some(url);
                            cfg.save();
                        }
                    }
                    Err(e) => self.set_error(e),
                },
                RefreshMsg::Detail(result) => match *result {
                    Ok(Some(t)) => {
                        self.detail_torrent = Some(t);
                        self.clamp_file_cursor();
                    }
                    Ok(None) => {
                        self.detail_torrent = None;
                        self.view = View::TorrentList;
                    }
                    Err(e) => self.set_error(e),
                },
                RefreshMsg::Stats {
                    stats,
                    free,
                    default_dir,
                } => {
                    if let Some(s) = stats {
                        self.stats = Some(s);
                    }
                    if let Some(dir) = default_dir {
                        self.default_download_dir = Some(dir);
                    }
                    if let Some(f) = free {
                        self.free = Some(f);
                    }
                    self.refresh_in_flight = false;
                }
            }
        }
    }

    #[cfg(feature = "rsync")]
    fn refresh_rsync(&mut self) {
        self.rsync_state = crate::rsync::RsyncState::load();
    }

    #[cfg(feature = "rsync")]
    fn handle_rsync_key(&mut self, key: KeyEvent) {
        let (code, mods) = (key.code, key.modifiers);
        let b = &self.bindings;
        if b.back.matches(code, mods) || b.quit.matches(code, mods) || code == KeyCode::Esc {
            self.view = View::TorrentList;
        } else if code == KeyCode::Char('R') && mods == KeyModifiers::SHIFT {
            self.refresh_rsync();
        }
    }

    fn sort_torrents(&self, list: &mut [Torrent]) {
        let asc = self.sort_ascending;
        if self.sort_column == SortColumn::Name {
            list.sort_by_cached_key(|t| t.name.to_lowercase());
            if !asc {
                list.reverse();
            }
            return;
        }
        list.sort_by(|a, b| {
            let ord = match self.sort_column {
                SortColumn::Name => unreachable!(),
                SortColumn::Size => a.total_size.cmp(&b.total_size),
                SortColumn::Progress => a
                    .percent_done
                    .partial_cmp(&b.percent_done)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Down => a.rate_download.cmp(&b.rate_download),
                SortColumn::Up => a.rate_upload.cmp(&b.rate_upload),
                SortColumn::Eta => a.eta.cmp(&b.eta),
                SortColumn::Ratio => a
                    .upload_ratio
                    .partial_cmp(&b.upload_ratio)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortColumn::Status => a.status.cmp(&b.status),
                SortColumn::Queue => a.queue_position.cmp(&b.queue_position),
            };
            if asc { ord } else { ord.reverse() }
        });
    }

    fn move_down(
        cursor: &mut usize,
        selected: &mut BTreeSet<usize>,
        limit: usize,
        selecting: bool,
    ) {
        if limit == 0 {
            return;
        }
        if selecting {
            selected.insert(*cursor);
            if *cursor + 1 < limit {
                *cursor += 1;
                selected.insert(*cursor);
            }
        } else if *cursor + 1 < limit {
            *cursor += 1;
        }
    }

    fn move_up(cursor: &mut usize, selected: &mut BTreeSet<usize>, limit: usize, selecting: bool) {
        if limit == 0 {
            return;
        }
        if selecting {
            selected.insert(*cursor);
            if *cursor > 0 {
                *cursor -= 1;
                selected.insert(*cursor);
            }
        } else if *cursor > 0 {
            *cursor -= 1;
        }
    }

    fn handle_torrent_list_key(&mut self, key: KeyEvent) {
        match &self.modal {
            Some(Modal::Auth { .. }) => {
                self.handle_auth_input(key);
                return;
            }
            Some(Modal::AddUrl(_))
            | Some(Modal::AddLocation { .. })
            | Some(Modal::ChangeLocation(_)) => {
                self.handle_add_input(key);
                return;
            }
            _ => {}
        }

        if self.label_editing {
            match key.code {
                KeyCode::Enter => self.handle_label_input(),
                KeyCode::Esc => {
                    self.label_editing = false;
                    self.label_input.clear();
                }
                KeyCode::Backspace => {
                    self.label_input.pop();
                }
                KeyCode::Char(c) => {
                    self.label_input.push(c);
                }
                _ => {}
            }
            return;
        }

        match &self.modal {
            Some(Modal::Filter) => {
                self.handle_filter_input(key);
                return;
            }
            Some(Modal::Confirm(confirm @ (Confirm::Remove | Confirm::DeleteFiles))) => {
                let confirm = *confirm;
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let ids = self.target_ids();
                        let delete = matches!(confirm, Confirm::DeleteFiles);
                        if let Err(e) = self.client.remove(&ids, delete) {
                            self.set_error(e);
                        }
                        self.selected.clear();
                        self.modal = None;
                    }
                    _ => self.modal = None,
                }
                return;
            }
            _ => {}
        }

        let visible_len = self.filtered_torrents().len();
        let (code, mods) = (key.code, key.modifiers);
        let b = &self.bindings;

        let is_down = b.down.matches(code, mods) || code == KeyCode::Down;
        let is_select_down = b.select_down.matches(code, mods)
            || (code == KeyCode::Down && mods.contains(KeyModifiers::SHIFT));
        let is_up = b.up.matches(code, mods) || code == KeyCode::Up;
        let is_select_up = b.select_up.matches(code, mods)
            || (code == KeyCode::Up && mods.contains(KeyModifiers::SHIFT));

        if b.quit.matches(code, mods) || code == KeyCode::Esc {
            self.running = false;
        } else if b.help.matches(code, mods) {
            self.help = Some(0);
        } else if is_down || is_select_down {
            Self::move_down(
                &mut self.cursor,
                &mut self.selected,
                visible_len,
                is_select_down,
            );
        } else if is_up || is_select_up {
            Self::move_up(
                &mut self.cursor,
                &mut self.selected,
                visible_len,
                is_select_up,
            );
        } else if b.top.matches(code, mods) || code == KeyCode::Home {
            self.cursor = 0;
        } else if b.bottom.matches(code, mods) || code == KeyCode::End {
            if visible_len > 0 {
                self.cursor = visible_len - 1;
            }
        } else if b.select_toggle.matches(code, mods) {
            if self.selected.contains(&self.cursor) {
                self.selected.remove(&self.cursor);
            } else {
                self.selected.insert(self.cursor);
            }
        } else if b.enter.matches(code, mods) {
            let visible = self.filtered_torrents();
            if let Some(&torrent) = visible.get(self.cursor) {
                let tid = torrent.id;
                match self.client.get_torrent(tid, TORRENT_DETAIL_FIELDS) {
                    Ok(Some(t)) => {
                        self.detail_torrent = Some(t);
                        self.file_cursor = 0;
                        self.file_selected.clear();
                        self.view = View::Files;
                    }
                    Ok(None) => self.set_error("torrent not found"),
                    Err(e) => self.set_error(e),
                }
            }
        } else if b.details.matches(code, mods) {
            let visible = self.filtered_torrents();
            if let Some(&torrent) = visible.get(self.cursor) {
                let tid = torrent.id;
                match self.client.get_torrent(tid, TORRENT_DETAIL_FIELDS) {
                    Ok(Some(t)) => {
                        self.detail_torrent = Some(t);
                        self.view = View::Details;
                    }
                    Ok(None) => self.set_error("torrent not found"),
                    Err(e) => self.set_error(e),
                }
            }
        } else if b.pause.matches(code, mods) {
            let ids = self.target_ids();
            if !ids.is_empty() {
                let mut stopped_ids: Vec<i64> = Vec::new();
                let mut running_ids: Vec<i64> = Vec::new();
                for t in self.torrents.iter().filter(|t| ids.contains(&t.id)) {
                    if t.is_stopped() {
                        stopped_ids.push(t.id);
                    } else {
                        running_ids.push(t.id);
                    }
                }
                if !stopped_ids.is_empty()
                    && let Err(e) = self.client.start(&stopped_ids)
                {
                    self.set_error(e);
                }
                if !running_ids.is_empty()
                    && let Err(e) = self.client.stop(&running_ids)
                {
                    self.set_error(e);
                }
                self.selected.clear();
            }
        } else if b.remove.matches(code, mods) {
            if !self.target_ids().is_empty() {
                self.modal = Some(Modal::Confirm(Confirm::Remove));
            }
        } else if b.delete.matches(code, mods) {
            if !self.target_ids().is_empty() {
                self.modal = Some(Modal::Confirm(Confirm::DeleteFiles));
            }
        } else if b.add.matches(code, mods) {
            self.modal = Some(Modal::AddUrl(String::new()));
        } else if b.change_location.matches(code, mods) {
            if !self.target_ids().is_empty() {
                // If only one torrent is selected, pre-fill its current location.
                let mut initial_loc = String::new();
                let ids = self.target_ids();
                if ids.len() == 1
                    && let Some(t) = self.torrents.iter().find(|t| t.id == ids[0])
                {
                    initial_loc = t.download_dir.clone();
                }
                self.modal = Some(Modal::ChangeLocation(initial_loc));
            }
        } else if b.reannounce.matches(code, mods) {
            let ids = self.target_ids();
            if ids.is_empty() {
                return;
            }
            if let Err(e) = self.client.reannounce(&ids) {
                self.set_error(e);
            }
            self.selected.clear();
        } else if b.verify.matches(code, mods) {
            let ids = self.target_ids();
            if ids.is_empty() {
                return;
            }
            if let Err(e) = self.client.verify(&ids) {
                self.set_error(e);
            }
            self.selected.clear();
        } else if b.queue_up.matches(code, mods) {
            let ids = self.target_ids();
            if ids.is_empty() {
                return;
            }
            if let Err(e) = self.client.queue_move("queue-move-up", &ids) {
                self.set_error(e);
            }
        } else if b.queue_down.matches(code, mods) {
            let ids = self.target_ids();
            if ids.is_empty() {
                return;
            }
            if let Err(e) = self.client.queue_move("queue-move-down", &ids) {
                self.set_error(e);
            }
        } else if b.filter.matches(code, mods) {
            self.modal = Some(Modal::Filter);
            self.filter_input.clear();
            self.rebuild_filter();
        } else if b.sort.matches(code, mods) {
            self.sort_column = self.sort_column.next();
            self.selected.clear();
            let mut list = std::mem::take(&mut self.torrents);
            self.sort_torrents(&mut list);
            self.torrents = list;
            self.rebuild_filter();
        } else if b.sort_reverse.matches(code, mods) {
            self.sort_ascending = !self.sort_ascending;
            self.selected.clear();
            let mut list = std::mem::take(&mut self.torrents);
            self.sort_torrents(&mut list);
            self.torrents = list;
            self.rebuild_filter();
        } else if b.edit_labels.matches(code, mods) {
            let visible = self.filtered_torrents();
            if let Some(t) = visible.get(self.cursor) {
                self.label_input = t.labels.join(", ");
                self.label_editing = true;
            }
        } else if b.sequential.matches(code, mods) {
            let ids = self.target_ids();
            if !ids.is_empty() {
                let any_sequential = self
                    .torrents
                    .iter()
                    .filter(|t| ids.contains(&t.id))
                    .any(|t| t.sequential_download);
                if let Err(e) = self.client.set_sequential(&ids, !any_sequential) {
                    self.set_error(e);
                }
                self.selected.clear();
            }
        }
        #[cfg(feature = "rsync")]
        if code == KeyCode::Char('R') && mods == KeyModifiers::SHIFT {
            self.refresh_rsync();
            self.view = View::Rsync;
        }
    }

    fn handle_filter_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                self.modal = None;
                self.cursor = 0;
                self.selected.clear();
            }
            KeyCode::Backspace => {
                self.filter_input.pop();
                self.rebuild_filter();
            }
            KeyCode::Char(c) => {
                self.filter_input.push(c);
                self.rebuild_filter();
            }
            _ => {}
        }
    }

    fn handle_add_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => match self.modal.take() {
                Some(Modal::AddUrl(s)) => {
                    let url = s.trim().to_string();
                    if url.is_empty() {
                        self.modal = None;
                    } else {
                        self.modal = Some(Modal::AddLocation {
                            url,
                            location: self.default_download_dir.clone().unwrap_or_default(),
                        });
                    }
                }
                Some(Modal::AddLocation { url, location }) => {
                    let location = location.trim().to_string();
                    let dir = if location.is_empty() {
                        None
                    } else {
                        Some(location.as_str())
                    };
                    // If the input is a local .torrent file, send its bytes as base64 metainfo
                    // so the RPC works against both local and remote daemons.
                    let file_bytes = if url.ends_with(".torrent") {
                        std::fs::read(&url).ok()
                    } else {
                        None
                    };
                    if let Some(bytes) = file_bytes {
                        if let Err(e) = self.client.add_metainfo(&bytes, dir) {
                            self.set_error(e);
                        }
                    } else if let Err(e) = self.client.add(&url, dir) {
                        self.set_error(e);
                    }
                    self.modal = None;
                }
                Some(Modal::ChangeLocation(location)) => {
                    let location = location.trim().to_string();
                    if !location.is_empty() {
                        let ids = self.target_ids();
                        // we move the files by default when changing location from UI
                        if let Err(e) = self.client.set_location(&ids, &location, true) {
                            self.set_error(e);
                        }
                        self.selected.clear();
                    }
                    self.modal = None;
                }
                _ => self.modal = None,
            },
            KeyCode::Esc => {
                self.modal = None;
            }
            KeyCode::Backspace => match self.modal {
                Some(Modal::AddUrl(ref mut s)) => {
                    s.pop();
                }
                Some(Modal::AddLocation {
                    ref mut location, ..
                }) => {
                    location.pop();
                }
                Some(Modal::ChangeLocation(ref mut location)) => {
                    location.pop();
                }
                _ => {}
            },
            KeyCode::Char(c) => match self.modal {
                Some(Modal::AddUrl(ref mut s)) => {
                    s.push(c);
                }
                Some(Modal::AddLocation {
                    ref mut location, ..
                }) => {
                    location.push(c);
                }
                Some(Modal::ChangeLocation(ref mut location)) => {
                    location.push(c);
                }
                _ => {}
            },
            KeyCode::Tab => {
                // For location modals, extract the current input first so we can call
                // complete_location (which needs &mut self) without a borrow conflict.
                let location_completion: Option<String> = if matches!(
                    self.modal,
                    Some(Modal::AddLocation { .. }) | Some(Modal::ChangeLocation(_))
                ) {
                    let current = match &self.modal {
                        Some(Modal::AddLocation { location, .. })
                        | Some(Modal::ChangeLocation(location)) => location.clone(),
                        _ => unreachable!(),
                    };
                    self.complete_location(&current)
                } else {
                    None
                };

                match self.modal {
                    Some(Modal::AddUrl(ref mut s)) => {
                        if let Some(completed) = util::autocomplete_torrent_path(s) {
                            *s = completed;
                        }
                    }
                    Some(Modal::AddLocation {
                        ref mut location, ..
                    })
                    | Some(Modal::ChangeLocation(ref mut location)) => {
                        if let Some(completed) = location_completion {
                            *location = completed;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn handle_auth_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.modal = None;
            }
            KeyCode::Tab => {
                if let Some(Modal::Auth {
                    ref mut focused, ..
                }) = self.modal
                {
                    *focused = match *focused {
                        AuthField::Username => AuthField::Password,
                        AuthField::Password => AuthField::Username,
                    };
                }
            }
            KeyCode::Down => {
                if let Some(Modal::Auth {
                    ref mut focused, ..
                }) = self.modal
                    && *focused == AuthField::Username
                {
                    *focused = AuthField::Password;
                }
            }
            KeyCode::Up => {
                if let Some(Modal::Auth {
                    ref mut focused, ..
                }) = self.modal
                    && *focused == AuthField::Password
                {
                    *focused = AuthField::Username;
                }
            }
            KeyCode::Enter => {
                if matches!(
                    self.modal,
                    Some(Modal::Auth {
                        focused: AuthField::Username,
                        ..
                    })
                ) {
                    if let Some(Modal::Auth {
                        ref mut focused, ..
                    }) = self.modal
                    {
                        *focused = AuthField::Password;
                    }
                    return;
                }
                let (username, password) = match self.modal.take() {
                    Some(Modal::Auth {
                        username, password, ..
                    }) => (username, password),
                    _ => return,
                };
                self.client.set_auth(&username, &password);
                let url = self.client.url.clone();
                let cfg_path = crate::config::config_path();
                std::thread::spawn(move || {
                    persist_credentials_impl(
                        &url,
                        &username,
                        &password,
                        &cfg_path,
                        credentials::save,
                    );
                });
                self.refresh_torrents();
            }
            KeyCode::Backspace => {
                if let Some(Modal::Auth {
                    ref mut username,
                    ref mut password,
                    focused,
                }) = self.modal
                {
                    match focused {
                        AuthField::Username => {
                            username.pop();
                        }
                        AuthField::Password => {
                            password.pop();
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(Modal::Auth {
                    ref mut username,
                    ref mut password,
                    focused,
                }) = self.modal
                {
                    match focused {
                        AuthField::Username => username.push(c),
                        AuthField::Password => password.push(c),
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_label_input(&mut self) {
        let ids = self.target_ids();
        if !ids.is_empty() {
            let labels: Vec<String> = self
                .label_input
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if let Err(e) = self.client.set_labels(&ids, &labels) {
                self.set_error(e);
            } else {
                self.refresh_torrents();
            }
        }
        self.label_editing = false;
        self.label_input.clear();
    }

    fn handle_files_key(&mut self, key: KeyEvent) {
        if matches!(
            self.modal,
            Some(Modal::Confirm(Confirm::DeleteFileFromDisk))
        ) {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.delete_files_from_disk();
                    self.modal = None;
                }
                _ => self.modal = None,
            }
            return;
        }

        let file_count = self
            .detail_torrent
            .as_ref()
            .map(|t| t.files.len())
            .unwrap_or(0);

        let (code, mods) = (key.code, key.modifiers);
        let b = &self.bindings;

        let is_down = b.down.matches(code, mods) || code == KeyCode::Down;
        let is_select_down = b.select_down.matches(code, mods)
            || (code == KeyCode::Down && mods.contains(KeyModifiers::SHIFT));
        let is_up = b.up.matches(code, mods) || code == KeyCode::Up;
        let is_select_up = b.select_up.matches(code, mods)
            || (code == KeyCode::Up && mods.contains(KeyModifiers::SHIFT));

        if b.back.matches(code, mods) || b.quit.matches(code, mods) {
            self.view = View::TorrentList;
            self.file_selected.clear();
        } else if b.help.matches(code, mods) {
            self.help = Some(0);
        } else if is_down || is_select_down {
            Self::move_down(
                &mut self.file_cursor,
                &mut self.file_selected,
                file_count,
                is_select_down,
            );
        } else if is_up || is_select_up {
            Self::move_up(
                &mut self.file_cursor,
                &mut self.file_selected,
                file_count,
                is_select_up,
            );
        } else if b.top.matches(code, mods) || code == KeyCode::Home {
            self.file_cursor = 0;
        } else if b.bottom.matches(code, mods) || code == KeyCode::End {
            if file_count > 0 {
                self.file_cursor = file_count - 1;
            }
        } else if b.select_toggle.matches(code, mods) {
            if self.file_selected.contains(&self.file_cursor) {
                self.file_selected.remove(&self.file_cursor);
            } else {
                self.file_selected.insert(self.file_cursor);
            }
        } else if b.priority_up.matches(code, mods) {
            self.adjust_file_priority(true);
        } else if b.priority_down.matches(code, mods) {
            self.adjust_file_priority(false);
        } else if b.toggle_wanted.matches(code, mods) {
            self.toggle_file_wanted();
        } else if b.delete.matches(code, mods) {
            self.modal = Some(Modal::Confirm(Confirm::DeleteFileFromDisk));
        } else if b.reannounce.matches(code, mods)
            && let Some(t) = &self.detail_torrent
            && let Err(e) = self.client.reannounce(&[t.id])
        {
            self.set_error(e);
        }
    }

    fn adjust_file_priority(&mut self, increase: bool) {
        let Some(torrent) = &self.detail_torrent else {
            return;
        };
        let tid = torrent.id;
        let indices = self.file_target_indices();
        let changes: Vec<(usize, FilePriority)> = indices
            .iter()
            .filter_map(|&i| {
                torrent.file_stats.get(i).map(|stats| {
                    let current = FilePriority::from_stats(stats);
                    let next = if increase {
                        current.next()
                    } else {
                        current.prev()
                    };
                    (i, next)
                })
            })
            .collect();

        if changes.is_empty() {
            return;
        }

        if let Err(e) = self.client.set_file_priorities(tid, &changes) {
            self.set_error(e);
            return;
        }
        self.file_selected.clear();
        self.refresh_detail();
    }

    fn toggle_file_wanted(&mut self) {
        let Some(torrent) = &self.detail_torrent else {
            return;
        };
        let tid = torrent.id;
        let indices = self.file_target_indices();
        let changes: Vec<(usize, FilePriority)> = indices
            .iter()
            .filter_map(|&i| {
                torrent.file_stats.get(i).map(|stats| {
                    let current = FilePriority::from_stats(stats);
                    let toggled = if current == FilePriority::Unwanted {
                        FilePriority::Normal
                    } else {
                        FilePriority::Unwanted
                    };
                    (i, toggled)
                })
            })
            .collect();

        if changes.is_empty() {
            return;
        }

        if let Err(e) = self.client.set_file_priorities(tid, &changes) {
            self.set_error(e);
            return;
        }
        self.file_selected.clear();
        self.refresh_detail();
    }

    pub fn is_local_daemon(&self) -> bool {
        let url = &self.client.url;
        // Parse the host from the URL (e.g. "http://127.0.0.1:9091/transmission/rpc")
        let host = if let Some(after_scheme) = url.find("://").map(|i| &url[i + 3..]) {
            let host_and_rest = after_scheme.split('/').next().unwrap_or("");
            // Strip port if present
            if host_and_rest.starts_with('[') {
                // IPv6 literal: [::1]:port
                host_and_rest
                    .trim_start_matches('[')
                    .split(']')
                    .next()
                    .unwrap_or("")
            } else {
                host_and_rest.split(':').next().unwrap_or("")
            }
        } else {
            ""
        };
        matches!(host, "localhost" | "127.0.0.1" | "::1")
    }

    fn delete_files_from_disk(&mut self) {
        if !self.is_local_daemon() {
            self.last_error = Some("delete from disk is only supported for local daemons".into());
            return;
        }
        let Some(torrent) = &self.detail_torrent else {
            return;
        };
        let dir = &torrent.download_dir;
        if dir.is_empty() {
            self.set_error("unknown download directory");
            return;
        }
        let indices = self.file_target_indices();
        let mut errors = Vec::new();
        for &i in &indices {
            if let Some(file) = torrent.files.get(i) {
                if !is_safe_relative_path(&file.name) {
                    errors.push(format!("{}: unsafe path rejected", file.name));
                    continue;
                }
                let path = std::path::Path::new(dir).join(&file.name);
                if let Err(e) = std::fs::remove_file(&path) {
                    errors.push(format!("{}: {e}", file.name));
                }
            }
        }
        if !errors.is_empty() {
            self.set_error(errors.join("; "));
        }
        self.file_selected.clear();
    }

    fn handle_details_key(&mut self, key: KeyEvent) {
        let (code, mods) = (key.code, key.modifiers);
        let b = &self.bindings;

        if b.back.matches(code, mods) || b.quit.matches(code, mods) {
            self.view = View::TorrentList;
        } else if b.help.matches(code, mods) {
            self.help = Some(0);
        } else if b.enter.matches(code, mods) {
            self.file_cursor = 0;
            self.file_selected.clear();
            self.view = View::Files;
        } else if b.reannounce.matches(code, mods)
            && let Some(t) = &self.detail_torrent
            && let Err(e) = self.client.reannounce(&[t.id])
        {
            self.set_error(e);
        }
    }

    fn handle_help_key(&mut self, key: KeyEvent) {
        let (code, mods) = (key.code, key.modifiers);
        let b = &self.bindings;
        let close = b.quit.matches(code, mods)
            || b.back.matches(code, mods)
            || b.help.matches(code, mods)
            || code == KeyCode::Esc;
        let dn = b.down.matches(code, mods) || code == KeyCode::Down;
        let up = b.up.matches(code, mods) || code == KeyCode::Up;
        let top = b.top.matches(code, mods) || code == KeyCode::Home;

        if close {
            self.help = None;
        } else if let Some(scroll) = &mut self.help {
            if dn {
                *scroll = scroll.saturating_add(1);
            } else if up {
                *scroll = scroll.saturating_sub(1);
            } else if top {
                *scroll = 0;
            } else if code == KeyCode::PageDown {
                *scroll = scroll.saturating_add(10);
            } else if code == KeyCode::PageUp {
                *scroll = scroll.saturating_sub(10);
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.help.is_some() {
            self.handle_help_key(key);
            return;
        }
        match self.view {
            View::TorrentList => self.handle_torrent_list_key(key),
            View::Files => self.handle_files_key(key),
            View::Details => self.handle_details_key(key),
            #[cfg(feature = "rsync")]
            View::Rsync => self.handle_rsync_key(key),
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        self.trigger_refresh();

        let tick_rate = Duration::from_secs(1);
        let mut last_tick = Instant::now() - tick_rate;

        loop {
            self.drain_results();
            terminal.draw(|f| ui::draw(f, &self))?;

            if !self.running {
                break;
            }

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)?
                && let Event::Key(key) = event::read()?
                && key.kind == event::KeyEventKind::Press
            {
                self.handle_key(key);
            }

            if last_tick.elapsed() >= tick_rate {
                self.handle_tick();
                last_tick = Instant::now();
            }
        }

        Ok(())
    }
}

fn persist_credentials_impl<F>(
    url: &str,
    username: &str,
    password: &str,
    cfg_path: &std::path::PathBuf,
    save_fn: F,
) where
    F: FnOnce(&str, &str, &str) -> Result<(), String>,
{
    if save_fn(url, username, password).is_err() {
        let mut cfg = crate::config::Config::load_from(cfg_path);
        cfg.connection.username = Some(username.to_string());
        cfg.connection.password = Some(password.to_string());
        cfg.save_to(cfg_path);
    }
}

fn is_safe_relative_path(name: &str) -> bool {
    use std::path::{Component, Path};
    !name.is_empty()
        && Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::TransmissionClient;
    use crate::config::Config;
    use crate::protocol::{Torrent, TrackerStats};

    #[test]
    fn test_sort_column() {
        assert_eq!(SortColumn::Name.label(), "name");
        assert_eq!(SortColumn::Name.next(), SortColumn::Size);
        assert_eq!(SortColumn::Queue.next(), SortColumn::Name);
    }

    #[test]
    fn test_sort_column_full_cycle() {
        // Cycling through all variants starting from Name must return to Name
        let variants = [
            SortColumn::Name,
            SortColumn::Size,
            SortColumn::Progress,
            SortColumn::Down,
            SortColumn::Up,
            SortColumn::Eta,
            SortColumn::Ratio,
            SortColumn::Status,
            SortColumn::Queue,
        ];
        let n = variants.len();
        for (i, &col) in variants.iter().enumerate() {
            assert_eq!(col.next(), variants[(i + 1) % n]);
        }
    }

    #[test]
    fn test_filtering() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        let t1 = Torrent {
            name: "ubuntu.iso".into(),
            status: 4, // Downloading
            tracker_stats: vec![TrackerStats {
                host: "tracker.ubuntu.com".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let t2 = Torrent {
            name: "debian.iso".into(),
            status: 6, // Seeding
            tracker_stats: vec![TrackerStats {
                host: "tracker.debian.org".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        app.torrents = vec![t1.clone(), t2.clone()];
        app.rebuild_filter();

        assert_eq!(app.filtered_torrents().len(), 2);

        app.filter_input = "ubuntu".into();
        app.rebuild_filter();
        assert_eq!(app.filtered_torrents().len(), 1);

        app.filter_input = "status:seeding".into();
        app.rebuild_filter();
        assert_eq!(app.filtered_torrents().len(), 1);
        assert_eq!(app.filtered_torrents()[0].name, "debian.iso");

        app.filter_input = "tracker:ubuntu.com".into();
        app.rebuild_filter();
        assert_eq!(app.filtered_torrents().len(), 1);
        assert_eq!(app.filtered_torrents()[0].name, "ubuntu.iso");

        let t3 = Torrent {
            name: "arch.iso".into(),
            labels: vec!["linux".into(), "iso".into()],
            ..Default::default()
        };
        app.torrents = vec![t1.clone(), t2.clone(), t3.clone()];

        app.filter_input = "label:linux".into();
        app.rebuild_filter();
        assert_eq!(app.filtered_torrents().len(), 1);
        assert_eq!(app.filtered_torrents()[0].name, "arch.iso");

        app.filter_input = "label:ISO".into(); // case-insensitive
        app.rebuild_filter();
        assert_eq!(app.filtered_torrents().len(), 1);

        app.filter_input = "label:nonexistent".into();
        app.rebuild_filter();
        assert_eq!(app.filtered_torrents().len(), 0);
    }

    #[test]
    fn test_cursor_movement() {
        let mut cursor = 0;
        let mut selected = std::collections::BTreeSet::new();

        App::move_down(&mut cursor, &mut selected, 5, false);
        assert_eq!(cursor, 1);
        assert!(selected.is_empty());

        App::move_down(&mut cursor, &mut selected, 5, true);
        assert_eq!(cursor, 2);
        assert!(selected.contains(&1));
        assert!(selected.contains(&2));

        App::move_up(&mut cursor, &mut selected, 5, false);
        assert_eq!(cursor, 1);
        assert_eq!(selected.len(), 2);

        App::move_up(&mut cursor, &mut selected, 5, true);
        assert_eq!(cursor, 0);
        assert!(selected.contains(&0));
        assert!(selected.contains(&1));
    }

    #[test]
    fn test_sorting() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        let t1 = Torrent {
            name: "B".into(),
            total_size: 100,
            percent_done: 0.5,
            rate_download: 50,
            eta: 10,
            queue_position: 2,
            status: 1,
            ..Default::default()
        };

        let t2 = Torrent {
            name: "A".into(),
            total_size: 200,
            percent_done: 0.8,
            rate_download: 20,
            eta: 20,
            queue_position: 1,
            status: 2,
            ..Default::default()
        };

        let mut list = vec![t1, t2];
        app.torrents = list.clone();
        app.rebuild_filter();

        // Sort by Name, asc
        app.sort_column = SortColumn::Name;
        app.sort_ascending = true;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "A");

        // Sort by Name, desc
        app.sort_ascending = false;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "B");

        // Sort by Size, asc
        app.sort_column = SortColumn::Size;
        app.sort_ascending = true;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "B");

        // Sort by Progress, desc
        app.sort_column = SortColumn::Progress;
        app.sort_ascending = false;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "A");

        // Sort by Down, desc
        app.sort_column = SortColumn::Down;
        app.sort_ascending = false;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "B");

        // Sort by ETA, asc
        app.sort_column = SortColumn::Eta;
        app.sort_ascending = true;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "B");

        // Sort by Queue, asc
        app.sort_column = SortColumn::Queue;
        app.sort_ascending = true;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "A");

        // Sort by Status, asc
        app.sort_column = SortColumn::Status;
        app.sort_ascending = true;
        app.sort_torrents(&mut list);
        assert_eq!(list[0].name, "B");
    }

    #[test]
    fn test_is_safe_relative_path() {
        assert!(is_safe_relative_path("file.txt"));
        assert!(is_safe_relative_path("subdir/file.txt"));
        assert!(!is_safe_relative_path(""));
        assert!(!is_safe_relative_path("../etc/passwd"));
        assert!(!is_safe_relative_path("/absolute/path"));
        assert!(!is_safe_relative_path("./dot/relative"));
    }

    #[test]
    fn test_target_ids_cursor() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        let t1 = Torrent {
            id: 1,
            ..Default::default()
        };
        let t2 = Torrent {
            id: 2,
            ..Default::default()
        };
        app.torrents = vec![t1, t2];
        app.rebuild_filter();

        app.cursor = 0;
        assert_eq!(app.target_ids(), vec![1]);

        app.cursor = 1;
        assert_eq!(app.target_ids(), vec![2]);
    }

    #[test]
    fn test_target_ids_selected() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        let t1 = Torrent {
            id: 10,
            ..Default::default()
        };
        let t2 = Torrent {
            id: 20,
            ..Default::default()
        };
        let t3 = Torrent {
            id: 30,
            ..Default::default()
        };
        app.torrents = vec![t1, t2, t3];
        app.rebuild_filter();

        app.selected.insert(0);
        app.selected.insert(2);
        let mut ids = app.target_ids();
        ids.sort();
        assert_eq!(ids, vec![10, 30]);
    }

    #[test]
    fn test_clamp_cursor() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        // Empty list: cursor stays 0
        app.cursor = 5;
        app.clamp_cursor();
        assert_eq!(app.cursor, 0);

        let t1 = Torrent {
            id: 1,
            ..Default::default()
        };
        let t2 = Torrent {
            id: 2,
            ..Default::default()
        };
        app.torrents = vec![t1, t2];
        app.rebuild_filter();

        app.cursor = 10;
        app.clamp_cursor();
        assert_eq!(app.cursor, 1);

        app.cursor = 1;
        app.clamp_cursor();
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn test_file_target_indices_cursor() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.file_cursor = 3;
        assert_eq!(app.file_target_indices(), vec![3]);
    }
    #[test]
    fn test_target_ids_from_labels() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        let t = Torrent {
            id: 10,
            labels: vec!["foo".into(), "bar".into()],
            ..Default::default()
        };
        app.torrents = vec![t];
        app.rebuild_filter();

        assert!(!app.label_editing);
        assert!(app.label_input.is_empty());

        app.cursor = 0;
        app.selected.insert(0);

        // Edit labels trigger
        app.label_input = "foo, bar, baz".into();
        app.label_editing = true;

        // Actually submit the labels
        app.handle_label_input();

        // Should close label editing
        assert!(!app.label_editing);
        assert!(app.label_input.is_empty());

        // The dummy client will return an error because it's a dummy HTTP agent
        assert!(app.last_error.is_some());
    }

    #[test]
    fn test_file_target_indices_selected() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.file_selected.insert(1);
        app.file_selected.insert(4);
        let mut idxs = app.file_target_indices();
        idxs.sort();
        assert_eq!(idxs, vec![1, 4]);
    }

    #[test]
    fn test_handle_add_input_transitions() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        // State 1: Enter AddUrl modal
        app.modal = Some(Modal::AddUrl(String::new()));

        // Type "http://test"
        for c in "http://test".chars() {
            app.handle_add_input(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::empty(),
            });
        }

        match &app.modal {
            Some(Modal::AddUrl(s)) => assert_eq!(s, "http://test"),
            _ => panic!("Expected AddUrl"),
        }

        // Press Enter to go to AddLocation
        app.handle_add_input(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });

        match &app.modal {
            Some(Modal::AddLocation { url, location }) => {
                assert_eq!(url, "http://test");
                assert_eq!(location, ""); // Default is empty here
            }
            _ => panic!("Expected AddLocation"),
        }

        // Type "/downloads"
        for c in "/downloads".chars() {
            app.handle_add_input(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::empty(),
            });
        }

        // Backspace once
        app.handle_add_input(KeyEvent {
            code: KeyCode::Backspace,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });

        match &app.modal {
            Some(Modal::AddLocation { url, location }) => {
                assert_eq!(url, "http://test");
                assert_eq!(location, "/download");
            }
            _ => panic!("Expected AddLocation with /download"),
        }

        // Press Enter. In a real environment, this sends an RPC. Since client has dummy agent,
        // it fails with a string error, setting last_error and clearing modal.
        app.handle_add_input(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });

        assert!(app.modal.is_none());
        assert!(app.last_error.is_some()); // Ureq dummy agent will fail.
    }

    #[test]
    fn test_handle_change_location_transitions() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        // State 1: Enter ChangeLocation modal
        app.modal = Some(Modal::ChangeLocation(String::new()));

        // Type "/new_path"
        for c in "/new_path".chars() {
            app.handle_add_input(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::empty(),
            });
        }

        match &app.modal {
            Some(Modal::ChangeLocation(s)) => assert_eq!(s, "/new_path"),
            _ => panic!("Expected ChangeLocation"),
        }

        // Press Enter. Should close modal and set last error (due to dummy client)
        app.handle_add_input(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });

        assert!(app.modal.is_none());
        assert!(app.last_error.is_some());
    }

    #[test]
    fn test_handle_torrent_list_key_change_location() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        app.torrents.push(crate::protocol::Torrent {
            id: 1,
            download_dir: "/default/path".to_string(),
            ..Default::default()
        });
        app.rebuild_filter();

        // select the torrent
        app.cursor = 0;

        // trigger change_location key ('m' is default)
        let key = KeyEvent {
            code: KeyCode::Char('m'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        app.handle_torrent_list_key(key);

        match &app.modal {
            Some(Modal::ChangeLocation(loc)) => assert_eq!(loc, "/default/path"),
            _ => panic!("Expected ChangeLocation modal with prefilled path"),
        }
    }

    // --- regression tests for fix/selection-survives-sort ---

    #[test]
    fn test_sort_clears_selection() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        app.torrents = vec![
            Torrent {
                id: 1,
                name: "alpha".into(),
                ..Default::default()
            },
            Torrent {
                id: 2,
                name: "beta".into(),
                ..Default::default()
            },
            Torrent {
                id: 3,
                name: "gamma".into(),
                ..Default::default()
            },
        ];
        app.rebuild_filter();

        // pre-select some indices
        app.selected.insert(0);
        app.selected.insert(2);
        assert!(!app.selected.is_empty());

        // press 's' — the default sort key
        let key = KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        app.handle_torrent_list_key(key);

        assert!(
            app.selected.is_empty(),
            "selection must be cleared when sort order changes"
        );
        // List must also be immediately re-sorted (name asc = alpha, beta, gamma)
        assert_eq!(
            app.torrents
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta", "gamma"],
            "torrents must be re-sorted immediately on keypress, not deferred to next tick"
        );
    }

    #[test]
    fn test_pause_toggles_per_torrent() {
        // When the selection contains both stopped and running torrents, pause must
        // start the stopped ones and stop the running ones independently.
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![
            Torrent {
                id: 1,
                status: 0,
                ..Default::default()
            }, // stopped
            Torrent {
                id: 2,
                status: 4,
                ..Default::default()
            }, // downloading (running)
            Torrent {
                id: 3,
                status: 0,
                ..Default::default()
            }, // stopped
        ];
        app.rebuild_filter();

        // Verify the per-torrent split logic directly
        let ids = vec![1i64, 2, 3];
        let mut stopped_ids: Vec<i64> = Vec::new();
        let mut running_ids: Vec<i64> = Vec::new();
        for t in app.torrents.iter().filter(|t| ids.contains(&t.id)) {
            if t.is_stopped() {
                stopped_ids.push(t.id);
            } else {
                running_ids.push(t.id);
            }
        }
        assert_eq!(stopped_ids, vec![1, 3], "stopped torrents must be started");
        assert_eq!(running_ids, vec![2], "running torrents must be stopped");
    }

    #[test]
    fn test_move_down_empty_list_does_not_populate_selected() {
        let mut cursor: usize = 0;
        let mut selected = BTreeSet::new();
        App::move_down(&mut cursor, &mut selected, 0, true);
        assert!(
            selected.is_empty(),
            "move_down on empty list must not insert into selected"
        );
        assert_eq!(cursor, 0);
    }

    #[test]
    fn test_move_up_empty_list_does_not_populate_selected() {
        let mut cursor: usize = 0;
        let mut selected = BTreeSet::new();
        App::move_up(&mut cursor, &mut selected, 0, true);
        assert!(
            selected.is_empty(),
            "move_up on empty list must not insert into selected"
        );
        assert_eq!(cursor, 0);
    }

    #[cfg(feature = "rsync")]
    #[test]
    fn test_r_key_opens_rsync_view() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        assert_eq!(app.view, View::TorrentList);
        app.handle_torrent_list_key(KeyEvent {
            code: KeyCode::Char('R'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
        assert_eq!(app.view, View::Rsync);
    }

    #[cfg(feature = "rsync")]
    #[test]
    fn test_rsync_view_back_returns_to_list() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Rsync;
        app.handle_rsync_key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
        assert_eq!(app.view, View::TorrentList);
    }

    #[test]
    fn test_refresh_detail_ok_none_clears_file_state() {
        // Simulate what refresh_detail() does on Ok(None): the torrent disappeared.
        // We set up the state as if the user was in the file view, then replicate
        // the Ok(None) branch and assert all file state is reset.
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );

        // Put app into file-view state
        app.view = View::Files;
        app.detail_torrent = Some(Torrent {
            id: 42,
            name: "some-torrent".into(),
            ..Default::default()
        });
        app.file_cursor = 3;
        app.file_selected.insert(1);
        app.file_selected.insert(2);

        // Replicate the Ok(None) branch from refresh_detail()
        app.detail_torrent = None;
        app.file_cursor = 0;
        app.file_selected.clear();
        app.view = View::TorrentList;

        assert!(app.detail_torrent.is_none(), "detail_torrent must be None");
        assert_eq!(app.file_cursor, 0, "file_cursor must be reset to 0");
        assert!(
            app.file_selected.is_empty(),
            "file_selected must be cleared"
        );
        assert!(
            matches!(app.view, View::TorrentList),
            "view must return to TorrentList"
        );
    }

    // -------------------------------------------------------------------------
    // Tests for the empty-id guard: reannounce, verify, queue_up, queue_down
    // -------------------------------------------------------------------------

    fn make_key(
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) -> crossterm::event::KeyEvent {
        use crossterm::event::{KeyEventKind, KeyEventState};
        crossterm::event::KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    /// Build an App with an empty torrent list connected to a dummy (unreachable) URL.
    /// Any actual RPC call would fail and set `last_error`.
    fn empty_app() -> App {
        App::new(
            TransmissionClient::new("http://dummy.invalid:9091/transmission/rpc", None, None),
            Config::default(),
        )
    }

    #[test]
    fn test_reannounce_empty_ids_no_error() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        // torrents is empty, so target_ids() returns []
        assert!(app.target_ids().is_empty());

        app.handle_torrent_list_key(make_key(KeyCode::Char('t'), KeyModifiers::NONE));
        assert!(
            app.last_error.is_none(),
            "reannounce with no torrents should not set last_error, got: {:?}",
            app.last_error
        );
    }

    #[test]
    fn test_verify_empty_ids_no_error() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        assert!(app.target_ids().is_empty());

        app.handle_torrent_list_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(
            app.last_error.is_none(),
            "verify with no torrents should not set last_error, got: {:?}",
            app.last_error
        );
    }

    #[test]
    fn test_queue_up_empty_ids_no_error() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        assert!(app.target_ids().is_empty());

        // queue_up is bound to 'K' (uppercase, so SHIFT modifier)
        app.handle_torrent_list_key(make_key(KeyCode::Char('K'), KeyModifiers::SHIFT));
        assert!(
            app.last_error.is_none(),
            "queue_up with no torrents should not set last_error, got: {:?}",
            app.last_error
        );
    }

    #[test]
    fn test_queue_down_empty_ids_no_error() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        assert!(app.target_ids().is_empty());

        // queue_down is bound to 'J' (uppercase, so SHIFT modifier)
        app.handle_torrent_list_key(make_key(KeyCode::Char('J'), KeyModifiers::SHIFT));
        assert!(
            app.last_error.is_none(),
            "queue_down with no torrents should not set last_error, got: {:?}",
            app.last_error
        );
    }

    // -------------------------------------------------------------------------
    // Tests for is_local_daemon
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_local_daemon_localhost() {
        let app = App::new(
            TransmissionClient::new("http://localhost:9091/transmission/rpc", None, None),
            Config::default(),
        );
        assert!(app.is_local_daemon());
    }

    #[test]
    fn test_is_local_daemon_ipv4_loopback() {
        let app = App::new(
            TransmissionClient::new("http://127.0.0.1:9091/transmission/rpc", None, None),
            Config::default(),
        );
        assert!(app.is_local_daemon());
    }

    #[test]
    fn test_is_local_daemon_ipv6_loopback() {
        let app = App::new(
            TransmissionClient::new("http://[::1]:9091/transmission/rpc", None, None),
            Config::default(),
        );
        assert!(app.is_local_daemon());
    }

    #[test]
    fn test_is_local_daemon_remote_ip() {
        let app = App::new(
            TransmissionClient::new("http://192.168.1.1:9091/transmission/rpc", None, None),
            Config::default(),
        );
        assert!(!app.is_local_daemon());
    }

    #[test]
    fn test_is_local_daemon_remote_hostname() {
        let app = App::new(
            TransmissionClient::new("http://remote.example.com/transmission/rpc", None, None),
            Config::default(),
        );
        assert!(!app.is_local_daemon());
    }

    // -------------------------------------------------------------------------
    // Test for delete_files_from_disk blocked on remote daemon
    // -------------------------------------------------------------------------

    #[test]
    fn test_delete_files_from_disk_blocked_on_remote() {
        use crate::protocol::TorrentFile;

        let mut app = App::new(
            TransmissionClient::new("http://192.168.1.1:9091/transmission/rpc", None, None),
            Config::default(),
        );

        // Set up a detail torrent with a file so the function would normally proceed
        app.detail_torrent = Some(Torrent {
            id: 1,
            download_dir: "/downloads".into(),
            files: vec![TorrentFile {
                name: "test_file.txt".into(),
                length: 100,
                bytes_completed: 100,
            }],
            ..Default::default()
        });
        app.file_cursor = 0;

        // Call delete_files_from_disk directly
        app.delete_files_from_disk();

        assert!(
            app.last_error.is_some(),
            "delete_files_from_disk on remote should set last_error"
        );
        let err = app.last_error.as_ref().unwrap();
        assert!(
            err.contains("local"),
            "error message should mention 'local', got: {err}"
        );
    }

    #[test]
    fn test_sort_column_index() {
        assert_eq!(SortColumn::Status.column_index(), Some(0));
        assert_eq!(SortColumn::Name.column_index(), Some(1));
        assert_eq!(SortColumn::Size.column_index(), Some(2));
        assert_eq!(SortColumn::Progress.column_index(), Some(3));
        assert_eq!(SortColumn::Down.column_index(), Some(4));
        assert_eq!(SortColumn::Up.column_index(), Some(5));
        assert_eq!(SortColumn::Eta.column_index(), Some(6));
        assert_eq!(SortColumn::Ratio.column_index(), Some(7));
        assert_eq!(SortColumn::Queue.column_index(), None);
    }

    #[test]
    fn test_handle_torrent_list_key_quit() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        assert!(app.running);
        app.handle_torrent_list_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!app.running);
    }

    #[test]
    fn test_handle_torrent_list_key_esc_quits() {
        let mut app = empty_app();
        app.handle_torrent_list_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.running);
    }

    #[test]
    fn test_handle_torrent_list_key_help() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        assert!(app.help.is_none());
        app.handle_torrent_list_key(make_key(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(app.help.is_some());
    }

    #[test]
    fn test_handle_torrent_list_key_navigate() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![
            Torrent {
                id: 1,
                ..Default::default()
            },
            Torrent {
                id: 2,
                ..Default::default()
            },
            Torrent {
                id: 3,
                ..Default::default()
            },
        ];
        app.rebuild_filter();
        assert_eq!(app.cursor, 0);
        app.handle_torrent_list_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.cursor, 1);
        app.handle_torrent_list_key(make_key(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.cursor, 2);
        app.handle_torrent_list_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.cursor, 1);
        app.handle_torrent_list_key(make_key(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.cursor, 0);
        app.handle_torrent_list_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn test_handle_torrent_list_key_home_end() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![
            Torrent {
                id: 1,
                ..Default::default()
            },
            Torrent {
                id: 2,
                ..Default::default()
            },
            Torrent {
                id: 3,
                ..Default::default()
            },
        ];
        app.rebuild_filter();
        app.cursor = 1;
        app.handle_torrent_list_key(make_key(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.cursor, 0);
        app.handle_torrent_list_key(make_key(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(app.cursor, 2);
        app.handle_torrent_list_key(make_key(KeyCode::Home, KeyModifiers::NONE));
        assert_eq!(app.cursor, 0);
        app.handle_torrent_list_key(make_key(KeyCode::End, KeyModifiers::NONE));
        assert_eq!(app.cursor, 2);
    }

    #[test]
    fn test_handle_torrent_list_key_select_toggle() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            ..Default::default()
        }];
        app.rebuild_filter();
        assert!(app.selected.is_empty());
        app.handle_torrent_list_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.selected.contains(&0));
        app.handle_torrent_list_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.selected.is_empty());
    }

    #[test]
    fn test_handle_torrent_list_key_remove_opens_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            ..Default::default()
        }];
        app.rebuild_filter();
        app.handle_torrent_list_key(make_key(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Some(Modal::Confirm(Confirm::Remove))));
    }

    #[test]
    fn test_handle_torrent_list_key_delete_opens_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            ..Default::default()
        }];
        app.rebuild_filter();
        app.handle_torrent_list_key(make_key(KeyCode::Char('D'), KeyModifiers::SHIFT));
        assert!(matches!(
            app.modal,
            Some(Modal::Confirm(Confirm::DeleteFiles))
        ));
    }

    #[test]
    fn test_handle_torrent_list_key_add_opens_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.handle_torrent_list_key(make_key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Some(Modal::AddUrl(_))));
    }

    #[test]
    fn test_handle_torrent_list_key_filter_opens_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.handle_torrent_list_key(make_key(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(matches!(app.modal, Some(Modal::Filter)));
    }

    #[test]
    fn test_handle_torrent_list_key_sort_reverse_toggles() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        assert!(app.sort_ascending);
        app.handle_torrent_list_key(make_key(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert!(!app.sort_ascending);
        app.handle_torrent_list_key(make_key(KeyCode::Char('S'), KeyModifiers::SHIFT));
        assert!(app.sort_ascending);
    }

    #[test]
    fn test_handle_torrent_list_key_edit_labels_prefills() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            labels: vec!["foo".into(), "bar".into()],
            ..Default::default()
        }];
        app.rebuild_filter();
        assert!(!app.label_editing);
        app.handle_torrent_list_key(make_key(KeyCode::Char('L'), KeyModifiers::SHIFT));
        assert!(app.label_editing);
        assert_eq!(app.label_input, "foo, bar");
    }

    #[test]
    fn test_handle_torrent_list_key_sequential_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            sequential_download: false,
            ..Default::default()
        }];
        app.rebuild_filter();
        app.handle_torrent_list_key(make_key(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(app.last_error.is_some());
    }

    #[test]
    fn test_handle_torrent_list_label_editing_mode() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.label_editing = true;
        app.handle_torrent_list_key(make_key(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(app.label_input, "a");
        app.handle_torrent_list_key(make_key(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(app.label_input.is_empty());
        app.label_input = "test".into();
        app.handle_torrent_list_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.label_editing);
        assert!(app.label_input.is_empty());
    }

    #[test]
    fn test_handle_torrent_list_key_confirm_remove_yes_clears_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            ..Default::default()
        }];
        app.rebuild_filter();
        app.modal = Some(Modal::Confirm(Confirm::Remove));
        app.handle_torrent_list_key(make_key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_handle_torrent_list_key_confirm_remove_n_clears_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            ..Default::default()
        }];
        app.rebuild_filter();
        app.modal = Some(Modal::Confirm(Confirm::Remove));
        app.handle_torrent_list_key(make_key(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_handle_filter_input_updates_filter() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![
            Torrent {
                id: 1,
                name: "xenon".into(),
                ..Default::default()
            },
            Torrent {
                id: 2,
                name: "beta".into(),
                ..Default::default()
            },
        ];
        app.rebuild_filter();
        app.modal = Some(Modal::Filter);
        app.handle_torrent_list_key(make_key(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(app.filter_input, "x");
        assert_eq!(app.filtered_torrents().len(), 1);
        app.handle_torrent_list_key(make_key(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(app.filter_input.is_empty());
        assert_eq!(app.filtered_torrents().len(), 2);
        app.handle_torrent_list_key(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_handle_filter_input_esc_closes() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.modal = Some(Modal::Filter);
        app.handle_filter_input(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_clamp_file_cursor_no_torrent() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.file_cursor = 5;
        app.clamp_file_cursor();
        assert_eq!(app.file_cursor, 0);
    }

    #[test]
    fn test_clamp_file_cursor_with_files() {
        use crate::protocol::TorrentFile;
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.detail_torrent = Some(Torrent {
            files: vec![
                TorrentFile {
                    name: "a".into(),
                    ..Default::default()
                },
                TorrentFile {
                    name: "b".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        app.file_cursor = 10;
        app.clamp_file_cursor();
        assert_eq!(app.file_cursor, 1);
    }

    #[test]
    fn test_handle_help_key_close() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.help = Some(5);
        app.handle_help_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.help.is_none());
    }

    #[test]
    fn test_handle_help_key_scroll_and_home() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.help = Some(0);
        app.handle_help_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.help, Some(1));
        app.handle_help_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.help, Some(0));
        app.handle_help_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.help, Some(0));
        app.help = Some(5);
        app.handle_help_key(make_key(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.help, Some(0));
        app.handle_help_key(make_key(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(app.help, Some(10));
        app.handle_help_key(make_key(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.help, Some(0));
    }

    #[test]
    fn test_handle_details_key_back() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Details;
        app.handle_details_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, View::TorrentList);
    }

    #[test]
    fn test_handle_details_key_help() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Details;
        assert!(app.help.is_none());
        app.handle_details_key(make_key(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(app.help.is_some());
    }

    #[test]
    fn test_handle_details_key_enter_goes_to_files() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Details;
        app.handle_details_key(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.view, View::Files);
    }

    #[test]
    fn test_handle_files_key_back() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Files;
        app.file_selected.insert(0);
        app.handle_files_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, View::TorrentList);
        assert!(app.file_selected.is_empty());
    }

    #[test]
    fn test_handle_files_key_navigate() {
        use crate::protocol::TorrentFile;
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Files;
        app.detail_torrent = Some(Torrent {
            files: vec![
                TorrentFile {
                    name: "a".into(),
                    ..Default::default()
                },
                TorrentFile {
                    name: "b".into(),
                    ..Default::default()
                },
                TorrentFile {
                    name: "c".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        assert_eq!(app.file_cursor, 0);
        app.handle_files_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(app.file_cursor, 1);
        app.handle_files_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(app.file_cursor, 0);
        app.handle_files_key(make_key(KeyCode::End, KeyModifiers::NONE));
        assert_eq!(app.file_cursor, 2);
        app.handle_files_key(make_key(KeyCode::Home, KeyModifiers::NONE));
        assert_eq!(app.file_cursor, 0);
        app.handle_files_key(make_key(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(app.file_cursor, 2);
        app.handle_files_key(make_key(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(app.file_cursor, 0);
    }

    #[test]
    fn test_handle_files_key_select_toggle() {
        use crate::protocol::TorrentFile;
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Files;
        app.detail_torrent = Some(Torrent {
            files: vec![TorrentFile {
                name: "a".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(app.file_selected.is_empty());
        app.handle_files_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.file_selected.contains(&0));
        app.handle_files_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.file_selected.is_empty());
    }

    #[test]
    fn test_handle_files_key_delete_opens_confirm() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Files;
        app.detail_torrent = Some(Torrent::default());
        app.handle_files_key(make_key(KeyCode::Char('D'), KeyModifiers::SHIFT));
        assert!(matches!(
            app.modal,
            Some(Modal::Confirm(Confirm::DeleteFileFromDisk))
        ));
    }

    #[test]
    fn test_handle_files_key_confirm_delete_n_cancels() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Files;
        app.modal = Some(Modal::Confirm(Confirm::DeleteFileFromDisk));
        app.handle_files_key(make_key(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_handle_files_key_confirm_delete_y_clears_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy.invalid", None, None),
            Config::default(),
        );
        app.view = View::Files;
        app.modal = Some(Modal::Confirm(Confirm::DeleteFileFromDisk));
        app.handle_files_key(make_key(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_handle_key_help_open_dispatches_to_help_handler() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.help = Some(5);
        app.handle_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.help.is_none());
        assert!(app.running);
    }

    #[test]
    fn test_handle_key_dispatches_to_files_view() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Files;
        app.file_selected.insert(1);
        app.handle_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, View::TorrentList);
        assert!(app.file_selected.is_empty());
    }

    #[test]
    fn test_handle_key_dispatches_to_details_view() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Details;
        app.handle_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, View::TorrentList);
    }

    #[test]
    fn test_handle_add_input_esc_closes_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.modal = Some(Modal::AddUrl("http://test".into()));
        app.handle_add_input(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_handle_add_input_empty_url_clears_modal() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = empty_app();
        app.modal = Some(Modal::AddUrl(String::new()));
        app.handle_add_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_handle_add_input_tab_autocompletes_torrent_path() {
        use crossterm::event::{KeyCode, KeyModifiers};
        // Create a temp dir with a single .torrent file so Tab has a unique completion.
        let dir = tempfile::tempdir().unwrap();
        std::fs::File::create(dir.path().join("debian.torrent")).unwrap();
        let prefix = format!("{}/deb", dir.path().to_str().unwrap());

        let mut app = empty_app();
        app.modal = Some(Modal::AddUrl(prefix));
        app.handle_add_input(make_key(KeyCode::Tab, KeyModifiers::NONE));

        match &app.modal {
            Some(Modal::AddUrl(s)) => {
                assert!(
                    s.ends_with("debian.torrent"),
                    "Tab should complete to the .torrent file, got: {s}"
                );
            }
            _ => panic!("expected AddUrl modal after Tab"),
        }
    }

    #[test]
    fn test_handle_add_input_torrent_file_dispatches_add_metainfo() {
        use crossterm::event::{KeyCode, KeyModifiers};
        // Create an actual (dummy-content) .torrent file that can be read.
        let dir = tempfile::tempdir().unwrap();
        let torrent_path = dir.path().join("test.torrent");
        std::fs::write(&torrent_path, b"d4:infod4:name4:teste").unwrap();
        let torrent_url = torrent_path.to_str().unwrap().to_string();

        let mut app = empty_app();
        // Simulate the state after URL entry: AddLocation with a .torrent URL.
        app.modal = Some(Modal::AddLocation {
            url: torrent_url,
            location: String::new(),
        });
        // Enter dispatches the add — the dummy client will fail at the RPC level,
        // setting last_error and clearing the modal.
        app.handle_add_input(make_key(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.modal.is_none(), "modal cleared after submit");
        // The dummy client can't reach a real server, so an error is expected.
        assert!(app.last_error.is_some(), "error set by failing RPC");
    }

    #[test]
    fn test_handle_add_input_nonexistent_torrent_falls_through_to_add() {
        use crossterm::event::{KeyCode, KeyModifiers};
        // A .torrent path that does not exist on disk — fs::read returns None,
        // so the code falls through to client.add() which also fails for a dummy client.
        let mut app = empty_app();
        app.modal = Some(Modal::AddLocation {
            url: "/nonexistent/path/that/does/not.torrent".to_string(),
            location: String::new(),
        });
        app.handle_add_input(make_key(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.modal.is_none(), "modal cleared");
        assert!(app.last_error.is_some(), "error set by failing add");
    }

    #[test]
    fn test_handle_torrent_list_key_select_down_shift() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![
            Torrent {
                id: 1,
                ..Default::default()
            },
            Torrent {
                id: 2,
                ..Default::default()
            },
            Torrent {
                id: 3,
                ..Default::default()
            },
        ];
        app.rebuild_filter();
        app.handle_torrent_list_key(make_key(KeyCode::Down, KeyModifiers::SHIFT));
        assert!(app.selected.contains(&0));
        assert!(app.selected.contains(&1));
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn test_handle_files_key_help() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.view = View::Files;
        assert!(app.help.is_none());
        app.handle_files_key(make_key(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(app.help.is_some());
    }

    // --- drain_results / trigger_refresh ---

    fn make_app() -> App {
        App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        )
    }

    #[test]
    fn test_drain_torrents_ok_updates_list_and_clears_error() {
        let mut app = make_app();
        app.last_error = Some("stale error".into());

        let torrents = vec![
            Torrent {
                id: 2,
                name: "beta".into(),
                ..Default::default()
            },
            Torrent {
                id: 1,
                name: "alpha".into(),
                ..Default::default()
            },
        ];
        app.refresh_tx
            .send(RefreshMsg::Torrents(Ok(torrents)))
            .unwrap();
        app.drain_results();

        assert_eq!(app.torrents.len(), 2);
        assert!(app.last_error.is_none(), "error must be cleared on success");
    }

    #[test]
    fn test_drain_torrents_err_sets_last_error() {
        let mut app = make_app();
        app.refresh_tx
            .send(RefreshMsg::Torrents(Err("connection refused".into())))
            .unwrap();
        app.drain_results();

        assert_eq!(app.last_error.as_deref(), Some("connection refused"));
        assert!(
            app.torrents.is_empty(),
            "torrent list must stay empty on error"
        );
    }

    #[test]
    fn test_drain_detail_ok_some_updates_detail_torrent() {
        let mut app = make_app();
        let t = Torrent {
            id: 42,
            name: "updated".into(),
            ..Default::default()
        };
        app.refresh_tx
            .send(RefreshMsg::Detail(Box::new(Ok(Some(t)))))
            .unwrap();
        app.drain_results();

        assert_eq!(app.detail_torrent.as_ref().unwrap().id, 42);
        assert_eq!(app.detail_torrent.as_ref().unwrap().name, "updated");
    }

    #[test]
    fn test_drain_detail_ok_none_resets_to_torrent_list() {
        let mut app = make_app();
        app.view = View::Files;
        app.detail_torrent = Some(Torrent {
            id: 7,
            ..Default::default()
        });
        app.file_cursor = 3;
        app.file_selected.insert(1);

        app.refresh_tx
            .send(RefreshMsg::Detail(Box::new(Ok(None))))
            .unwrap();
        app.drain_results();

        assert!(app.detail_torrent.is_none());
        assert_eq!(app.view, View::TorrentList);
    }

    #[test]
    fn test_drain_detail_err_sets_last_error() {
        let mut app = make_app();
        app.refresh_tx
            .send(RefreshMsg::Detail(Box::new(Err("timeout".into()))))
            .unwrap();
        app.drain_results();

        assert_eq!(app.last_error.as_deref(), Some("timeout"));
    }

    #[test]
    fn test_drain_stats_updates_fields_and_clears_in_flight() {
        use crate::protocol::{FreeSpace, SessionStats};

        let mut app = make_app();
        app.refresh_in_flight = true;

        let stats = SessionStats {
            torrent_count: 5,
            download_speed: 100,
            ..Default::default()
        };
        let free = FreeSpace {
            size_bytes: 999,
            ..Default::default()
        };

        app.refresh_tx
            .send(RefreshMsg::Stats {
                stats: Some(stats),
                free: Some(free),
                default_dir: Some("/downloads".into()),
            })
            .unwrap();
        app.drain_results();

        assert!(
            !app.refresh_in_flight,
            "Stats message must clear refresh_in_flight"
        );
        assert_eq!(app.stats.as_ref().unwrap().torrent_count, 5);
        assert_eq!(app.free.as_ref().unwrap().size_bytes, 999);
        assert_eq!(app.default_download_dir.as_deref(), Some("/downloads"));
    }

    #[test]
    fn test_pending_url_save_consumed_on_first_success() {
        let mut app =
            make_app().with_pending_url_save(Some("http://myserver/transmission/rpc".into()));

        app.refresh_tx
            .send(RefreshMsg::Torrents(Ok(vec![])))
            .unwrap();
        app.drain_results();

        assert!(
            app.pending_url_save.is_none(),
            "pending_url_save must be cleared after first success"
        );

        // Second success must not panic (nothing left to save).
        app.refresh_tx
            .send(RefreshMsg::Torrents(Ok(vec![])))
            .unwrap();
        app.drain_results();
    }

    #[test]
    fn test_pending_url_save_not_triggered_on_error() {
        let mut app =
            make_app().with_pending_url_save(Some("http://myserver/transmission/rpc".into()));

        app.refresh_tx
            .send(RefreshMsg::Torrents(Err("refused".into())))
            .unwrap();
        app.drain_results();

        assert!(
            app.pending_url_save.is_some(),
            "pending_url_save must not be consumed on a failed refresh"
        );
    }

    #[test]
    fn test_drain_stats_none_fields_do_not_overwrite() {
        use crate::protocol::{FreeSpace, SessionStats};

        let mut app = make_app();
        app.stats = Some(SessionStats {
            torrent_count: 3,
            ..Default::default()
        });
        app.free = Some(FreeSpace {
            size_bytes: 500,
            ..Default::default()
        });
        app.default_download_dir = Some("/existing".into());

        app.refresh_tx
            .send(RefreshMsg::Stats {
                stats: None,
                free: None,
                default_dir: None,
            })
            .unwrap();
        app.drain_results();

        assert_eq!(
            app.stats.as_ref().unwrap().torrent_count,
            3,
            "stats must not be cleared when None"
        );
        assert_eq!(
            app.free.as_ref().unwrap().size_bytes,
            500,
            "free must not be cleared when None"
        );
        assert_eq!(
            app.default_download_dir.as_deref(),
            Some("/existing"),
            "dir must not be cleared when None"
        );
    }

    #[test]
    fn test_trigger_refresh_guard_noop_when_in_flight() {
        let mut app = make_app();
        assert!(!app.refresh_in_flight);

        app.trigger_refresh();
        assert!(app.refresh_in_flight);
        let tick_after_first = app.free_space_tick;

        app.trigger_refresh();
        assert_eq!(
            app.free_space_tick, tick_after_first,
            "second trigger_refresh must be a no-op: free_space_tick must not advance"
        );
    }

    #[test]
    fn test_drain_multiple_messages_all_applied() {
        use crate::protocol::SessionStats;

        let mut app = make_app();
        app.refresh_in_flight = true;

        let torrents = vec![Torrent {
            id: 1,
            name: "foo".into(),
            ..Default::default()
        }];
        app.refresh_tx
            .send(RefreshMsg::Torrents(Ok(torrents)))
            .unwrap();
        app.refresh_tx
            .send(RefreshMsg::Stats {
                stats: Some(SessionStats {
                    torrent_count: 1,
                    ..Default::default()
                }),
                free: None,
                default_dir: None,
            })
            .unwrap();
        app.drain_results();

        assert_eq!(app.torrents.len(), 1);
        assert_eq!(app.stats.as_ref().unwrap().torrent_count, 1);
        assert!(!app.refresh_in_flight);
    }

    #[test]
    fn test_sequential_predicate_uses_ids_not_positions() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![
            Torrent {
                id: 10,
                sequential_download: true,
                ..Default::default()
            },
            Torrent {
                id: 20,
                sequential_download: false,
                ..Default::default()
            },
        ];
        app.rebuild_filter();
        app.cursor = 0;
        let ids = app.target_ids();
        assert_eq!(ids, vec![10], "target_ids must resolve to id 10");
        let any_sequential = app
            .torrents
            .iter()
            .filter(|t| ids.contains(&t.id))
            .any(|t| t.sequential_download);
        assert!(
            any_sequential,
            "sequential predicate must be true for id=10 (sequential_download=true)"
        );
        app.selected.insert(99);
        let ids_stale = app.target_ids();
        assert!(
            ids_stale.is_empty(),
            "stale out-of-bounds selected index must yield no target ids"
        );
    }

    #[test]
    fn test_set_error_opens_auth_modal_on_401() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.set_error("HTTP 401 Unauthorized");
        assert!(matches!(app.modal, Some(Modal::Auth { .. })));
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_set_error_sets_last_error_for_non_401() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.set_error("connection refused");
        assert!(app.modal.is_none());
        assert_eq!(app.last_error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn test_set_error_does_not_replace_open_auth_modal() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: "alice".into(),
            password: "secret".into(),
            focused: AuthField::Password,
        });
        // A second 401 while the modal is already open should not reset it.
        app.set_error("HTTP 401 Unauthorized");
        assert!(matches!(
            app.modal,
            Some(Modal::Auth { ref username, .. }) if username == "alice"
        ));
    }

    #[test]
    fn test_auth_modal_tab_switches_field() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Username,
        });
        app.handle_auth_input(make_key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(matches!(
            app.modal,
            Some(Modal::Auth {
                focused: AuthField::Password,
                ..
            })
        ));
        app.handle_auth_input(make_key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(matches!(
            app.modal,
            Some(Modal::Auth {
                focused: AuthField::Username,
                ..
            })
        ));
    }

    #[test]
    fn test_auth_modal_char_input_routes_to_focused_field() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Username,
        });
        app.handle_auth_input(make_key(KeyCode::Char('u'), KeyModifiers::NONE));
        app.handle_auth_input(make_key(KeyCode::Tab, KeyModifiers::NONE));
        app.handle_auth_input(make_key(KeyCode::Char('p'), KeyModifiers::NONE));
        match &app.modal {
            Some(Modal::Auth {
                username, password, ..
            }) => {
                assert_eq!(username, "u");
                assert_eq!(password, "p");
            }
            _ => panic!("expected Auth modal"),
        }
    }

    #[test]
    fn test_auth_modal_esc_closes_modal() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: "u".into(),
            password: "p".into(),
            focused: AuthField::Username,
        });
        app.handle_auth_input(make_key(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_drain_results_401_opens_auth_modal() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.refresh_in_flight = true;
        app.refresh_tx
            .send(RefreshMsg::Torrents(Err("HTTP 401 Unauthorized".into())))
            .unwrap();
        app.drain_results();
        assert!(matches!(app.modal, Some(Modal::Auth { .. })));
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_drain_results_detail_401_opens_auth_modal() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.refresh_in_flight = true;
        app.refresh_tx
            .send(RefreshMsg::Detail(Box::new(Err(
                "HTTP 401 Unauthorized".into()
            ))))
            .unwrap();
        app.drain_results();
        assert!(matches!(app.modal, Some(Modal::Auth { .. })));
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_auth_modal_backspace_username() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: "ab".into(),
            password: "xy".into(),
            focused: AuthField::Username,
        });
        app.handle_auth_input(make_key(KeyCode::Backspace, KeyModifiers::NONE));
        match &app.modal {
            Some(Modal::Auth {
                username, password, ..
            }) => {
                assert_eq!(username, "a");
                assert_eq!(password, "xy");
            }
            _ => panic!("expected Auth modal"),
        }
    }

    #[test]
    fn test_auth_modal_backspace_password() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: "ab".into(),
            password: "xy".into(),
            focused: AuthField::Password,
        });
        app.handle_auth_input(make_key(KeyCode::Backspace, KeyModifiers::NONE));
        match &app.modal {
            Some(Modal::Auth {
                username, password, ..
            }) => {
                assert_eq!(username, "ab");
                assert_eq!(password, "x");
            }
            _ => panic!("expected Auth modal"),
        }
    }

    #[test]
    fn test_auth_modal_enter_on_username_advances_to_password() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: "user".into(),
            password: "pass".into(),
            focused: AuthField::Username,
        });
        app.handle_auth_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            app.modal,
            Some(Modal::Auth {
                focused: AuthField::Password,
                ..
            })
        ));
    }

    #[test]
    fn test_auth_modal_enter_on_password_submits() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: "user".into(),
            password: "pass".into(),
            focused: AuthField::Password,
        });
        app.handle_auth_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_auth_modal_down_advances_field() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Username,
        });
        app.handle_auth_input(make_key(KeyCode::Down, KeyModifiers::NONE));
        assert!(matches!(
            app.modal,
            Some(Modal::Auth {
                focused: AuthField::Password,
                ..
            })
        ));
    }

    #[test]
    fn test_auth_modal_down_noop_on_password() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Password,
        });
        app.handle_auth_input(make_key(KeyCode::Down, KeyModifiers::NONE));
        assert!(matches!(
            app.modal,
            Some(Modal::Auth {
                focused: AuthField::Password,
                ..
            })
        ));
    }

    #[test]
    fn test_auth_modal_up_reverses_field() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Password,
        });
        app.handle_auth_input(make_key(KeyCode::Up, KeyModifiers::NONE));
        assert!(matches!(
            app.modal,
            Some(Modal::Auth {
                focused: AuthField::Username,
                ..
            })
        ));
    }

    #[test]
    fn test_auth_modal_up_noop_on_username() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Username,
        });
        app.handle_auth_input(make_key(KeyCode::Up, KeyModifiers::NONE));
        assert!(matches!(
            app.modal,
            Some(Modal::Auth {
                focused: AuthField::Username,
                ..
            })
        ));
    }

    #[test]
    fn test_set_error_sets_error_since() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        assert!(app.error_since.is_none());
        app.set_error("something broke");
        assert!(app.error_since.is_some());
        assert_eq!(app.last_error.as_deref(), Some("something broke"));
    }

    #[test]
    fn test_set_error_401_does_not_set_error_since() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.set_error("HTTP 401 Unauthorized");
        assert!(app.error_since.is_none());
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_tick_autoclear_clears_after_expiry() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.last_error = Some("stale error".into());
        // Simulate an error that occurred 11 seconds ago.
        app.error_since = Some(Instant::now() - Duration::from_secs(11));
        app.tick_autoclear();
        assert!(app.last_error.is_none());
        assert!(app.error_since.is_none());
    }

    #[test]
    fn test_tick_autoclear_does_not_clear_recent_error() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.last_error = Some("fresh error".into());
        app.error_since = Some(Instant::now());
        app.tick_autoclear();
        assert_eq!(app.last_error.as_deref(), Some("fresh error"));
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_tick_autoclear_noop_when_no_error() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.tick_autoclear();
        assert!(app.last_error.is_none());
        assert!(app.error_since.is_none());
    }

    #[test]
    fn test_drain_results_success_clears_error_since() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.last_error = Some("prior error".into());
        app.error_since = Some(Instant::now());
        app.refresh_in_flight = true;
        app.refresh_tx
            .send(RefreshMsg::Torrents(Ok(vec![])))
            .unwrap();
        app.drain_results();
        assert!(app.last_error.is_none());
        assert!(app.error_since.is_none());
    }

    fn torrent_in_list(app: &mut App) {
        app.torrents = vec![Torrent {
            id: 1,
            ..Default::default()
        }];
        app.rebuild_filter();
    }

    #[test]
    fn test_handle_torrent_list_key_enter_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        torrent_in_list(&mut app);
        app.handle_torrent_list_key(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_torrent_list_key_details_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        torrent_in_list(&mut app);
        app.handle_torrent_list_key(make_key(KeyCode::Tab, KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_torrent_list_key_pause_stopped_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            status: 0, // stopped
            ..Default::default()
        }];
        app.rebuild_filter();
        app.handle_torrent_list_key(make_key(KeyCode::Char('p'), KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_torrent_list_key_pause_running_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.torrents = vec![Torrent {
            id: 1,
            status: 4, // seeding
            ..Default::default()
        }];
        app.rebuild_filter();
        app.handle_torrent_list_key(make_key(KeyCode::Char('p'), KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_torrent_list_key_reannounce_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        torrent_in_list(&mut app);
        app.handle_torrent_list_key(make_key(KeyCode::Char('t'), KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_torrent_list_key_verify_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        torrent_in_list(&mut app);
        app.handle_torrent_list_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_label_input_sets_error_on_dummy() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        torrent_in_list(&mut app);
        app.label_input = "tag".into();
        app.handle_label_input();
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_details_key_reannounce_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.detail_torrent = Some(Torrent {
            id: 1,
            ..Default::default()
        });
        app.view = View::Details;
        app.handle_details_key(make_key(KeyCode::Char('t'), KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_adjust_file_priority_sets_error_on_dummy() {
        use crossterm::event::{KeyCode, KeyModifiers};
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.detail_torrent = Some(Torrent {
            id: 1,
            file_stats: vec![FileStats {
                wanted: true,
                priority: 0,
                bytes_completed: 0,
            }],
            ..Default::default()
        });
        app.view = View::Files;
        app.handle_files_key(make_key(KeyCode::Char('+'), KeyModifiers::NONE));
        assert!(app.last_error.is_some());
        assert!(app.error_since.is_some());
    }

    #[test]
    fn test_handle_auth_enter_noop_when_no_modal() {
        let mut app = make_app();
        app.handle_auth_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.modal.is_none());
    }

    #[test]
    fn test_persist_credentials_impl_config_fallback_on_save_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("config.toml");
        persist_credentials_impl(
            "http://test.invalid/rpc",
            "alice",
            "secret",
            &cfg_path,
            |_, _, _| Err("simulated keyring failure".into()),
        );
        let cfg = crate::config::Config::load_from(&cfg_path);
        assert_eq!(cfg.connection.username.as_deref(), Some("alice"));
        assert_eq!(cfg.connection.password.as_deref(), Some("secret"));
    }

    #[test]
    fn test_persist_credentials_impl_no_config_write_on_save_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("config.toml");
        persist_credentials_impl(
            "http://test.invalid/rpc",
            "bob",
            "hunter2",
            &cfg_path,
            |_, _, _| Ok(()),
        );
        assert!(
            !cfg_path.exists(),
            "config must not be created when keyring save succeeds"
        );
    }

    #[test]
    fn test_handle_tick_triggers_refresh_without_modal() {
        let mut app = make_app();
        assert!(!app.refresh_in_flight);
        app.handle_tick();
        assert!(
            app.refresh_in_flight,
            "handle_tick must trigger a refresh when idle"
        );
    }

    #[test]
    fn test_handle_tick_skips_refresh_during_auth_modal() {
        let mut app = make_app();
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Username,
        });
        app.handle_tick();
        assert!(
            !app.refresh_in_flight,
            "handle_tick must not refresh while auth modal is open"
        );
    }

    #[test]
    fn test_handle_tick_skips_refresh_with_help_open() {
        let mut app = make_app();
        app.help = Some(0);
        app.handle_tick();
        assert!(
            !app.refresh_in_flight,
            "handle_tick must not refresh while help is open"
        );
    }

    // -------------------------------------------------------------------------
    // location_parent_dir
    // -------------------------------------------------------------------------

    #[test]
    fn test_location_parent_dir_trailing_slash() {
        assert_eq!(util::location_parent_dir("/foo/bar/"), "/foo/bar/");
    }

    #[test]
    fn test_location_parent_dir_empty() {
        assert_eq!(util::location_parent_dir(""), "");
    }

    #[test]
    fn test_location_parent_dir_nested() {
        assert_eq!(util::location_parent_dir("/foo/bar"), "/foo/");
    }

    #[test]
    fn test_location_parent_dir_top_level() {
        // "/foo" → parent is "/" → returns "/" (not "//")
        assert_eq!(util::location_parent_dir("/foo"), "/");
    }

    // -------------------------------------------------------------------------
    // ssh_host
    // -------------------------------------------------------------------------

    #[test]
    fn test_ssh_host_remote_hostname() {
        let app = App::new(
            TransmissionClient::new(
                "http://myserver.example.com:9091/transmission/rpc",
                None,
                None,
            ),
            Config::default(),
        );
        assert_eq!(app.ssh_host(), Some("myserver.example.com".to_string()));
    }

    #[test]
    fn test_ssh_host_localhost_returns_none() {
        for url in &[
            "http://localhost:9091/transmission/rpc",
            "http://127.0.0.1:9091/transmission/rpc",
            "http://[::1]:9091/transmission/rpc",
        ] {
            let app = App::new(TransmissionClient::new(url, None, None), Config::default());
            assert_eq!(app.ssh_host(), None, "expected None for {url}");
        }
    }

    #[test]
    fn test_ssh_host_flag_shaped_rejected() {
        let app = App::new(
            TransmissionClient::new("http://-oProxyCommand=evil:9091/rpc", None, None),
            Config::default(),
        );
        assert_eq!(
            app.ssh_host(),
            None,
            "flag-shaped hostname must be rejected"
        );
    }

    #[test]
    fn test_ssh_host_https_scheme() {
        let app = App::new(
            TransmissionClient::new("https://remote.host/transmission/rpc", None, None),
            Config::default(),
        );
        assert_eq!(app.ssh_host(), Some("remote.host".to_string()));
    }

    // -------------------------------------------------------------------------
    // complete_location
    // -------------------------------------------------------------------------

    #[test]
    fn test_tab_completes_change_location_modal() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("completed_dir")).unwrap();

        // Use localhost so complete_location falls back to local filesystem.
        let mut app = App::new(
            TransmissionClient::new("http://localhost:9091/rpc", None, None),
            Config::default(),
        );
        let prefix = format!("{}/comp", dir.path().to_str().unwrap());
        app.modal = Some(Modal::ChangeLocation(prefix));

        app.handle_add_input(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });

        match &app.modal {
            Some(Modal::ChangeLocation(s)) => {
                assert!(
                    s.contains("completed_dir"),
                    "Tab should complete to the directory"
                );
            }
            _ => panic!("expected ChangeLocation modal"),
        }
    }

    #[test]
    fn test_tab_completes_add_location_modal() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("download_dir")).unwrap();

        let mut app = App::new(
            TransmissionClient::new("http://localhost:9091/rpc", None, None),
            Config::default(),
        );
        let prefix = format!("{}/down", dir.path().to_str().unwrap());
        app.modal = Some(Modal::AddLocation {
            url: "magnet:?xt=test".to_string(),
            location: prefix,
        });

        app.handle_add_input(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });

        match &app.modal {
            Some(Modal::AddLocation { location, .. }) => {
                assert!(
                    location.contains("download_dir"),
                    "Tab should complete to the directory"
                );
            }
            _ => panic!("expected AddLocation modal"),
        }
    }

    #[test]
    fn test_complete_location_local_uses_filesystem() {
        // localhost → ssh_host() returns None → falls back to local autocomplete.
        let mut app = App::new(
            TransmissionClient::new("http://localhost:9091/rpc", None, None),
            Config::default(),
        );
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("only_match")).unwrap();
        let input = format!("{}/only", dir.path().to_str().unwrap());
        let result = app.complete_location(&input);
        assert!(result.is_some());
        assert!(result.unwrap().contains("only_match"));
    }

    #[test]
    fn test_complete_location_remote_cache_hit() {
        // Remote app with a pre-populated cache → returns from cache without SSH.
        let mut app = App::new(
            TransmissionClient::new("http://remotehost:9091/rpc", None, None),
            Config::default(),
        );
        app.location_dir_cache = Some((
            "/srv/".to_string(),
            vec!["/srv/downloads".to_string(), "/srv/media".to_string()],
        ));
        let result = app.complete_location("/srv/d");
        assert_eq!(result, Some("/srv/downloads/".to_string()));
    }

    #[test]
    fn test_complete_location_remote_error_cache_prevents_retry() {
        // When SSH previously failed, location_dir_cache is set to Some((dir, [])).
        // A second complete_location call for the same dir must use the cache
        // (no SSH) and return None (no completions available).
        let mut app = App::new(
            TransmissionClient::new("http://remotehost:9091/rpc", None, None),
            Config::default(),
        );
        // Pre-populate the cache as if a prior SSH call already failed.
        app.location_dir_cache = Some(("/srv/".to_string(), vec![]));

        let result = app.complete_location("/srv/data");
        // Cache hit: empty listing → no completions, no new SSH call.
        assert_eq!(result, None);
        // Cache must remain intact (not overwritten by a retry).
        assert_eq!(app.location_dir_cache, Some(("/srv/".to_string(), vec![])));
    }

    #[test]
    fn test_complete_location_remote_ssh_error_sets_last_error() {
        // Install a fake `ssh` that immediately exits non-zero so this test
        // does not require network access.
        use std::io::Write;
        let fake_dir = tempfile::tempdir().unwrap();
        let fake_ssh = fake_dir.path().join("ssh");
        {
            let mut f = std::fs::File::create(&fake_ssh).unwrap();
            writeln!(f, "#!/bin/sh").unwrap();
            writeln!(f, "echo 'Permission denied (publickey).' >&2").unwrap();
            writeln!(f, "exit 255").unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let original_path = std::env::var("PATH").unwrap_or_default();
        // SAFETY: single-threaded test; no other threads read PATH concurrently.
        unsafe {
            std::env::set_var(
                "PATH",
                format!("{}:{}", fake_dir.path().display(), original_path),
            );
        }

        let mut app = App::new(
            TransmissionClient::new("http://remotehost:9091/rpc", None, None),
            Config::default(),
        );
        let _result = app.complete_location("/srv/");

        // SAFETY: restoring the value we read above.
        unsafe { std::env::set_var("PATH", original_path) };

        assert!(
            app.last_error.is_some(),
            "SSH failure should set last_error"
        );
        assert!(
            app.error_since.is_some(),
            "SSH failure should set error_since"
        );
        // Cache populated with empty listing so repeated presses don't retry.
        assert_eq!(app.location_dir_cache, Some(("/srv/".to_string(), vec![])));
    }

    #[test]
    fn test_location_parent_dir_relative_no_slash() {
        // A path with no directory component — parent is "" which is resolved to "/".
        assert_eq!(util::location_parent_dir("foo"), "/");
    }

    #[test]
    fn test_complete_location_remote_cache_miss_then_hit() {
        // After a cache-miss SSH call populates the cache, a second call with the
        // same parent dir uses the cache and does not re-invoke SSH.
        let mut app = App::new(
            TransmissionClient::new("http://remotehost:9091/rpc", None, None),
            Config::default(),
        );
        app.location_dir_cache = Some((
            "/data/".to_string(),
            vec!["/data/archive".to_string(), "/data/active".to_string()],
        ));
        // Both completions should come from the cache (same parent "/data/").
        let r1 = app.complete_location("/data/ar");
        let r2 = app.complete_location("/data/ac");
        assert_eq!(r1, Some("/data/archive/".to_string()));
        assert_eq!(r2, Some("/data/active/".to_string()));
    }
}
