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

pub fn percent(fraction: f64) -> String {
    format!("{:.1}%", fraction * 100.0)
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
}
