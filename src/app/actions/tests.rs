#![allow(unused_imports)]
use super::*;
use crate::client::TransmissionClient;
use crate::config::Config;
use crate::protocol::{FileStats, FreeSpace, SessionStats, Torrent, TorrentFile, TrackerStats};
use crate::test_support::{ScriptedServer, success};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

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

fn empty_app() -> App {
    App::new(
        TransmissionClient::new("http://dummy.invalid:9091/transmission/rpc", None, None),
        Config::default(),
    )
}

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

#[test]
fn test_set_error_opens_auth_modal_on_401() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.set_error("HTTP 401 Unauthorized");
    assert!(matches!(app.modal, Some(Modal::Auth { .. })));
    assert!(app.last_error.is_none());
}

#[test]
fn test_set_error_sets_last_error_for_non_401() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.set_error("connection refused");
    assert!(app.modal.is_none());
    assert_eq!(app.last_error.as_deref(), Some("connection refused"));
}

#[test]
fn test_set_error_does_not_replace_open_auth_modal() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: "alice".into(),
        password: "secret".into(),
        focused: AuthField::Password,
    });
    // A second 401 while the modal is already open should not reset it.
    app.set_error("HTTP 401 Unauthorized");
    assert!(matches!(
        app.modal,
        Some(Modal::Auth { ref username, .. }) if username == "alice"
    ));
}

#[test]
fn test_set_error_sets_error_since() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    assert!(app.error_since.is_none());
    app.set_error("something broke");
    assert!(app.error_since.is_some());
    assert_eq!(app.last_error.as_deref(), Some("something broke"));
}

#[test]
fn test_set_error_401_does_not_set_error_since() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.set_error("HTTP 401 Unauthorized");
    assert!(app.error_since.is_none());
    assert!(app.last_error.is_none());
}

#[test]
fn test_tick_autoclear_clears_after_expiry() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.last_error = Some("stale error".into());
    // Simulate an error that occurred 11 seconds ago.
    app.error_since = Some(Instant::now() - Duration::from_secs(11));
    app.tick_autoclear();
    assert!(app.last_error.is_none());
    assert!(app.error_since.is_none());
}

#[test]
fn test_tick_autoclear_does_not_clear_recent_error() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.last_error = Some("fresh error".into());
    app.error_since = Some(Instant::now());
    app.tick_autoclear();
    assert_eq!(app.last_error.as_deref(), Some("fresh error"));
    assert!(app.error_since.is_some());
}

#[test]
fn test_tick_autoclear_noop_when_no_error() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.tick_autoclear();
    assert!(app.last_error.is_none());
    assert!(app.error_since.is_none());
}

#[test]
fn test_adjust_file_priority_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.detail_torrent = Some(Torrent {
        id: 1,
        file_stats: vec![FileStats {
            wanted: true,
            priority: 0,
            bytes_completed: 0,
        }],
        ..Default::default()
    });
    app.view = View::Files;
    app.handle_files_key(make_key(KeyCode::Char('+'), KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_complete_location_local_uses_filesystem() {
    // localhost → ssh_host() returns None → falls back to local autocomplete.
    let mut app = App::new(
        TransmissionClient::new("http://localhost:9091/rpc", None, None),
        Config::default(),
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("only_match")).unwrap();
    let input = format!("{}/only", dir.path().to_str().unwrap());
    let result = app.complete_location(&input);
    assert!(result.is_some());
    assert!(result.unwrap().contains("only_match"));
}

#[test]
fn test_complete_location_remote_cache_hit() {
    // Remote app with a pre-populated cache → returns from cache without SSH.
    let mut app = App::new(
        TransmissionClient::new("http://remotehost:9091/rpc", None, None),
        Config::default(),
    );
    app.location_dir_cache = Some((
        "/srv/".to_string(),
        vec!["/srv/downloads".to_string(), "/srv/media".to_string()],
    ));
    let result = app.complete_location("/srv/d");
    assert_eq!(result, Some("/srv/downloads/".to_string()));
}

#[test]
fn test_complete_location_remote_error_cache_prevents_retry() {
    // When SSH previously failed, location_dir_cache is set to Some((dir, [])).
    // A second complete_location call for the same dir must use the cache
    // (no SSH) and return None (no completions available).
    let mut app = App::new(
        TransmissionClient::new("http://remotehost:9091/rpc", None, None),
        Config::default(),
    );
    // Pre-populate the cache as if a prior SSH call already failed.
    app.location_dir_cache = Some(("/srv/".to_string(), vec![]));

    let result = app.complete_location("/srv/data");
    // Cache hit: empty listing → no completions, no new SSH call.
    assert_eq!(result, None);
    // Cache must remain intact (not overwritten by a retry).
    assert_eq!(app.location_dir_cache, Some(("/srv/".to_string(), vec![])));
}

#[test]
fn test_complete_location_remote_ssh_error_sets_last_error() {
    let mut app = App::new(
        TransmissionClient::new("http://remotehost:9091/rpc", None, None),
        Config::default(),
    );
    app.remote_dir_lister = |host, dir| {
        assert_eq!((host, dir), ("remotehost", "/srv/"));
        Err("Permission denied (publickey).".into())
    };

    assert_eq!(app.complete_location("/srv/"), None);

    assert_eq!(
        app.last_error.as_deref(),
        Some("SSH directory listing failed: Permission denied (publickey).")
    );
    assert!(app.error_since.is_some());
    assert_eq!(app.location_dir_cache, Some(("/srv/".to_string(), vec![])));
}

#[test]
fn test_complete_location_remote_success_populates_cache() {
    let mut app = App::new(
        TransmissionClient::new("http://remotehost:9091/rpc", None, None),
        Config::default(),
    );
    app.remote_dir_lister = |host, dir| {
        assert_eq!((host, dir), ("remotehost", "/srv/"));
        Ok(vec!["/srv/media".into(), "/srv/archive".into()])
    };

    assert_eq!(app.complete_location("/srv/me"), Some("/srv/media/".into()));
    assert_eq!(
        app.location_dir_cache,
        Some((
            "/srv/".into(),
            vec!["/srv/media".into(), "/srv/archive".into()]
        ))
    );
}

#[test]
fn test_complete_location_remote_cache_miss_then_hit() {
    // After a cache-miss SSH call populates the cache, a second call with the
    // same parent dir uses the cache and does not re-invoke SSH.
    let mut app = App::new(
        TransmissionClient::new("http://remotehost:9091/rpc", None, None),
        Config::default(),
    );
    app.location_dir_cache = Some((
        "/data/".to_string(),
        vec!["/data/archive".to_string(), "/data/active".to_string()],
    ));
    // Both completions should come from the cache (same parent "/data/").
    let r1 = app.complete_location("/data/ar");
    let r2 = app.complete_location("/data/ac");
    assert_eq!(r1, Some("/data/archive/".to_string()));
    assert_eq!(r2, Some("/data/active/".to_string()));
}

fn app_for_server(server: &ScriptedServer) -> App {
    App::new(
        TransmissionClient::new(&server.url, None, Some(2)),
        Config::default(),
    )
}

fn local_app() -> App {
    App::new(
        TransmissionClient::new("http://localhost:9091/transmission/rpc", None, None),
        Config::default(),
    )
}

#[test]
fn adjust_file_priority_sends_selected_transitions_and_refreshes_detail() {
    let server = ScriptedServer::start(vec![
        success(serde_json::json!({})),
        success(serde_json::json!({"torrents": [{
            "id": 17,
            "fileStats": [
                {"wanted": true, "priority": 0},
                {"wanted": false, "priority": 0}
            ]
        }]})),
        success(serde_json::json!({})),
        success(serde_json::json!({"torrents": [{
            "id": 17,
            "fileStats": [{"wanted": true, "priority": -1}]
        }]})),
    ]);
    let mut app = app_for_server(&server);
    app.detail_torrent = Some(Torrent {
        id: 17,
        file_stats: vec![
            FileStats {
                wanted: true,
                priority: -1,
                ..Default::default()
            },
            FileStats {
                wanted: true,
                priority: 1,
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    app.file_selected.extend([0, 1]);

    app.adjust_file_priority(true);

    let mutation = server.request();
    assert_eq!(mutation.method(), "torrent-set");
    assert_eq!(
        mutation.arguments(),
        &serde_json::json!({
            "ids": [17],
            "priority-normal": [0],
            "files-wanted": [0],
            "files-unwanted": [1]
        })
    );
    let refresh = server.request();
    assert_eq!(refresh.method(), "torrent-get");
    assert_eq!(refresh.arguments()["ids"], serde_json::json!([17]));
    assert!(app.file_selected.is_empty());
    assert!(!app.detail_torrent.as_ref().unwrap().file_stats[1].wanted);

    app.file_selected.insert(0);
    app.adjust_file_priority(false);
    assert_eq!(
        server.request().arguments(),
        &serde_json::json!({
            "ids": [17],
            "priority-low": [0],
            "files-wanted": [0]
        })
    );
    assert_eq!(server.request().method(), "torrent-get");
    assert_eq!(
        app.detail_torrent.as_ref().unwrap().file_stats[0].priority,
        -1
    );
}

#[test]
fn toggle_file_wanted_sends_inverse_states_for_selected_files() {
    let server = ScriptedServer::start(vec![
        success(serde_json::json!({})),
        success(serde_json::json!({"torrents": [{"id": 21}]})),
    ]);
    let mut app = app_for_server(&server);
    app.detail_torrent = Some(Torrent {
        id: 21,
        file_stats: vec![
            FileStats {
                wanted: true,
                priority: 0,
                ..Default::default()
            },
            FileStats {
                wanted: false,
                priority: 0,
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    app.file_selected.extend([0, 1]);

    app.toggle_file_wanted();

    assert_eq!(
        server.request().arguments(),
        &serde_json::json!({
            "ids": [21],
            "priority-normal": [1],
            "files-wanted": [1],
            "files-unwanted": [0]
        })
    );
    assert_eq!(server.request().method(), "torrent-get");
    assert!(app.file_selected.is_empty());
}

#[test]
fn toggle_file_wanted_preserves_selection_when_rpc_fails() {
    let server = ScriptedServer::start(vec![crate::test_support::Response::status(
        503,
        "Service Unavailable",
    )]);
    let mut app = app_for_server(&server);
    app.detail_torrent = Some(Torrent {
        id: 21,
        file_stats: vec![FileStats {
            wanted: true,
            priority: 0,
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.toggle_file_wanted();

    assert_eq!(
        app.last_error.as_deref(),
        Some("HTTP 503 Service Unavailable")
    );
    assert_eq!(app.file_selected, BTreeSet::from([0]));
    server.request();
}

#[test]
fn file_mutations_are_noops_without_a_detail_or_valid_file() {
    let mut app = empty_app();
    app.adjust_file_priority(true);
    app.toggle_file_wanted();
    assert!(app.last_error.is_none());

    app.detail_torrent = Some(Torrent {
        id: 1,
        file_stats: vec![],
        ..Default::default()
    });
    app.file_cursor = 4;
    app.adjust_file_priority(false);
    app.toggle_file_wanted();
    assert!(app.last_error.is_none());

    let mut local = local_app();
    local.delete_files_from_disk();
    assert!(local.last_error.is_none());
}

#[test]
fn delete_files_from_disk_removes_safe_targets_and_reports_rejected_paths() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    let outside = dir.path().join("outside.txt");
    std::fs::create_dir(&downloads).unwrap();
    std::fs::write(downloads.join("remove.txt"), "remove me").unwrap();
    std::fs::write(&outside, "must remain").unwrap();

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: downloads.to_string_lossy().into_owned(),
        files: vec![
            TorrentFile {
                name: "remove.txt".into(),
                ..Default::default()
            },
            TorrentFile {
                name: "../outside.txt".into(),
                ..Default::default()
            },
            TorrentFile {
                name: "missing.txt".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    app.file_selected.extend([0, 1, 2]);

    app.delete_files_from_disk();

    assert!(!downloads.join("remove.txt").exists());
    assert!(outside.exists());
    let error = app.last_error.as_deref().unwrap();
    assert!(error.contains("unsafe path rejected"), "{error}");
    assert!(error.contains("missing.txt"), "{error}");
    assert!(app.file_selected.is_empty());
}

#[test]
fn delete_files_from_disk_rejects_unknown_download_directory() {
    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        files: vec![TorrentFile {
            name: "file.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });

    app.delete_files_from_disk();

    assert_eq!(
        app.last_error.as_deref(),
        Some("unknown download directory")
    );
}

#[test]
fn location_completion_falls_back_to_distinct_known_torrent_directories() {
    let mut local = local_app();
    local.torrents = vec![
        Torrent {
            download_dir: "/known/archive".into(),
            ..Default::default()
        },
        Torrent {
            download_dir: "/known/archive".into(),
            ..Default::default()
        },
        Torrent::default(),
    ];
    assert_eq!(
        local.complete_location("/known/ar"),
        Some("/known/archive/".into())
    );

    let mut remote = App::new(
        TransmissionClient::new("http://remote.example:9091/rpc", None, None),
        Config::default(),
    );
    remote.torrents = local.torrents;
    remote.location_dir_cache = Some(("/known/".into(), vec![]));
    assert_eq!(
        remote.complete_location("/known/ar"),
        Some("/known/archive/".into())
    );
}
