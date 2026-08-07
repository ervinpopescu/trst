use super::*;

/// Trait for background data fetching and state synchronization with the Transmission daemon.
pub trait AppRefresh {
    /// Synchronously fetches and updates the list of torrents.
    fn refresh_torrents(&mut self);

    /// Synchronously fetches and updates details for the currently active detail torrent.
    fn refresh_detail(&mut self);

    /// Spawns a background thread to fetch torrent list, detail, or stats for the current tick.
    fn trigger_refresh(&mut self);

    /// Drains and applies pending background messages from the refresh channel.
    fn drain_results(&mut self);

    /// Reloads rsync state files when the rsync view is active.
    #[cfg(feature = "rsync")]
    fn refresh_rsync(&mut self);
}

impl AppRefresh for App {
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
}

#[cfg(test)]
mod tests;
