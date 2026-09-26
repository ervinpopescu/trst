use super::*;
use crate::app::{AppNavigation, AppRefresh};
use crate::protocol::FilePriority;
use crate::util;
use std::time::{Duration, Instant};

/// Trait defining user actions, location completion, and file mutations on the application state.
pub trait AppActions {
    /// Tab-completes a location input, using SSH for remote daemons and the local filesystem for local ones.
    /// Results are cached by parent directory so that repeated Tab presses in the same directory don't re-run SSH.
    ///
    /// In both cases, known torrent download directories are used as a fallback
    /// when neither SSH nor the local filesystem produces a completion.
    fn complete_location(&mut self, input: &str) -> Option<String>;

    /// Clears any visible error message if the 10-second error display TTL has expired.
    fn tick_autoclear(&mut self);

    /// Sets the active error message and records the timestamp for auto-clearing.
    /// Opens the auth modal for `HTTP 401 Unauthorized` and discards pending
    /// credentials for HTTP 401 or 403 errors.
    fn set_error(&mut self, e: impl Into<String>);

    /// Adjusts priority (increase or decrease) for the selected or highlighted files in the active detail torrent.
    fn adjust_file_priority(&mut self, increase: bool);

    /// Toggles the wanted/unwanted download state for the selected or highlighted files in the active detail torrent.
    fn toggle_file_wanted(&mut self);

    /// Deletes selected or highlighted files directly from disk for local daemons.
    /// On Unix, target ancestors below the canonical download directory are resolved
    /// without following symlinks; this operation is unavailable on other platforms.
    fn delete_files_from_disk(&mut self);
}

impl AppActions for App {
    /// Tab-completes a location input, using SSH for remote daemons and the local
    /// filesystem for local ones.  Results are cached by parent directory so that
    /// repeated Tab presses in the same directory don't re-run SSH.
    ///
    /// In both cases, known torrent download directories are used as a fallback
    /// when neither SSH nor the local filesystem produces a completion - this
    /// ensures Tab is useful even with an empty input or after an SSH failure.
    fn complete_location(&mut self, input: &str) -> Option<String> {
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
                    match (self.remote_dir_lister)(&host, &dir) {
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
                // candidates at all - not merely when they share no common prefix.
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

    fn set_error(&mut self, e: impl Into<String>) {
        let e = e.into();
        if e.starts_with("HTTP 401") || e.starts_with("HTTP 403") {
            self.pending_credentials_save = None;
        }
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
                if let Err(e) = remove_torrent_file_without_symlink_ancestors(dir, &file.name) {
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

fn remove_torrent_file_without_symlink_ancestors(
    download_dir: &str,
    file_name: &str,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        remove_torrent_file_unix(download_dir, file_name)
    }
    #[cfg(not(unix))]
    {
        let _ = (download_dir, file_name);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "race-resistant disk deletion is unavailable on this platform",
        ))
    }
}

#[cfg(unix)]
fn canonical_root_components(
    canonical_root: &std::path::Path,
) -> std::io::Result<Vec<&std::ffi::OsStr>> {
    use std::path::Component;

    canonical_root
        .components()
        .filter_map(|part| match part {
            Component::RootDir => None,
            Component::Normal(name) => Some(Ok(name)),
            _ => Some(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "download directory contains unsafe components",
            ))),
        })
        .collect()
}

#[cfg(unix)]
fn remove_torrent_file_unix(download_dir: &str, file_name: &str) -> std::io::Result<()> {
    remove_torrent_file_unix_with_root_opener(download_dir, file_name, open_root_directory)
}

#[cfg(unix)]
fn open_root_directory() -> std::io::Result<std::os::fd::OwnedFd> {
    use std::ffi::CString;
    use std::os::fd::{FromRawFd, OwnedFd};

    let slash = CString::new("/").expect("static path has no NUL");
    // SAFETY: slash is NUL-terminated and flags request a read-only directory handle.
    let root_fd = unsafe {
        libc::open(
            slash.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: open returned a new owned descriptor.
    Ok(unsafe { OwnedFd::from_raw_fd(root_fd) })
}

#[cfg(unix)]
fn remove_torrent_file_unix_with_root_opener(
    download_dir: &str,
    file_name: &str,
    open_root: impl FnOnce() -> std::io::Result<std::os::fd::OwnedFd>,
) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Component, Path};

    fn c_path(path: &std::ffi::OsStr) -> std::io::Result<CString> {
        CString::new(path.as_bytes()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL byte")
        })
    }

    fn open_child_directory(parent: &OwnedFd, name: &std::ffi::OsStr) -> std::io::Result<OwnedFd> {
        let name = c_path(name)?;
        // SAFETY: parent is an open directory descriptor and name is NUL-terminated.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: openat returned a new owned descriptor.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    let root = Path::new(download_dir);
    if !root.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "download directory must be absolute",
        ));
    }
    let canonical_root = root.canonicalize()?;
    let root_components = canonical_root_components(&canonical_root)?;
    let relative = Path::new(file_name);
    let components: Vec<_> = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => Ok(name),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "unsafe relative path component",
            )),
        })
        .collect::<std::io::Result<_>>()?;
    let (leaf, parents) = components.split_last().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty relative path")
    })?;

    // Start at / and resolve every root/target ancestor relative to an already-open
    // directory handle. O_NOFOLLOW prevents a concurrent symlink swap from redirecting
    // traversal; unlinkat removes from the held parent itself rather than resolving a path.
    let mut parent = open_root()?;
    for component in root_components.into_iter().chain(parents.iter().copied()) {
        parent = open_child_directory(&parent, component)?;
    }

    let leaf = c_path(leaf)?;
    // unlinkat with flags=0 removes a file or a final-component symlink without
    // following it, and fails for directories just like the former remove_file path.
    // SAFETY: parent is an open directory descriptor and leaf is NUL-terminated.
    let result = unsafe { libc::unlinkat(parent.as_raw_fd(), leaf.as_ptr(), 0) };
    if result < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
