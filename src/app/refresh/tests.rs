#![allow(unused_imports)]
use super::*;
use crate::client::TransmissionClient;
use crate::config::Config;
use crate::protocol::{FreeSpace, SessionStats, Torrent, TrackerStats};
use crate::test_support::{Response, ScriptedServer, success};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

fn make_app() -> App {
    App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    )
}

fn app_for_server(server: &ScriptedServer) -> App {
    App::new(
        TransmissionClient::new(&server.url, None, Some(2)),
        Config::default(),
    )
}

#[test]
fn refresh_detail_missing_torrent_clears_file_state() {
    let server = ScriptedServer::start(vec![success(serde_json::json!({"torrents": []}))]);
    let mut app = app_for_server(&server);
    app.view = View::Files;
    app.detail_torrent = Some(Torrent {
        id: 42,
        name: "some-torrent".into(),
        ..Default::default()
    });
    app.file_cursor = 3;
    app.file_selected.extend([1, 2]);

    app.refresh_detail();

    let request = server.request();
    assert_eq!(request.method(), "torrent-get");
    assert_eq!(request.arguments()["ids"], serde_json::json!([42]));
    assert!(app.detail_torrent.is_none());
    assert_eq!(app.file_cursor, 0);
    assert!(app.file_selected.is_empty());
    assert_eq!(app.view, View::TorrentList);
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

#[test]
fn refresh_torrents_sorts_filters_and_clears_stale_error() {
    let server = ScriptedServer::start(vec![success(serde_json::json!({"torrents": [
        {"id": 2, "name": "Zulu"},
        {"id": 1, "name": "Alpha"}
    ]}))]);
    let mut app = app_for_server(&server);
    app.sort_column = SortColumn::Name;
    app.sort_ascending = true;
    app.cursor = 9;
    app.last_error = Some("stale".into());
    app.error_since = Some(Instant::now());

    app.refresh_torrents();

    assert_eq!(
        app.torrents
            .iter()
            .map(|torrent| torrent.id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(app.cursor, 1);
    assert!(app.last_error.is_none());
    assert!(app.error_since.is_none());
    assert_eq!(server.request().method(), "torrent-get");
}

#[test]
fn synchronous_refreshes_surface_transport_errors() {
    let torrent_server = ScriptedServer::start(vec![Response::status(500, "Broken")]);
    let mut app = app_for_server(&torrent_server);
    app.refresh_torrents();
    assert_eq!(
        app.last_error.as_deref(),
        Some("HTTP 500 Internal Server Error")
    );
    torrent_server.request();

    let detail_server = ScriptedServer::start(vec![Response::status(502, "Bad Gateway")]);
    let mut app = app_for_server(&detail_server);
    app.detail_torrent = Some(Torrent {
        id: 8,
        ..Default::default()
    });
    app.refresh_detail();
    assert_eq!(app.last_error.as_deref(), Some("HTTP 502 Bad Gateway"));
    detail_server.request();
}

#[test]
fn refresh_detail_updates_torrent_and_clamps_file_cursor() {
    let server = ScriptedServer::start(vec![success(serde_json::json!({"torrents": [{
        "id": 8,
        "name": "updated",
        "files": [{"name": "only-file", "length": 10}],
        "fileStats": [{"wanted": true, "priority": 0}]
    }]}))]);
    let mut app = app_for_server(&server);
    app.detail_torrent = Some(Torrent {
        id: 8,
        ..Default::default()
    });
    app.file_cursor = 7;

    app.refresh_detail();

    assert_eq!(app.detail_torrent.as_ref().unwrap().name, "updated");
    assert_eq!(app.file_cursor, 0);
    assert_eq!(server.request().arguments()["ids"], serde_json::json!([8]));
}

fn wait_for_background_refresh(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while app.refresh_in_flight && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.drain_results();
    }
    assert!(!app.refresh_in_flight, "background refresh did not finish");
}

#[test]
fn background_torrent_refresh_discovers_directory_stats_and_free_space() {
    let server = ScriptedServer::start(vec![
        success(serde_json::json!({"torrents": [{"id": 4, "name": "Linux"}]})),
        success(serde_json::json!({"torrentCount": 1, "downloadSpeed": 25})),
        success(serde_json::json!({"torrents": [{"id": 4, "downloadDir": "/downloads"}]})),
        success(serde_json::json!({"path": "/downloads", "size-bytes": 4096})),
    ]);
    let mut app = app_for_server(&server);

    app.trigger_refresh();
    wait_for_background_refresh(&mut app);

    assert_eq!(app.torrents[0].name, "Linux");
    assert_eq!(app.stats.as_ref().unwrap().torrent_count, 1);
    assert_eq!(app.default_download_dir.as_deref(), Some("/downloads"));
    assert_eq!(app.free.as_ref().unwrap().size_bytes, 4096);
    let requests = (0..4).map(|_| server.request()).collect::<Vec<_>>();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.method())
            .collect::<Vec<_>>(),
        ["torrent-get", "session-stats", "torrent-get", "free-space"]
    );
    assert_eq!(
        requests[3].arguments(),
        &serde_json::json!({"path": "/downloads"})
    );
}

#[test]
fn background_detail_refresh_uses_existing_directory_and_skips_periodic_free_space() {
    let server = ScriptedServer::start(vec![
        success(serde_json::json!({"torrents": [{"id": 11, "name": "Detail"}]})),
        success(serde_json::json!({"torrentCount": 1})),
    ]);
    let mut app = app_for_server(&server);
    app.view = View::Details;
    app.detail_torrent = Some(Torrent {
        id: 11,
        ..Default::default()
    });
    app.default_download_dir = Some("/existing".into());
    app.free_space_tick = 1;

    app.trigger_refresh();
    wait_for_background_refresh(&mut app);

    assert_eq!(app.detail_torrent.as_ref().unwrap().name, "Detail");
    assert_eq!(app.free_space_tick, 2);
    assert_eq!(server.request().method(), "torrent-get");
    assert_eq!(server.request().method(), "session-stats");
}

#[cfg(feature = "rsync")]
#[test]
fn background_rsync_view_refreshes_torrent_list() {
    let server = ScriptedServer::start(vec![
        success(serde_json::json!({"torrents": [{"id": 3, "name": "Synced"}]})),
        success(serde_json::json!({"torrentCount": 1})),
    ]);
    let mut app = app_for_server(&server);
    app.view = View::Rsync;
    app.default_download_dir = Some("/downloads".into());
    app.free_space_tick = 1;

    app.trigger_refresh();
    wait_for_background_refresh(&mut app);

    assert_eq!(app.torrents[0].name, "Synced");
    assert_eq!(server.request().method(), "torrent-get");
    assert_eq!(server.request().method(), "session-stats");
}
