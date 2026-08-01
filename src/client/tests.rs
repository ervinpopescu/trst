use super::*;

#[test]
fn test_set_auth_updates_header() {
    let client = TransmissionClient::new("http://dummy", None, None);
    assert!(client.auth_header.lock().unwrap().is_none());
    client.set_auth("alice", "secret");
    let header = client.auth_header.lock().unwrap().clone().unwrap();
    assert!(header.starts_with("Basic "));
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(header.strip_prefix("Basic ").unwrap())
        .unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), "alice:secret");
}

#[test]
fn test_set_auth_replaces_existing_header() {
    let client = TransmissionClient::new("http://dummy", Some(("old", "creds")), None);
    client.set_auth("new", "pass");
    let header = client.auth_header.lock().unwrap().clone().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(header.strip_prefix("Basic ").unwrap())
        .unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), "new:pass");
}

// `add_metainfo` encodes the supplied bytes as base64 and sends a `torrent-add`
// RPC.  A dummy (unreachable) client lets us exercise the encoding + arg-building
// code paths without a running daemon; the network call is expected to fail.
#[test]
fn test_add_metainfo_without_download_dir_fails_on_dummy() {
    let client = TransmissionClient::new("http://dummy.invalid:9091/transmission/rpc", None, None);
    // Any non-empty byte slice; content doesn't matter for a dummy-client test.
    let result = client.add_metainfo(b"fake torrent bytes", None);
    assert!(result.is_err(), "dummy client should return Err");
}

#[test]
fn test_add_metainfo_with_download_dir_fails_on_dummy() {
    let client = TransmissionClient::new("http://dummy.invalid:9091/transmission/rpc", None, None);
    // Exercise the `download-dir` insertion branch (lines inside the `if let Some(dir)` guard).
    let result = client.add_metainfo(b"fake torrent bytes", Some("/downloads"));
    assert!(result.is_err(), "dummy client should return Err");
}
