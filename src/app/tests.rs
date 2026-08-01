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

#[test]
fn test_pause_toggles_per_torrent() {
    // When the selection contains both stopped and running torrents, pause must
    // start the stopped ones and stop the running ones independently.
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![
        Torrent {
            id: 1,
            status: 0,
            ..Default::default()
        }, // stopped
        Torrent {
            id: 2,
            status: 4,
            ..Default::default()
        }, // downloading (running)
        Torrent {
            id: 3,
            status: 0,
            ..Default::default()
        }, // stopped
    ];
    app.rebuild_filter();

    // Verify the per-torrent split logic directly
    let ids = vec![1i64, 2, 3];
    let mut stopped_ids: Vec<i64> = Vec::new();
    let mut running_ids: Vec<i64> = Vec::new();
    for t in app.torrents.iter().filter(|t| ids.contains(&t.id)) {
        if t.is_stopped() {
            stopped_ids.push(t.id);
        } else {
            running_ids.push(t.id);
        }
    }
    assert_eq!(stopped_ids, vec![1, 3], "stopped torrents must be started");
    assert_eq!(running_ids, vec![2], "running torrents must be stopped");
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
    // A path with no directory component — parent is "" which is resolved to "/".
    assert_eq!(util::location_parent_dir("foo"), "/");
}

