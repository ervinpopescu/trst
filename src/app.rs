pub mod actions;
pub mod filter;
pub mod handlers;
pub mod navigation;
pub mod refresh;

use std::collections::BTreeSet;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
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
    pub fn ssh_host(&self) -> Option<String> {
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
mod tests;
