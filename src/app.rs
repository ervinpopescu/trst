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

type RemoteDirLister = fn(&str, &str) -> Result<Vec<String>, String>;

/// Message types sent from background worker threads to the main UI thread.
pub enum RefreshMsg {
    /// Result of an event automation torrent snapshot poll.
    EventSnapshot(Result<Vec<Torrent>, String>),
    /// Notification that an event rule action execution finished.
    ActionComplete {
        torrent_id: i64,
        #[allow(dead_code)]
        kind: crate::events::LifecycleEventKind,
        result: Result<crate::events::ActionProgress, crate::events::ActionFailure>,
    },
    /// Periodic torrent list update from Transmission daemon.
    Torrents(Result<Vec<Torrent>, String>),
    /// Periodic detail update for the active detail torrent.
    Detail(Box<Result<Option<Torrent>, String>>),
    /// Session stats, free disk space, and default download directory update.
    Stats {
        stats: Option<SessionStats>,
        free: Option<FreeSpace>,
        default_dir: Option<String>,
    },
}

/// TUI view modes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    /// Main list displaying all torrents.
    TorrentList,
    /// Detailed file listing for a selected torrent.
    Files,
    /// Detailed metadata view (trackers, piece sizes, ratios) for a selected torrent.
    Details,
    /// External rsync daemon synchronization status panel.
    #[cfg(feature = "rsync")]
    Rsync,
}

/// Confirmation modal types requiring user acknowledgement before destructive actions.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// Confirm removing torrent from Transmission (keeps local files).
    Remove,
    /// Confirm removing torrent and deleting all downloaded files from Transmission daemon storage.
    DeleteFiles,
    /// Confirm deleting selected file(s) directly from disk.
    DeleteFileFromDisk,
}

/// Focused field in the authentication credentials modal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthField {
    /// Username input field.
    Username,
    /// Password input field.
    Password,
}

/// Modal overlay dialogs for user interaction.
pub enum Modal {
    /// Prompt for a magnet link, HTTP URL, or local `.torrent` file path.
    AddUrl(String),
    /// Prompt for destination directory when adding a torrent.
    AddLocation { url: String, location: String },
    /// Prompt for moving/changing the location of existing torrent(s).
    ChangeLocation(String),
    /// Filter input modal for searching or filtering torrents.
    Filter,
    /// Action confirmation modal.
    Confirm(Confirm),
    /// Authentication modal when Transmission returns HTTP 401.
    Auth {
        username: String,
        password: String,
        focused: AuthField,
    },
}

/// Column fields by which torrent lists can be sorted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortColumn {
    /// Sort by torrent name.
    Name,
    /// Sort by total size in bytes.
    Size,
    /// Sort by download progress fraction (0.0 to 1.0).
    Progress,
    /// Sort by download speed (bytes/sec).
    Down,
    /// Sort by upload speed (bytes/sec).
    Up,
    /// Sort by estimated completion time (ETA seconds).
    Eta,
    /// Sort by upload/download ratio.
    Ratio,
    /// Sort by status code (downloading, seeding, stopped, etc.).
    Status,
    /// Sort by Transmission queue position.
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

/// Main application state machine managing TUI state, Transmission RPC connection,
/// view models, modals, navigation, and background worker thread channels.
pub struct App {
    /// Thread-safe client wrapper for Transmission RPC calls.
    pub client: Arc<TransmissionClient>,
    /// Configured keybinding mappings for user navigation and shortcuts.
    pub bindings: Bindings,
    /// Color theme configuration for TUI styling.
    pub theme: ThemeConfig,
    /// Compiled rule-based automation event configurations.
    pub events: crate::events::CompiledEventsConfig,
    /// Active TUI view panel (torrent list, file listing, details, or rsync).
    pub view: View,
    /// Flag controlling the main application loop execution.
    pub running: bool,
    /// Scroll offset when the help overlay is open; `None` when closed.
    pub help: Option<u16>,

    /// Complete list of torrents fetched from Transmission daemon.
    pub torrents: Vec<Torrent>,
    /// Cursor index in the filtered torrent list.
    pub cursor: usize,
    /// Set of selected indices in the filtered torrent list for multi-torrent operations.
    pub selected: BTreeSet<usize>,
    /// Active column used for sorting the torrent list.
    pub sort_column: SortColumn,
    /// Sort direction (`true` for ascending, `false` for descending).
    pub sort_ascending: bool,

    /// Flag indicating whether label editing mode is active.
    pub label_editing: bool,
    /// Text buffer for editing comma-separated labels.
    pub label_input: String,

    /// Active modal dialog overlay, if any.
    pub modal: Option<Modal>,
    /// Persistent text filter string for searching/filtering torrents.
    pub filter_input: String,
    /// Cached indices into `self.torrents` matching the active `filter_input`.
    filtered_indices: Vec<usize>,

    /// Torrent metadata object currently opened in file or details view.
    pub detail_torrent: Option<Torrent>,
    /// Cursor index in the file listing of `detail_torrent`.
    pub file_cursor: usize,
    /// Set of selected file indices in `detail_torrent` for bulk priority/wanted operations.
    pub file_selected: BTreeSet<usize>,

    /// Cached status and log history for the external rsync automation daemon.
    #[cfg(feature = "rsync")]
    pub rsync_state: crate::rsync::RsyncState,

    /// Session transfer statistics from Transmission daemon.
    pub stats: Option<SessionStats>,
    /// Free space on Transmission daemon storage directory.
    pub free: Option<FreeSpace>,
    /// Latest user-facing error message.
    pub last_error: Option<String>,
    /// Timestamp when `last_error` was set, used for auto-clearing after 10 seconds.
    error_since: Option<Instant>,
    /// Default download directory reported by Transmission daemon.
    pub default_download_dir: Option<String>,
    /// Counter tick for throttling free space RPC requests.
    free_space_tick: u8,

    /// Channel sender for background worker threads.
    refresh_tx: mpsc::Sender<RefreshMsg>,
    /// Channel receiver for receiving background thread updates in the main loop.
    refresh_rx: mpsc::Receiver<RefreshMsg>,
    /// Guard preventing concurrent periodic refresh threads.
    refresh_in_flight: bool,
    /// Guard preventing concurrent event snapshot poll threads.
    pub event_snapshot_in_flight: bool,
    /// Tracker observing torrent lifecycle transitions (added, started, finished).
    pub lifecycle_tracker: crate::events::LifecycleTracker,
    /// Scheduler managing queued event action execution queues per torrent.
    pub event_scheduler: crate::events::EventScheduler,
    /// Pluggable backend for executing rule actions (defaults to TransmissionClient).
    pub action_backend: std::sync::Arc<dyn crate::events::ActionBackend>,

    /// Optional URL string to save to config upon first successful connection.
    pending_url_save: Option<String>,

    /// SSH directory listing cache for location autocompletion: `(parent_dir, subdirs)`.
    pub location_dir_cache: Option<(String, Vec<String>)>,
    /// Function pointer for remote SSH directory listing, mockable in unit tests.
    pub(crate) remote_dir_lister: RemoteDirLister,
}

impl App {
    /// Attempts to construct a new `App` instance with given client and configuration.
    ///
    /// Validates and compiles event automation rules. Returns `Err(String)` if event rules contain invalid regex.
    pub fn try_new(client: TransmissionClient, config: Config) -> Result<Self, String> {
        let bindings = Bindings::from_config(&config.keys);
        let (refresh_tx, refresh_rx) = mpsc::channel();
        let events = crate::events::CompiledEventsConfig::from_config(config.events)?;
        let client = std::sync::Arc::new(client);
        let action_backend: std::sync::Arc<dyn crate::events::ActionBackend> = client.clone();
        Ok(Self {
            client,
            bindings,
            theme: config.theme,
            events,
            event_snapshot_in_flight: false,
            lifecycle_tracker: crate::events::LifecycleTracker::new(),
            event_scheduler: crate::events::EventScheduler::new(),
            action_backend,
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
            remote_dir_lister: util::list_remote_dirs,
        })
    }

    pub fn with_pending_url_save(mut self, url: Option<String>) -> Self {
        self.pending_url_save = url;
        self
    }

    fn daemon_host(&self) -> Option<(String, bool)> {
        let url = url::Url::parse(&self.client.url).ok()?;
        match url.host()? {
            url::Host::Domain(host) => {
                Some((host.to_string(), host.eq_ignore_ascii_case("localhost")))
            }
            url::Host::Ipv4(address) => Some((address.to_string(), address.is_loopback())),
            url::Host::Ipv6(address) => Some((address.to_string(), address.is_loopback())),
        }
    }

    /// Returns the remote hostname for SSH if the connection is not to localhost.
    pub fn ssh_host(&self) -> Option<String> {
        let (host, is_local) = self.daemon_host()?;
        if is_local || host.starts_with('-') {
            None
        } else {
            Some(host)
        }
    }

    pub fn is_local_daemon(&self) -> bool {
        self.daemon_host().is_some_and(|(_, is_local)| is_local)
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

impl App {
    /// Evaluates a new torrent snapshot against compiled event rules and enqueues matching actions.
    pub fn process_event_snapshot(&mut self, torrents: &[Torrent]) {
        for (torrent, kind) in self.lifecycle_tracker.observe(torrents) {
            let rules = match kind {
                crate::events::LifecycleEventKind::Added => &self.events.on_torrent_added,
                crate::events::LifecycleEventKind::DownloadStarted => {
                    &self.events.on_download_started
                }
                crate::events::LifecycleEventKind::DownloadFinished => {
                    &self.events.on_download_finished
                }
            };
            let actions = rules
                .iter()
                .filter(|rule| crate::events::matches_compiled_rule(&torrent, rule))
                .flat_map(|rule| rule.actions.iter().cloned())
                .collect::<Vec<_>>();
            if !actions.is_empty() {
                self.event_scheduler.enqueue(crate::events::EventBatch {
                    torrent,
                    kind,
                    actions,
                    next_action: 0,
                    current_dir: None,
                    current_labels: None,
                });
            }
        }
        self.start_ready_event_actions();
    }

    /// Dequeues ready event batches and spawns background worker threads to execute actions.
    pub fn start_ready_event_actions(&mut self) {
        for batch in self.event_scheduler.take_ready() {
            let backend = std::sync::Arc::clone(&self.action_backend);
            let tx = self.refresh_tx.clone();
            std::thread::spawn(move || {
                let result = crate::events::execute_compiled_actions_resumable(
                    backend.as_ref(),
                    &batch.torrent,
                    &batch.actions,
                    batch.next_action,
                    batch.current_dir,
                    batch.current_labels,
                );
                let _ = tx.send(RefreshMsg::ActionComplete {
                    torrent_id: batch.torrent.id,
                    kind: batch.kind,
                    result,
                });
            });
        }
    }
}

#[cfg(test)]
impl App {
    pub fn new(client: TransmissionClient, config: crate::config::Config) -> Self {
        match Self::try_new(client, config) {
            Ok(app) => app,
            Err(error) => panic!("test configuration must be valid: {error}"),
        }
    }
}
