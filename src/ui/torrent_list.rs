use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Row, Table};
use ratatui::layout::Constraint;

use crate::app::App;
use crate::config::parse_color;
use crate::util;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let visible = app.filtered_torrents();

    let header = Row::new([
        "Status", "Name", "Size", "Progress", "↓", "↑", "ETA", "Ratio", "Peers",
    ])
    .style(Style::default().fg(parse_color(&th.header)).add_modifier(Modifier::BOLD))
    .bottom_margin(0);

    let rows: Vec<Row> = visible
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let progress = format!(
                "{} {}",
                util::progress_bar(t.percent_done, 10),
                util::percent(t.percent_done),
            );
            let peers = format!("{}/{}", t.peers_sending_to_us, t.peers_getting_from_us);
            let ratio = if t.upload_ratio < 0.0 {
                "—".into()
            } else {
                format!("{:.2}", t.upload_ratio)
            };

            let cells = [
                t.status_str().to_string(),
                t.name.clone(),
                util::human_bytes(t.total_size),
                progress,
                util::human_speed(t.rate_download),
                util::human_speed(t.rate_upload),
                util::human_eta(t.eta),
                ratio,
                peers,
            ];

            let is_selected = app.selected.contains(&i);
            let is_cursor = i == app.cursor;
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
                (false, false) => style_for_status(t.status, th),
            };

            Row::new(cells).style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(13),
        Constraint::Min(20),
        Constraint::Length(10),
        Constraint::Length(22),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Length(7),
    ];

    let title = format!(" Torrents ({}) ", visible.len());
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL))
        .row_highlight_style(Style::default());

    f.render_widget(table, area);
}

fn style_for_status(status: i64, th: &crate::config::ThemeConfig) -> Style {
    let color = match status {
        0 => parse_color(&th.stopped),
        1 | 2 => parse_color(&th.verifying),
        3 | 5 => parse_color(&th.queued),
        4 => parse_color(&th.downloading),
        6 => parse_color(&th.seeding),
        _ => parse_color("reset"),
    };
    Style::default().fg(color)
}
