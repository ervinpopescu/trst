use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;
use crate::config::parse_color;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let state = &app.rsync_state;

    let chunks = Layout::vertical([
        Constraint::Length(area.height / 3),
        Constraint::Min(4),
        Constraint::Length(3),
    ])
    .split(area);

    draw_hashes(f, state, th, chunks[0]);
    draw_log(f, state, chunks[1]);
    draw_idle(f, state, th, chunks[2]);
}

fn draw_hashes(
    f: &mut Frame,
    state: &crate::rsync::RsyncState,
    th: &crate::config::ThemeConfig,
    area: Rect,
) {
    let header_style = Style::default()
        .fg(parse_color(&th.header))
        .add_modifier(Modifier::BOLD);

    let items: Vec<ListItem> = if state.synced_hashes.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            " (none)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        state
            .synced_hashes
            .iter()
            .map(|h| ListItem::new(Line::from(format!(" {h}"))))
            .collect()
    };

    let block = Block::default()
        .title(Span::styled(" Synced hashes ", header_style))
        .borders(Borders::ALL);

    f.render_widget(List::new(items).block(block), area);
}

fn draw_log(f: &mut Frame, state: &crate::rsync::RsyncState, area: Rect) {
    let lines: Vec<Line> = if state.log_lines.is_empty() {
        vec![Line::from(Span::styled(
            " (no log entries)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        state
            .log_lines
            .iter()
            .map(|l| Line::from(format!(" {l}")))
            .collect()
    };

    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(2); // subtract borders
    let scroll = total.saturating_sub(visible);

    let block = Block::default().title(" Sync log ").borders(Borders::ALL);

    f.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
}

fn draw_idle(
    f: &mut Frame,
    state: &crate::rsync::RsyncState,
    th: &crate::config::ThemeConfig,
    area: Rect,
) {
    let text = match state.idle_seconds {
        None => Span::styled(
            " Active (idle clock not started)",
            Style::default().fg(parse_color(&th.speed_up)),
        ),
        Some(secs) => {
            let remaining = state.idle_threshold.saturating_sub(secs);
            let color = if secs >= state.idle_threshold {
                parse_color(&th.error)
            } else if remaining < 300 {
                Color::Yellow
            } else {
                Color::Green
            };
            Span::styled(
                format!(
                    " Idle {secs}s / {}s  (shutdown in {remaining}s)",
                    state.idle_threshold
                ),
                Style::default().fg(color),
            )
        }
    };

    let block = Block::default().title(" Idle ").borders(Borders::ALL);
    f.render_widget(Paragraph::new(Line::from(text)).block(block), area);
}
