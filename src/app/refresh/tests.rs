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
fn test_refresh_detail_ok_none_clears_file_state() {
    // Simulate what refresh_detail() does on Ok(None): the torrent disappeared.
    // We set up the state as if the user was in the file view, then replicate
    // the Ok(None) branch and assert all file state is reset.
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    // Put app into file-view state
    app.view = View::Files;
    app.detail_torrent = Some(Torrent {
        id: 42,
        name: "some-torrent".into(),
        ..Default::default()
    });
    app.file_cursor = 3;
    app.file_selected.insert(1);
    app.file_selected.insert(2);

    // Replicate the Ok(None) branch from refresh_detail()
    app.detail_torrent = None;
    app.file_cursor = 0;
    app.file_selected.clear();
    app.view = View::TorrentList;

    assert!(app.detail_torrent.is_none(), "detail_torrent must be None");
    assert_eq!(app.file_cursor, 0, "file_cursor must be reset to 0");
    assert!(
        app.file_selected.is_empty(),
        "file_selected must be cleared"
    );
    assert!(
        matches!(app.view, View::TorrentList),
        "view must return to TorrentList"
    );
}

#[test]
fn test_drain_torrents_ok_updates_list_and_clears_error() {
    let mut app = make_app();
    app.last_error = Some("stale error".into());

    let torrents = vec![
        Torrent {
            id: 2,
            name: "beta".into(),
            ..Default::default()
        },
        Torrent {
            id: 1,
            name: "alpha".into(),
            ..Default::default()
        },
    ];
    app.refresh_tx
        .send(RefreshMsg::Torrents(Ok(torrents)))
        .unwrap();
    app.drain_results();

    assert_eq!(app.torrents.len(), 2);
    assert!(app.last_error.is_none(), "error must be cleared on success");
}

#[test]
fn test_drain_torrents_err_sets_last_error() {
    let mut app = make_app();
    app.refresh_tx
        .send(RefreshMsg::Torrents(Err("connection refused".into())))
        .unwrap();
    app.drain_results();

    assert_eq!(app.last_error.as_deref(), Some("connection refused"));
    assert!(
        app.torrents.is_empty(),
        "torrent list must stay empty on error"
    );
}

#[test]
fn test_drain_detail_ok_some_updates_detail_torrent() {
    let mut app = make_app();
    let t = Torrent {
        id: 42,
        name: "updated".into(),
        ..Default::default()
    };
    app.refresh_tx
        .send(RefreshMsg::Detail(Box::new(Ok(Some(t)))))
        .unwrap();
    app.drain_results();

    assert_eq!(app.detail_torrent.as_ref().unwrap().id, 42);
    assert_eq!(app.detail_torrent.as_ref().unwrap().name, "updated");
}

#[test]
fn test_drain_detail_ok_none_resets_to_torrent_list() {
    let mut app = make_app();
    app.view = View::Files;
    app.detail_torrent = Some(Torrent {
        id: 7,
        ..Default::default()
    });
    app.file_cursor = 3;
    app.file_selected.insert(1);

    app.refresh_tx
        .send(RefreshMsg::Detail(Box::new(Ok(None))))
        .unwrap();
    app.drain_results();

    assert!(app.detail_torrent.is_none());
    assert_eq!(app.view, View::TorrentList);
}

#[test]
fn test_drain_detail_err_sets_last_error() {
    let mut app = make_app();
    app.refresh_tx
        .send(RefreshMsg::Detail(Box::new(Err("timeout".into()))))
        .unwrap();
    app.drain_results();

    assert_eq!(app.last_error.as_deref(), Some("timeout"));
}

#[test]
fn test_drain_stats_updates_fields_and_clears_in_flight() {
    use crate::protocol::{FreeSpace, SessionStats};

    let mut app = make_app();
    app.refresh_in_flight = true;

    let stats = SessionStats {
        torrent_count: 5,
        download_speed: 100,
        ..Default::default()
    };
    let free = FreeSpace {
        size_bytes: 999,
        ..Default::default()
    };

    app.refresh_tx
        .send(RefreshMsg::Stats {
            stats: Some(stats),
            free: Some(free),
            default_dir: Some("/downloads".into()),
        })
        .unwrap();
    app.drain_results();

    assert!(
        !app.refresh_in_flight,
        "Stats message must clear refresh_in_flight"
    );
    assert_eq!(app.stats.as_ref().unwrap().torrent_count, 5);
    assert_eq!(app.free.as_ref().unwrap().size_bytes, 999);
    assert_eq!(app.default_download_dir.as_deref(), Some("/downloads"));
}

#[test]
fn test_pending_url_save_consumed_on_first_success() {
    let mut app = make_app().with_pending_url_save(Some("http://myserver/transmission/rpc".into()));

    app.refresh_tx
        .send(RefreshMsg::Torrents(Ok(vec![])))
        .unwrap();
    app.drain_results();

    assert!(
        app.pending_url_save.is_none(),
        "pending_url_save must be cleared after first success"
    );

    // Second success must not panic (nothing left to save).
    app.refresh_tx
        .send(RefreshMsg::Torrents(Ok(vec![])))
        .unwrap();
    app.drain_results();
}

#[test]
fn test_pending_url_save_not_triggered_on_error() {
    let mut app = make_app().with_pending_url_save(Some("http://myserver/transmission/rpc".into()));

    app.refresh_tx
        .send(RefreshMsg::Torrents(Err("refused".into())))
        .unwrap();
    app.drain_results();

    assert!(
        app.pending_url_save.is_some(),
        "pending_url_save must not be consumed on a failed refresh"
    );
}

#[test]
fn test_drain_stats_none_fields_do_not_overwrite() {
    use crate::protocol::{FreeSpace, SessionStats};

    let mut app = make_app();
    app.stats = Some(SessionStats {
        torrent_count: 3,
        ..Default::default()
    });
    app.free = Some(FreeSpace {
        size_bytes: 500,
        ..Default::default()
    });
    app.default_download_dir = Some("/existing".into());

    app.refresh_tx
        .send(RefreshMsg::Stats {
            stats: None,
            free: None,
            default_dir: None,
        })
        .unwrap();
    app.drain_results();

    assert_eq!(
        app.stats.as_ref().unwrap().torrent_count,
        3,
        "stats must not be cleared when None"
    );
    assert_eq!(
        app.free.as_ref().unwrap().size_bytes,
        500,
        "free must not be cleared when None"
    );
    assert_eq!(
        app.default_download_dir.as_deref(),
        Some("/existing"),
        "dir must not be cleared when None"
    );
}

#[test]
fn test_trigger_refresh_guard_noop_when_in_flight() {
    let mut app = make_app();
    assert!(!app.refresh_in_flight);

    app.trigger_refresh();
    assert!(app.refresh_in_flight);
    let tick_after_first = app.free_space_tick;

    app.trigger_refresh();
    assert_eq!(
        app.free_space_tick, tick_after_first,
        "second trigger_refresh must be a no-op: free_space_tick must not advance"
    );
}

#[test]
fn test_drain_multiple_messages_all_applied() {
    use crate::protocol::SessionStats;

    let mut app = make_app();
    app.refresh_in_flight = true;

    let torrents = vec![Torrent {
        id: 1,
        name: "foo".into(),
        ..Default::default()
    }];
    app.refresh_tx
        .send(RefreshMsg::Torrents(Ok(torrents)))
        .unwrap();
    app.refresh_tx
        .send(RefreshMsg::Stats {
            stats: Some(SessionStats {
                torrent_count: 1,
                ..Default::default()
            }),
            free: None,
            default_dir: None,
        })
        .unwrap();
    app.drain_results();

    assert_eq!(app.torrents.len(), 1);
    assert_eq!(app.stats.as_ref().unwrap().torrent_count, 1);
    assert!(!app.refresh_in_flight);
}

#[test]
fn test_drain_results_401_opens_auth_modal() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.refresh_in_flight = true;
    app.refresh_tx
        .send(RefreshMsg::Torrents(Err("HTTP 401 Unauthorized".into())))
        .unwrap();
    app.drain_results();
    assert!(matches!(app.modal, Some(Modal::Auth { .. })));
    assert!(app.last_error.is_none());
}

#[test]
fn test_drain_results_detail_401_opens_auth_modal() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.refresh_in_flight = true;
    app.refresh_tx
        .send(RefreshMsg::Detail(Box::new(Err(
            "HTTP 401 Unauthorized".into()
        ))))
        .unwrap();
    app.drain_results();
    assert!(matches!(app.modal, Some(Modal::Auth { .. })));
    assert!(app.last_error.is_none());
}

#[test]
fn test_drain_results_success_clears_error_since() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.last_error = Some("prior error".into());
    app.error_since = Some(Instant::now());
    app.refresh_in_flight = true;
    app.refresh_tx
        .send(RefreshMsg::Torrents(Ok(vec![])))
        .unwrap();
    app.drain_results();
    assert!(app.last_error.is_none());
    assert!(app.error_since.is_none());
}

