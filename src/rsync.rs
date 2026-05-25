use std::fs;
use std::path::{Path, PathBuf};

/// Runtime state read from rsync-torrents data files on every tick.
#[derive(Default, Clone)]
pub struct RsyncState {
    /// Hashes recorded in SYNCED_HASHES, newest-first.
    pub synced_hashes: Vec<String>,
    /// Last N lines of the sync log, newest-first.
    pub log_lines: Vec<String>,
    /// Unix timestamp from last-active file (None if file is missing/unreadable).
    pub last_active_ts: Option<u64>,
    pub idle_threshold: u64,
}

const LOG_TAIL: usize = 100;

fn xdg_data() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("~/.local/share"))
}

fn xdg_config() -> PathBuf {
    dirs::config_local_dir().unwrap_or_else(|| PathBuf::from("~/.config"))
}

fn hashes_path() -> PathBuf {
    xdg_data().join("rsync-torrents/synced-hashes")
}

fn log_path() -> PathBuf {
    xdg_data().join("rsync-torrents/sync.log")
}

fn state_file_path() -> PathBuf {
    xdg_data().join("rsync-torrents/last-active")
}

fn idle_threshold_from_config() -> u64 {
    let config = xdg_config().join("rsync-torrents/config.toml");
    let Ok(text) = fs::read_to_string(&config) else {
        return 1800;
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("idle_threshold")
            && let Some(val) = rest.trim_start_matches([' ', '=', '\t']).split('#').next()
            && let Ok(n) = val.trim().parse::<u64>()
        {
            return n;
        }
    }
    1800
}

fn read_hashes(path: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut hashes: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect();
    hashes.reverse();
    hashes
}

fn tail_log(path: &Path, n: usize) -> Vec<String> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut result: Vec<String> = lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| l.to_string())
        .collect();
    result.reverse();
    result
}

fn read_last_active_ts(state_path: &Path) -> Option<u64> {
    let text = fs::read_to_string(state_path).ok()?;
    text.trim().parse().ok()
}

impl RsyncState {
    pub fn load() -> Self {
        Self {
            synced_hashes: read_hashes(&hashes_path()),
            log_lines: tail_log(&log_path(), LOG_TAIL),
            last_active_ts: read_last_active_ts(&state_file_path()),
            idle_threshold: idle_threshold_from_config(),
        }
    }
}
