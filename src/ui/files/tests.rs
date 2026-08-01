use super::*;
use crate::app::App;
use crate::client::TransmissionClient;
use crate::config::Config;
use crate::protocol::{FileStats, Torrent, TorrentFile};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn make_app() -> App {
    App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    )
}

#[test]
fn test_draw_no_detail_torrent_does_not_panic() {
    let app = make_app();
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app, f.area())).unwrap();
}

#[test]
fn test_draw_with_files_all_priority_variants() {
    let mut app = make_app();
    app.detail_torrent = Some(Torrent {
        id: 1,
        name: "test-torrent".into(),
        sequential_download: true,
        files: vec![
            TorrentFile {
                name: "test-torrent/high.bin".into(),
                length: 1024,
                bytes_completed: 512,
            },
            TorrentFile {
                name: "test-torrent/normal.bin".into(),
                length: 2048,
                bytes_completed: 2048,
            },
            TorrentFile {
                name: "test-torrent/low.bin".into(),
                length: 512,
                bytes_completed: 0,
            },
            TorrentFile {
                name: "test-torrent/skip.bin".into(),
                length: 256,
                bytes_completed: 0,
            },
        ],
        file_stats: vec![
            FileStats {
                wanted: true,
                priority: 1,
                bytes_completed: 512,
            },
            FileStats {
                wanted: true,
                priority: 0,
                bytes_completed: 2048,
            },
            FileStats {
                wanted: true,
                priority: -1,
                bytes_completed: 0,
            },
            FileStats {
                wanted: false,
                priority: 0,
                bytes_completed: 0,
            },
        ],
        ..Default::default()
    });
    app.file_cursor = 1;
    app.file_selected.insert(0);

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app, f.area())).unwrap();
}

#[test]
fn test_draw_selected_cursor_overlap() {
    let mut app = make_app();
    app.detail_torrent = Some(Torrent {
        id: 2,
        name: "overlap".into(),
        files: vec![TorrentFile {
            name: "overlap/file.mkv".into(),
            length: 1_000_000,
            bytes_completed: 500_000,
        }],
        file_stats: vec![FileStats {
            wanted: true,
            priority: 0,
            bytes_completed: 500_000,
        }],
        ..Default::default()
    });
    app.file_cursor = 0;
    app.file_selected.insert(0);

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app, f.area())).unwrap();
}

#[test]
fn test_draw_zero_length_file() {
    let mut app = make_app();
    app.detail_torrent = Some(Torrent {
        id: 3,
        name: "empty".into(),
        files: vec![TorrentFile {
            name: "empty/zero.bin".into(),
            length: 0,
            bytes_completed: 0,
        }],
        file_stats: vec![FileStats {
            wanted: true,
            priority: 0,
            bytes_completed: 0,
        }],
        ..Default::default()
    });

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app, f.area())).unwrap();
}

#[test]
fn test_draw_name_strip_prefix() {
    let mut app = make_app();
    // name that does NOT start with torrent name → display as-is
    app.detail_torrent = Some(Torrent {
        id: 4,
        name: "my-torrent".into(),
        files: vec![TorrentFile {
            name: "other-torrent/file.bin".into(),
            length: 100,
            bytes_completed: 100,
        }],
        file_stats: vec![FileStats {
            wanted: true,
            priority: 0,
            bytes_completed: 100,
        }],
        ..Default::default()
    });

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app, f.area())).unwrap();
}
