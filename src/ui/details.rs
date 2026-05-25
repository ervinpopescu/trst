use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;
use crate::config::parse_color;
use crate::util;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let Some(t) = &app.detail_torrent else {
        return;
    };

    let label_color = parse_color(&th.detail_label);

    let mut lines = vec![
        detail_line("Name", &t.name, label_color),
        detail_line("Hash", &t.hash_string, label_color),
        detail_line("Status", t.status_str(), label_color),
        detail_line("Location", &t.download_dir, label_color),
        Line::raw(""),
        detail_line("Size", &util::human_bytes(t.total_size), label_color),
        detail_line(
            "Downloaded",
            &util::human_bytes(t.downloaded_ever),
            label_color,
        ),
        detail_line("Uploaded", &util::human_bytes(t.uploaded_ever), label_color),
        detail_line("Ratio", &format!("{:.2}", t.upload_ratio), label_color),
        detail_line(
            "Progress",
            &format!(
                "{} {}",
                util::progress_bar(t.percent_done, 20),
                util::percent(t.percent_done)
            ),
            label_color,
        ),
        Line::raw(""),
        detail_line(
            "Down speed",
            &util::human_speed(t.rate_download),
            label_color,
        ),
        detail_line("Up speed", &util::human_speed(t.rate_upload), label_color),
        detail_line("ETA", &util::human_eta(t.eta), label_color),
        detail_line(
            "Seq",
            if t.sequential_download { "yes" } else { "no" },
            label_color,
        ),
        detail_line(
            "Peers",
            &format!(
                "{} connected, {} sending, {} receiving",
                t.peers_connected, t.peers_sending_to_us, t.peers_getting_from_us
            ),
            label_color,
        ),
        Line::raw(""),
        detail_line("Added", &format_timestamp(t.added_date), label_color),
        detail_line("Completed", &format_timestamp(t.done_date), label_color),
        detail_line("Queue", &t.queue_position.to_string(), label_color),
        detail_line("Files", &t.files.len().to_string(), label_color),
        Line::raw(""),
        detail_line("Trackers", &format_trackers(t), label_color),
        Line::raw(""),
        detail_line(
            "Comment",
            if t.comment.is_empty() {
                "—"
            } else {
                &t.comment
            },
            label_color,
        ),
        {
            let labels_str = if t.labels.is_empty() {
                "—".to_string()
            } else {
                t.labels.join(", ")
            };
            detail_line("Labels", &labels_str, label_color)
        },
    ];

    if t.error != 0 {
        lines.push(detail_line("Error", &t.error_string, label_color));
    }

    let title = format!(" {} [enter -> files, q back] ", t.name);
    let paragraph = Paragraph::new(lines)
        .block(Block::default().title(title).borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn detail_line(label: &str, value: &str, label_color: ratatui::style::Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {label:<14} "),
            Style::default()
                .fg(label_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn format_timestamp(ts: i64) -> String {
    if ts <= 0 {
        return "—".into();
    }
    let days = ts / 86400;
    let (y, m, d) = days_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_to_ymd(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn format_trackers(t: &crate::protocol::Torrent) -> String {
    if t.tracker_stats.is_empty() {
        return "—".into();
    }
    t.tracker_stats
        .iter()
        .map(|tr| format!("{} (S:{} L:{})", tr.host, tr.seeder_count, tr.leecher_count))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Torrent, TrackerStats};

    #[test]
    fn test_format_timestamp_zero() {
        assert_eq!(format_timestamp(0), "—");
        assert_eq!(format_timestamp(-1), "—");
    }

    #[test]
    fn test_format_timestamp_epoch() {
        // Unix epoch + 1 day = 1970-01-02
        assert_eq!(format_timestamp(86400), "1970-01-02");
        // 2024-01-01 = 1704067200
        assert_eq!(format_timestamp(1704067200), "2024-01-01");
    }

    #[test]
    fn test_days_to_ymd() {
        // day 0 = 1970-01-01
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        // day 365 = 1971-01-01
        assert_eq!(days_to_ymd(365), (1971, 1, 1));
        // day 19722 = 2023-12-31
        assert_eq!(days_to_ymd(19722), (2023, 12, 31));
    }

    #[test]
    fn test_format_trackers_empty() {
        let t = Torrent::default();
        assert_eq!(format_trackers(&t), "—");
    }

    #[test]
    fn test_format_trackers_one() {
        let mut t = Torrent::default();
        t.tracker_stats.push(TrackerStats {
            host: "tracker.example.com".into(),
            seeder_count: 5,
            leecher_count: 3,
            ..Default::default()
        });
        assert_eq!(format_trackers(&t), "tracker.example.com (S:5 L:3)");
    }

    #[test]
    fn test_draw_no_torrent_does_not_panic() {
        use crate::app::App;
        use crate::client::TransmissionClient;
        use crate::config::Config;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_draw_with_full_torrent() {
        use crate::app::App;
        use crate::client::TransmissionClient;
        use crate::config::Config;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        let mut t = Torrent {
            id: 1,
            name: "Some Movie (2024)".into(),
            hash_string: "abc123def456".into(),
            status: 6,
            download_dir: "/downloads".into(),
            total_size: 4 * 1024 * 1024 * 1024,
            downloaded_ever: 4 * 1024 * 1024 * 1024,
            uploaded_ever: 2 * 1024 * 1024 * 1024,
            upload_ratio: 0.5,
            percent_done: 1.0,
            rate_download: 0,
            rate_upload: 512 * 1024,
            eta: -1,
            sequential_download: true,
            peers_connected: 10,
            peers_sending_to_us: 3,
            peers_getting_from_us: 5,
            added_date: 1704067200,
            done_date: 1704153600,
            queue_position: 0,
            comment: "Great movie".into(),
            labels: vec!["movies".into(), "hd".into()],
            error: 1,
            error_string: "connection error".into(),
            ..Default::default()
        };
        t.tracker_stats.push(TrackerStats {
            host: "tracker.example.com".into(),
            seeder_count: 100,
            leecher_count: 20,
            ..Default::default()
        });
        app.detail_torrent = Some(t);

        let backend = TestBackend::new(100, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_draw_torrent_no_labels_no_comment() {
        use crate::app::App;
        use crate::client::TransmissionClient;
        use crate::config::Config;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        );
        app.detail_torrent = Some(Torrent {
            id: 2,
            name: "Minimal Torrent".into(),
            done_date: 0,
            added_date: 0,
            ..Default::default()
        });

        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &app, f.area())).unwrap();
    }

    #[test]
    fn test_format_trackers_multiple() {
        let mut t = Torrent::default();
        t.tracker_stats.push(TrackerStats {
            host: "a.example.com".into(),
            seeder_count: 1,
            leecher_count: 0,
            ..Default::default()
        });
        t.tracker_stats.push(TrackerStats {
            host: "b.example.com".into(),
            seeder_count: 2,
            leecher_count: 1,
            ..Default::default()
        });
        assert_eq!(
            format_trackers(&t),
            "a.example.com (S:1 L:0), b.example.com (S:2 L:1)"
        );
    }
}
