use crate::client::TransmissionClient;
use crate::protocol::{FilePriority, TORRENT_DETAIL_FIELDS, Torrent};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone)]
pub enum CompiledAction {
    SetSequential {
        enabled: bool,
    },
    PrioritizeFiles {
        first_alphabetical: Option<usize>,
        pattern: Option<regex::Regex>,
        priority: crate::config::FilePriorityConfig,
    },
    SetLabels {
        labels: Vec<String>,
    },
    SetLocation {
        path: String,
    },
    Execute {
        command: String,
        args: Vec<String>,
    },
    Stop,
    Start,
    Remove {
        delete_local_data: bool,
    },
}

#[derive(Clone)]
pub struct CompiledRule {
    pub require_labels: Option<Vec<String>>,
    pub require_tracker: Option<String>,
    pub name_pattern: Option<regex::Regex>,
    pub actions: Vec<CompiledAction>,
}

#[derive(Clone)]
pub struct CompiledEventsConfig {
    pub on_torrent_added: Vec<CompiledRule>,
    pub on_download_started: Vec<CompiledRule>,
    pub on_download_finished: Vec<CompiledRule>,
}

impl CompiledEventsConfig {
    pub fn from_config(cfg: crate::config::EventsConfig) -> Result<Self, String> {
        let compile_actions =
            |actions: Vec<crate::config::ActionConfig>| -> Result<Vec<CompiledAction>, String> {
                actions
                    .into_iter()
                    .map(|a| match a {
                        crate::config::ActionConfig::SetSequential { enabled } => {
                            Ok(CompiledAction::SetSequential { enabled })
                        }
                        crate::config::ActionConfig::PrioritizeFiles {
                            first_alphabetical,
                            pattern,
                            priority,
                        } => {
                            let compiled_pattern = match pattern {
                                Some(p) => Some(
                                    regex::Regex::new(&p)
                                        .map_err(|e| format!("Invalid regex '{}': {}", p, e))?,
                                ),
                                None => None,
                            };
                            Ok(CompiledAction::PrioritizeFiles {
                                first_alphabetical,
                                pattern: compiled_pattern,
                                priority,
                            })
                        }
                        crate::config::ActionConfig::SetLabels { labels } => {
                            Ok(CompiledAction::SetLabels { labels })
                        }
                        crate::config::ActionConfig::SetLocation { path } => {
                            Ok(CompiledAction::SetLocation { path })
                        }
                        crate::config::ActionConfig::Execute { command, args } => {
                            Ok(CompiledAction::Execute { command, args })
                        }
                        crate::config::ActionConfig::Stop => Ok(CompiledAction::Stop),
                        crate::config::ActionConfig::Start => Ok(CompiledAction::Start),
                        crate::config::ActionConfig::Remove { delete_local_data } => {
                            Ok(CompiledAction::Remove { delete_local_data })
                        }
                    })
                    .collect()
            };

        let compile_rules =
            |rules: Vec<crate::config::RuleConfig>| -> Result<Vec<CompiledRule>, String> {
                rules
                    .into_iter()
                    .map(|r| {
                        let compiled_pattern = match r.name_pattern {
                            Some(p) => Some(
                                regex::Regex::new(&p)
                                    .map_err(|e| format!("Invalid regex '{}': {}", p, e))?,
                            ),
                            None => None,
                        };
                        Ok(CompiledRule {
                            require_labels: r.require_labels,
                            require_tracker: r.require_tracker,
                            name_pattern: compiled_pattern,
                            actions: compile_actions(r.actions)?,
                        })
                    })
                    .collect()
            };
        Ok(Self {
            on_torrent_added: compile_rules(cfg.on_torrent_added)?,
            on_download_started: compile_rules(cfg.on_download_started)?,
            on_download_finished: compile_rules(cfg.on_download_finished)?,
        })
    }
}

pub fn matches_compiled_rule(t: &Torrent, rule: &CompiledRule) -> bool {
    if let Some(req_labels) = &rule.require_labels
        && !req_labels.iter().any(|l| t.labels.contains(l))
    {
        return false;
    }
    if let Some(req_tracker) = &rule.require_tracker {
        let matches = t
            .tracker_stats
            .iter()
            .any(|ts| ts.host.contains(req_tracker));
        if !matches {
            return false;
        }
    }
    if let Some(re) = &rule.name_pattern
        && !re.is_match(&t.name)
    {
        return false;
    }
    true
}

pub trait ActionBackend: Send + Sync {
    fn set_sequential(&self, id: i64, enabled: bool) -> Result<(), String>;
    fn get_detail(&self, id: i64) -> Result<Option<Torrent>, String>;
    fn set_file_priorities(
        &self,
        id: i64,
        priorities: &[(usize, FilePriority)],
    ) -> Result<(), String>;
    fn set_labels(&self, id: i64, labels: &[String]) -> Result<(), String>;
    fn set_location(&self, id: i64, path: &str) -> Result<(), String>;
    fn run_command(&self, command: &str, args: &[String]) -> Result<(), String>;
    fn stop(&self, id: i64) -> Result<(), String>;
    fn start(&self, id: i64) -> Result<(), String>;
    fn remove(&self, id: i64, delete_local_data: bool) -> Result<(), String>;
}

pub fn substitute_torrent_vars(value: &str, torrent: &Torrent, current_dir: &str) -> String {
    value
        .replace("{id}", &torrent.id.to_string())
        .replace("{name}", &torrent.name)
        .replace("{dir}", current_dir)
}

#[derive(Clone, Debug, Default)]
pub struct ActionProgress {
    pub next_action: usize,
    pub current_dir: Option<String>,
    pub current_labels: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionFailureKind {
    Retryable,
    MissingTorrent,
}

#[derive(Debug)]
pub struct ActionFailure {
    pub progress: ActionProgress,
    pub kind: ActionFailureKind,
    pub error: String,
}

pub fn execute_compiled_actions_resumable(
    backend: &dyn ActionBackend,
    torrent: &Torrent,
    actions: &[CompiledAction],
    start_action: usize,
    current_dir: Option<String>,
    current_labels: Option<Vec<String>>,
) -> Result<ActionProgress, ActionFailure> {
    let mut progress = ActionProgress {
        next_action: start_action,
        current_dir,
        current_labels,
    };
    let detail = match backend.get_detail(torrent.id) {
        Ok(Some(detail)) => detail,
        Ok(None) => {
            return Err(ActionFailure {
                progress,
                kind: ActionFailureKind::MissingTorrent,
                error: format!("torrent {} no longer exists", torrent.id),
            });
        }
        Err(error) => {
            return Err(ActionFailure {
                progress,
                kind: ActionFailureKind::Retryable,
                error,
            });
        }
    };
    let mut current_dir = progress
        .current_dir
        .take()
        .unwrap_or_else(|| detail.download_dir.clone());
    let mut current_labels = progress
        .current_labels
        .take()
        .unwrap_or_else(|| detail.labels.clone());

    for (index, action) in actions.iter().enumerate().skip(start_action) {
        let result = match action {
            CompiledAction::SetSequential { enabled } => {
                backend.set_sequential(torrent.id, *enabled)
            }
            CompiledAction::PrioritizeFiles {
                first_alphabetical,
                pattern,
                priority,
            } => {
                let mut changes = Vec::new();
                let target_priority = priority.to_protocol();

                if let Some(re) = pattern {
                    for (index, file) in detail.files.iter().enumerate() {
                        if re.is_match(&file.name) {
                            changes.push((index, target_priority));
                        }
                    }
                }

                if let Some(count) = first_alphabetical {
                    let mut files: Vec<_> = detail.files.iter().enumerate().collect();
                    files.sort_by(|a, b| a.1.name.cmp(&b.1.name));
                    for (index, _) in files.into_iter().take(*count) {
                        changes.push((index, target_priority));
                    }
                }

                changes.sort_by_key(|change| change.0);
                changes.dedup_by_key(|change| change.0);
                if changes.is_empty() {
                    Ok(())
                } else {
                    backend.set_file_priorities(torrent.id, &changes)
                }
            }
            CompiledAction::SetLabels { labels } => {
                let mut merged = current_labels.clone();
                for label in labels {
                    if !merged.contains(label) {
                        merged.push(label.clone());
                    }
                }
                backend.set_labels(torrent.id, &merged).map(|()| {
                    current_labels = merged;
                })
            }
            CompiledAction::SetLocation { path } => {
                let path = substitute_torrent_vars(path, torrent, &current_dir);
                backend.set_location(torrent.id, &path).map(|()| {
                    current_dir = path;
                })
            }
            CompiledAction::Execute { command, args } => {
                let args = args
                    .iter()
                    .map(|arg| substitute_torrent_vars(arg, torrent, &current_dir))
                    .collect::<Vec<_>>();
                backend.run_command(command, &args)
            }
            CompiledAction::Stop => backend.stop(torrent.id),
            CompiledAction::Start => backend.start(torrent.id),
            CompiledAction::Remove { delete_local_data } => {
                backend.remove(torrent.id, *delete_local_data)
            }
        };
        if let Err(error) = result {
            return Err(ActionFailure {
                progress: ActionProgress {
                    next_action: index,
                    current_dir: Some(current_dir),
                    current_labels: Some(current_labels),
                },
                kind: ActionFailureKind::Retryable,
                error,
            });
        }
        progress.next_action = index + 1;
    }

    progress.current_dir = Some(current_dir);
    progress.current_labels = Some(current_labels);
    Ok(progress)
}

#[cfg(test)]
pub(crate) fn execute_compiled_actions(
    backend: &dyn ActionBackend,
    torrent: &Torrent,
    actions: &[CompiledAction],
) -> Result<(), String> {
    execute_compiled_actions_resumable(backend, torrent, actions, 0, None, None)
        .map(|_| ())
        .map_err(|failure| failure.error)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleEventKind {
    Added,
    DownloadStarted,
    DownloadFinished,
}

#[derive(Clone, Copy)]
struct LifecycleState {
    active: bool,
    complete: bool,
}

impl LifecycleState {
    fn from_torrent(torrent: &Torrent) -> Self {
        Self {
            active: torrent.status == 4,
            complete: torrent.percent_done >= 1.0,
        }
    }
}

#[derive(Default)]
pub struct LifecycleTracker {
    initialized: bool,
    previous: BTreeMap<i64, LifecycleState>,
}

impl LifecycleTracker {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn observe(&mut self, torrents: &[Torrent]) -> Vec<(Torrent, LifecycleEventKind)> {
        if !self.initialized {
            self.previous = torrents
                .iter()
                .map(|torrent| (torrent.id, LifecycleState::from_torrent(torrent)))
                .collect();
            self.initialized = true;
            return Vec::new();
        }

        let current_ids: BTreeSet<_> = torrents.iter().map(|torrent| torrent.id).collect();
        self.previous.retain(|id, _| current_ids.contains(id));
        let mut events = Vec::new();

        for torrent in torrents {
            let current = LifecycleState::from_torrent(torrent);
            match self.previous.get(&torrent.id).copied() {
                None => {
                    events.push((torrent.clone(), LifecycleEventKind::Added));
                    if current.active || current.complete {
                        events.push((torrent.clone(), LifecycleEventKind::DownloadStarted));
                    }
                    if current.complete {
                        events.push((torrent.clone(), LifecycleEventKind::DownloadFinished));
                    }
                }
                Some(previous) => {
                    let started = !previous.active && current.active;
                    let finished = !previous.complete && current.complete;
                    if started || (finished && !previous.active && !current.active) {
                        events.push((torrent.clone(), LifecycleEventKind::DownloadStarted));
                    }
                    if finished {
                        events.push((torrent.clone(), LifecycleEventKind::DownloadFinished));
                    }
                }
            }
            self.previous.insert(torrent.id, current);
        }

        events
    }
}

#[derive(Clone)]
pub struct EventBatch {
    pub torrent: Torrent,
    pub kind: LifecycleEventKind,
    pub actions: Vec<CompiledAction>,
    pub next_action: usize,
    pub current_dir: Option<String>,
    pub current_labels: Option<Vec<String>>,
}

#[derive(Default)]
struct TorrentEventQueue {
    pending: VecDeque<EventBatch>,
    running: bool,
}

#[derive(Default)]
pub struct EventScheduler {
    queues: BTreeMap<i64, TorrentEventQueue>,
}

impl EventScheduler {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn enqueue(&mut self, batch: EventBatch) {
        self.queues
            .entry(batch.torrent.id)
            .or_default()
            .pending
            .push_back(batch);
    }

    pub fn take_ready(&mut self) -> Vec<EventBatch> {
        let mut ready = Vec::new();
        for queue in self.queues.values_mut() {
            if !queue.running
                && let Some(batch) = queue.pending.front().cloned()
            {
                queue.running = true;
                ready.push(batch);
            }
        }
        ready
    }

    pub fn complete(&mut self, torrent_id: i64, succeeded: bool, progress: ActionProgress) {
        if let Some(queue) = self.queues.get_mut(&torrent_id) {
            queue.running = false;
            if succeeded {
                queue.pending.pop_front();
            } else if let Some(batch) = queue.pending.front_mut() {
                batch.next_action = progress.next_action;
                batch.current_dir = progress.current_dir;
                batch.current_labels = progress.current_labels;
            }
        }
        self.queues
            .retain(|_, queue| queue.running || !queue.pending.is_empty());
    }

    pub fn fail(&mut self, torrent_id: i64, failure: ActionFailure) {
        if failure.kind == ActionFailureKind::MissingTorrent {
            self.queues.remove(&torrent_id);
        } else {
            self.complete(torrent_id, false, failure.progress);
        }
    }
}
impl ActionBackend for TransmissionClient {
    fn set_sequential(&self, id: i64, enabled: bool) -> Result<(), String> {
        TransmissionClient::set_sequential(self, &[id], enabled)
    }

    fn get_detail(&self, id: i64) -> Result<Option<Torrent>, String> {
        TransmissionClient::get_torrent(self, id, TORRENT_DETAIL_FIELDS)
    }

    fn set_file_priorities(
        &self,
        id: i64,
        priorities: &[(usize, FilePriority)],
    ) -> Result<(), String> {
        TransmissionClient::set_file_priorities(self, id, priorities)
    }

    fn set_labels(&self, id: i64, labels: &[String]) -> Result<(), String> {
        TransmissionClient::set_labels(self, &[id], labels)
    }

    fn set_location(&self, id: i64, path: &str) -> Result<(), String> {
        TransmissionClient::set_location(self, &[id], path, true)
    }

    fn run_command(&self, command: &str, args: &[String]) -> Result<(), String> {
        let status = std::process::Command::new(command)
            .args(args)
            .status()
            .map_err(|e| format!("failed to run command {command:?}: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("command {command:?} exited with status {status}"))
        }
    }

    fn stop(&self, id: i64) -> Result<(), String> {
        TransmissionClient::stop(self, &[id])
    }
    fn start(&self, id: i64) -> Result<(), String> {
        TransmissionClient::start(self, &[id])
    }

    fn remove(&self, id: i64, delete_local_data: bool) -> Result<(), String> {
        TransmissionClient::remove(self, &[id], delete_local_data)
    }
}

#[cfg(test)]
mod tests;
