use super::*;
use crate::config::ThemeConfig;

#[test]
fn test_style_for_status_produces_fg() {
    let th = ThemeConfig::default();
    // Just verify each status variant returns a style without panicking
    for status in [0i64, 1, 2, 3, 4, 5, 6, 99] {
        let style = style_for_status(status, &th);
        assert!(style.fg.is_some(), "status {status} should have fg color");
    }
}

#[test]
fn test_style_for_status_distinct_colors() {
    let th = ThemeConfig::default();
    let stopped = style_for_status(0, &th);
    let downloading = style_for_status(4, &th);
    let seeding = style_for_status(6, &th);
    // All three should be visually distinct (different fg colors)
    assert_ne!(stopped.fg, downloading.fg);
    assert_ne!(downloading.fg, seeding.fg);
}

#[test]
fn test_draw_empty_list() {
    use crate::app::App;
    use crate::client::TransmissionClient;
    use crate::config::Config;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.rebuild_filter();

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            draw(f, &app, f.area());
        })
        .unwrap();
}

#[test]
fn test_draw_with_torrents_and_cursor() {
    use crate::app::{App, SortColumn};
    use crate::client::TransmissionClient;
    use crate::config::Config;
    use crate::protocol::Torrent;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![
        Torrent {
            id: 1,
            name: "alpha".into(),
            status: 4,
            rate_download: 1024,
            ..Default::default()
        },
        Torrent {
            id: 2,
            name: "beta".into(),
            status: 6,
            upload_ratio: 1.5,
            ..Default::default()
        },
        Torrent {
            id: 3,
            name: "gamma".into(),
            status: 0,
            ..Default::default()
        },
    ];
    app.rebuild_filter();
    app.cursor = 1;
    app.selected.insert(0);
    app.sort_column = SortColumn::Name;
    app.sort_ascending = false;

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| {
            draw(f, &app, f.area());
        })
        .unwrap();
}

#[test]
fn test_draw_selected_and_cursor_combined() {
    use crate::app::App;
    use crate::client::TransmissionClient;
    use crate::config::Config;
    use crate::protocol::Torrent;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        status: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
    app.cursor = 0;
    app.selected.insert(0);

    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| draw(f, &app, f.area())).unwrap();
}
