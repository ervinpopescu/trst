use super::*;
use crate::test_support::{Response, ScriptedServer, success};
use std::collections::VecDeque;
use std::io;
use std::time::{Duration, Instant};
use ureq::unversioned::transport::{Buffers, LazyBuffers, NextTimeout, Transport};

#[derive(Debug, Default)]
struct InterruptFirstReadConnector;

impl<In: Transport> ureq::unversioned::transport::Connector<In> for InterruptFirstReadConnector {
    type Out = InterruptFirstReadTransport<In>;

    fn connect(
        &self,
        _details: &ureq::unversioned::transport::ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| InterruptFirstReadTransport {
            inner,
            interrupted: false,
        }))
    }
}

#[derive(Debug)]
struct InterruptFirstReadTransport<T> {
    inner: T,
    interrupted: bool,
}

impl<T: Transport> Transport for InterruptFirstReadTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(interrupted_error());
        }
        self.inner.await_input(timeout)
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

struct DiagnosticTransport {
    state: &'static str,
    buffers: LazyBuffers,
}

impl std::fmt::Debug for DiagnosticTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiagnosticTransport")
            .field("state", &self.state)
            .finish()
    }
}

impl Transport for DiagnosticTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(
        &mut self,
        _amount: usize,
        _timeout: NextTimeout,
    ) -> Result<(), ureq::Error> {
        Ok(())
    }

    fn await_input(&mut self, _timeout: NextTimeout) -> Result<bool, ureq::Error> {
        Ok(false)
    }

    fn is_open(&mut self) -> bool {
        true
    }

    fn is_tls(&self) -> bool {
        false
    }
}

#[test]
fn interrupted_read_transport_debug_includes_inner_diagnostics() {
    let transport = InterruptedReadTransport::new(DiagnosticTransport {
        state: "diagnostic-state-visible",
        buffers: LazyBuffers::new(64, 64),
    });

    let debug = format!("{transport:?}");

    assert!(debug.contains("diagnostic-state-visible"));
}

fn client(server: &ScriptedServer) -> TransmissionClient {
    TransmissionClient::new(&server.url, None, Some(2))
}

#[derive(Debug)]
struct FakeTransport {
    buffers: LazyBuffers,
    outcomes: VecDeque<Result<bool, ureq::Error>>,
    seen_timeouts: Vec<NextTimeout>,
    seen_writes: Vec<(usize, NextTimeout)>,
    open: bool,
    tls: bool,
}

impl FakeTransport {
    fn new(outcomes: Vec<Result<bool, ureq::Error>>) -> Self {
        Self {
            buffers: LazyBuffers::new(64, 64),
            outcomes: outcomes.into(),
            seen_timeouts: Vec::new(),
            seen_writes: Vec::new(),
            open: true,
            tls: false,
        }
    }
}

impl Transport for FakeTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.seen_writes.push((amount, timeout));
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.seen_timeouts.push(timeout);
        match self
            .outcomes
            .pop_front()
            .unwrap_or_else(|| Err(interrupted_error()))
        {
            Ok(progress) => {
                if progress {
                    self.buffers.input_append_buf()[0] = b'x';
                    self.buffers.input_appended(1);
                }
                Ok(progress)
            }
            Err(error) => Err(error),
        }
    }

    fn is_open(&mut self) -> bool {
        self.open
    }

    fn is_tls(&self) -> bool {
        self.tls
    }
}

fn interrupted_error() -> ureq::Error {
    ureq::Error::Io(io::Error::new(io::ErrorKind::Interrupted, "interrupted"))
}

fn scripted_clock(values: impl IntoIterator<Item = Instant>) -> impl FnMut() -> Instant {
    let mut values = values.into_iter();
    move || values.next().expect("scripted clock exhausted")
}

#[test]
fn tcp_rpc_retries_an_interrupted_response_read_without_replaying_post() {
    let server = ScriptedServer::start(vec![success(json!({"torrentCount": 3}))]);
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .http_status_as_error(false)
        .build();
    let connector = ()
        .chain(ureq::unversioned::transport::TcpConnector::default())
        .chain(InterruptFirstReadConnector)
        .chain(InterruptedReadConnector)
        .chain(ureq::unversioned::transport::RustlsConnector::default());
    let agent = ureq::Agent::with_parts(
        config,
        connector,
        ureq::unversioned::resolver::DefaultResolver::default(),
    );

    let response = agent
        .post(&server.url)
        .send(r#"{"method":"session-stats"}"#)
        .unwrap();
    assert_eq!(response.status(), 200);
    let request = server.request();
    assert_eq!(request.method(), "session-stats");
}

#[test]
fn interrupted_input_is_retried_without_changing_buffers() {
    let base = Instant::now();
    let mut transport =
        InterruptedReadTransport::new(FakeTransport::new(vec![Err(interrupted_error()), Ok(true)]));

    assert!(
        transport
            .await_input_with_clock(
                NextTimeout {
                    after: Duration::from_secs(1).into(),
                    reason: ureq::Timeout::RecvResponse,
                },
                scripted_clock([
                    base,
                    base + Duration::from_millis(1),
                    base + Duration::from_millis(2),
                ]),
            )
            .unwrap()
    );
    assert_eq!(transport.buffers().input(), b"x");
}

#[test]
fn zero_timeout_input_is_delegated_without_creating_a_retry_deadline() {
    let timeout = NextTimeout {
        after: ureq::unversioned::transport::time::Duration::NotHappening,
        reason: ureq::Timeout::RecvBody,
    };
    let mut transport = InterruptedReadTransport::new(FakeTransport::new(vec![Ok(false)]));

    assert!(
        !transport
            .await_input_with_clock(timeout, || panic!("zero timeout must not read clock"))
            .unwrap()
    );
    assert_eq!(transport.inner.seen_timeouts, [timeout]);
}

#[test]
fn adapter_delegates_transport_state_and_ordinary_input_results() {
    let base = Instant::now();
    let mut inner = FakeTransport::new(vec![Ok(true)]);
    inner.open = false;
    inner.tls = true;
    let mut transport = InterruptedReadTransport::new(inner);

    assert!(
        transport
            .await_input_with_clock(
                NextTimeout {
                    after: Duration::from_secs(1).into(),
                    reason: ureq::Timeout::RecvResponse,
                },
                scripted_clock([base, base + Duration::from_millis(1)]),
            )
            .unwrap()
    );
    assert_eq!(transport.inner.seen_timeouts.len(), 1);
    assert!(!transport.is_open());
    assert!(transport.is_tls());
}

#[test]
fn interrupted_input_uses_decreasing_remaining_budget() {
    let base = Instant::now();
    let mut transport = InterruptedReadTransport::new(FakeTransport::new(vec![
        Err(interrupted_error()),
        Err(interrupted_error()),
        Ok(true),
    ]));

    transport
        .await_input_with_clock(
            NextTimeout {
                after: Duration::from_millis(10).into(),
                reason: ureq::Timeout::RecvResponse,
            },
            scripted_clock([
                base,
                base + Duration::from_millis(2),
                base + Duration::from_millis(5),
                base + Duration::from_millis(7),
            ]),
        )
        .unwrap();

    let seen: Vec<Duration> = transport
        .inner
        .seen_timeouts
        .iter()
        .map(|timeout| (*timeout.after).into())
        .collect();
    assert_eq!(
        seen,
        [
            Duration::from_millis(8),
            Duration::from_millis(5),
            Duration::from_millis(3)
        ]
    );
}

#[test]
fn interrupted_input_expiry_preserves_original_reason() {
    let base = Instant::now();
    let mut transport =
        InterruptedReadTransport::new(FakeTransport::new(vec![Err(interrupted_error())]));

    let error = transport
        .await_input_with_clock(
            NextTimeout {
                after: Duration::from_millis(10).into(),
                reason: ureq::Timeout::RecvBody,
            },
            scripted_clock([
                base,
                base + Duration::from_millis(4),
                base + Duration::from_millis(10),
            ]),
        )
        .unwrap_err();

    assert!(matches!(
        error,
        ureq::Error::Timeout(ureq::Timeout::RecvBody)
    ));
    assert_eq!(transport.inner.seen_timeouts.len(), 1);
}

#[test]
fn non_interrupted_input_error_is_propagated() {
    let base = Instant::now();
    let mut transport = InterruptedReadTransport::new(FakeTransport::new(vec![Err(
        ureq::Error::Io(io::Error::new(io::ErrorKind::ConnectionReset, "reset")),
    )]));

    let error = transport
        .await_input_with_clock(
            NextTimeout {
                after: Duration::from_secs(1).into(),
                reason: ureq::Timeout::RecvResponse,
            },
            scripted_clock([base, base + Duration::from_millis(1)]),
        )
        .unwrap_err();
    assert!(
        matches!(error, ureq::Error::Io(error) if error.kind() == io::ErrorKind::ConnectionReset)
    );
}

#[test]
fn interrupted_input_preserves_eof() {
    let base = Instant::now();
    let mut transport = InterruptedReadTransport::new(FakeTransport::new(vec![
        Err(interrupted_error()),
        Ok(false),
    ]));

    assert!(
        !transport
            .await_input_with_clock(
                NextTimeout {
                    after: Duration::from_secs(1).into(),
                    reason: ureq::Timeout::RecvBody,
                },
                scripted_clock([
                    base,
                    base + Duration::from_millis(1),
                    base + Duration::from_millis(2),
                ]),
            )
            .unwrap()
    );
    assert!(transport.buffers().input().is_empty());
}

#[test]
fn interrupted_input_does_not_change_write_behavior() {
    let timeout = NextTimeout {
        after: Duration::from_secs(1).into(),
        reason: ureq::Timeout::SendBody,
    };
    let mut transport = InterruptedReadTransport::new(FakeTransport::new(Vec::new()));

    transport.transmit_output(7, timeout).unwrap();

    assert_eq!(transport.inner.seen_writes, [(7, timeout)]);
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
