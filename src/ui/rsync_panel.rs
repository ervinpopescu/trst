use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;
use crate::config::parse_color;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let state = &app.rsync_state;
    let th = &app.theme;
    let accent = parse_color(&th.selected.fg);

    let chunks = Layout::vertical([
        Constraint::Percentage(35),
        Constraint::Percentage(45),
        Constraint::Length(3),
    ])
    .split(area);

    // --- Synced hashes ---
    let hash_items: Vec<ListItem> = state
        .synced_hashes
        .iter()
        .map(|h| ListItem::new(h.as_str()))
        .collect();
    let hash_list = List::new(hash_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Synced hashes (newest first) ")
            .border_style(Style::default().fg(accent)),
    );
    f.render_widget(hash_list, chunks[0]);

    // --- Sync log tail ---
    let log_items: Vec<ListItem> = state
        .log_lines
        .iter()
        .map(|l| ListItem::new(l.as_str()))
        .collect();
    let log_list = List::new(log_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Sync log (last 100 lines) ")
            .border_style(Style::default().fg(accent)),
    );
    f.render_widget(log_list, chunks[1]);

    // --- Idle countdown ---
    let idle_line = match state.idle_seconds {
        None => Line::from(Span::styled(
            " rsync-torrents: state file missing (daemon active or never synced) ",
            Style::default().fg(Color::DarkGray),
        )),
        Some(idle) => {
            let threshold = state.idle_threshold;
            let remaining = threshold.saturating_sub(idle);
            let color = if idle >= threshold {
                Color::Red
            } else if remaining < threshold / 4 {
                Color::Yellow
            } else {
                Color::Green
            };
            Line::from(Span::styled(
                format!(
                    " Idle {}s / {}s  (shutdown in {}s) ",
                    idle, threshold, remaining
                ),
                Style::default().fg(color),
            ))
        }
    };

    let idle_widget = Paragraph::new(idle_line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Idle timer ")
            .border_style(Style::default().fg(accent)),
    );
    f.render_widget(idle_widget, chunks[2]);
}
