#![allow(unused_imports)]
use super::*;
use crate::client::TransmissionClient;
use crate::config::Config;
use crate::protocol::{Torrent, SessionStats, FreeSpace, TrackerStats};
use crossterm::event::{KeyCode, KeyModifiers, KeyEvent};
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

fn torrent_in_list(app: &mut App) {
    app.torrents = vec![Torrent {
        id: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
}

fn make_app() -> App {
    App::new(
        TransmissionClient::new("http://dummy", None, None),
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
    // Install a fake `ssh` that immediately exits non-zero so this test
    // does not require network access.
    use std::io::Write;
    let fake_dir = tempfile::tempdir().unwrap();
    let fake_ssh = fake_dir.path().join("ssh");
    {
        let mut f = std::fs::File::create(&fake_ssh).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "echo 'Permission denied (publickey).' >&2").unwrap();
        writeln!(f, "exit 255").unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let original_path = std::env::var("PATH").unwrap_or_default();
    // SAFETY: single-threaded test; no other threads read PATH concurrently.
    unsafe {
        std::env::set_var(
            "PATH",
            format!("{}:{}", fake_dir.path().display(), original_path),
        );
    }

    let mut app = App::new(
        TransmissionClient::new("http://remotehost:9091/rpc", None, None),
        Config::default(),
    );
    let _result = app.complete_location("/srv/");

    // SAFETY: restoring the value we read above.
    unsafe { std::env::set_var("PATH", original_path) };

    assert!(
        app.last_error.is_some(),
        "SSH failure should set last_error"
    );
    assert!(
        app.error_since.is_some(),
        "SSH failure should set error_since"
    );
    // Cache populated with empty listing so repeated presses don't retry.
    assert_eq!(app.location_dir_cache, Some(("/srv/".to_string(), vec![])));
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

