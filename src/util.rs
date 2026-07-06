pub fn human_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let mut val = bytes as f64;
    for &unit in UNITS {
        if val.abs() < 1024.0 {
            return if val.fract() < 0.05 {
                format!("{val:.0} {unit}")
            } else {
                format!("{val:.1} {unit}")
            };
        }
        val /= 1024.0;
    }
    format!("{val:.1} PB")
}

pub fn human_speed(bytes_per_sec: i64) -> String {
    if bytes_per_sec == 0 {
        return "0 B/s".into();
    }
    format!("{}/s", human_bytes(bytes_per_sec))
}

pub fn human_eta(seconds: i64) -> String {
    if seconds < 0 {
        return "∞".into();
    }
    if seconds == 0 {
        return "done".into();
    }
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 24 {
        let days = h / 24;
        format!("{days}d {hh}h", hh = h % 24)
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

pub fn progress_bar(fraction: f64, width: usize) -> String {
    let filled = (fraction * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty),)
}

pub fn percent(ratio: f64) -> String {
    format!("{:.1}%", ratio * 100.0)
}

/// Returns a list of directories and `.torrent` files matching the current input prefix.
///
/// Used by the add-torrent modal to provide completion for local `.torrent` file paths.
pub fn get_torrent_file_suggestions(input: &str) -> Vec<String> {
    if input.is_empty() {
        return vec![];
    }

    let path = std::path::Path::new(input);
    let (dir, file_prefix) = if input.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| std::path::Path::new(".")),
            path.file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default(),
        )
    };

    let dir_str = if dir.as_os_str().is_empty() {
        "."
    } else {
        dir.to_str().unwrap_or(".")
    };

    let mut matches = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir_str) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string()
                && name.starts_with(file_prefix)
                && name != file_prefix
                && let Ok(file_type) = entry.file_type()
                && (file_type.is_dir()
                    || file_type.is_symlink()
                    || (file_type.is_file() && name.ends_with(".torrent")))
            {
                matches.push(name);
            }
        }
    }
    matches.sort();
    matches
}

/// Tab-completes a partially typed path, including `.torrent` files as completion targets.
///
/// Like `autocomplete_path` but does not restrict to directories — `.torrent` files are also
/// valid completions and do not receive a trailing slash.
pub fn autocomplete_torrent_path(input: &str) -> Option<String> {
    if input.is_empty() {
        return None;
    }

    let path = std::path::Path::new(input);
    let (dir, file_prefix) = if input.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| std::path::Path::new(".")),
            path.file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default(),
        )
    };

    let matches = get_torrent_file_suggestions(input);

    if matches.len() == 1 {
        let completed_path = dir.join(&matches[0]);
        let mut completed = completed_path.to_string_lossy().to_string();
        // Append trailing slash for directories so the user can keep typing the next component.
        if completed_path.is_dir() {
            completed.push('/');
        }
        return Some(completed);
    } else if matches.len() > 1 {
        let mut common_prefix = matches[0].clone();
        for m in &matches[1..] {
            let mut new_prefix = String::new();
            for (c1, c2) in common_prefix.chars().zip(m.chars()) {
                if c1 == c2 {
                    new_prefix.push(c1);
                } else {
                    break;
                }
            }
            common_prefix = new_prefix;
        }
        if common_prefix.len() > file_prefix.len() {
            return Some(dir.join(common_prefix).to_string_lossy().to_string());
        }
    }

    None
}

/// Returns the parent directory of `input` with a trailing slash, used as the
/// cache key and SSH `find` argument for Tab-completion in location modals.
///
/// - Input ending with `/` or empty: returned as-is
/// - Input of the form `/foo`: returns `"/"` (not `"//"`)
/// - Input of the form `/foo/bar`: returns `"/foo/"`
pub fn location_parent_dir(input: &str) -> String {
    if input.ends_with('/') || input.is_empty() {
        return input.to_string();
    }
    std::path::Path::new(input)
        .parent()
        .map(|p| {
            let s = p.to_string_lossy();
            // Avoid returning "//" for top-level paths like "/foo".
            if s.is_empty() || s == "/" {
                "/".to_string()
            } else {
                format!("{}/", s)
            }
        })
        .unwrap_or_else(|| "/".to_string())
}

/// Returns a list of directory names that match the current input prefix.
pub fn get_path_suggestions(input: &str) -> Vec<String> {
    if input.is_empty() {
        return vec![];
    }

    let path = std::path::Path::new(input);
    let (dir, file_prefix) = if input.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| std::path::Path::new(".")),
            path.file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default(),
        )
    };

    let dir_str = if dir.as_os_str().is_empty() {
        "."
    } else {
        dir.to_str().unwrap_or(".")
    };

    let mut matches = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir_str) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string()
                && name.starts_with(file_prefix)
                && name != file_prefix
                && let Ok(file_type) = entry.file_type()
                && (file_type.is_dir() || file_type.is_symlink())
            {
                matches.push(name);
            }
        }
    }
    matches.sort();
    matches
}

/// Lists immediate subdirectories of `dir` on `host` by running `find` over SSH.
///
/// Uses `BatchMode=yes` so it never prompts for a password. Returns `Ok(dirs)` on
/// success or `Err(msg)` when SSH fails (auth denied, host unreachable, timeout, etc.).
/// Callers should surface the error message so the user knows to configure key auth.
pub fn list_remote_dirs(host: &str, dir: &str) -> Result<Vec<String>, String> {
    let escaped = dir.replace('\'', "'\\''");
    let cmd = format!(
        "find '{}' -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort",
        escaped
    );

    let (tx, rx) = std::sync::mpsc::channel();
    let host = host.to_string();
    std::thread::spawn(move || {
        let result = std::process::Command::new("ssh")
            .args([
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=2",
                "--", // argv terminator: everything after is positional, not a flag
                &host,
                &cmd,
            ])
            .output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(std::time::Duration::from_secs(3)) {
        Ok(Ok(output)) if output.status.success() => Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim_end_matches('/').to_string())
            .filter(|l| !l.is_empty())
            .collect()),
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(if stderr.is_empty() {
                format!(
                    "ssh exited with status {}",
                    output.status.code().unwrap_or(-1)
                )
            } else {
                stderr
            })
        }
        Ok(Err(e)) => Err(format!("ssh: {e}")),
        Err(_) => Err("ssh: timed out".to_string()),
    }
}

/// Returns known remote directory paths from `known_dirs` that start with `input`.
///
/// An exact match is excluded — only entries that extend beyond `input` are returned,
/// which is what makes them useful as completions.
pub fn get_remote_dir_suggestions(input: &str, known_dirs: &[String]) -> Vec<String> {
    known_dirs
        .iter()
        .filter(|d| d.starts_with(input) && d.as_str() != input)
        .cloned()
        .collect()
}

/// Tab-completes a partially typed path against a set of known remote directory paths.
///
/// Works like `autocomplete_path` but uses `known_dirs` (paths from the remote daemon)
/// instead of reading the local filesystem.
pub fn autocomplete_remote_path(input: &str, known_dirs: &[String]) -> Option<String> {
    if input.is_empty() {
        return None;
    }

    let matches: Vec<&str> = known_dirs
        .iter()
        .filter(|d| d.starts_with(input) && d.as_str() != input)
        .map(String::as_str)
        .collect();

    if matches.len() == 1 {
        let mut completed = matches[0].to_string();
        if !completed.ends_with('/') {
            completed.push('/');
        }
        return Some(completed);
    } else if matches.len() > 1 {
        let mut common_prefix = matches[0].to_string();
        for m in &matches[1..] {
            let new_prefix: String = common_prefix
                .chars()
                .zip(m.chars())
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect();
            common_prefix = new_prefix;
        }
        if common_prefix.len() > input.len() {
            return Some(common_prefix);
        }
    }

    None
}

/// Attempts to auto-complete a partially typed path by listing the directory contents.
pub fn autocomplete_path(input: &str) -> Option<String> {
    if input.is_empty() {
        return None;
    }

    // If the input is itself an existing directory without a trailing slash,
    // append one so the user can Tab again to list its contents.
    if !input.ends_with('/') && std::path::Path::new(input).is_dir() {
        return Some(format!("{}/", input));
    }

    let path = std::path::Path::new(input);
    let (dir, file_prefix) = if input.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or_else(|| std::path::Path::new(".")),
            path.file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default(),
        )
    };

    let matches = get_path_suggestions(input);

    if matches.len() == 1 {
        // If there's exactly one match, complete it.
        let mut completed = dir.join(&matches[0]).to_string_lossy().to_string();
        // If it's a directory, add a trailing slash for convenience.
        if std::path::Path::new(&completed).is_dir() {
            completed.push('/');
        }
        return Some(completed);
    } else if matches.len() > 1 {
        // Find the longest common prefix among multiple matches
        let mut common_prefix = matches[0].clone();
        for m in &matches[1..] {
            let mut new_prefix = String::new();
            for (c1, c2) in common_prefix.chars().zip(m.chars()) {
                if c1 == c2 {
                    new_prefix.push(c1);
                } else {
                    break;
                }
            }
            common_prefix = new_prefix;
        }
        if common_prefix.len() > file_prefix.len() {
            return Some(dir.join(common_prefix).to_string_lossy().to_string());
        }
    }

    None
}
#[cfg(test)]
mod tests {
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
        std::fs::write(&fake_ssh, "#!/bin/sh\necho 'Permission denied (publickey).' >&2\nexit 255").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let original_path = std::env::var("PATH").unwrap_or_default();
        // Prepend the fake directory so our stub wins over the real ssh.
        // SAFETY: single-threaded test; no other threads read PATH concurrently.
        unsafe {
            std::env::set_var("PATH", format!("{}:{}", fake_dir.path().display(), original_path));
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
}
