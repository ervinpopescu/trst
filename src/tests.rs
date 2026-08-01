use super::*;

fn args(slice: &[&str]) -> impl Iterator<Item = String> {
    slice
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

/// No arguments → url is empty (main() will fall back to config / hardcoded default).
#[test]
fn test_parse_args_no_args_url_is_empty() {
    let a = parse_args_from(args(&[]));
    assert!(a.url.is_empty(), "url must be empty when no args are given");
    assert!(a.username.is_none());
    assert!(a.password.is_none());
}

/// Positional host argument → url is derived from the host.
#[test]
fn test_parse_args_positional_host_sets_url() {
    let a = parse_args_from(args(&["myserver"]));
    assert_eq!(a.url, "http://myserver:9091/transmission/rpc");
}

/// Positional host with explicit port.
#[test]
fn test_parse_args_positional_host_with_port_sets_url() {
    let a = parse_args_from(args(&["myserver:8080"]));
    assert_eq!(a.url, "http://myserver:8080/transmission/rpc");
}

/// Positional full HTTP URL is passed through unchanged.
#[test]
fn test_parse_args_positional_full_url_passthrough() {
    let a = parse_args_from(args(&["http://myserver/transmission/rpc"]));
    assert_eq!(a.url, "http://myserver/transmission/rpc");
}

/// --url flag sets the url.
#[test]
fn test_parse_args_url_flag_sets_url() {
    let a = parse_args_from(args(&["--url", "http://remotehost:9091/transmission/rpc"]));
    assert_eq!(a.url, "http://remotehost:9091/transmission/rpc");
}

/// -u short flag also sets the url.
#[test]
fn test_parse_args_url_short_flag_sets_url() {
    let a = parse_args_from(args(&["-u", "http://remotehost/transmission/rpc"]));
    assert_eq!(a.url, "http://remotehost/transmission/rpc");
}

/// --url flag takes precedence over a positional host.
#[test]
fn test_parse_args_url_flag_overrides_positional() {
    let a = parse_args_from(args(&[
        "somehost",
        "--url",
        "http://explicit/transmission/rpc",
    ]));
    assert_eq!(a.url, "http://explicit/transmission/rpc");
}

#[test]
fn test_parse_args_clear_auth_flag() {
    let a = parse_args_from(args(&["--clear-auth"]));
    assert!(a.clear_auth);
    assert!(a.url.is_empty());
}

#[test]
fn test_parse_args_clear_auth_with_host() {
    let a = parse_args_from(args(&["myserver:9092", "--clear-auth"]));
    assert!(a.clear_auth);
    assert_eq!(a.url, "http://myserver:9092/transmission/rpc");
}

#[test]
fn test_init_keyring_backend_makes_entry_usable() {
    // After backend initialisation, Entry::new must succeed on this platform.
    // This exercises the fallback path on CI/systems without a secret-service daemon.
    init_keyring_backend();
    let entry = keyring::Entry::new("trst", "http://test-init-backend");
    assert!(
        entry.is_ok(),
        "Entry::new failed after init_keyring_backend: {:?}",
        entry.err()
    );
}
