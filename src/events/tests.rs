use super::*;
use crate::protocol::{Torrent, TrackerStats};
use std::sync::Mutex;

#[derive(Default)]
struct FakeActionBackend {
    calls: Mutex<Vec<String>>,
    detail: Mutex<Option<Torrent>>,
    fail_at: Mutex<Option<usize>>,
}

impl FakeActionBackend {
    fn record(&self, call: String) -> Result<(), String> {
        let mut calls = self.calls.lock().unwrap();
        let index = calls.len();
        calls.push(call);
        let should_fail = {
            let mut fail_at = self.fail_at.lock().unwrap();
            if *fail_at == Some(index) {
                *fail_at = None;
                true
            } else {
                false
            }
        };
        if should_fail {
            Err("injected failure".into())
        } else {
            Ok(())
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl ActionBackend for FakeActionBackend {
    fn set_sequential(&self, id: i64, enabled: bool) -> Result<(), String> {
        self.record(format!("sequential:{id}:{enabled}"))
    }

    fn get_detail(&self, id: i64) -> Result<Option<Torrent>, String> {
        self.record(format!("detail:{id}"))?;
        Ok(self.detail.lock().unwrap().clone())
    }

    fn set_file_priorities(
        &self,
        id: i64,
        priorities: &[(usize, crate::protocol::FilePriority)],
    ) -> Result<(), String> {
        self.record(format!("priorities:{id}:{priorities:?}"))
    }

    fn set_labels(&self, id: i64, labels: &[String]) -> Result<(), String> {
        self.record(format!("labels:{id}:{}", labels.join(",")))
    }

    fn set_location(&self, id: i64, path: &str) -> Result<(), String> {
        self.record(format!("location:{id}:{path}"))
    }

    fn run_command(&self, command: &str, args: &[String]) -> Result<(), String> {
        self.record(format!("command:{command}:{}", args.join(",")))
    }

    fn stop(&self, id: i64) -> Result<(), String> {
        self.record(format!("stop:{id}"))
    }

    fn start(&self, id: i64) -> Result<(), String> {
        self.record(format!("start:{id}"))
    }

    fn remove(&self, id: i64, delete_local_data: bool) -> Result<(), String> {
        self.record(format!("remove:{id}:{delete_local_data}"))
    }
}

fn event_torrent(id: i64, status: i64, percent_done: f64) -> Torrent {
    Torrent {
        id,
        name: format!("torrent-{id}"),
        status,
        percent_done,
        ..Default::default()
    }
}

fn event_batch(id: i64, kind: LifecycleEventKind) -> EventBatch {
    EventBatch {
        torrent: event_torrent(id, 0, 0.0),
        kind,
        actions: vec![CompiledAction::Stop],
        next_action: 0,
        current_dir: None,
        current_labels: None,
    }
}

#[test]
fn test_event_lifecycle_baselines_and_detects_repeated_transitions() {
    let mut tracker = LifecycleTracker::default();
    assert!(tracker.observe(&[event_torrent(1, 0, 0.0)]).is_empty());

    let events = tracker.observe(&[event_torrent(1, 4, 0.2)]);
    assert_eq!(events[0].1, LifecycleEventKind::DownloadStarted);
    assert_eq!(events.len(), 1);
    assert!(tracker.observe(&[event_torrent(1, 4, 0.3)]).is_empty());
    assert!(tracker.observe(&[event_torrent(1, 0, 0.3)]).is_empty());

    let events = tracker.observe(&[event_torrent(1, 4, 0.4)]);
    assert_eq!(events[0].1, LifecycleEventKind::DownloadStarted);
    let events = tracker.observe(&[event_torrent(1, 6, 1.0)]);
    assert_eq!(events[0].1, LifecycleEventKind::DownloadFinished);
}

#[test]
fn test_event_lifecycle_orders_inferred_start_before_finish() {
    let mut tracker = LifecycleTracker::default();
    tracker.observe(&[event_torrent(1, 0, 0.5)]);

    let kinds = tracker
        .observe(&[event_torrent(1, 6, 1.0)])
        .into_iter()
        .map(|(_, kind)| kind)
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        vec![
            LifecycleEventKind::DownloadStarted,
            LifecycleEventKind::DownloadFinished
        ]
    );
}

#[test]
fn test_event_lifecycle_orders_new_complete_torrent_events() {
    let mut tracker = LifecycleTracker::default();
    tracker.observe(&[]);

    let kinds = tracker
        .observe(&[event_torrent(9, 6, 1.0)])
        .into_iter()
        .map(|(_, kind)| kind)
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        vec![
            LifecycleEventKind::Added,
            LifecycleEventKind::DownloadStarted,
            LifecycleEventKind::DownloadFinished
        ]
    );
}

#[test]
fn test_event_rule_matches_labels_tracker_and_name() {
    let torrent = Torrent {
        name: "Example Show S01E01".into(),
        labels: vec!["tv".into()],
        tracker_stats: vec![TrackerStats {
            host: "tracker.example.com".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let rule = CompiledRule {
        require_labels: Some(vec!["movies".into(), "tv".into()]),
        require_tracker: Some("example.com".into()),
        name_pattern: Some(regex::Regex::new("S\\d{2}E\\d{2}").unwrap()),
        actions: vec![],
    };

    assert!(matches_compiled_rule(&torrent, &rule));
    let mut mismatch = rule.clone();
    mismatch.require_tracker = Some("other.invalid".into());
    assert!(!matches_compiled_rule(&torrent, &mismatch));
}

#[test]
fn test_event_scheduler_serializes_and_retries_failed_front() {
    let mut scheduler = EventScheduler::default();
    scheduler.enqueue(event_batch(1, LifecycleEventKind::Added));
    let first = scheduler.take_ready();
    assert_eq!(first[0].kind, LifecycleEventKind::Added);

    scheduler.enqueue(event_batch(1, LifecycleEventKind::DownloadStarted));
    assert!(scheduler.take_ready().is_empty());
    scheduler.complete(
        1,
        false,
        ActionProgress {
            next_action: 0,
            current_dir: None,
            current_labels: None,
        },
    );
    let retry = scheduler.take_ready();
    assert_eq!(retry[0].kind, LifecycleEventKind::Added);

    scheduler.complete(
        1,
        true,
        ActionProgress {
            next_action: 1,
            current_dir: None,
            current_labels: None,
        },
    );
    let second = scheduler.take_ready();
    assert_eq!(second[0].kind, LifecycleEventKind::DownloadStarted);
    scheduler.complete(
        1,
        true,
        ActionProgress {
            next_action: 1,
            current_dir: None,
            current_labels: None,
        },
    );
    assert!(scheduler.queues.is_empty());
}

#[test]
fn test_event_scheduler_runs_different_torrents_concurrently() {
    let mut scheduler = EventScheduler::default();
    scheduler.enqueue(event_batch(1, LifecycleEventKind::Added));
    scheduler.enqueue(event_batch(2, LifecycleEventKind::Added));

    let ready = scheduler.take_ready();

    assert_eq!(ready.len(), 2);
}

#[test]
fn test_event_scheduler_cancels_successor_batches_after_remove() {
    let torrent = event_torrent(9, 6, 1.0);
    let backend = FakeActionBackend {
        detail: Mutex::new(Some(torrent.clone())),
        ..Default::default()
    };
    let mut scheduler = EventScheduler::default();
    scheduler.enqueue(EventBatch {
        torrent: torrent.clone(),
        kind: LifecycleEventKind::Added,
        actions: vec![CompiledAction::Remove {
            delete_local_data: false,
        }],
        next_action: 0,
        current_dir: None,
        current_labels: None,
    });
    scheduler.enqueue(event_batch(9, LifecycleEventKind::DownloadStarted));
    scheduler.enqueue(event_batch(9, LifecycleEventKind::DownloadFinished));

    let first = scheduler.take_ready().pop().unwrap();
    let progress = execute_compiled_actions_resumable(
        &backend,
        &first.torrent,
        &first.actions,
        first.next_action,
        first.current_dir,
        first.current_labels,
    )
    .unwrap();
    scheduler.complete(9, true, progress);
    *backend.detail.lock().unwrap() = None;

    let second = scheduler.take_ready().pop().unwrap();
    let failure = execute_compiled_actions_resumable(
        &backend,
        &second.torrent,
        &second.actions,
        second.next_action,
        second.current_dir,
        second.current_labels,
    )
    .unwrap_err();
    assert_eq!(failure.kind, ActionFailureKind::MissingTorrent);
    scheduler.fail(9, failure);

    assert!(scheduler.queues.is_empty());
    assert_eq!(
        backend.calls(),
        vec!["detail:9", "remove:9:false", "detail:9"]
    );
}

#[test]
fn test_event_scheduler_cancels_queued_batch_after_external_disappearance() {
    let torrent = event_torrent(10, 0, 0.0);
    let backend = FakeActionBackend {
        detail: Mutex::new(Some(torrent.clone())),
        ..Default::default()
    };
    let mut scheduler = EventScheduler::default();
    scheduler.enqueue(event_batch(10, LifecycleEventKind::Added));
    scheduler.enqueue(event_batch(10, LifecycleEventKind::DownloadStarted));

    let first = scheduler.take_ready().pop().unwrap();
    let progress = execute_compiled_actions_resumable(
        &backend,
        &first.torrent,
        &first.actions,
        first.next_action,
        first.current_dir,
        first.current_labels,
    )
    .unwrap();
    scheduler.complete(10, true, progress);
    *backend.detail.lock().unwrap() = None;

    let queued = scheduler.take_ready().pop().unwrap();
    let failure = execute_compiled_actions_resumable(
        &backend,
        &queued.torrent,
        &queued.actions,
        queued.next_action,
        queued.current_dir,
        queued.current_labels,
    )
    .unwrap_err();
    assert_eq!(failure.kind, ActionFailureKind::MissingTorrent);
    scheduler.fail(10, failure);

    assert!(scheduler.queues.is_empty());
}

#[test]
fn test_compiled_events_config_from_config_success_and_errors() {
    use crate::config::{ActionConfig, EventsConfig, FilePriorityConfig, RuleConfig};

    let cfg = EventsConfig {
        on_torrent_added: vec![RuleConfig {
            require_labels: Some(vec!["tv".into()]),
            require_tracker: Some("example.com".into()),
            name_pattern: Some("^Test".into()),
            actions: vec![
                ActionConfig::SetSequential { enabled: true },
                ActionConfig::PrioritizeFiles {
                    first_alphabetical: Some(1),
                    pattern: Some("\\.mkv$".into()),
                    priority: FilePriorityConfig::High,
                },
                ActionConfig::SetLabels {
                    labels: vec!["added".into()],
                },
                ActionConfig::SetLocation {
                    path: "/downloads/{name}".into(),
                },
                ActionConfig::Execute {
                    command: "echo".into(),
                    args: vec!["{id}".into()],
                },
                ActionConfig::Stop,
                ActionConfig::Start,
                ActionConfig::Remove {
                    delete_local_data: false,
                },
            ],
        }],
        on_download_started: vec![],
        on_download_finished: vec![],
    };

    let compiled = CompiledEventsConfig::from_config(cfg).unwrap();
    assert_eq!(compiled.on_torrent_added.len(), 1);

    let err_rule = EventsConfig {
        on_torrent_added: vec![RuleConfig {
            require_labels: None,
            require_tracker: None,
            name_pattern: Some("[unclosed".into()),
            actions: vec![],
        }],
        on_download_started: vec![],
        on_download_finished: vec![],
    };
    assert!(CompiledEventsConfig::from_config(err_rule).is_err());

    let err_action = EventsConfig {
        on_torrent_added: vec![RuleConfig {
            require_labels: None,
            require_tracker: None,
            name_pattern: None,
            actions: vec![ActionConfig::PrioritizeFiles {
                first_alphabetical: None,
                pattern: Some("[invalid".into()),
                priority: FilePriorityConfig::High,
            }],
        }],
        on_download_started: vec![],
        on_download_finished: vec![],
    };
    assert!(CompiledEventsConfig::from_config(err_action).is_err());
}

#[test]
fn test_substitute_torrent_vars() {
    let torrent = Torrent {
        id: 42,
        name: "MyTorrent".into(),
        ..Default::default()
    };
    let res = substitute_torrent_vars("/path/{id}/{name}/{dir}", &torrent, "/downloads");
    assert_eq!(res, "/path/42/MyTorrent//downloads");
}

#[test]
fn test_execute_compiled_actions_all_variants_and_errors() {
    use crate::config::FilePriorityConfig;
    use crate::protocol::TorrentFile;

    let torrent = Torrent {
        id: 5,
        name: "TestTorrent".into(),
        download_dir: "/initial".into(),
        labels: vec!["existing".into()],
        files: vec![
            TorrentFile {
                name: "b.txt".into(),
                ..Default::default()
            },
            TorrentFile {
                name: "a.mkv".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let backend = FakeActionBackend {
        detail: Mutex::new(Some(torrent.clone())),
        ..Default::default()
    };

    let actions = vec![
        CompiledAction::SetSequential { enabled: true },
        CompiledAction::PrioritizeFiles {
            first_alphabetical: Some(1),
            pattern: Some(regex::Regex::new("\\.mkv$").unwrap()),
            priority: FilePriorityConfig::High,
        },
        CompiledAction::SetLabels {
            labels: vec!["newlabel".into()],
        },
        CompiledAction::SetLocation {
            path: "/moved/{name}".into(),
        },
        CompiledAction::Execute {
            command: "echo".into(),
            args: vec!["{id}".into(), "{name}".into(), "{dir}".into()],
        },
        CompiledAction::Stop,
        CompiledAction::Start,
        CompiledAction::Remove {
            delete_local_data: true,
        },
    ];

    let progress =
        execute_compiled_actions_resumable(&backend, &torrent, &actions, 0, None, None).unwrap();

    assert_eq!(progress.next_action, 8);
    assert_eq!(progress.current_dir.as_deref(), Some("/moved/TestTorrent"));
    assert_eq!(
        progress.current_labels.as_ref().unwrap(),
        &vec!["existing".to_string(), "newlabel".to_string()]
    );

    let calls = backend.calls();
    assert!(calls.contains(&"sequential:5:true".to_string()));
    assert!(calls.contains(&"labels:5:existing,newlabel".to_string()));
    assert!(calls.contains(&"location:5:/moved/TestTorrent".to_string()));
    assert!(calls.contains(&"command:echo:5,TestTorrent,/moved/TestTorrent".to_string()));
    assert!(calls.contains(&"stop:5".to_string()));
    assert!(calls.contains(&"start:5".to_string()));
    assert!(calls.contains(&"remove:5:true".to_string()));

    // Test error when get_detail returns Err
    let backend_err = FakeActionBackend {
        calls: Mutex::new(Vec::new()),
        detail: Mutex::new(Some(torrent.clone())),
        fail_at: Mutex::new(Some(0)),
    };
    let fail = execute_compiled_actions_resumable(&backend_err, &torrent, &actions, 0, None, None)
        .unwrap_err();
    assert_eq!(fail.kind, ActionFailureKind::Retryable);

    // Test error during action execution
    let backend_action_err = FakeActionBackend {
        calls: Mutex::new(Vec::new()),
        detail: Mutex::new(Some(torrent.clone())),
        fail_at: Mutex::new(Some(1)), // set_sequential fails (index 1 is after detail:5)
    };
    let fail2 =
        execute_compiled_actions_resumable(&backend_action_err, &torrent, &actions, 0, None, None)
            .unwrap_err();
    assert_eq!(fail2.kind, ActionFailureKind::Retryable);
    assert_eq!(fail2.progress.next_action, 0);

    // Test helper execute_compiled_actions
    assert!(execute_compiled_actions(&backend, &torrent, &actions).is_ok());
}

#[test]
fn test_transmission_client_action_backend_impl() {
    use crate::test_support::{Response, ScriptedServer};

    let server = ScriptedServer::start(vec![
        Response::json(serde_json::json!({"result": "success", "arguments": {}})), // set_sequential
        Response::json(
            serde_json::json!({"result": "success", "arguments": {"torrents": [{"id": 1, "name": "srv_torrent"}]}}),
        ), // get_detail
        Response::json(serde_json::json!({"result": "success", "arguments": {}})), // set_file_priorities
        Response::json(serde_json::json!({"result": "success", "arguments": {}})), // set_labels
        Response::json(serde_json::json!({"result": "success", "arguments": {}})), // set_location
        Response::json(serde_json::json!({"result": "success", "arguments": {}})), // stop
        Response::json(serde_json::json!({"result": "success", "arguments": {}})), // start
        Response::json(serde_json::json!({"result": "success", "arguments": {}})), // remove
    ]);

    let client = TransmissionClient::new(&server.url, None, None);
    assert!(ActionBackend::set_sequential(&client, 1, true).is_ok());
    let detail = ActionBackend::get_detail(&client, 1).unwrap().unwrap();
    assert_eq!(detail.name, "srv_torrent");
    assert!(ActionBackend::set_file_priorities(&client, 1, &[(0, FilePriority::High)]).is_ok());
    assert!(ActionBackend::set_labels(&client, 1, &["tag".into()]).is_ok());
    assert!(ActionBackend::set_location(&client, 1, "/new").is_ok());
    assert!(ActionBackend::stop(&client, 1).is_ok());
    assert!(ActionBackend::start(&client, 1).is_ok());
    assert!(ActionBackend::remove(&client, 1, false).is_ok());

    assert!(ActionBackend::run_command(&client, "echo", &["hello".into()]).is_ok());
    assert!(ActionBackend::run_command(&client, "nonexistent_cmd_xyz_123", &[]).is_err());
    assert!(ActionBackend::run_command(&client, "false", &[]).is_err());
}

#[test]
fn test_constructors_and_missing_torrent_handling() {
    let _tracker = LifecycleTracker::new();
    let _scheduler = EventScheduler::new();

    let torrent = Torrent {
        id: 99,
        name: "MissingTorrent".into(),
        ..Default::default()
    };
    let backend = FakeActionBackend {
        detail: Mutex::new(None),
        ..Default::default()
    };
    let actions = vec![CompiledAction::Stop];

    let failure = execute_compiled_actions_resumable(&backend, &torrent, &actions, 0, None, None)
        .unwrap_err();
    assert_eq!(failure.kind, ActionFailureKind::MissingTorrent);
    assert!(failure.error.contains("torrent 99 no longer exists"));
}

#[test]
fn test_matches_compiled_rule_mismatches() {
    let torrent = Torrent {
        name: "Movie.2026.1080p".into(),
        labels: vec!["movies".into()],
        tracker_stats: vec![],
        ..Default::default()
    };

    // Label mismatch
    let rule_label = CompiledRule {
        require_labels: Some(vec!["tv".into()]),
        require_tracker: None,
        name_pattern: None,
        actions: vec![],
    };
    assert!(!matches_compiled_rule(&torrent, &rule_label));

    // Tracker mismatch
    let rule_tracker = CompiledRule {
        require_labels: None,
        require_tracker: Some("example.com".into()),
        name_pattern: None,
        actions: vec![],
    };
    assert!(!matches_compiled_rule(&torrent, &rule_tracker));

    // Name pattern mismatch
    let rule_name = CompiledRule {
        require_labels: None,
        require_tracker: None,
        name_pattern: Some(regex::Regex::new("^Show").unwrap()),
        actions: vec![],
    };
    assert!(!matches_compiled_rule(&torrent, &rule_name));
}

#[test]
fn test_event_scheduler_fail_missing_torrent() {
    let mut scheduler = EventScheduler::new();
    scheduler.enqueue(event_batch(99, LifecycleEventKind::Added));
    assert_eq!(scheduler.take_ready().len(), 1);

    let failure = ActionFailure {
        progress: ActionProgress::default(),
        kind: ActionFailureKind::MissingTorrent,
        error: "torrent missing".into(),
    };
    scheduler.fail(99, failure);
    assert!(scheduler.take_ready().is_empty());
}

#[test]
fn test_events_coverage_completion() {
    // 1. matches_compiled_rule with matching require_tracker
    let torrent_tracker = Torrent {
        name: "TrackerTorrent".into(),
        tracker_stats: vec![TrackerStats {
            host: "tracker.example.com".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let rule_tracker = CompiledRule {
        require_labels: None,
        require_tracker: Some("example.com".into()),
        name_pattern: None,
        actions: vec![],
    };
    assert!(matches_compiled_rule(&torrent_tracker, &rule_tracker));

    // 2. PrioritizeFiles with regex pattern matching files
    use crate::config::FilePriorityConfig;
    use crate::protocol::{FileStats, TorrentFile};

    let torrent = Torrent {
        id: 12,
        name: "PatternTorrent".into(),
        download_dir: "/downloads".into(),
        files: vec![TorrentFile {
            name: "video.mkv".into(),
            ..Default::default()
        }],
        file_stats: vec![FileStats {
            priority: 0,
            wanted: true,
            bytes_completed: 0,
        }],
        ..Default::default()
    };
    let backend = FakeActionBackend {
        detail: Mutex::new(Some(torrent.clone())),
        ..Default::default()
    };
    let actions = vec![CompiledAction::PrioritizeFiles {
        first_alphabetical: None,
        pattern: Some(regex::Regex::new("\\.mkv$").expect("regex")),
        priority: FilePriorityConfig::High,
    }];
    assert!(
        execute_compiled_actions_resumable(&backend, &torrent, &actions, 0, None, None).is_ok()
    );

    // 3. TransmissionClient ActionBackend::run_command error paths
    use crate::test_support::ScriptedServer;
    let server = ScriptedServer::start(vec![]);
    let client = TransmissionClient::new(&server.url, None, None);
    assert!(ActionBackend::run_command(&client, "nonexistent_command_xyz_999", &[]).is_err());
    assert!(ActionBackend::run_command(&client, "false", &[]).is_err());
}

#[test]
fn test_event_automation_audit_trail_generation() {
    use crate::config::{ActionConfig, EventsConfig, FilePriorityConfig, RuleConfig};
    use crate::protocol::{FileStats, TorrentFile};
    use std::fs::File;
    use std::io::Write;

    let mut log_content = String::new();
    log_content.push_str("=================================================================\n");
    log_content.push_str("          TRST EVENT AUTOMATION AUDIT TRAIL EVIDENCE\n");
    log_content.push_str("=================================================================\n\n");

    // 1. Define configuration
    log_content.push_str("### 1. DEFINING RULE-BASED AUTOMATION CONFIGURATION\n");
    let cfg = EventsConfig {
        on_torrent_added: vec![RuleConfig {
            require_labels: None,
            require_tracker: Some("nyaa.si".into()),
            name_pattern: None,
            actions: vec![
                ActionConfig::SetLabels {
                    labels: vec!["anime".into()],
                },
                ActionConfig::PrioritizeFiles {
                    first_alphabetical: None,
                    pattern: Some("(?i).*\\.mkv$".into()),
                    priority: FilePriorityConfig::High,
                },
                ActionConfig::PrioritizeFiles {
                    first_alphabetical: None,
                    pattern: Some("(?i).*\\.txt$".into()),
                    priority: FilePriorityConfig::Skip,
                },
            ],
        }],
        on_download_started: vec![RuleConfig {
            require_labels: Some(vec!["tv".into(), "movies".into()]),
            require_tracker: None,
            name_pattern: None,
            actions: vec![ActionConfig::SetSequential { enabled: true }],
        }],
        on_download_finished: vec![RuleConfig {
            require_labels: None,
            require_tracker: None,
            name_pattern: Some("(?i).*S\\d{2}E\\d{2}.*".into()),
            actions: vec![
                ActionConfig::SetLocation {
                    path: "/media/tv_shows".into(),
                },
                ActionConfig::Execute {
                    command: "echo".into(),
                    args: vec!["Processed {name} (ID: {id}) in {dir}".into()],
                },
            ],
        }],
    };
    log_content.push_str(&format!("{:#?}\n\n", cfg));

    // 2. Compile configuration
    log_content.push_str("### 2. COMPILING EVENTS CONFIGURATION\n");
    let compiled = CompiledEventsConfig::from_config(cfg).expect("compile rules");
    log_content.push_str("✓ Successfully compiled all regex filters and rules.\n\n");

    // 3. Setup backend and simulation tracker
    let mut tracker = LifecycleTracker::default();
    tracker.observe(&[]);

    // Torrent A: Anime Torrent added
    let torrent_a = Torrent {
        id: 101,
        name: "Frieren Beyond Journeys End - 01.mkv".into(),
        status: 0, // Stopped
        percent_done: 0.0,
        tracker_stats: vec![TrackerStats {
            host: "tracker.nyaa.si".into(),
            ..Default::default()
        }],
        files: vec![
            TorrentFile {
                name: "Frieren Beyond Journeys End - 01.mkv".into(),
                ..Default::default()
            },
            TorrentFile {
                name: "Readme.txt".into(),
                ..Default::default()
            },
        ],
        file_stats: vec![
            FileStats {
                priority: 0,
                wanted: true,
                bytes_completed: 0,
            },
            FileStats {
                priority: 0,
                wanted: true,
                bytes_completed: 0,
            },
        ],
        ..Default::default()
    };

    // Torrent B: Movies/TV Torrent starting download
    let torrent_b = Torrent {
        id: 202,
        name: "Inception.2010.1080p.mkv".into(),
        status: 4, // Downloading
        percent_done: 0.1,
        labels: vec!["movies".into()],
        ..Default::default()
    };

    // Torrent C: Episode finishing download
    let torrent_c_before = Torrent {
        id: 303,
        name: "The.Simpsons.S35E05.1080p.mkv".into(),
        status: 4, // Downloading
        percent_done: 0.99,
        download_dir: "/downloads/incoming".into(),
        ..Default::default()
    };
    let torrent_c_after = Torrent {
        id: 303,
        name: "The.Simpsons.S35E05.1080p.mkv".into(),
        status: 6, // Seeding / Done
        percent_done: 1.0,
        download_dir: "/downloads/incoming".into(),
        ..Default::default()
    };

    let backend = FakeActionBackend {
        detail: Mutex::new(Some(torrent_a.clone())),
        ..Default::default()
    };

    // 4. Run through automation pipeline and log matches/actions
    log_content.push_str("### 4. SIMULATING LIFECYCLE EVENTS & PIPELINE ACTIONS\n");

    // --- CASE 1: Torrent A added ---
    log_content.push_str(&format!(
        "--> [Scenario A] Torrent added: '{}'\n",
        torrent_a.name
    ));
    let observed = tracker.observe(std::slice::from_ref(&torrent_a));
    assert_eq!(observed.len(), 1);
    let (t, kind) = &observed[0];
    assert_eq!(*kind, LifecycleEventKind::Added);
    log_content.push_str(&format!("  ✓ Detected event: {:?}\n", kind));

    // Evaluate rules for on_torrent_added
    let rules = &compiled.on_torrent_added;
    for (rule_idx, rule) in rules.iter().enumerate() {
        let is_match = matches_compiled_rule(t, rule);
        log_content.push_str(&format!(
            "  Evaluating Rule #{}: Match? {}\n",
            rule_idx + 1,
            is_match
        ));
        if is_match {
            let result =
                execute_compiled_actions_resumable(&backend, t, &rule.actions, 0, None, None);
            log_content.push_str(&format!(
                "    Executing Actions: {:#?}\n",
                rule.actions
                    .iter()
                    .map(|a| match a {
                        CompiledAction::SetLabels { labels } => format!("SetLabels({:?})", labels),
                        CompiledAction::PrioritizeFiles {
                            pattern, priority, ..
                        } => format!(
                            "PrioritizeFiles(pattern: {:?}, priority: {:?})",
                            pattern, priority
                        ),
                        _ => "Other".into(),
                    })
                    .collect::<Vec<_>>()
            ));
            assert!(result.is_ok());
            log_content.push_str("    ✓ Execution complete.\n");
        }
    }
    log_content.push('\n');

    // --- CASE 2: Torrent B download started ---
    // Seed tracker with initial torrent state (as if it was stopped)
    let mut tracker_b = LifecycleTracker::default();
    tracker_b.observe(&[]);
    let torrent_b_stopped = Torrent {
        status: 0,
        ..torrent_b.clone()
    };
    tracker_b.observe(&[torrent_b_stopped]);

    log_content.push_str(&format!(
        "--> [Scenario B] Torrent download started: '{}'\n",
        torrent_b.name
    ));
    let observed_b = tracker_b.observe(std::slice::from_ref(&torrent_b));
    assert_eq!(observed_b.len(), 1);
    let (t_b, kind_b) = &observed_b[0];
    assert_eq!(*kind_b, LifecycleEventKind::DownloadStarted);
    log_content.push_str(&format!("  ✓ Detected event: {:?}\n", kind_b));

    *backend.detail.lock().unwrap() = Some(torrent_b.clone());

    // Evaluate rules for on_download_started
    let rules_b = &compiled.on_download_started;
    for (rule_idx, rule) in rules_b.iter().enumerate() {
        let is_match = matches_compiled_rule(t_b, rule);
        log_content.push_str(&format!(
            "  Evaluating Rule #{}: Match? {}\n",
            rule_idx + 1,
            is_match
        ));
        if is_match {
            let result =
                execute_compiled_actions_resumable(&backend, t_b, &rule.actions, 0, None, None);
            log_content.push_str(&format!(
                "    Executing Actions: {:#?}\n",
                rule.actions
                    .iter()
                    .map(|a| match a {
                        CompiledAction::SetSequential { enabled } =>
                            format!("SetSequential(enabled: {})", enabled),
                        _ => "Other".into(),
                    })
                    .collect::<Vec<_>>()
            ));
            assert!(result.is_ok());
            log_content.push_str("    ✓ Execution complete.\n");
        }
    }
    log_content.push('\n');

    // --- CASE 3: Torrent C download finished ---
    let mut tracker_c = LifecycleTracker::default();
    tracker_c.observe(&[]);
    tracker_c.observe(&[torrent_c_before]);

    log_content.push_str(&format!(
        "--> [Scenario C] Torrent download finished: '{}'\n",
        torrent_c_after.name
    ));
    let observed_c = tracker_c.observe(std::slice::from_ref(&torrent_c_after));
    assert_eq!(observed_c.len(), 1);
    let (t_c, kind_c) = &observed_c[0];
    assert_eq!(*kind_c, LifecycleEventKind::DownloadFinished);
    log_content.push_str(&format!("  ✓ Detected event: {:?}\n", kind_c));

    *backend.detail.lock().unwrap() = Some(torrent_c_after.clone());

    // Evaluate rules for on_download_finished
    let rules_c = &compiled.on_download_finished;
    for (rule_idx, rule) in rules_c.iter().enumerate() {
        let is_match = matches_compiled_rule(t_c, rule);
        log_content.push_str(&format!(
            "  Evaluating Rule #{}: Match? {}\n",
            rule_idx + 1,
            is_match
        ));
        if is_match {
            let result =
                execute_compiled_actions_resumable(&backend, t_c, &rule.actions, 0, None, None);
            log_content.push_str(&format!(
                "    Executing Actions: {:#?}\n",
                rule.actions
                    .iter()
                    .map(|a| match a {
                        CompiledAction::SetLocation { path } =>
                            format!("SetLocation(path: {})", path),
                        CompiledAction::Execute { command, args } =>
                            format!("Execute(command: {}, args: {:?})", command, args),
                        _ => "Other".into(),
                    })
                    .collect::<Vec<_>>()
            ));
            assert!(result.is_ok());
            log_content.push_str("    ✓ Execution complete.\n");
        }
    }
    log_content.push('\n');

    // Write backend simulation results
    log_content.push_str("### 5. SIMULATED BACKEND INTERACTION CALLS RECORDED\n");
    for (idx, call) in backend.calls().iter().enumerate() {
        log_content.push_str(&format!(
            "  [{}] RPC / Action Dispatched -> {}\n",
            idx + 1,
            call
        ));
    }
    log_content.push_str("\n=================================================================\n");

    // 5. Save the log file
    let evidence_dir = std::env::temp_dir().join("no-mistakes-evidence");
    let _ = std::fs::create_dir_all(&evidence_dir);
    let evidence_path = evidence_dir.join("event-automation-audit.log");
    if let Ok(mut file) = File::create(&evidence_path) {
        let _ = file.write_all(log_content.as_bytes());
        println!(
            "✓ Successfully generated event automation audit trail at: {}",
            evidence_path.display()
        );
    }
}
