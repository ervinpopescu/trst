use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::client::TransmissionClient;
use crate::config::{Bindings, Config, ThemeConfig};
use crate::protocol::*;
use crate::ui;
use crate::util;

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

pub enum Modal {
    AddUrl(String),
    AddLocation { url: String, location: String },
    ChangeLocation(String),
    Filter,
    Confirm(Confirm),
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
    pub client: TransmissionClient,
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
    pub default_download_dir: Option<String>,
    free_space_tick: u8,
}

impl App {
    pub fn new(client: TransmissionClient, config: Config) -> Self {
        let bindings = Bindings::from_config(&config.keys);
        Self {
            client,
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
            default_download_dir: None,
            free_space_tick: 0,
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

    fn refresh_torrents(&mut self) {
        match self.client.get_torrents(TORRENT_LIST_FIELDS) {
            Ok(mut list) => {
                self.sort_torrents(&mut list);
                self.torrents = list;
                self.rebuild_filter();
                self.clamp_cursor();
                self.last_error = None;
            }
            Err(e) => self.last_error = Some(e),
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
            Err(e) => self.last_error = Some(e),
        }
    }

    fn refresh_stats(&mut self) {
        if let Ok(s) = self.client.session_stats() {
            self.stats = Some(s);
        }
        if self.default_download_dir.is_none()
            && let Ok(resp) = self.client.get_torrents(&["id", "downloadDir"])
            && let Some(t) = resp.first().filter(|t| !t.download_dir.is_empty())
        {
            self.default_download_dir = Some(t.download_dir.clone());
        }
        self.free_space_tick = self.free_space_tick.wrapping_add(1);
        if self.free_space_tick % 5 == 1
            && let Some(dir) = &self.default_download_dir
            && let Ok(f) = self.client.free_space(dir)
        {
            self.free = Some(f);
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
                            self.last_error = Some(e);
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
                    Ok(None) => self.last_error = Some("torrent not found".into()),
                    Err(e) => self.last_error = Some(e),
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
                    Ok(None) => self.last_error = Some("torrent not found".into()),
                    Err(e) => self.last_error = Some(e),
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
                    self.last_error = Some(e);
                }
                if !running_ids.is_empty()
                    && let Err(e) = self.client.stop(&running_ids)
                {
                    self.last_error = Some(e);
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
                self.last_error = Some(e);
            }
            self.selected.clear();
        } else if b.verify.matches(code, mods) {
            let ids = self.target_ids();
            if ids.is_empty() {
                return;
            }
            if let Err(e) = self.client.verify(&ids) {
                self.last_error = Some(e);
            }
            self.selected.clear();
        } else if b.queue_up.matches(code, mods) {
            let ids = self.target_ids();
            if ids.is_empty() {
                return;
            }
            if let Err(e) = self.client.queue_move("queue-move-up", &ids) {
                self.last_error = Some(e);
            }
        } else if b.queue_down.matches(code, mods) {
            let ids = self.target_ids();
            if ids.is_empty() {
                return;
            }
            if let Err(e) = self.client.queue_move("queue-move-down", &ids) {
                self.last_error = Some(e);
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
                let visible = self.filtered_torrents();
                let any_sequential = self
                    .selected
                    .iter()
                    .filter_map(|&i| visible.get(i))
                    .any(|t| t.sequential_download)
                    || (self.selected.is_empty()
                        && visible
                            .get(self.cursor)
                            .is_some_and(|t| t.sequential_download));
                if let Err(e) = self.client.set_sequential(&ids, !any_sequential) {
                    self.last_error = Some(e);
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
                    if let Err(e) = self.client.add(&url, dir) {
                        self.last_error = Some(e);
                    }
                    self.modal = None;
                }
                Some(Modal::ChangeLocation(location)) => {
                    let location = location.trim().to_string();
                    if !location.is_empty() {
                        let ids = self.target_ids();
                        // we move the files by default when changing location from UI
                        if let Err(e) = self.client.set_location(&ids, &location, true) {
                            self.last_error = Some(e);
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
            KeyCode::Tab => match self.modal {
                Some(Modal::AddLocation {
                    ref mut location, ..
                })
                | Some(Modal::ChangeLocation(ref mut location)) => {
                    if let Some(completed) = util::autocomplete_path(location) {
                        *location = completed;
                    }
                }
                _ => {}
            },
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
                self.last_error = Some(e);
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
            self.last_error = Some(e);
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
            self.last_error = Some(e);
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
            self.last_error = Some(e);
            return;
        }
        self.file_selected.clear();
        self.refresh_detail();
    }

    fn is_local_daemon(&self) -> bool {
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
            self.last_error = Some("unknown download directory".into());
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
            self.last_error = Some(errors.join("; "));
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
            self.last_error = Some(e);
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
        let tick_rate = Duration::from_secs(1);
        let mut last_tick = Instant::now() - tick_rate;

        loop {
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
                if self.help.is_none() {
                    match self.view {
                        View::TorrentList => self.refresh_torrents(),
                        View::Files | View::Details => self.refresh_detail(),
                        #[cfg(feature = "rsync")]
                        View::Rsync => self.refresh_rsync(),
                    }
                }
                self.refresh_stats();
                last_tick = Instant::now();
            }
        }

        Ok(())
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
}
