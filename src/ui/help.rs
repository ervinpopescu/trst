use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::App;

pub fn draw(f: &mut Frame, _app: &App, area: Rect) {
    let sections = [
        (
            "Torrent list",
            vec![
                ("j/k, ↑/↓", "Move cursor"),
                ("Shift+j/k", "Extend selection"),
                ("Space", "Toggle select"),
                ("g/G", "Top / bottom"),
                ("Enter", "Open files"),
                ("Tab", "Torrent details"),
                ("p", "Pause / resume"),
                ("d", "Remove torrent"),
                ("D", "Remove + delete files"),
                ("a", "Add torrent (magnet/URL)"),
                ("t", "Reannounce"),
                ("c", "Verify"),
                ("K/J", "Queue up / down"),
                ("/", "Filter by name"),
                ("s", "Cycle sort column"),
                ("S", "Toggle sort direction"),
                ("q", "Quit"),
            ],
        ),
        (
            "File list",
            vec![
                ("j/k, ↑/↓", "Move cursor"),
                ("Shift+j/k", "Extend selection"),
                ("Space", "Toggle select"),
                ("+/l", "Increase priority"),
                ("-/h", "Decrease priority"),
                ("q/Esc", "Back to list"),
            ],
        ),
        (
            "Details",
            vec![
                ("Enter", "Open files"),
                ("q/Esc", "Back to list"),
            ],
        ),
    ];

    let mut lines = vec![Line::raw("")];
    for (title, bindings) in &sections {
        lines.push(Line::from(Span::styled(
            format!("  {title}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        for (key, desc) in bindings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    {key:<16}"),
                    Style::default().fg(Color::Cyan),
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
