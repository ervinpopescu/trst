use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(crate) fn handle_tick(&mut self) {
        if self.help.is_none() && !matches!(self.modal, Some(Modal::Auth { .. })) {
            self.trigger_refresh();
            #[cfg(feature = "rsync")]
            if self.view == View::Rsync {
                self.refresh_rsync();
            }
        }
        self.tick_autoclear();
    }

    #[cfg(feature = "rsync")]
    pub(crate) fn handle_rsync_key(&mut self, key: KeyEvent) {
        let (code, mods) = (key.code, key.modifiers);
        let b = &self.bindings;
        if b.back.matches(code, mods) || b.quit.matches(code, mods) || code == KeyCode::Esc {
            self.view = View::TorrentList;
        } else if code == KeyCode::Char('R') && mods == KeyModifiers::SHIFT {
            self.refresh_rsync();
        }
    }

    pub(crate) fn handle_torrent_list_key(&mut self, key: KeyEvent) {
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

    pub(crate) fn handle_filter_input(&mut self, key: KeyEvent) {
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

    pub(crate) fn handle_add_input(&mut self, key: KeyEvent) {
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

    pub(crate) fn handle_auth_input(&mut self, key: KeyEvent) {
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

    pub(crate) fn handle_label_input(&mut self) {
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

    pub(crate) fn handle_files_key(&mut self, key: KeyEvent) {
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

    pub(crate) fn handle_details_key(&mut self, key: KeyEvent) {
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

    pub(crate) fn handle_help_key(&mut self, key: KeyEvent) {
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

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
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
}

#[cfg(test)]
mod tests;
