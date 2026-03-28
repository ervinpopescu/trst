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

    let title = format!(
        " {} — files ({}) [+/- priority, x toggle, q back] ",
        torrent.name,
        torrent.files.len()
    );
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL));

    f.render_widget(table, area);
}
