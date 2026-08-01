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
// list_remote_dirs
// -------------------------------------------------------------------------

#[test]
fn test_list_remote_dirs_ssh_failure_returns_err() {
    // Install a minimal fake `ssh` script that exits non-zero immediately,
    // so this test requires no network access.
    let fake_dir = tempfile::tempdir().unwrap();
    let fake_ssh = fake_dir.path().join("ssh");
    std::fs::write(
        &fake_ssh,
        "#!/bin/sh\necho 'Permission denied (publickey).' >&2\nexit 255",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let original_path = std::env::var("PATH").unwrap_or_default();
    // Prepend the fake directory so our stub wins over the real ssh.
    // SAFETY: single-threaded test; no other threads read PATH concurrently.
    unsafe {
        std::env::set_var(
            "PATH",
            format!("{}:{}", fake_dir.path().display(), original_path),
        );
    }

    let result = list_remote_dirs("myhost", "/");

    // SAFETY: restoring the value we read above.
    unsafe { std::env::set_var("PATH", original_path) };
    assert!(result.is_err(), "expected Err when ssh exits with failure");
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
