use super::*;
use crate::test_support::{Response, ScriptedServer, success};

fn client(server: &ScriptedServer) -> TransmissionClient {
    TransmissionClient::new(&server.url, None, Some(2))
}

#[test]
fn set_auth_updates_and_replaces_basic_credentials() {
    let client = TransmissionClient::new("http://dummy", Some(("old", "creds")), None);
    client.set_auth("alice", "secret");

    let header = client.auth_header.lock().unwrap().clone().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(header.strip_prefix("Basic ").unwrap())
        .unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), "alice:secret");
}

#[test]
fn rpc_negotiates_session_id_and_preserves_authorization() {
    let server = ScriptedServer::start(vec![
        Response::status(409, "Conflict").header("X-Transmission-Session-Id", "session-42"),
        success(json!({"torrentCount": 3})),
    ]);
    let client = TransmissionClient::new(&server.url, Some(("alice", "secret")), Some(2));

    let stats = client.session_stats().unwrap();
    assert_eq!(stats.torrent_count, 3);

    let first = server.request();
    assert_eq!(first.method(), "session-stats");
    assert!(!first.headers.contains_key("x-transmission-session-id"));
    assert_eq!(
        first.headers.get("authorization").map(String::as_str),
        Some("Basic YWxpY2U6c2VjcmV0")
    );
    let second = server.request();
    assert_eq!(
        second
            .headers
            .get("x-transmission-session-id")
            .map(String::as_str),
        Some("session-42")
    );
}

#[test]
fn rpc_rejects_invalid_session_negotiation_responses() {
    let missing = ScriptedServer::start(vec![Response::status(409, "Conflict")]);
    assert_eq!(
        client(&missing).session_stats().unwrap_err(),
        "409 without session ID header"
    );
    missing.request();

    let empty = ScriptedServer::start(vec![
        Response::status(409, "Conflict").header("X-Transmission-Session-Id", ""),
    ]);
    assert_eq!(
        client(&empty).session_stats().unwrap_err(),
        "malformed session ID in response header"
    );
    empty.request();

    let repeated = ScriptedServer::start(vec![
        Response::status(409, "Conflict").header("X-Transmission-Session-Id", "one"),
        Response::status(409, "Conflict").header("X-Transmission-Session-Id", "two"),
    ]);
    assert_eq!(
        client(&repeated).session_stats().unwrap_err(),
        "session ID negotiation failed"
    );
    repeated.request();
    repeated.request();
}

#[test]
fn rpc_distinguishes_http_json_and_protocol_failures() {
    let http = ScriptedServer::start(vec![Response::status(503, "Service Unavailable")]);
    assert_eq!(
        client(&http).session_stats().unwrap_err(),
        "HTTP 503 Service Unavailable"
    );
    http.request();

    let malformed = ScriptedServer::start(vec![
        Response::status(200, "OK")
            .header("Content-Type", "application/json")
            .body("not json"),
    ]);
    let error = client(&malformed).session_stats().unwrap_err();
    assert!(
        error.contains("expected") || error.contains("JSON"),
        "{error}"
    );
    malformed.request();

    let protocol = ScriptedServer::start(vec![Response::json(json!({
        "result": "permission denied",
        "arguments": {}
    }))]);
    assert_eq!(
        client(&protocol).session_stats().unwrap_err(),
        "RPC error: permission denied"
    );
    protocol.request();
}

#[test]
fn torrent_queries_send_fields_and_deserialize_results() {
    let server = ScriptedServer::start(vec![
        success(json!({"torrents": [
            {"id": 7, "name": "alpha", "downloadDir": "/data"},
            {"id": 8, "name": "beta"}
        ]})),
        success(json!({"torrents": [{"id": 8, "name": "beta"}]})),
        success(json!({"torrents": []})),
    ]);
    let client = client(&server);

    let torrents = client.get_torrents(&["id", "name", "downloadDir"]).unwrap();
    assert_eq!(torrents.len(), 2);
    assert_eq!(torrents[0].download_dir, "/data");
    let all = server.request();
    assert_eq!(all.method(), "torrent-get");
    assert_eq!(
        all.arguments()["fields"],
        json!(["id", "name", "downloadDir"])
    );

    let torrent = client.get_torrent(8, &["id", "name"]).unwrap().unwrap();
    assert_eq!((torrent.id, torrent.name.as_str()), (8, "beta"));
    let one = server.request();
    assert_eq!(one.arguments()["ids"], json!([8]));
    assert_eq!(one.arguments()["fields"], json!(["id", "name"]));

    assert!(client.get_torrent(99, &["id"]).unwrap().is_none());
    server.request();
}

#[test]
fn malformed_torrent_and_stat_payloads_are_reported() {
    let server = ScriptedServer::start(vec![
        success(json!({"torrents": "not-an-array"})),
        success(json!({"torrentCount": "many"})),
        success(json!({"path": "/data", "size-bytes": "huge"})),
    ]);
    let client = client(&server);

    assert!(
        client
            .get_torrents(&["id"])
            .unwrap_err()
            .contains("sequence")
    );
    server.request();
    assert!(client.session_stats().unwrap_err().contains("invalid type"));
    server.request();
    assert!(
        client
            .free_space("/data")
            .unwrap_err()
            .contains("invalid type")
    );
    server.request();
}

#[test]
fn torrent_actions_emit_expected_rpc_payloads() {
    let server = ScriptedServer::start((0..14).map(|_| success(json!({}))).collect());
    let client = client(&server);

    for (method, invoke) in [
        (
            "torrent-start",
            TransmissionClient::start as fn(&TransmissionClient, &[i64]) -> _,
        ),
        ("torrent-stop", TransmissionClient::stop),
        ("torrent-verify", TransmissionClient::verify),
        ("torrent-reannounce", TransmissionClient::reannounce),
    ] {
        invoke(&client, &[2, 5]).unwrap();
        let request = server.request();
        assert_eq!(request.method(), method);
        assert_eq!(request.arguments(), &json!({"ids": [2, 5]}));
    }

    client.remove(&[2], false).unwrap();
    let keep = server.request();
    assert_eq!(keep.method(), "torrent-remove");
    assert_eq!(
        keep.arguments(),
        &json!({"ids": [2], "delete-local-data": false})
    );

    client.remove(&[5], true).unwrap();
    assert_eq!(
        server.request().arguments(),
        &json!({"ids": [5], "delete-local-data": true})
    );

    client.add("magnet:?xt=urn:btih:abc", None).unwrap();
    assert_eq!(
        server.request().arguments(),
        &json!({"filename": "magnet:?xt=urn:btih:abc"})
    );
    client
        .add("https://example.test/file.torrent", Some("/downloads"))
        .unwrap();
    assert_eq!(
        server.request().arguments(),
        &json!({"filename": "https://example.test/file.torrent", "download-dir": "/downloads"})
    );

    client.add_metainfo(b"torrent bytes", None).unwrap();
    assert_eq!(
        server.request().arguments(),
        &json!({"metainfo": "dG9ycmVudCBieXRlcw=="})
    );
    client.add_metainfo(b"data", Some("/incoming")).unwrap();
    assert_eq!(
        server.request().arguments(),
        &json!({"metainfo": "ZGF0YQ==", "download-dir": "/incoming"})
    );

    client.set_sequential(&[9], true).unwrap();
    assert_eq!(
        server.request().arguments(),
        &json!({"ids": [9], "sequential_download": true})
    );
    client.queue_move("queue-move-top", &[9]).unwrap();
    let queue = server.request();
    assert_eq!(queue.method(), "queue-move-top");
    assert_eq!(queue.arguments(), &json!({"ids": [9]}));

    client
        .set_labels(&[9], &["linux".into(), "iso".into()])
        .unwrap();
    assert_eq!(
        server.request().arguments(),
        &json!({"ids": [9], "labels": ["linux", "iso"]})
    );
    client.set_location(&[9], "/archive", true).unwrap();
    let location = server.request();
    assert_eq!(location.method(), "torrent-set-location");
    assert_eq!(
        location.arguments(),
        &json!({"ids": [9], "location": "/archive", "move": true})
    );
}

#[test]
fn file_priority_payload_groups_wanted_and_unwanted_files() {
    let server = ScriptedServer::start(vec![success(json!({})), success(json!({}))]);
    let client = client(&server);

    client
        .set_file_priorities(
            41,
            &[
                (0, FilePriority::High),
                (1, FilePriority::Normal),
                (2, FilePriority::Low),
                (3, FilePriority::Unwanted),
            ],
        )
        .unwrap();
    let request = server.request();
    assert_eq!(request.method(), "torrent-set");
    assert_eq!(
        request.arguments(),
        &json!({
            "ids": [41],
            "priority-high": [0],
            "priority-normal": [1],
            "priority-low": [2],
            "files-wanted": [0, 1, 2],
            "files-unwanted": [3]
        })
    );

    client.set_file_priorities(41, &[]).unwrap();
    assert_eq!(server.request().arguments(), &json!({"ids": [41]}));
}

#[test]
fn session_stats_and_free_space_deserialize_daemon_values() {
    let server = ScriptedServer::start(vec![
        success(json!({
            "torrentCount": 4,
            "activeTorrentCount": 3,
            "downloadSpeed": 1200,
            "uploadSpeed": 400
        })),
        success(json!({"path": "/downloads", "size-bytes": 987654})),
    ]);
    let client = client(&server);

    let stats = client.session_stats().unwrap();
    assert_eq!(stats.torrent_count, 4);
    assert_eq!(stats.download_speed, 1200);
    let stats_request = server.request();
    assert_eq!(stats_request.method(), "session-stats");
    assert!(stats_request.body.get("arguments").is_none());

    let free = client.free_space("/downloads").unwrap();
    assert_eq!(free.size_bytes, 987654);
    let free_request = server.request();
    assert_eq!(free_request.method(), "free-space");
    assert_eq!(free_request.arguments(), &json!({"path": "/downloads"}));
}
