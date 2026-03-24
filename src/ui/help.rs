use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;
use crate::config::parse_color;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.theme;
    let k = &app.bindings;
    let section_color = parse_color(&th.help_section);
    let key_color = parse_color(&th.help_key);

    // build binding labels from actual config
    let sections = [
        (
            "Torrent list",
            vec![
                (format_bind(&k.down), "Move down"),
                (format_bind(&k.up), "Move up"),
                (format_bind(&k.select_down), "Select down"),
                (format_bind(&k.select_up), "Select up"),
                (format_bind(&k.select_toggle), "Toggle select"),
                (format_bind(&k.top), "Top"),
                (format_bind(&k.bottom), "Bottom"),
                (format_bind(&k.enter), "Open files"),
                (format_bind(&k.details), "Torrent details"),
                (format_bind(&k.pause), "Pause / resume"),
                (format_bind(&k.remove), "Remove torrent"),
                (format_bind(&k.delete), "Remove + delete files"),
                (format_bind(&k.add), "Add torrent (magnet/URL)"),
                (format_bind(&k.reannounce), "Reannounce"),
                (format_bind(&k.verify), "Verify"),
                (format_bind(&k.queue_up), "Queue up"),
                (format_bind(&k.queue_down), "Queue down"),
                (format_bind(&k.filter), "Filter by name"),
                (format_bind(&k.sort), "Cycle sort column"),
                (format_bind(&k.sort_reverse), "Toggle sort direction"),
                (format_bind(&k.quit), "Quit"),
            ],
        ),
        (
            "File list",
            vec![
                (format_bind(&k.down), "Move down"),
                (format_bind(&k.up), "Move up"),
                (format_bind(&k.select_down), "Select down"),
                (format_bind(&k.select_up), "Select up"),
                (format_bind(&k.select_toggle), "Toggle select"),
                (format_bind(&k.priority_up), "Increase priority"),
                (format_bind(&k.priority_down), "Decrease priority"),
                (format_bind(&k.toggle_wanted), "Toggle download (wanted/skip)"),
                (format_bind(&k.reannounce), "Reannounce"),
                (format_bind(&k.back), "Back to list"),
            ],
        ),
        (
            "Details",
            vec![
                (format_bind(&k.enter), "Open files"),
                (format_bind(&k.reannounce), "Reannounce"),
                (format_bind(&k.back), "Back to list"),
            ],
        ),
    ];

    let mut lines = vec![Line::raw("")];
    for (title, bindings) in &sections {
        lines.push(Line::from(Span::styled(
            format!("  {title}"),
            Style::default()
                .fg(section_color)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        for (key, desc) in bindings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {key:<16}"),
                    Style::default().fg(key_color),
                ),
                Span::raw(*desc),
            ]));
        }
        lines.push(Line::raw(""));
    }

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Keybindings — press any key to close ")
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

use crate::config::KeyBind;

fn format_bind(kb: &KeyBind) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut parts = Vec::new();
    if kb.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("ctrl");
    }
    if kb.modifiers.contains(KeyModifiers::ALT) {
        parts.push("alt");
    }
    // only show shift for non-char keys (uppercase chars already imply shift)
    let show_shift = kb.modifiers.contains(KeyModifiers::SHIFT)
        && !matches!(kb.code, KeyCode::Char(c) if c.is_ascii_uppercase());
    if show_shift {
        parts.push("shift");
    }

    let key_name = match kb.code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Delete => "del".into(),
        KeyCode::Insert => "ins".into(),
        _ => "?".into(),
    };

    if parts.is_empty() {
        key_name
    } else {
        parts.push(&key_name);
        parts.join("+")
    }
}
