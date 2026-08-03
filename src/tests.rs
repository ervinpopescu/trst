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

fn empty_args() -> Args {
    Args {
        url: String::new(),
        username: None,
        password: None,
        clear_auth: false,
    }
}

#[test]
fn parse_args_captures_credentials_and_https_urls() {
    let parsed = parse_args_from(args(&[
        "https://transmission.example/rpc",
        "--username",
        "alice",
        "--password",
        "secret",
    ]));

    assert_eq!(parsed.url, "https://transmission.example/rpc");
    assert_eq!(parsed.username.as_deref(), Some("alice"));
    assert_eq!(parsed.password.as_deref(), Some("secret"));
}

#[test]
fn resolve_url_obeys_cli_config_default_precedence() {
    let mut config = config::Config::default();
    config.connection.url = Some("https://configured.example/rpc".into());

    let mut cli = empty_args();
    cli.url = "https://cli.example/rpc".into();
    assert_eq!(
        resolve_url(&cli, &config),
        (
            "https://cli.example/rpc".into(),
            Some("https://cli.example/rpc".into())
        )
    );

    assert_eq!(
        resolve_url(&empty_args(), &config),
        ("https://configured.example/rpc".into(), None)
    );

    assert_eq!(
        resolve_url(&empty_args(), &config::Config::default()),
        ("http://localhost:9091/transmission/rpc".into(), None)
    );
}

#[test]
fn resolve_auth_saves_cli_credentials_and_clears_plaintext_config() {
    let mut cli = empty_args();
    cli.username = Some("cli-user".into());
    cli.password = Some("cli-pass".into());
    let mut config = config::Config::default();
    config.connection.username = Some("old-user".into());
    config.connection.password = Some("old-pass".into());
    let mut saved = None;

    let (auth, changed) = resolve_auth(
        &cli,
        &mut config,
        "https://example.test/rpc",
        |url, username, password| {
            saved = Some((url.to_string(), username.to_string(), password.to_string()));
            Ok(())
        },
        |_| panic!("stored credentials must not be loaded when CLI auth is complete"),
    );

    assert_eq!(auth, Some(("cli-user".into(), "cli-pass".into())));
    assert!(changed);
    assert_eq!(
        saved,
        Some((
            "https://example.test/rpc".into(),
            "cli-user".into(),
            "cli-pass".into()
        ))
    );
    assert_eq!(
        config.connection.url.as_deref(),
        Some("https://example.test/rpc")
    );
    assert!(config.connection.username.is_none());
    assert!(config.connection.password.is_none());
}

#[test]
fn resolve_auth_retains_plaintext_fallback_when_credential_store_fails() {
    let mut cli = empty_args();
    cli.username = Some("alice".into());
    cli.password = Some("secret".into());
    let mut config = config::Config::default();

    let (auth, changed) = resolve_auth(
        &cli,
        &mut config,
        "https://example.test/rpc",
        |_, _, _| Err("keyring unavailable".into()),
        |_| None,
    );

    assert_eq!(auth, Some(("alice".into(), "secret".into())));
    assert!(changed);
    assert_eq!(config.connection.username.as_deref(), Some("alice"));
    assert_eq!(config.connection.password.as_deref(), Some("secret"));
}

#[test]
fn resolve_auth_combines_cli_and_config_then_migrates_to_store() {
    let mut cli = empty_args();
    cli.username = Some("new-user".into());
    let mut config = config::Config::default();
    let configured_value = ["configured", "test", "value"].join("-");
    config.connection.username = Some("old-user".into());
    config.connection.password = Some(configured_value.clone());

    let (auth, changed) = resolve_auth(
        &cli,
        &mut config,
        "https://example.test/rpc",
        |_, username, password| {
            assert_eq!(username, "new-user");
            assert_eq!(password, configured_value);
            Ok(())
        },
        |_| None,
    );

    assert_eq!(auth, Some(("new-user".into(), configured_value)));
    assert!(changed);
    assert!(config.connection.username.is_none());
    assert!(config.connection.password.is_none());
}

#[test]
fn resolve_auth_loads_stored_credentials_when_inputs_are_incomplete() {
    let mut config = config::Config::default();
    config.connection.username = Some("orphaned-user".into());
    let mut loaded_url = None;

    let (auth, changed) = resolve_auth(
        &empty_args(),
        &mut config,
        "https://example.test/rpc",
        |_, _, _| panic!("incomplete credentials must not be saved"),
        |url| {
            loaded_url = Some(url.to_string());
            Some(("stored-user".into(), "stored-pass".into()))
        },
    );

    assert_eq!(loaded_url.as_deref(), Some("https://example.test/rpc"));
    assert_eq!(auth, Some(("stored-user".into(), "stored-pass".into())));
    assert!(!changed);
}

#[test]
fn insecure_auth_warning_only_applies_to_remote_plain_http() {
    for (url, has_auth, expected) in [
        ("http://remote.example/rpc", true, true),
        ("https://remote.example/rpc", true, false),
        ("http://remote.example/rpc", false, false),
        ("http://localhost:9091/rpc", true, false),
        ("http://127.0.0.1:9091/rpc", true, false),
        ("http://[::1]:9091/rpc", true, false),
        ("http://localhost.attacker.example/rpc", true, true),
        ("http://localhost@remote.example/rpc", true, true),
        ("http://user@localhost:9091/rpc", true, false),
        ("HTTP://remote.example/rpc", true, true),
        ("http://LOCALHOST:9091/rpc", true, false),
        ("http://remote.example?next=@localhost", true, true),
        ("http://remote.example#@localhost", true, true),
        ("not a URL", true, false),
    ] {
        assert_eq!(
            should_warn_insecure_auth(url, has_auth),
            expected,
            "unexpected warning decision for {url}"
        );
    }
}
