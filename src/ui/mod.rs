mod details;
mod files;
mod help;
mod torrent_list;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{App, Confirm, View};
use crate::config::parse_color;
use crate::util;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());

    match app.view {
        View::TorrentList => torrent_list::draw(f, app, chunks[0]),
        View::Files => files::draw(f, app, chunks[0]),
        View::Details => details::draw(f, app, chunks[0]),
        View::Help => help::draw(f, app, chunks[0]),
    }

    draw_status_bar(f, app, chunks[1]);

    if let Some(confirm) = &app.confirm {
        draw_confirm(f, *confirm, f.area());
    }
    if app.adding {
        draw_input(f, "Add torrent (magnet/URL):", &app.add_input, f.area());
    }
    if app.filter_active {
        draw_input(f, "Filter:", &app.filter_input, f.area());
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let bar_bg = parse_color(&th.status_bar_bg);
    let bar_fg = parse_color(&th.status_bar_fg);
    let bar_style = Style::default().bg(bar_bg).fg(bar_fg);

    let mut left_parts: Vec<Span> = vec![];

    if let Some(stats) = &app.stats {
        left_parts.push(Span::styled(
            format!(" {} torrents", stats.torrent_count),
            Style::default().fg(bar_fg),
        ));
        left_parts.push(Span::raw("  "));
        left_parts.push(Span::styled(
            format!("↓ {}", util::human_speed(stats.download_speed)),
            Style::default().fg(parse_color(&th.speed_down)),
        ));
        left_parts.push(Span::raw("  "));
        left_parts.push(Span::styled(
            format!("↑ {}", util::human_speed(stats.upload_speed)),
            Style::default().fg(parse_color(&th.speed_up)),
        ));
    }

    if let Some(err) = &app.last_error {
        left_parts.push(Span::raw("  "));
        left_parts.push(Span::styled(
            format!("err: {err}"),
            Style::default().fg(parse_color(&th.error)),
        ));
    }

    let right = if let Some(free) = &app.free {
        format!("free: {} ", util::human_bytes(free.size_bytes))
    } else {
        String::new()
    };

    let sort_info = format!(
        " sort: {}{}",
        app.sort_column.label(),
        if app.sort_ascending { "↑" } else { "↓" }
    );
    left_parts.push(Span::styled(
        sort_info,
        Style::default().fg(Color::DarkGray),
    ));

    let left = Line::from(left_parts);
    let right = Line::from(Span::styled(right, Style::default().fg(Color::DarkGray)));

    let halves =
        Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)]).split(area);

    f.render_widget(Paragraph::new(left).style(bar_style), halves[0]);
    f.render_widget(
        Paragraph::new(right)
            .style(bar_style)
            .alignment(ratatui::layout::Alignment::Right),
        halves[1],
    );
}

fn draw_confirm(f: &mut Frame, confirm: Confirm, area: Rect) {
    let msg = match confirm {
        Confirm::Remove => "Remove torrent? (y/N)",
        Confirm::DeleteFiles => "Remove torrent AND delete files? (y/N)",
        Confirm::DeleteFileFromDisk => "Delete selected file(s) from disk? (y/N)",
    };
    draw_centered_popup(f, msg, area);
}

fn draw_input(f: &mut Frame, label: &str, input: &str, area: Rect) {
    let text = format!("{label} {input}█");
    draw_centered_popup(f, &text, area);
}

fn draw_centered_popup(f: &mut Frame, text: &str, area: Rect) {
    let width = (text.len() as u16 + 4).min(area.width);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + area.height / 2;
    let popup = Rect::new(x, y, width, 3);

    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(Style::default().fg(Color::Yellow)),
            )
            .alignment(ratatui::layout::Alignment::Center),
        popup,
    );
}
