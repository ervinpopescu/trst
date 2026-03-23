use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Row, Table};
use ratatui::layout::Constraint;

use crate::app::App;
use crate::util;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let visible = app.filtered_torrents();

    let header = Row::new([
        "Status", "Name", "Size", "Progress", "↓", "↑", "ETA", "Ratio", "Peers",
    ])
    .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
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
                (true, true) => Style::default().bg(Color::LightBlue).fg(Color::Black),
                (true, false) => Style::default().bg(Color::Blue).fg(Color::White),
                (false, true) => Style::default().add_modifier(Modifier::REVERSED),
                (false, false) => style_for_status(t.status),
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

fn style_for_status(status: i64) -> Style {
    match status {
        0 => Style::default().fg(Color::DarkGray),
        1 | 2 => Style::default().fg(Color::Magenta),
        3 | 5 => Style::default().fg(Color::DarkGray),
        4 => Style::default().fg(Color::Green),
        6 => Style::default().fg(Color::Cyan),
        _ => Style::default(),
    }
}
