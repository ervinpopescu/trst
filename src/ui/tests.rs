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
