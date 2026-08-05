#![allow(unused_imports)]
use super::*;
use crate::client::TransmissionClient;
use crate::config::Config;
use crate::protocol::{FreeSpace, SessionStats, Torrent, TrackerStats};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_cursor_movement() {
    let mut cursor = 0;
    let mut selected = std::collections::BTreeSet::new();

    App::move_down(&mut cursor, &mut selected, 5, false);
    assert_eq!(cursor, 1);
    assert!(selected.is_empty());

    App::move_down(&mut cursor, &mut selected, 5, true);
    assert_eq!(cursor, 2);
    assert!(selected.contains(&1));
    assert!(selected.contains(&2));

    App::move_up(&mut cursor, &mut selected, 5, false);
    assert_eq!(cursor, 1);
    assert_eq!(selected.len(), 2);

    App::move_up(&mut cursor, &mut selected, 5, true);
    assert_eq!(cursor, 0);
    assert!(selected.contains(&0));
    assert!(selected.contains(&1));
}

#[test]
fn test_is_safe_relative_path() {
    assert!(is_safe_relative_path("file.txt"));
    assert!(is_safe_relative_path("subdir/file.txt"));
    assert!(!is_safe_relative_path(""));
    assert!(!is_safe_relative_path("../etc/passwd"));
    assert!(!is_safe_relative_path("/absolute/path"));
    assert!(!is_safe_relative_path("./dot/relative"));
}

#[cfg(feature = "rsync")]
#[test]
fn test_r_key_opens_rsync_view() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    assert_eq!(app.view, View::TorrentList);
    app.handle_torrent_list_key(KeyEvent {
        code: KeyCode::Char('R'),
        modifiers: KeyModifiers::SHIFT,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });
    assert_eq!(app.view, View::Rsync);
}

#[cfg(feature = "rsync")]
#[test]
fn test_rsync_view_back_returns_to_list() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Rsync;
    app.handle_rsync_key(KeyEvent {
        code: KeyCode::Esc,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });
    assert_eq!(app.view, View::TorrentList);
}

#[test]
fn test_is_local_daemon_localhost() {
    let app = App::new(
        TransmissionClient::new("http://localhost:9091/transmission/rpc", None, None),
        Config::default(),
    );
    assert!(app.is_local_daemon());
}

#[test]
fn test_is_local_daemon_ipv4_loopback() {
    let app = App::new(
        TransmissionClient::new("http://127.0.0.1:9091/transmission/rpc", None, None),
        Config::default(),
    );
    assert!(app.is_local_daemon());
}

#[test]
fn test_is_local_daemon_ipv6_loopback() {
    let app = App::new(
        TransmissionClient::new("http://[::1]:9091/transmission/rpc", None, None),
        Config::default(),
    );
    assert!(app.is_local_daemon());
}

#[test]
fn test_is_local_daemon_remote_ip() {
    let app = App::new(
        TransmissionClient::new("http://192.168.1.1:9091/transmission/rpc", None, None),
        Config::default(),
    );
    assert!(!app.is_local_daemon());
}

#[test]
fn test_is_local_daemon_remote_hostname() {
    let app = App::new(
        TransmissionClient::new("http://remote.example.com/transmission/rpc", None, None),
        Config::default(),
    );
    assert!(!app.is_local_daemon());
}

#[test]
fn test_sequential_predicate_uses_ids_not_positions() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![
        Torrent {
            id: 10,
            sequential_download: true,
            ..Default::default()
        },
        Torrent {
            id: 20,
            sequential_download: false,
            ..Default::default()
        },
    ];
    app.rebuild_filter();
    app.cursor = 0;
    let ids = app.target_ids();
    assert_eq!(ids, vec![10], "target_ids must resolve to id 10");
    let any_sequential = app
        .torrents
        .iter()
        .filter(|t| ids.contains(&t.id))
        .any(|t| t.sequential_download);
    assert!(
        any_sequential,
        "sequential predicate must be true for id=10 (sequential_download=true)"
    );
    app.selected.insert(99);
    let ids_stale = app.target_ids();
    assert!(
        ids_stale.is_empty(),
        "stale out-of-bounds selected index must yield no target ids"
    );
}

#[test]
fn test_persist_credentials_impl_config_fallback_on_save_error() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.toml");
    persist_credentials_impl(
        "http://test.invalid/rpc",
        "alice",
        "secret",
        &cfg_path,
        |_, _, _| Err("simulated keyring failure".into()),
    );
    let cfg = crate::config::Config::load_from(&cfg_path);
    assert_eq!(cfg.connection.username.as_deref(), Some("alice"));
    assert_eq!(cfg.connection.password.as_deref(), Some("secret"));
}

#[test]
fn test_persist_credentials_impl_no_config_write_on_save_ok() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg_path = tmp.path().join("config.toml");
    persist_credentials_impl(
        "http://test.invalid/rpc",
        "bob",
        "hunter2",
        &cfg_path,
        |_, _, _| Ok(()),
    );
    assert!(
        !cfg_path.exists(),
        "config must not be created when keyring save succeeds"
    );
}

#[test]
fn test_location_parent_dir_trailing_slash() {
    assert_eq!(util::location_parent_dir("/foo/bar/"), "/foo/bar/");
}

#[test]
fn test_location_parent_dir_empty() {
    assert_eq!(util::location_parent_dir(""), "");
}

#[test]
fn test_location_parent_dir_nested() {
    assert_eq!(util::location_parent_dir("/foo/bar"), "/foo/");
}

#[test]
fn test_location_parent_dir_top_level() {
    // "/foo" → parent is "/" → returns "/" (not "//")
    assert_eq!(util::location_parent_dir("/foo"), "/");
}

#[test]
fn test_ssh_host_remote_hostname() {
    let app = App::new(
        TransmissionClient::new(
            "http://myserver.example.com:9091/transmission/rpc",
            None,
            None,
        ),
        Config::default(),
    );
    assert_eq!(app.ssh_host(), Some("myserver.example.com".to_string()));
}

#[test]
fn test_ssh_host_localhost_returns_none() {
    for url in &[
        "http://localhost:9091/transmission/rpc",
        "http://127.0.0.1:9091/transmission/rpc",
        "http://[::1]:9091/transmission/rpc",
    ] {
        let app = App::new(TransmissionClient::new(url, None, None), Config::default());
        assert_eq!(app.ssh_host(), None, "expected None for {url}");
    }
}

#[test]
fn test_ssh_host_flag_shaped_rejected() {
    let app = App::new(
        TransmissionClient::new("http://-oProxyCommand=evil:9091/rpc", None, None),
        Config::default(),
    );
    assert_eq!(
        app.ssh_host(),
        None,
        "flag-shaped hostname must be rejected"
    );
}

#[test]
fn test_ssh_host_https_scheme() {
    let app = App::new(
        TransmissionClient::new("https://remote.host/transmission/rpc", None, None),
        Config::default(),
    );
    assert_eq!(app.ssh_host(), Some("remote.host".to_string()));
}

#[test]
fn test_location_parent_dir_relative_no_slash() {
    // A path with no directory component maps to the root cache key.
    assert_eq!(util::location_parent_dir("foo"), "/");
}

#[test]
fn sort_column_labels_are_stable_user_facing_names() {
    assert_eq!(
        [
            SortColumn::Name,
            SortColumn::Size,
            SortColumn::Progress,
            SortColumn::Down,
            SortColumn::Up,
            SortColumn::Eta,
            SortColumn::Ratio,
            SortColumn::Status,
            SortColumn::Queue,
        ]
        .map(SortColumn::label),
        [
            "name", "size", "progress", "down", "up", "eta", "ratio", "status", "queue"
        ]
    );
}

#[test]
fn malformed_or_scheme_less_urls_are_not_treated_as_local_daemons() {
    for url in ["localhost:9091", "", "://missing-scheme"] {
        let app = App::new(TransmissionClient::new(url, None, None), Config::default());
        assert!(!app.is_local_daemon(), "{url:?} must not be assumed local");
        assert_eq!(app.ssh_host(), None);
    }
}

#[test]
fn userinfo_cannot_disguise_a_remote_daemon_as_local() {
    let remote = App::new(
        TransmissionClient::new(
            "http://localhost:secret@remote.example:9091/transmission/rpc",
            None,
            None,
        ),
        Config::default(),
    );
    assert!(!remote.is_local_daemon());
    assert_eq!(remote.ssh_host().as_deref(), Some("remote.example"));

    let local = App::new(
        TransmissionClient::new(
            "http://user:secret@LOCALHOST:9091/transmission/rpc",
            None,
            None,
        ),
        Config::default(),
    );
    assert!(local.is_local_daemon());
    assert_eq!(local.ssh_host(), None);
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_is_safe_relative_path_rejects_unsafe(path in ".*") {
        let is_safe = super::is_safe_relative_path(&path);

        if path.is_empty() {
            assert!(!is_safe);
        } else if path.starts_with('/') {
            assert!(!is_safe);
        } else if path.contains("..") {
            // Not strictly true for file names like `foo..bar`, but `Component::ParentDir` checking is robust.
            // Let's just check standard unsafe things
            if path == ".." || path.starts_with("../") || path.contains("/../") || path.ends_with("/..") {
                assert!(!is_safe);
            }
        } else if path.starts_with("./") || path == "." {
            assert!(!is_safe);
        }
    }
}

proptest! {
    #[test]
    fn prop_location_parent_dir_never_panics(path in ".*") {
        let result = crate::util::location_parent_dir(&path);
        // It should either return the path as is (if empty or ends with /)
        // or a parent directory with a trailing slash.
        if path.is_empty() || path.ends_with('/') {
            assert_eq!(result, path);
        } else {
            assert!(result.ends_with('/'));
        }
    }
}
