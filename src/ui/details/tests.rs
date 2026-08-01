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
