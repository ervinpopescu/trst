mod details;
mod files;
mod help;
#[cfg(feature = "rsync")]
mod rsync_panel;
mod torrent_list;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AuthField, Confirm, Modal, View};
use crate::config::parse_color;
use crate::protocol::Torrent;
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
            #[cfg(feature = "rsync")]
            View::Rsync => rsync_panel::draw(f, app, chunks[0]),
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
        Some(Modal::AddUrl(s)) => {
            let suggestions = if s.starts_with('/') || s.starts_with("./") || s.starts_with("~/") {
                Some(util::get_torrent_file_suggestions(s))
            } else {
                None
            };
            draw_input(
                f,
                "Add torrent (magnet/URL/file):",
                s,
                f.area(),
                suggestions,
            )
        }
        Some(Modal::AddLocation { location, .. }) => {
            let suggestions = location_suggestions(location, app);
            draw_input(
                f,
                "Download location:",
                location,
                f.area(),
                Some(suggestions),
            )
        }
        Some(Modal::ChangeLocation(location)) => {
            let suggestions = location_suggestions(location, app);
            draw_input(
                f,
                "Change location to:",
                location,
                f.area(),
                Some(suggestions),
            )
        }
        Some(Modal::Filter) => draw_input(f, "Filter:", &app.filter_input, f.area(), None),
        Some(Modal::Auth {
            username,
            password,
            focused,
        }) => draw_auth_modal(f, username, password, *focused, f.area()),
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
    left_parts.push(Span::styled(sort_info, Style::default().fg(bar_fg)));

    let left = Line::from(left_parts);
    let right = Line::from(Span::styled(right, Style::default().fg(bar_fg)));

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

fn draw_auth_modal(f: &mut Frame, username: &str, password: &str, focused: AuthField, area: Rect) {
    let masked: String = "•".repeat(password.chars().count());
    let user_cursor = if focused == AuthField::Username {
        "█"
    } else {
        " "
    };
    let pass_cursor = if focused == AuthField::Password {
        "█"
    } else {
        " "
    };
    let user_line = format!(" Username: {username}{user_cursor}");
    let pass_line = format!(" Password: {masked}{pass_cursor}");

    let width = (user_line.width().max(pass_line.width()) as u16 + 4)
        .max(44)
        .min(area.width);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + area.height / 2 - 1;
    let popup = Rect::new(x, y, width, 4);

    f.render_widget(Clear, popup);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::raw(user_line)),
            Line::from(Span::raw(pass_line)),
        ])
        .block(
            Block::default()
                .title("Authentication required")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Yellow)),
        )
        .alignment(ratatui::layout::Alignment::Left),
        popup,
    );
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

    use crate::app::{App, Confirm, Modal};
    use crate::client::TransmissionClient;
    use crate::config::Config;
    use crate::protocol::{SessionStats, Torrent};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn make_app() -> App {
        App::new(
            TransmissionClient::new("http://dummy", None, None),
            Config::default(),
        )
    }

    fn make_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(120, 30)).unwrap()
    }

    #[test]
    fn test_draw_torrent_list_view() {
        let mut app = make_app();
        app.rebuild_filter();
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_files_view() {
        use crate::app::View;
        let mut app = make_app();
        app.view = View::Files;
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_details_view() {
        use crate::app::View;
        let mut app = make_app();
        app.view = View::Details;
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_help_open() {
        let mut app = make_app();
        app.help = Some(0);
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_status_bar_with_stats_and_error() {
        let mut app = make_app();
        app.stats = Some(SessionStats {
            torrent_count: 5,
            download_speed: 1024,
            upload_speed: 512,
            ..Default::default()
        });
        app.last_error = Some("something went wrong".into());
        app.free = Some(crate::protocol::FreeSpace {
            size_bytes: 10 * 1024 * 1024 * 1024,
            ..Default::default()
        });
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_confirm_remove() {
        let mut app = make_app();
        app.modal = Some(Modal::Confirm(Confirm::Remove));
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_confirm_delete_files() {
        let mut app = make_app();
        app.modal = Some(Modal::Confirm(Confirm::DeleteFiles));
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_confirm_delete_file_from_disk() {
        let mut app = make_app();
        app.modal = Some(Modal::Confirm(Confirm::DeleteFileFromDisk));
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_add_url() {
        let mut app = make_app();
        app.modal = Some(Modal::AddUrl("magnet:?xt=urn:btih:abc".into()));
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_add_url_with_local_path_shows_suggestions() {
        // Exercises the `if s.starts_with('/') …` branch in ui/mod.rs that calls
        // `util::get_torrent_file_suggestions` and passes suggestions to `draw_input`.
        let dir = tempfile::tempdir().unwrap();
        std::fs::File::create(dir.path().join("ubuntu.torrent")).unwrap();
        let partial = format!("{}/ubuntu", dir.path().to_str().unwrap());

        let mut app = make_app();
        app.modal = Some(Modal::AddUrl(partial));
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_filter() {
        let mut app = make_app();
        app.modal = Some(Modal::Filter);
        app.filter_input = "rust".into();
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_add_location_with_completions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("downloads")).unwrap();
        let location = dir.path().to_str().unwrap().to_string() + "/dow";
        let mut app = make_app();
        app.modal = Some(Modal::AddLocation {
            location,
            url: "magnet:?xt=urn:btih:abc".into(),
        });
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_change_location() {
        let mut app = make_app();
        app.modal = Some(Modal::ChangeLocation("/tmp/new".into()));
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_auth_password_focused() {
        use crate::app::AuthField;
        let mut app = make_app();
        app.modal = Some(Modal::Auth {
            username: "alice".into(),
            password: "secret".into(),
            focused: AuthField::Password,
        });
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_auth_username_focused() {
        use crate::app::AuthField;
        let mut app = make_app();
        app.modal = Some(Modal::Auth {
            username: String::new(),
            password: String::new(),
            focused: AuthField::Username,
        });
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_modal_auth_narrow_terminal_clamps_width() {
        use crate::app::AuthField;
        let mut app = make_app();
        app.modal = Some(Modal::Auth {
            username: "u".into(),
            password: "p".into(),
            focused: AuthField::Username,
        });
        let mut term = Terminal::new(TestBackend::new(30, 10)).unwrap();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_label_editing() {
        let mut app = make_app();
        app.label_editing = true;
        app.label_input = "linux,rust".into();
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }

    #[test]
    fn test_draw_with_torrents_in_list() {
        let mut app = make_app();
        app.torrents = vec![
            Torrent {
                id: 1,
                name: "arch-linux.iso".into(),
                status: 4,
                rate_download: 2048,
                percent_done: 0.5,
                ..Default::default()
            },
            Torrent {
                id: 2,
                name: "debian.iso".into(),
                status: 6,
                upload_ratio: 1.2,
                ..Default::default()
            },
        ];
        app.rebuild_filter();
        let mut term = make_terminal();
        term.draw(|f| super::draw(f, &app)).unwrap();
    }
}

/// Returns location suggestions for the modal dropdown.
///
/// Priority order for **localhost** (no SSH host):
/// 1. Local filesystem via `get_path_suggestions`
/// 2. Download directories already known from existing torrents
///
/// Priority order for **remote hosts**:
/// 1. SSH-populated directory cache (accurate remote dirs, filled on Tab press)
/// 2. Download directories already known from existing torrents
/// 3. Local filesystem — fallback before the first Tab press populates the cache
fn location_suggestions(input: &str, app: &App) -> Vec<String> {
    // For localhost the remote host is None and SSH is never used, so local
    // filesystem suggestions are always the most relevant source.
    if app.ssh_host().is_none() {
        let parent = util::location_parent_dir(input);
        let fs_suggestions: Vec<String> = util::get_path_suggestions(input)
            .into_iter()
            .map(|name| format!("{}{}", parent, name))
            .collect();
        if !fs_suggestions.is_empty() {
            return fs_suggestions;
        }
        return util::get_remote_dir_suggestions(input, &remote_download_dirs(&app.torrents));
    }

    if let Some((cached_dir, listing)) = &app.location_dir_cache {
        let input_parent = util::location_parent_dir(input);
        if *cached_dir == input_parent {
            let suggestions = util::get_remote_dir_suggestions(input, listing);
            if !suggestions.is_empty() || listing.iter().any(|d| d.starts_with(input)) {
                return suggestions;
            }
            // SSH was attempted for this directory (cache is present). Don't fall through
            // to the local filesystem — those paths belong to this machine, not the
            // remote host. Known torrent download dirs are still valid: the daemon
            // reported them, so they exist on the remote side.
            return util::get_remote_dir_suggestions(input, &remote_download_dirs(&app.torrents));
        }
    }
    let known = remote_download_dirs(&app.torrents);
    let torrent_suggestions = util::get_remote_dir_suggestions(input, &known);
    if !torrent_suggestions.is_empty() {
        return torrent_suggestions;
    }
    // Local filesystem: prefix bare names with the parent directory so the
    // dropdown shows full paths, consistent with every other branch.
    let parent = util::location_parent_dir(input);
    util::get_path_suggestions(input)
        .into_iter()
        .map(|name| format!("{}{}", parent, name))
        .collect()
}

/// Collects the unique, sorted download directories known to the remote daemon.
fn remote_download_dirs(torrents: &[Torrent]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut dirs: Vec<String> = torrents
        .iter()
        .filter(|t| !t.download_dir.is_empty())
        .filter(|t| seen.insert(t.download_dir.clone()))
        .map(|t| t.download_dir.clone())
        .collect();
    dirs.sort();
    dirs
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
