use super::*;

#[test]
fn test_human_bytes() {
    assert_eq!(human_bytes(0), "0 B");
    assert_eq!(human_bytes(500), "500 B");
    assert_eq!(human_bytes(1024), "1 KB");
    assert_eq!(human_bytes(1025), "1 KB");
    assert_eq!(human_bytes(1024 * 1024), "1 MB");
    assert_eq!(human_bytes((1.5 * 1024.0 * 1024.0) as i64), "1.5 MB");
    assert_eq!(human_bytes(1024 * 1024 * 1024), "1 GB");
    assert_eq!(human_bytes(1024 * 1024 * 1024 * 1024), "1 TB");
    assert_eq!(human_bytes(1024 * 1024 * 1024 * 1024 * 1024), "1.0 PB");
}

#[test]
fn test_human_speed() {
    assert_eq!(human_speed(0), "0 B/s");
    assert_eq!(human_speed(1024), "1 KB/s");
    assert_eq!(human_speed(1536), "1.5 KB/s");
}

#[test]
fn test_human_eta() {
    assert_eq!(human_eta(-1), "∞");
    assert_eq!(human_eta(0), "done");
    assert_eq!(human_eta(30), "30s");
    assert_eq!(human_eta(90), "1m 30s");
    assert_eq!(human_eta(3600), "1h 00m");
    assert_eq!(human_eta(3665), "1h 01m");
    assert_eq!(human_eta(86400), "24h 00m");
    assert_eq!(human_eta(90000), "1d 1h");
}

#[test]
fn test_progress_bar() {
    assert_eq!(progress_bar(0.0, 10), "░░░░░░░░░░");
    assert_eq!(progress_bar(0.5, 10), "█████░░░░░");
    assert_eq!(progress_bar(1.0, 10), "██████████");
    assert_eq!(progress_bar(0.25, 4), "█░░░");
}

#[test]
fn test_percent() {
    assert_eq!(percent(0.0), "0.0%");
    assert_eq!(percent(0.55), "55.0%");
    assert_eq!(percent(1.0), "100.0%");
}

#[test]
fn test_get_path_suggestions_empty_input() {
    let result = get_path_suggestions("");
    assert!(result.is_empty());
}

#[test]
fn test_get_path_suggestions_with_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::create_dir(dir.path().join("alpha")).unwrap();
    std::fs::create_dir(dir.path().join("beta")).unwrap();
    std::fs::File::create(dir.path().join("gamma.txt")).unwrap();

    // prefix ending with '/' lists all subdirs
    let input = format!("{}/", base);
    let results = get_path_suggestions(&input);
    assert!(results.contains(&"alpha".to_string()));
    assert!(results.contains(&"beta".to_string()));
    assert!(
        !results.contains(&"gamma.txt".to_string()),
        "files excluded"
    );

    // prefix matching 'al' returns only 'alpha'
    let input = format!("{}/al", base);
    let results = get_path_suggestions(&input);
    assert_eq!(results, vec!["alpha".to_string()]);

    // prefix with no match returns empty
    let input = format!("{}/xyz", base);
    let results = get_path_suggestions(&input);
    assert!(results.is_empty());
}

#[test]
fn test_autocomplete_path_empty() {
    assert_eq!(autocomplete_path(""), None);
}

#[test]
fn test_autocomplete_path_single_match() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::create_dir(dir.path().join("only_match")).unwrap();

    let input = format!("{}/only", base);
    let result = autocomplete_path(&input);
    assert!(result.is_some());
    let completed = result.unwrap();
    assert!(completed.contains("only_match"));
    assert!(completed.ends_with('/'));
}

#[test]
fn test_autocomplete_path_multiple_matches_common_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::create_dir(dir.path().join("common_a")).unwrap();
    std::fs::create_dir(dir.path().join("common_b")).unwrap();

    let input = format!("{}/comm", base);
    let result = autocomplete_path(&input);
    assert!(result.is_some());
    let completed = result.unwrap();
    assert!(completed.contains("common"));

    // Exact match at the common prefix boundary → None (no extension beyond "comm")
    let input2 = format!("{}/common_", base);
    // Two entries share prefix "common_" so common prefix == "common_" == input prefix → None
    let result2 = autocomplete_path(&input2);
    assert!(result2.is_none());
}

#[test]
fn test_autocomplete_path_no_match() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let input = format!("{}/nonexistent", base);
    assert_eq!(autocomplete_path(&input), None);
}

#[test]
fn test_get_path_suggestions_trailing_slash() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();

    let input = format!("{}/", base);
    let results = get_path_suggestions(&input);
    assert!(results.contains(&"sub".to_string()));
}

// -------------------------------------------------------------------------
// get_torrent_file_suggestions
// -------------------------------------------------------------------------

#[test]
fn test_get_torrent_file_suggestions_empty_input() {
    let result = get_torrent_file_suggestions("");
    assert!(result.is_empty());
}

#[test]
fn test_get_torrent_file_suggestions_includes_directories() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::create_dir(dir.path().join("downloads")).unwrap();
    std::fs::File::create(dir.path().join("plain.txt")).unwrap();

    let input = format!("{}/", base);
    let results = get_torrent_file_suggestions(&input);
    // Directories are always included.
    assert!(results.contains(&"downloads".to_string()), "dir included");
    // Plain files (non-.torrent) are excluded.
    assert!(
        !results.contains(&"plain.txt".to_string()),
        "non-torrent file excluded"
    );
}

#[test]
fn test_get_torrent_file_suggestions_includes_torrent_files() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::File::create(dir.path().join("ubuntu.torrent")).unwrap();
    std::fs::File::create(dir.path().join("readme.txt")).unwrap();

    let input = format!("{}/", base);
    let results = get_torrent_file_suggestions(&input);
    // .torrent files are included.
    assert!(
        results.contains(&"ubuntu.torrent".to_string()),
        ".torrent included"
    );
    // Non-.torrent files are excluded.
    assert!(
        !results.contains(&"readme.txt".to_string()),
        "non-torrent excluded"
    );
}

#[test]
fn test_get_torrent_file_suggestions_prefix_filter() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::File::create(dir.path().join("arch.torrent")).unwrap();
    std::fs::File::create(dir.path().join("ubuntu.torrent")).unwrap();

    // Prefix "arch" should only return "arch.torrent".
    let input = format!("{}/arch", base);
    let results = get_torrent_file_suggestions(&input);
    assert_eq!(results, vec!["arch.torrent".to_string()]);

    // Prefix "xyz" should return nothing.
    let input2 = format!("{}/xyz", base);
    assert!(get_torrent_file_suggestions(&input2).is_empty());
}

#[test]
fn test_get_torrent_file_suggestions_trailing_slash() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::File::create(dir.path().join("test.torrent")).unwrap();

    let input = format!("{}/", base);
    let results = get_torrent_file_suggestions(&input);
    assert!(results.contains(&"sub".to_string()));
    assert!(results.contains(&"test.torrent".to_string()));
}

// -------------------------------------------------------------------------
// autocomplete_torrent_path
// -------------------------------------------------------------------------

#[test]
fn test_autocomplete_torrent_path_empty() {
    assert_eq!(autocomplete_torrent_path(""), None);
}

#[test]
fn test_autocomplete_torrent_path_no_match() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let input = format!("{}/nonexistent", base);
    assert_eq!(autocomplete_torrent_path(&input), None);
}

#[test]
fn test_autocomplete_torrent_path_single_dir_match_adds_slash() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::create_dir(dir.path().join("only_dir")).unwrap();

    let input = format!("{}/only", base);
    let result = autocomplete_torrent_path(&input);
    assert!(result.is_some(), "should autocomplete");
    let completed = result.unwrap();
    assert!(completed.contains("only_dir"), "contains match name");
    assert!(completed.ends_with('/'), "trailing slash for directory");
}

#[test]
fn test_autocomplete_torrent_path_single_torrent_match_no_slash() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::File::create(dir.path().join("debian.torrent")).unwrap();

    let input = format!("{}/deb", base);
    let result = autocomplete_torrent_path(&input);
    assert!(result.is_some(), "should autocomplete");
    let completed = result.unwrap();
    assert!(completed.contains("debian.torrent"), "contains match name");
    // .torrent files must NOT get a trailing slash.
    assert!(
        !completed.ends_with('/'),
        "no trailing slash for .torrent file"
    );
}

#[test]
fn test_autocomplete_torrent_path_multiple_matches_common_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    std::fs::File::create(dir.path().join("common_a.torrent")).unwrap();
    std::fs::File::create(dir.path().join("common_b.torrent")).unwrap();

    // Both share the prefix "common_" — autocomplete should extend to "common_".
    let input = format!("{}/comm", base);
    let result = autocomplete_torrent_path(&input);
    assert!(result.is_some(), "common prefix extended");
    let completed = result.unwrap();
    assert!(completed.contains("common_"), "extended to common_");

    // At the common-prefix boundary → no further extension → None.
    let input2 = format!("{}/common_", base);
    assert_eq!(
        autocomplete_torrent_path(&input2),
        None,
        "no extension at boundary"
    );
}

// Exercise the `dir.as_os_str().is_empty()` → `"."` branch:  when the input
// has no directory component (e.g. a bare filename), `path.parent()` returns
// the empty path `""`, so we substitute `"."` (current directory).
#[test]
fn test_get_torrent_file_suggestions_bare_filename_uses_cwd() {
    // Using a prefix that almost certainly doesn't match anything in cwd keeps
    // the test deterministic while still exercising the fallback branch.
    let results = get_torrent_file_suggestions("zzzz_unlikely_prefix_xyz");
    // We only care that the function runs without panic and returns a sorted vec.
    assert!(results.is_sorted());
}

// Exercise the `is_symlink()` arm: create a symlink inside the temp dir and verify
// that `get_torrent_file_suggestions` includes it in the results.
#[test]
fn test_get_torrent_file_suggestions_includes_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target_dir");
    std::fs::create_dir(&target).unwrap();
    let link = dir.path().join("link_to_dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(not(unix))]
    {
        // On non-Unix platforms just create a regular dir so the test compiles.
        std::fs::create_dir(&link).unwrap();
    }

    let input = format!("{}/link", dir.path().to_str().unwrap());
    let results = get_torrent_file_suggestions(&input);
    assert!(
        results.contains(&"link_to_dir".to_string()),
        "symlink included in suggestions"
    );
}

// -------------------------------------------------------------------------
// get_remote_dir_suggestions
// -------------------------------------------------------------------------

#[test]
fn test_get_remote_dir_suggestions_prefix_match() {
    let known = vec![
        "/srv/downloads".to_string(),
        "/srv/media".to_string(),
        "/home/user".to_string(),
    ];
    let result = get_remote_dir_suggestions("/srv/", &known);
    assert!(result.contains(&"/srv/downloads".to_string()));
    assert!(result.contains(&"/srv/media".to_string()));
    assert!(!result.contains(&"/home/user".to_string()));
}

#[test]
fn test_get_remote_dir_suggestions_excludes_exact_match() {
    let known = vec![
        "/srv/downloads".to_string(),
        "/srv/downloads/complete".to_string(),
    ];
    let result = get_remote_dir_suggestions("/srv/downloads", &known);
    assert!(
        !result.contains(&"/srv/downloads".to_string()),
        "exact match excluded"
    );
    assert!(result.contains(&"/srv/downloads/complete".to_string()));
}

#[test]
fn test_get_remote_dir_suggestions_no_match() {
    let known = vec!["/srv/downloads".to_string()];
    let result = get_remote_dir_suggestions("/home/", &known);
    assert!(result.is_empty());
}

// -------------------------------------------------------------------------
// autocomplete_remote_path
// -------------------------------------------------------------------------

#[test]
fn test_autocomplete_remote_path_empty_input() {
    let known = vec!["/srv/downloads".to_string()];
    assert_eq!(autocomplete_remote_path("", &known), None);
}

#[test]
fn test_autocomplete_remote_path_single_match_appends_slash() {
    let known = vec!["/srv/downloads".to_string()];
    let result = autocomplete_remote_path("/srv/down", &known);
    assert_eq!(result, Some("/srv/downloads/".to_string()));
}

#[test]
fn test_autocomplete_remote_path_already_has_trailing_slash() {
    let known = vec!["/srv/downloads/".to_string()];
    let result = autocomplete_remote_path("/srv/down", &known);
    assert_eq!(result, Some("/srv/downloads/".to_string()));
}

#[test]
fn test_autocomplete_remote_path_multiple_matches_extended_prefix() {
    let known = vec!["/srv/downloads".to_string(), "/srv/downstairs".to_string()];
    // Both share "/srv/down" which extends beyond the input "/srv/d".
    let result = autocomplete_remote_path("/srv/d", &known);
    assert_eq!(result, Some("/srv/down".to_string()));
}

#[test]
fn test_autocomplete_remote_path_multiple_no_extension() {
    let known = vec!["/srv/downloads".to_string(), "/srv/data".to_string()];
    // "/srv/downloads" and "/srv/data" share only "/srv/d" which equals the input → None.
    let result = autocomplete_remote_path("/srv/d", &known);
    assert_eq!(result, None);
}

#[test]
fn test_autocomplete_remote_path_no_match() {
    let known = vec!["/srv/downloads".to_string()];
    assert_eq!(autocomplete_remote_path("/home/", &known), None);
}

#[cfg(unix)]
fn executable_script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn remote_directory_listing_parses_output_and_uses_argv_terminator() {
    let dir = tempfile::tempdir().unwrap();
    let command_log = dir.path().join("command.txt");
    let ssh = executable_script(
        dir.path(),
        "ssh-success",
        &format!(
            r#"
if [ "$#" -ne 7 ] || [ "$1" != "-o" ] || [ "$2" != "BatchMode=yes" ] ||
   [ "$3" != "-o" ] || [ "$4" != "ConnectTimeout=2" ] ||
   [ "$5" != "--" ] || [ "$6" != "remote.example" ]; then
    echo "unexpected arguments: $*" >&2
    exit 9
fi
printf '%s' "$7" > '{}'
printf '/srv/alpha/\n\n/srv/beta\n'
"#,
            command_log.display()
        ),
    );

    let listed = list_remote_dirs_with_program(
        &ssh,
        "remote.example",
        "/srv/it's here/",
        std::time::Duration::from_secs(1),
    )
    .unwrap();

    assert_eq!(listed, ["/srv/alpha", "/srv/beta"]);
    assert_eq!(
        std::fs::read_to_string(command_log).unwrap(),
        "find '/srv/it'\\''s here/' -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort"
    );
}

#[cfg(unix)]
#[test]
fn remote_directory_listing_reports_stderr_exit_status_and_spawn_failures() {
    let dir = tempfile::tempdir().unwrap();
    let stderr = executable_script(
        dir.path(),
        "ssh-stderr",
        "echo 'Permission denied (publickey).' >&2; exit 255",
    );
    assert_eq!(
        list_remote_dirs_with_program(
            &stderr,
            "remote.example",
            "/srv/",
            std::time::Duration::from_secs(1)
        ),
        Err("Permission denied (publickey).".into())
    );

    let silent = executable_script(dir.path(), "ssh-silent", "exit 7");
    assert_eq!(
        list_remote_dirs_with_program(
            &silent,
            "remote.example",
            "/srv/",
            std::time::Duration::from_secs(1)
        ),
        Err("ssh exited with status 7".into())
    );

    let missing = dir.path().join("does-not-exist");
    let error = list_remote_dirs_with_program(
        &missing,
        "remote.example",
        "/srv/",
        std::time::Duration::from_secs(1),
    )
    .unwrap_err();
    assert!(error.starts_with("ssh: "), "{error}");
}

#[cfg(unix)]
#[test]
fn remote_directory_listing_honors_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let slow = executable_script(dir.path(), "ssh-slow", "exec sleep 1");

    assert_eq!(
        list_remote_dirs_with_program(
            &slow,
            "remote.example",
            "/srv/",
            std::time::Duration::from_millis(10)
        ),
        Err("ssh: timed out".into())
    );
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_human_bytes_never_panics(bytes in any::<i64>()) {
        let result = human_bytes(bytes);
        assert!(!result.is_empty());
        assert!(result.ends_with(" B") || result.ends_with(" KB") || result.ends_with(" MB") || result.ends_with(" GB") || result.ends_with(" TB") || result.ends_with(" PB"));
    }

    #[test]
    fn prop_human_speed_never_panics(bytes_per_sec in any::<i64>()) {
        let result = human_speed(bytes_per_sec);
        assert!(!result.is_empty());
        assert!(result.ends_with("B/s"));
    }

    #[test]
    fn prop_human_eta_never_panics(seconds in any::<i64>()) {
        let result = human_eta(seconds);
        assert!(!result.is_empty());
    }

    #[test]
    fn prop_progress_bar_valid_width(fraction in 0.0..=1.0f64, width in 0..1000usize) {
        let result = progress_bar(fraction, width);
        assert_eq!(result.chars().count(), width);
    }
}
