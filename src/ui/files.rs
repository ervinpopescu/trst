use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Row, Table};

use crate::app::App;
use crate::config::parse_color;
use crate::protocol::FilePriority;
use crate::util;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let Some(torrent) = &app.detail_torrent else {
        return;
    };

    let header = Row::new(["Pri", "Name", "Size", "Done", "Progress"]).style(
        Style::default()
            .fg(parse_color(&th.header))
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = torrent
        .files
        .iter()
        .enumerate()
        .map(|(i, file)| {
            let stats = torrent.file_stats.get(i);
            let prio = stats
                .map(FilePriority::from_stats)
                .unwrap_or(FilePriority::Normal);

            let done_bytes = stats.map(|s| s.bytes_completed).unwrap_or(0);
            let fraction = if file.length > 0 {
                done_bytes as f64 / file.length as f64
            } else {
                1.0
            };

            let progress = format!(
                "{} {}",
                util::progress_bar(fraction, 10),
                util::percent(fraction),
            );

            let prio_color = match prio {
                FilePriority::Unwanted => parse_color(&th.priority_skip),
                FilePriority::Low => parse_color(&th.priority_low),
                FilePriority::Normal => parse_color(&th.priority_normal),
                FilePriority::High => parse_color(&th.priority_high),
            };

            let display_name = file
                .name
                .strip_prefix(&torrent.name)
                .and_then(|s| s.strip_prefix('/'))
                .unwrap_or(&file.name);

            let cells = vec![
                ratatui::text::Text::styled(
                    prio.label().to_string(),
                    Style::default().fg(prio_color),
                ),
                ratatui::text::Text::raw(display_name.to_string()),
                ratatui::text::Text::raw(util::human_bytes(file.length)),
                ratatui::text::Text::raw(util::human_bytes(done_bytes)),
                ratatui::text::Text::raw(progress),
            ];

            let is_selected = app.file_selected.contains(&i);
            let is_cursor = i == app.file_cursor;
            let style = match (is_selected, is_cursor) {
                (true, true) => Style::default()
                    .bg(parse_color(&th.selected_cursor.bg))
                    .fg(parse_color(&th.selected_cursor.fg)),
                (true, false) => Style::default()
                    .bg(parse_color(&th.selected.bg))
                    .fg(parse_color(&th.selected.fg)),
                (false, true) => Style::default()
                    .bg(parse_color(&th.cursor.bg))
                    .fg(parse_color(&th.cursor.fg)),
                (false, false) => Style::default(),
            };

            Row::new(cells).style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(8),
        Constraint::Min(20),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(22),
    ];

    let seq_label = if torrent.sequential_download {
        "seq"
    } else {
        "no-seq"
    };
    let title = format!(
        " {} — files ({}) [{}] [+/- priority, x toggle, q back] ",
        torrent.name,
        torrent.files.len(),
        seq_label
    );
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL));

    f.render_widget(table, area);
}

#[cfg(test)]
mod tests {
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
}
