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
