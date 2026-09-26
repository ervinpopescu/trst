use std::process::Command;

fn trst(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_trst"))
        .args(args)
        .output()
        .expect("run trst binary")
}

#[test]
fn help_is_a_successful_end_user_command() {
    let output = trst(&["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Transmission remote TUI"));
    assert!(stdout.contains("Usage: trst [HOST[:PORT]] [OPTIONS]"));
    assert!(stdout.contains("--clear-auth"));
}

#[test]
fn missing_url_value_reports_actionable_error() {
    let output = trst(&["--url"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "error: --url requires a value"
    );
}

#[test]
fn missing_credential_value_reports_actionable_error() {
    for option in [
        "--username",
        "--password",
        "--username --clear-auth",
        "--password --clear-auth",
    ] {
        let args = option.split_whitespace().collect::<Vec<_>>();
        let output = trst(&args);
        assert_eq!(output.status.code(), Some(1), "args: {option}");
        assert!(output.stdout.is_empty(), "args: {option}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            stderr.contains("requires a value"),
            "args: {option}: {stderr}"
        );
    }
}

#[test]
fn clear_auth_persists_removal_of_configured_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let config_dir = dir.path().join("trst");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config_path = config_dir.join("config.toml");
    std::fs::write(
        &config_path,
        "[connection]\nusername = \"alice\"\npassword = \"secret\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_trst"))
        .args([
            "--clear-auth",
            "--url",
            "http://localhost:9091/transmission/rpc",
        ])
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .expect("run trst clear-auth command");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
    let connection = config.get("connection").unwrap();
    assert!(connection.get("username").is_none());
    assert!(connection.get("password").is_none());
}

#[test]
fn clear_auth_propagates_config_save_failure() {
    let dir = tempfile::tempdir().unwrap();
    // Config::load sees a missing config below this path, but cannot create the
    // parent directory because it is a regular file. --clear-auth must surface
    // the subsequent config save error rather than reporting success.
    std::fs::write(dir.path().join("trst"), "not a directory").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_trst"))
        .args([
            "--clear-auth",
            "--url",
            "http://localhost:9091/transmission/rpc",
        ])
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .expect("run trst clear-auth command");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Not a directory"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unknown_option_reports_error_and_help_hint() {
    let output = trst(&["--definitely-unknown"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unknown argument: \"--definitely-unknown\""));
    assert!(stderr.contains("try 'trst --help' for usage"));
}

#[test]
fn password_flag_warns_before_help_exits() {
    let output = trst(&["--password", "visible-secret", "--help"]);
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("passing password via -p is visible"));
}
