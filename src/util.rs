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
            {
                if file_type.is_dir() || file_type.is_symlink() {
                    matches.push(name);
                } else if file_type.is_file() && name.ends_with(".torrent") {
                    matches.push(name);
                }
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

/// Attempts to auto-complete a partially typed path by listing the directory contents.
pub fn autocomplete_path(input: &str) -> Option<String> {
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
}
