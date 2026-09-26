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
fn test_set_error_403_discards_pending_credentials() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.pending_credentials_save = Some(("alice".into(), "secret".into()));

    app.set_error("HTTP 403 Forbidden");

    assert!(app.pending_credentials_save.is_none());
    assert_eq!(app.last_error.as_deref(), Some("HTTP 403 Forbidden"));
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
fn test_complete_location_non_empty_no_common_prefix_branches() {
    let dir = tempfile::tempdir().expect("tempdir");
    let base = dir.path().to_str().expect("to_str");
    std::fs::create_dir(dir.path().join("dir1")).expect("create_dir dir1");
    std::fs::create_dir(dir.path().join("dir2")).expect("create_dir dir2");

    let mut app = local_app();
    app.torrents = vec![Torrent {
        download_dir: "/known/dir".into(),
        ..Default::default()
    }];

    // fs_matches non-empty ("dir1", "dir2"), autocomplete_path returns None -> returns None
    let input = format!("{}/dir", base);
    assert_eq!(app.complete_location(&input), None);

    // Remote lister non-empty ("alpha", "beta") with no common prefix beyond "/remote/" -> returns None
    let mut remote_app = App::new(
        TransmissionClient::new("http://remote.host:9091/transmission/rpc", None, None),
        Config::default(),
    );
    remote_app.remote_dir_lister = |_, _| Ok(vec!["/remote/alpha".into(), "/remote/beta".into()]);
    assert_eq!(remote_app.complete_location("/remote/"), None);
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

#[cfg(unix)]
#[test]
fn delete_files_from_disk_succeeds_with_symlinked_download_directory() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let real_downloads = dir.path().join("real-downloads");
    let downloads = dir.path().join("downloads");
    std::fs::create_dir(&real_downloads).unwrap();
    std::fs::write(real_downloads.join("target.txt"), "remove me").unwrap();
    symlink(&real_downloads, &downloads).unwrap();

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: downloads.to_string_lossy().into_owned(),
        files: vec![TorrentFile {
            name: "target.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert!(!real_downloads.join("target.txt").exists());
    assert!(app.last_error.is_none());
}

#[cfg(unix)]
#[test]
fn delete_files_from_disk_rejects_symlinked_ancestor() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&downloads).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("valuable.txt"), "must remain").unwrap();
    symlink(&outside, downloads.join("subdir")).unwrap();

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: downloads.to_string_lossy().into_owned(),
        files: vec![TorrentFile {
            name: "subdir/valuable.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert!(outside.join("valuable.txt").exists());
    assert!(app.last_error.is_some());
}

#[cfg(unix)]
#[test]
fn delete_files_from_disk_unlinks_final_symlink_without_touching_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    let outside = dir.path().join("outside.txt");
    std::fs::create_dir(&downloads).unwrap();
    std::fs::write(&outside, "must remain").unwrap();
    symlink(&outside, downloads.join("link.txt")).unwrap();

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: downloads.to_string_lossy().into_owned(),
        files: vec![TorrentFile {
            name: "link.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert!(!downloads.join("link.txt").exists());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "must remain");
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
fn delete_files_from_disk_canonicalizes_download_directory_with_parent_component() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    std::fs::write(downloads.join("keep.txt"), "remove me").unwrap();
    let parent_component_path = format!("{}/downloads/../downloads", dir.path().display());

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: parent_component_path,
        files: vec![TorrentFile {
            name: "keep.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert!(!downloads.join("keep.txt").exists());
    assert!(app.last_error.is_none());
    assert!(app.file_selected.is_empty());
}

#[cfg(unix)]
#[test]
fn delete_files_from_disk_rejects_invalid_relative_paths_directly() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().to_string_lossy().into_owned();

    for file_name in ["../outside.txt", ""] {
        let error = remove_torrent_file_without_symlink_ancestors(&downloads, file_name)
            .expect_err("unsafe/empty relative paths must be rejected");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[cfg(unix)]
#[test]
fn delete_files_from_disk_requires_an_absolute_download_root() {
    let error = remove_torrent_file_without_symlink_ancestors("relative/downloads", "file.txt")
        .expect_err("relative download roots must be rejected");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[cfg(unix)]
#[test]
fn canonical_root_components_reject_noncanonical_parent_components() {
    let error = canonical_root_components(std::path::Path::new("/tmp/../var"))
        .expect_err("a root containing ParentDir must fail closed");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
}

#[cfg(unix)]
#[test]
fn delete_files_from_disk_propagates_root_open_failure_without_deleting() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("keep.txt");
    std::fs::write(&target, "keep me").unwrap();
    let root = dir.path().to_string_lossy();

    let error = remove_torrent_file_unix_with_root_opener(&root, "keep.txt", || {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "injected root-open failure",
        ))
    })
    .expect_err("root-open failure must abort deletion");

    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep me");
}

#[test]
fn delete_files_from_disk_reports_missing_download_directory() {
    let dir = tempfile::tempdir().unwrap();
    let missing_downloads = dir.path().join("not-created");

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: missing_downloads.to_string_lossy().into_owned(),
        files: vec![TorrentFile {
            name: "file.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert!(app.last_error.as_deref().is_some_and(|error| {
        error.contains("file.txt") && !error.contains("unsafe path rejected")
    }));
    assert!(app.file_selected.is_empty());
}

#[test]
fn delete_files_from_disk_does_not_unlink_directory_target() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    let target = downloads.join("nested");
    std::fs::create_dir(&target).unwrap();

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: downloads.to_string_lossy().into_owned(),
        files: vec![TorrentFile {
            name: "nested".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert!(target.is_dir());
    assert!(app.last_error.as_deref().is_some_and(|error| {
        error.contains("nested") && !error.contains("unsafe path rejected")
    }));
    assert!(app.file_selected.is_empty());
}

#[test]
fn delete_files_from_disk_rejects_nul_byte_in_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    std::fs::create_dir(&downloads).unwrap();

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: downloads.to_string_lossy().into_owned(),
        files: vec![TorrentFile {
            name: "bad\0name.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert!(
        app.last_error
            .as_deref()
            .is_some_and(|error| { error.contains("path contains NUL byte") })
    );
    assert!(app.file_selected.is_empty());
}

#[test]
fn delete_files_from_disk_reports_non_directory_ancestor_without_deleting_outside() {
    let dir = tempfile::tempdir().unwrap();
    let downloads = dir.path().join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    std::fs::write(downloads.join("not-a-directory"), "keep me").unwrap();

    let mut app = local_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        download_dir: downloads.to_string_lossy().into_owned(),
        files: vec![TorrentFile {
            name: "not-a-directory/child.txt".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    app.file_selected.insert(0);

    app.delete_files_from_disk();

    assert_eq!(
        std::fs::read_to_string(downloads.join("not-a-directory")).unwrap(),
        "keep me"
    );
    assert!(app.last_error.as_deref().is_some_and(|error| {
        error.contains("not-a-directory/child.txt") && !error.contains("unsafe path rejected")
    }));
    assert!(app.file_selected.is_empty());
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
