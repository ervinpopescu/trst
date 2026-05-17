mod details;
mod files;
mod help;
mod torrent_list;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Confirm, Modal, View};
use crate::config::parse_color;
use crate::util;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(f.area());

    if app.help.is_some() {
        help::draw(f, app, chunks[0]);
    } else {
        match app.view {
            View::TorrentList => torrent_list::draw(f, app, chunks[0]),
            View::Files => files::draw(f, app, chunks[0]),
            View::Details => details::draw(f, app, chunks[0]),
        }
    }

    draw_status_bar(f, app, chunks[1]);

    if app.label_editing {
        draw_input(
            f,
            "Labels (comma-separated):",
            &app.label_input,
            f.area(),
            None,
        );
    }

    match &app.modal {
        Some(Modal::Confirm(c)) => draw_confirm(f, *c, f.area()),
        Some(Modal::AddUrl(s)) => draw_input(f, "Add torrent (magnet/URL):", s, f.area(), None),
        Some(Modal::AddLocation { location, .. }) => {
            let suggestions = util::get_path_suggestions(location);
            draw_input(
                f,
                "Download location:",
                location,
                f.area(),
                Some(suggestions),
            )
        }
        Some(Modal::ChangeLocation(location)) => {
            let suggestions = util::get_path_suggestions(location);
            draw_input(
                f,
                "Change location to:",
                location,
                f.area(),
                Some(suggestions),
            )
        }
        Some(Modal::Filter) => draw_input(f, "Filter:", &app.filter_input, f.area(), None),
        None => {}
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

fn truncate_input(input: &str, max_len: usize) -> String {
    let char_count = input.chars().count();
    if char_count > max_len && max_len > 3 {
        let take = max_len - 3;
        let tail: String = input.chars().skip(char_count - take).collect();
        format!("...{tail}")
    } else {
        input.to_string()
    }
}

fn draw_input(
    f: &mut Frame,
    label: &str,
    input: &str,
    area: Rect,
    completions: Option<Vec<String>>,
) {
    let max_input_len = area.width.saturating_sub(label.width() as u16 + 10) as usize;

    let display_input = truncate_input(input, max_input_len);

    let text = format!("{label} {display_input}█");

    let width = (text.width() as u16 + 4).min(area.width);
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

    if let Some(mut comps) = completions
        && !comps.is_empty()
    {
        // Cap the number of completions shown
        comps.truncate(10);
        let lines: Vec<Line> = comps.into_iter().map(Line::from).collect();

        // Calculate width based on the longest completion string
        let max_comp_len = lines.iter().map(|l| l.width()).max().unwrap_or(0);
        let comp_width = (max_comp_len as u16 + 4).min(area.width);
        let comp_height = lines.len() as u16 + 2; // +2 for borders

        // Center the dropdown below the input box
        let comp_x = area.x + (area.width.saturating_sub(comp_width)) / 2;
        let comp_popup = Rect::new(comp_x, y + 3, comp_width, comp_height);

        f.render_widget(Clear, comp_popup);
        f.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::DarkGray)),
                )
                .alignment(ratatui::layout::Alignment::Left),
            comp_popup,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_input;

    #[test]
    fn ascii_shorter_than_max_returned_as_is() {
        assert_eq!(truncate_input("hello", 10), "hello");
    }

    #[test]
    fn ascii_exactly_at_max_returned_as_is() {
        assert_eq!(truncate_input("hello", 5), "hello");
    }

    #[test]
    fn ascii_longer_than_max_truncated_with_prefix() {
        let result = truncate_input("abcdefghij", 7);
        assert!(
            result.starts_with("..."),
            "expected '...' prefix, got: {result}"
        );
        assert_eq!(result.chars().count(), 7);
    }

    #[test]
    fn multibyte_utf8_longer_than_max_no_panic() {
        // "héllo" is 5 chars but 6 bytes; truncating by char index must not panic
        let result = truncate_input("héllo world", 7);
        assert!(
            result.starts_with("..."),
            "expected '...' prefix, got: {result}"
        );
        assert_eq!(result.chars().count(), 7);
    }

    #[test]
    fn cjk_longer_than_max_no_panic() {
        // Each hiragana char is 3 bytes
        let result = truncate_input("こんにちは世界", 5);
        assert!(
            result.starts_with("..."),
            "expected '...' prefix, got: {result}"
        );
        assert_eq!(result.chars().count(), 5);
    }

    #[test]
    fn emoji_longer_than_max_no_panic() {
        // Each crab emoji is 4 bytes
        let result = truncate_input("🦀🦀🦀🦀🦀", 4);
        assert!(
            result.starts_with("..."),
            "expected '...' prefix, got: {result}"
        );
        assert_eq!(result.chars().count(), 4);
    }

    #[test]
    fn max_len_three_or_fewer_returns_input_as_is() {
        // max_len <= 3 should never truncate (guard condition)
        assert_eq!(truncate_input("abcdef", 3), "abcdef");
        assert_eq!(truncate_input("abcdef", 1), "abcdef");
    }
}

fn draw_centered_popup(f: &mut Frame, text: &str, area: Rect) {
    let width = (text.width() as u16 + 4).min(area.width);
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
