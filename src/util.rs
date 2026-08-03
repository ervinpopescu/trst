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
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
    list_remote_dirs_with_program(std::path::Path::new("ssh"), host, dir, TIMEOUT)
}

fn list_remote_dirs_with_program(
    program: &std::path::Path,
    host: &str,
    dir: &str,
    timeout: std::time::Duration,
) -> Result<Vec<String>, String> {
    let escaped = dir.replace('\'', "'\\''");
    let cmd = format!(
        "find '{}' -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort",
        escaped
    );

    let mut child = std::process::Command::new(program)
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=2",
            "--", // argv terminator: everything after is positional, not a flag
            host,
            &cmd,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("ssh: {e}"))?;

    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Err("ssh: failed to capture stdout".to_string());
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = child.kill();
        return Err("ssh: failed to capture stderr".to_string());
    };
    let stdout_reader = std::thread::spawn(move || {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        let _ = std::io::BufReader::new(stdout).read_to_end(&mut bytes);
        bytes
    });
    let stderr_reader = std::thread::spawn(move || {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        let _ = std::io::BufReader::new(stderr).read_to_end(&mut bytes);
        bytes
    });

    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("ssh: timed out".to_string());
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("ssh: {e}"));
            }
        }
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    if status.success() {
        Ok(String::from_utf8_lossy(&stdout)
            .lines()
            .map(|line| line.trim_end_matches('/').to_string())
            .filter(|line| !line.is_empty())
            .collect())
    } else {
        let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("ssh exited with status {}", status.code().unwrap_or(-1))
        } else {
            stderr
        })
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
mod tests;
