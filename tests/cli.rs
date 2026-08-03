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
