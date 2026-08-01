use super::*;
use crate::protocol::FilePriority;
use crate::util;
use std::time::{Duration, Instant};

impl App {
    /// Tab-completes a location input, using SSH for remote daemons and the local
    /// filesystem for local ones.  Results are cached by parent directory so that
    /// repeated Tab presses in the same directory don't re-run SSH.
    ///
    /// In both cases, known torrent download directories are used as a fallback
    /// when neither SSH nor the local filesystem produces a completion — this
    /// ensures Tab is useful even with an empty input or after an SSH failure.
    pub(crate) fn complete_location(&mut self, input: &str) -> Option<String> {
        // Pre-collect known torrent dirs for the fallback; used in both branches.
        let known_dirs: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            self.torrents
                .iter()
                .filter(|t| !t.download_dir.is_empty())
                .filter(|t| seen.insert(t.download_dir.clone()))
                .map(|t| t.download_dir.clone())
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

    pub(crate) fn tick_autoclear(&mut self) {
        if self
            .error_since
            .map(|t| t.elapsed() >= Duration::from_secs(10))
            .unwrap_or(false)
        {
            self.last_error = None;
            self.error_since = None;
        }
    }

    pub(crate) fn set_error(&mut self, e: impl Into<String>) {
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

    pub(crate) fn adjust_file_priority(&mut self, increase: bool) {
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

    pub(crate) fn toggle_file_wanted(&mut self) {
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

    pub(crate) fn delete_files_from_disk(&mut self) {
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
}

#[cfg(test)]
mod tests;
