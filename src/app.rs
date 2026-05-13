use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::client::TransmissionClient;
use crate::config::{Bindings, Config, ThemeConfig};
use crate::protocol::*;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    TorrentList,
    Files,
    Details,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    Remove,
    DeleteFiles,
    DeleteFileFromDisk,
}

pub enum Modal {
    Add(String),
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
            modal: None,
            filter_input: String::new(),
            filtered_indices: Vec::new(),
            detail_torrent: None,
            file_cursor: 0,
            file_selected: BTreeSet::new(),
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
        if self.free_space_tick % 5 == 1 {
            if let Some(dir) = &self.default_download_dir
                && let Ok(f) = self.client.free_space(dir)
            {
                self.free = Some(f);
            }
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

    fn move_up(cursor: &mut usize, selected: &mut BTreeSet<usize>, selecting: bool) {
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
            Some(Modal::Add(_)) => {
                self.handle_add_input(key);
                return;
            }
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
            Self::move_up(&mut self.cursor, &mut self.selected, is_select_up);
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
                let visible = self.filtered_torrents();
                let any_stopped = self
                    .selected
                    .iter()
                    .filter_map(|&i| visible.get(i))
                    .any(|t| t.is_stopped())
                    || (self.selected.is_empty()
                        && visible.get(self.cursor).is_some_and(|t| t.is_stopped()));
                let result = if any_stopped {
                    self.client.start(&ids)
                } else {
                    self.client.stop(&ids)
                };
                if let Err(e) = result {
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
            self.modal = Some(Modal::Add(String::new()));
        } else if b.reannounce.matches(code, mods) {
            let ids = self.target_ids();
            if let Err(e) = self.client.reannounce(&ids) {
                self.last_error = Some(e);
            }
            self.selected.clear();
        } else if b.verify.matches(code, mods) {
            let ids = self.target_ids();
            if let Err(e) = self.client.verify(&ids) {
                self.last_error = Some(e);
            }
            self.selected.clear();
        } else if b.queue_up.matches(code, mods) {
            let ids = self.target_ids();
            if let Err(e) = self.client.queue_move("queue-move-up", &ids) {
                self.last_error = Some(e);
            }
        } else if b.queue_down.matches(code, mods) {
            let ids = self.target_ids();
            if let Err(e) = self.client.queue_move("queue-move-down", &ids) {
                self.last_error = Some(e);
            }
        } else if b.filter.matches(code, mods) {
            self.modal = Some(Modal::Filter);
            self.filter_input.clear();
            self.rebuild_filter();
        } else if b.sort.matches(code, mods) {
            self.sort_column = self.sort_column.next();
        } else if b.sort_reverse.matches(code, mods) {
            self.sort_ascending = !self.sort_ascending;
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
            KeyCode::Enter => {
                let loc = if let Some(Modal::Add(ref s)) = self.modal {
                    s.trim().to_string()
                } else {
                    return;
                };
                self.modal = None;
                if !loc.is_empty() {
                    if let Err(e) = self.client.add(&loc) {
                        self.last_error = Some(e);
                    }
                }
            }
            KeyCode::Esc => {
                self.modal = None;
            }
            KeyCode::Backspace => {
                if let Some(Modal::Add(ref mut s)) = self.modal {
                    s.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(Modal::Add(ref mut s)) = self.modal {
                    s.push(c);
                }
            }
            _ => {}
        }
    }

    fn handle_files_key(&mut self, key: KeyEvent) {
        if matches!(self.modal, Some(Modal::Confirm(Confirm::DeleteFileFromDisk))) {
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
            Self::move_up(&mut self.file_cursor, &mut self.file_selected, is_select_up);
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

    fn delete_files_from_disk(&mut self) {
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
        }
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> std::io::Result<()> {
        self.refresh_torrents();
        self.refresh_stats();

        let tick_rate = Duration::from_secs(1);
        let mut last_tick = Instant::now();

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
            TransmissionClient::new("http://dummy", None),
            Config::default(),
        );

        let mut t1 = Torrent::default();
        t1.name = "ubuntu.iso".into();
        t1.status = 4; // Downloading
        t1.tracker_stats.push(TrackerStats {
            host: "tracker.ubuntu.com".into(),
            ..Default::default()
        });

        let mut t2 = Torrent::default();
        t2.name = "debian.iso".into();
        t2.status = 6; // Seeding
        t2.tracker_stats.push(TrackerStats {
            host: "tracker.debian.org".into(),
            ..Default::default()
        });

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

        App::move_up(&mut cursor, &mut selected, false);
        assert_eq!(cursor, 1);
        assert_eq!(selected.len(), 2);

        App::move_up(&mut cursor, &mut selected, true);
        assert_eq!(cursor, 0);
        assert!(selected.contains(&0));
        assert!(selected.contains(&1));
    }

    #[test]
    fn test_sorting() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None),
            Config::default(),
        );

        let mut t1 = Torrent::default();
        t1.name = "B".into();
        t1.total_size = 100;
        t1.percent_done = 0.5;
        t1.rate_download = 50;
        t1.eta = 10;
        t1.queue_position = 2;
        t1.status = 1;

        let mut t2 = Torrent::default();
        t2.name = "A".into();
        t2.total_size = 200;
        t2.percent_done = 0.8;
        t2.rate_download = 20;
        t2.eta = 20;
        t2.queue_position = 1;
        t2.status = 2;

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
            TransmissionClient::new("http://dummy", None),
            Config::default(),
        );
        let mut t1 = Torrent::default();
        t1.id = 1;
        let mut t2 = Torrent::default();
        t2.id = 2;
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
            TransmissionClient::new("http://dummy", None),
            Config::default(),
        );
        let mut t1 = Torrent::default();
        t1.id = 10;
        let mut t2 = Torrent::default();
        t2.id = 20;
        let mut t3 = Torrent::default();
        t3.id = 30;
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
            TransmissionClient::new("http://dummy", None),
            Config::default(),
        );
        // Empty list: cursor stays 0
        app.cursor = 5;
        app.clamp_cursor();
        assert_eq!(app.cursor, 0);

        let mut t1 = Torrent::default();
        t1.id = 1;
        let mut t2 = Torrent::default();
        t2.id = 2;
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
            TransmissionClient::new("http://dummy", None),
            Config::default(),
        );
        app.file_cursor = 3;
        assert_eq!(app.file_target_indices(), vec![3]);
    }

    #[test]
    fn test_file_target_indices_selected() {
        let mut app = App::new(
            TransmissionClient::new("http://dummy", None),
            Config::default(),
        );
        app.file_selected.insert(1);
        app.file_selected.insert(4);
        let mut idxs = app.file_target_indices();
        idxs.sort();
        assert_eq!(idxs, vec![1, 4]);
    }
}
