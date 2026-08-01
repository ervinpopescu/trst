#![allow(unused_imports)]
use super::*;
use crate::client::TransmissionClient;
use crate::config::Config;
use crate::protocol::{Torrent, SessionStats, FreeSpace, TrackerStats};
use crossterm::event::{KeyCode, KeyModifiers, KeyEvent};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;


fn make_key(
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) -> crossterm::event::KeyEvent {
    use crossterm::event::{KeyEventKind, KeyEventState};
    crossterm::event::KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

fn empty_app() -> App {
    App::new(
        TransmissionClient::new("http://dummy.invalid:9091/transmission/rpc", None, None),
        Config::default(),
    )
}

fn torrent_in_list(app: &mut App) {
    app.torrents = vec![Torrent {
        id: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
}

fn make_app() -> App {
    App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    )
}

#[test]
fn test_handle_add_input_transitions() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    // State 1: Enter AddUrl modal
    app.modal = Some(Modal::AddUrl(String::new()));

    // Type "http://test"
    for c in "http://test".chars() {
        app.handle_add_input(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
    }

    match &app.modal {
        Some(Modal::AddUrl(s)) => assert_eq!(s, "http://test"),
        _ => panic!("Expected AddUrl"),
    }

    // Press Enter to go to AddLocation
    app.handle_add_input(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });

    match &app.modal {
        Some(Modal::AddLocation { url, location }) => {
            assert_eq!(url, "http://test");
            assert_eq!(location, ""); // Default is empty here
        }
        _ => panic!("Expected AddLocation"),
    }

    // Type "/downloads"
    for c in "/downloads".chars() {
        app.handle_add_input(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
    }

    // Backspace once
    app.handle_add_input(KeyEvent {
        code: KeyCode::Backspace,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });

    match &app.modal {
        Some(Modal::AddLocation { url, location }) => {
            assert_eq!(url, "http://test");
            assert_eq!(location, "/download");
        }
        _ => panic!("Expected AddLocation with /download"),
    }

    // Press Enter. In a real environment, this sends an RPC. Since client has dummy agent,
    // it fails with a string error, setting last_error and clearing modal.
    app.handle_add_input(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });

    assert!(app.modal.is_none());
    assert!(app.last_error.is_some()); // Ureq dummy agent will fail.
}

#[test]
fn test_handle_change_location_transitions() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    // State 1: Enter ChangeLocation modal
    app.modal = Some(Modal::ChangeLocation(String::new()));

    // Type "/new_path"
    for c in "/new_path".chars() {
        app.handle_add_input(KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        });
    }

    match &app.modal {
        Some(Modal::ChangeLocation(s)) => assert_eq!(s, "/new_path"),
        _ => panic!("Expected ChangeLocation"),
    }

    // Press Enter. Should close modal and set last error (due to dummy client)
    app.handle_add_input(KeyEvent {
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });

    assert!(app.modal.is_none());
    assert!(app.last_error.is_some());
}

#[test]
fn test_handle_torrent_list_key_change_location() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    app.torrents.push(crate::protocol::Torrent {
        id: 1,
        download_dir: "/default/path".to_string(),
        ..Default::default()
    });
    app.rebuild_filter();

    // select the torrent
    app.cursor = 0;

    // trigger change_location key ('m' is default)
    let key = KeyEvent {
        code: KeyCode::Char('m'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };
    app.handle_torrent_list_key(key);

    match &app.modal {
        Some(Modal::ChangeLocation(loc)) => assert_eq!(loc, "/default/path"),
        _ => panic!("Expected ChangeLocation modal with prefilled path"),
    }
}

#[test]
fn test_reannounce_empty_ids_no_error() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    // torrents is empty, so target_ids() returns []
    assert!(app.target_ids().is_empty());

    app.handle_torrent_list_key(make_key(KeyCode::Char('t'), KeyModifiers::NONE));
    assert!(
        app.last_error.is_none(),
        "reannounce with no torrents should not set last_error, got: {:?}",
        app.last_error
    );
}

#[test]
fn test_verify_empty_ids_no_error() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    assert!(app.target_ids().is_empty());

    app.handle_torrent_list_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(
        app.last_error.is_none(),
        "verify with no torrents should not set last_error, got: {:?}",
        app.last_error
    );
}

#[test]
fn test_queue_up_empty_ids_no_error() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    assert!(app.target_ids().is_empty());

    // queue_up is bound to 'K' (uppercase, so SHIFT modifier)
    app.handle_torrent_list_key(make_key(KeyCode::Char('K'), KeyModifiers::SHIFT));
    assert!(
        app.last_error.is_none(),
        "queue_up with no torrents should not set last_error, got: {:?}",
        app.last_error
    );
}

#[test]
fn test_queue_down_empty_ids_no_error() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    assert!(app.target_ids().is_empty());

    // queue_down is bound to 'J' (uppercase, so SHIFT modifier)
    app.handle_torrent_list_key(make_key(KeyCode::Char('J'), KeyModifiers::SHIFT));
    assert!(
        app.last_error.is_none(),
        "queue_down with no torrents should not set last_error, got: {:?}",
        app.last_error
    );
}

#[test]
fn test_handle_torrent_list_key_quit() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    assert!(app.running);
    app.handle_torrent_list_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(!app.running);
}

#[test]
fn test_handle_torrent_list_key_esc_quits() {
    let mut app = empty_app();
    app.handle_torrent_list_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.running);
}

#[test]
fn test_handle_torrent_list_key_help() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    assert!(app.help.is_none());
    app.handle_torrent_list_key(make_key(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.help.is_some());
}

#[test]
fn test_handle_torrent_list_key_navigate() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![
        Torrent {
            id: 1,
            ..Default::default()
        },
        Torrent {
            id: 2,
            ..Default::default()
        },
        Torrent {
            id: 3,
            ..Default::default()
        },
    ];
    app.rebuild_filter();
    assert_eq!(app.cursor, 0);
    app.handle_torrent_list_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.cursor, 1);
    app.handle_torrent_list_key(make_key(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.cursor, 2);
    app.handle_torrent_list_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.cursor, 1);
    app.handle_torrent_list_key(make_key(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.cursor, 0);
    app.handle_torrent_list_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_handle_torrent_list_key_home_end() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![
        Torrent {
            id: 1,
            ..Default::default()
        },
        Torrent {
            id: 2,
            ..Default::default()
        },
        Torrent {
            id: 3,
            ..Default::default()
        },
    ];
    app.rebuild_filter();
    app.cursor = 1;
    app.handle_torrent_list_key(make_key(KeyCode::Char('g'), KeyModifiers::NONE));
    assert_eq!(app.cursor, 0);
    app.handle_torrent_list_key(make_key(KeyCode::Char('G'), KeyModifiers::SHIFT));
    assert_eq!(app.cursor, 2);
    app.handle_torrent_list_key(make_key(KeyCode::Home, KeyModifiers::NONE));
    assert_eq!(app.cursor, 0);
    app.handle_torrent_list_key(make_key(KeyCode::End, KeyModifiers::NONE));
    assert_eq!(app.cursor, 2);
}

#[test]
fn test_handle_torrent_list_key_select_toggle() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
    assert!(app.selected.is_empty());
    app.handle_torrent_list_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.selected.contains(&0));
    app.handle_torrent_list_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.selected.is_empty());
}

#[test]
fn test_handle_torrent_list_key_remove_opens_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
    app.handle_torrent_list_key(make_key(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(matches!(app.modal, Some(Modal::Confirm(Confirm::Remove))));
}

#[test]
fn test_handle_torrent_list_key_delete_opens_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
    app.handle_torrent_list_key(make_key(KeyCode::Char('D'), KeyModifiers::SHIFT));
    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm::DeleteFiles))
    ));
}

#[test]
fn test_handle_torrent_list_key_add_opens_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.handle_torrent_list_key(make_key(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(matches!(app.modal, Some(Modal::AddUrl(_))));
}

#[test]
fn test_handle_torrent_list_key_filter_opens_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.handle_torrent_list_key(make_key(KeyCode::Char('/'), KeyModifiers::NONE));
    assert!(matches!(app.modal, Some(Modal::Filter)));
}

#[test]
fn test_handle_torrent_list_key_sort_reverse_toggles() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    assert!(app.sort_ascending);
    app.handle_torrent_list_key(make_key(KeyCode::Char('S'), KeyModifiers::SHIFT));
    assert!(!app.sort_ascending);
    app.handle_torrent_list_key(make_key(KeyCode::Char('S'), KeyModifiers::SHIFT));
    assert!(app.sort_ascending);
}

#[test]
fn test_handle_torrent_list_key_edit_labels_prefills() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        labels: vec!["foo".into(), "bar".into()],
        ..Default::default()
    }];
    app.rebuild_filter();
    assert!(!app.label_editing);
    app.handle_torrent_list_key(make_key(KeyCode::Char('L'), KeyModifiers::SHIFT));
    assert!(app.label_editing);
    assert_eq!(app.label_input, "foo, bar");
}

#[test]
fn test_handle_torrent_list_key_sequential_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        sequential_download: false,
        ..Default::default()
    }];
    app.rebuild_filter();
    app.handle_torrent_list_key(make_key(KeyCode::Char('e'), KeyModifiers::NONE));
    assert!(app.last_error.is_some());
}

#[test]
fn test_handle_torrent_list_label_editing_mode() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.label_editing = true;
    app.handle_torrent_list_key(make_key(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(app.label_input, "a");
    app.handle_torrent_list_key(make_key(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(app.label_input.is_empty());
    app.label_input = "test".into();
    app.handle_torrent_list_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.label_editing);
    assert!(app.label_input.is_empty());
}

#[test]
fn test_handle_torrent_list_key_confirm_remove_yes_clears_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
    app.modal = Some(Modal::Confirm(Confirm::Remove));
    app.handle_torrent_list_key(make_key(KeyCode::Char('y'), KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_torrent_list_key_confirm_remove_n_clears_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        ..Default::default()
    }];
    app.rebuild_filter();
    app.modal = Some(Modal::Confirm(Confirm::Remove));
    app.handle_torrent_list_key(make_key(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_filter_input_updates_filter() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![
        Torrent {
            id: 1,
            name: "xenon".into(),
            ..Default::default()
        },
        Torrent {
            id: 2,
            name: "beta".into(),
            ..Default::default()
        },
    ];
    app.rebuild_filter();
    app.modal = Some(Modal::Filter);
    app.handle_torrent_list_key(make_key(KeyCode::Char('x'), KeyModifiers::NONE));
    assert_eq!(app.filter_input, "x");
    assert_eq!(app.filtered_torrents().len(), 1);
    app.handle_torrent_list_key(make_key(KeyCode::Backspace, KeyModifiers::NONE));
    assert!(app.filter_input.is_empty());
    assert_eq!(app.filtered_torrents().len(), 2);
    app.handle_torrent_list_key(make_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_filter_input_esc_closes() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.modal = Some(Modal::Filter);
    app.handle_filter_input(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_help_key_close() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.help = Some(5);
    app.handle_help_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.help.is_none());
}

#[test]
fn test_handle_help_key_scroll_and_home() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.help = Some(0);
    app.handle_help_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.help, Some(1));
    app.handle_help_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.help, Some(0));
    app.handle_help_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.help, Some(0));
    app.help = Some(5);
    app.handle_help_key(make_key(KeyCode::Char('g'), KeyModifiers::NONE));
    assert_eq!(app.help, Some(0));
    app.handle_help_key(make_key(KeyCode::PageDown, KeyModifiers::NONE));
    assert_eq!(app.help, Some(10));
    app.handle_help_key(make_key(KeyCode::PageUp, KeyModifiers::NONE));
    assert_eq!(app.help, Some(0));
}

#[test]
fn test_handle_details_key_back() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Details;
    app.handle_details_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.view, View::TorrentList);
}

#[test]
fn test_handle_details_key_help() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Details;
    assert!(app.help.is_none());
    app.handle_details_key(make_key(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.help.is_some());
}

#[test]
fn test_handle_details_key_enter_goes_to_files() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Details;
    app.handle_details_key(make_key(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.view, View::Files);
}

#[test]
fn test_handle_files_key_back() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Files;
    app.file_selected.insert(0);
    app.handle_files_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.view, View::TorrentList);
    assert!(app.file_selected.is_empty());
}

#[test]
fn test_handle_files_key_navigate() {
    use crate::protocol::TorrentFile;
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Files;
    app.detail_torrent = Some(Torrent {
        files: vec![
            TorrentFile {
                name: "a".into(),
                ..Default::default()
            },
            TorrentFile {
                name: "b".into(),
                ..Default::default()
            },
            TorrentFile {
                name: "c".into(),
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    assert_eq!(app.file_cursor, 0);
    app.handle_files_key(make_key(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.file_cursor, 1);
    app.handle_files_key(make_key(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.file_cursor, 0);
    app.handle_files_key(make_key(KeyCode::End, KeyModifiers::NONE));
    assert_eq!(app.file_cursor, 2);
    app.handle_files_key(make_key(KeyCode::Home, KeyModifiers::NONE));
    assert_eq!(app.file_cursor, 0);
    app.handle_files_key(make_key(KeyCode::Char('G'), KeyModifiers::SHIFT));
    assert_eq!(app.file_cursor, 2);
    app.handle_files_key(make_key(KeyCode::Char('g'), KeyModifiers::NONE));
    assert_eq!(app.file_cursor, 0);
}

#[test]
fn test_handle_files_key_select_toggle() {
    use crate::protocol::TorrentFile;
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Files;
    app.detail_torrent = Some(Torrent {
        files: vec![TorrentFile {
            name: "a".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    assert!(app.file_selected.is_empty());
    app.handle_files_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.file_selected.contains(&0));
    app.handle_files_key(make_key(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.file_selected.is_empty());
}

#[test]
fn test_handle_files_key_delete_opens_confirm() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Files;
    app.detail_torrent = Some(Torrent::default());
    app.handle_files_key(make_key(KeyCode::Char('D'), KeyModifiers::SHIFT));
    assert!(matches!(
        app.modal,
        Some(Modal::Confirm(Confirm::DeleteFileFromDisk))
    ));
}

#[test]
fn test_handle_files_key_confirm_delete_n_cancels() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Files;
    app.modal = Some(Modal::Confirm(Confirm::DeleteFileFromDisk));
    app.handle_files_key(make_key(KeyCode::Char('n'), KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_files_key_confirm_delete_y_clears_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy.invalid", None, None),
        Config::default(),
    );
    app.view = View::Files;
    app.modal = Some(Modal::Confirm(Confirm::DeleteFileFromDisk));
    app.handle_files_key(make_key(KeyCode::Char('y'), KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_key_help_open_dispatches_to_help_handler() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.help = Some(5);
    app.handle_key(make_key(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(app.help.is_none());
    assert!(app.running);
}

#[test]
fn test_handle_key_dispatches_to_files_view() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Files;
    app.file_selected.insert(1);
    app.handle_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.view, View::TorrentList);
    assert!(app.file_selected.is_empty());
}

#[test]
fn test_handle_key_dispatches_to_details_view() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Details;
    app.handle_key(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.view, View::TorrentList);
}

#[test]
fn test_handle_add_input_esc_closes_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.modal = Some(Modal::AddUrl("http://test".into()));
    app.handle_add_input(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_add_input_empty_url_clears_modal() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = empty_app();
    app.modal = Some(Modal::AddUrl(String::new()));
    app.handle_add_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_add_input_tab_autocompletes_torrent_path() {
    use crossterm::event::{KeyCode, KeyModifiers};
    // Create a temp dir with a single .torrent file so Tab has a unique completion.
    let dir = tempfile::tempdir().unwrap();
    std::fs::File::create(dir.path().join("debian.torrent")).unwrap();
    let prefix = format!("{}/deb", dir.path().to_str().unwrap());

    let mut app = empty_app();
    app.modal = Some(Modal::AddUrl(prefix));
    app.handle_add_input(make_key(KeyCode::Tab, KeyModifiers::NONE));

    match &app.modal {
        Some(Modal::AddUrl(s)) => {
            assert!(
                s.ends_with("debian.torrent"),
                "Tab should complete to the .torrent file, got: {s}"
            );
        }
        _ => panic!("expected AddUrl modal after Tab"),
    }
}

#[test]
fn test_handle_add_input_torrent_file_dispatches_add_metainfo() {
    use crossterm::event::{KeyCode, KeyModifiers};
    // Create an actual (dummy-content) .torrent file that can be read.
    let dir = tempfile::tempdir().unwrap();
    let torrent_path = dir.path().join("test.torrent");
    std::fs::write(&torrent_path, b"d4:infod4:name4:teste").unwrap();
    let torrent_url = torrent_path.to_str().unwrap().to_string();

    let mut app = empty_app();
    // Simulate the state after URL entry: AddLocation with a .torrent URL.
    app.modal = Some(Modal::AddLocation {
        url: torrent_url,
        location: String::new(),
    });
    // Enter dispatches the add — the dummy client will fail at the RPC level,
    // setting last_error and clearing the modal.
    app.handle_add_input(make_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.modal.is_none(), "modal cleared after submit");
    // The dummy client can't reach a real server, so an error is expected.
    assert!(app.last_error.is_some(), "error set by failing RPC");
}

#[test]
fn test_handle_add_input_nonexistent_torrent_falls_through_to_add() {
    use crossterm::event::{KeyCode, KeyModifiers};
    // A .torrent path that does not exist on disk — fs::read returns None,
    // so the code falls through to client.add() which also fails for a dummy client.
    let mut app = empty_app();
    app.modal = Some(Modal::AddLocation {
        url: "/nonexistent/path/that/does/not.torrent".to_string(),
        location: String::new(),
    });
    app.handle_add_input(make_key(KeyCode::Enter, KeyModifiers::NONE));

    assert!(app.modal.is_none(), "modal cleared");
    assert!(app.last_error.is_some(), "error set by failing add");
}

#[test]
fn test_handle_torrent_list_key_select_down_shift() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![
        Torrent {
            id: 1,
            ..Default::default()
        },
        Torrent {
            id: 2,
            ..Default::default()
        },
        Torrent {
            id: 3,
            ..Default::default()
        },
    ];
    app.rebuild_filter();
    app.handle_torrent_list_key(make_key(KeyCode::Down, KeyModifiers::SHIFT));
    assert!(app.selected.contains(&0));
    assert!(app.selected.contains(&1));
    assert_eq!(app.cursor, 1);
}

#[test]
fn test_handle_files_key_help() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.view = View::Files;
    assert!(app.help.is_none());
    app.handle_files_key(make_key(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.help.is_some());
}

#[test]
fn test_auth_modal_tab_switches_field() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: String::new(),
        password: String::new(),
        focused: AuthField::Username,
    });
    app.handle_auth_input(make_key(KeyCode::Tab, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::Auth {
            focused: AuthField::Password,
            ..
        })
    ));
    app.handle_auth_input(make_key(KeyCode::Tab, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::Auth {
            focused: AuthField::Username,
            ..
        })
    ));
}

#[test]
fn test_auth_modal_char_input_routes_to_focused_field() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: String::new(),
        password: String::new(),
        focused: AuthField::Username,
    });
    app.handle_auth_input(make_key(KeyCode::Char('u'), KeyModifiers::NONE));
    app.handle_auth_input(make_key(KeyCode::Tab, KeyModifiers::NONE));
    app.handle_auth_input(make_key(KeyCode::Char('p'), KeyModifiers::NONE));
    match &app.modal {
        Some(Modal::Auth {
            username, password, ..
        }) => {
            assert_eq!(username, "u");
            assert_eq!(password, "p");
        }
        _ => panic!("expected Auth modal"),
    }
}

#[test]
fn test_auth_modal_esc_closes_modal() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: "u".into(),
        password: "p".into(),
        focused: AuthField::Username,
    });
    app.handle_auth_input(make_key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_auth_modal_backspace_username() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: "ab".into(),
        password: "xy".into(),
        focused: AuthField::Username,
    });
    app.handle_auth_input(make_key(KeyCode::Backspace, KeyModifiers::NONE));
    match &app.modal {
        Some(Modal::Auth {
            username, password, ..
        }) => {
            assert_eq!(username, "a");
            assert_eq!(password, "xy");
        }
        _ => panic!("expected Auth modal"),
    }
}

#[test]
fn test_auth_modal_backspace_password() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: "ab".into(),
        password: "xy".into(),
        focused: AuthField::Password,
    });
    app.handle_auth_input(make_key(KeyCode::Backspace, KeyModifiers::NONE));
    match &app.modal {
        Some(Modal::Auth {
            username, password, ..
        }) => {
            assert_eq!(username, "ab");
            assert_eq!(password, "x");
        }
        _ => panic!("expected Auth modal"),
    }
}

#[test]
fn test_auth_modal_enter_on_username_advances_to_password() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: "user".into(),
        password: "pass".into(),
        focused: AuthField::Username,
    });
    app.handle_auth_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::Auth {
            focused: AuthField::Password,
            ..
        })
    ));
}

#[test]
fn test_auth_modal_enter_on_password_submits() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: "user".into(),
        password: "pass".into(),
        focused: AuthField::Password,
    });
    app.handle_auth_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_auth_modal_down_advances_field() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: String::new(),
        password: String::new(),
        focused: AuthField::Username,
    });
    app.handle_auth_input(make_key(KeyCode::Down, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::Auth {
            focused: AuthField::Password,
            ..
        })
    ));
}

#[test]
fn test_auth_modal_down_noop_on_password() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: String::new(),
        password: String::new(),
        focused: AuthField::Password,
    });
    app.handle_auth_input(make_key(KeyCode::Down, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::Auth {
            focused: AuthField::Password,
            ..
        })
    ));
}

#[test]
fn test_auth_modal_up_reverses_field() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: String::new(),
        password: String::new(),
        focused: AuthField::Password,
    });
    app.handle_auth_input(make_key(KeyCode::Up, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::Auth {
            focused: AuthField::Username,
            ..
        })
    ));
}

#[test]
fn test_auth_modal_up_noop_on_username() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.modal = Some(Modal::Auth {
        username: String::new(),
        password: String::new(),
        focused: AuthField::Username,
    });
    app.handle_auth_input(make_key(KeyCode::Up, KeyModifiers::NONE));
    assert!(matches!(
        app.modal,
        Some(Modal::Auth {
            focused: AuthField::Username,
            ..
        })
    ));
}

#[test]
fn test_handle_torrent_list_key_enter_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    torrent_in_list(&mut app);
    app.handle_torrent_list_key(make_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_torrent_list_key_details_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    torrent_in_list(&mut app);
    app.handle_torrent_list_key(make_key(KeyCode::Tab, KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_torrent_list_key_pause_stopped_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        status: 0, // stopped
        ..Default::default()
    }];
    app.rebuild_filter();
    app.handle_torrent_list_key(make_key(KeyCode::Char('p'), KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_torrent_list_key_pause_running_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.torrents = vec![Torrent {
        id: 1,
        status: 4, // seeding
        ..Default::default()
    }];
    app.rebuild_filter();
    app.handle_torrent_list_key(make_key(KeyCode::Char('p'), KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_torrent_list_key_reannounce_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    torrent_in_list(&mut app);
    app.handle_torrent_list_key(make_key(KeyCode::Char('t'), KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_torrent_list_key_verify_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    torrent_in_list(&mut app);
    app.handle_torrent_list_key(make_key(KeyCode::Char('v'), KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_label_input_sets_error_on_dummy() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    torrent_in_list(&mut app);
    app.label_input = "tag".into();
    app.handle_label_input();
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_details_key_reannounce_sets_error_on_dummy() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.detail_torrent = Some(Torrent {
        id: 1,
        ..Default::default()
    });
    app.view = View::Details;
    app.handle_details_key(make_key(KeyCode::Char('t'), KeyModifiers::NONE));
    assert!(app.last_error.is_some());
    assert!(app.error_since.is_some());
}

#[test]
fn test_handle_auth_enter_noop_when_no_modal() {
    let mut app = make_app();
    app.handle_auth_input(make_key(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.modal.is_none());
}

#[test]
fn test_handle_tick_triggers_refresh_without_modal() {
    let mut app = make_app();
    assert!(!app.refresh_in_flight);
    app.handle_tick();
    assert!(
        app.refresh_in_flight,
        "handle_tick must trigger a refresh when idle"
    );
}

#[test]
fn test_handle_tick_skips_refresh_during_auth_modal() {
    let mut app = make_app();
    app.modal = Some(Modal::Auth {
        username: String::new(),
        password: String::new(),
        focused: AuthField::Username,
    });
    app.handle_tick();
    assert!(
        !app.refresh_in_flight,
        "handle_tick must not refresh while auth modal is open"
    );
}

#[test]
fn test_handle_tick_skips_refresh_with_help_open() {
    let mut app = make_app();
    app.help = Some(0);
    app.handle_tick();
    assert!(
        !app.refresh_in_flight,
        "handle_tick must not refresh while help is open"
    );
}

#[test]
fn test_tab_completes_change_location_modal() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("completed_dir")).unwrap();

    // Use localhost so complete_location falls back to local filesystem.
    let mut app = App::new(
        TransmissionClient::new("http://localhost:9091/rpc", None, None),
        Config::default(),
    );
    let prefix = format!("{}/comp", dir.path().to_str().unwrap());
    app.modal = Some(Modal::ChangeLocation(prefix));

    app.handle_add_input(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });

    match &app.modal {
        Some(Modal::ChangeLocation(s)) => {
            assert!(
                s.contains("completed_dir"),
                "Tab should complete to the directory"
            );
        }
        _ => panic!("expected ChangeLocation modal"),
    }
}

#[test]
fn test_tab_completes_add_location_modal() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("download_dir")).unwrap();

    let mut app = App::new(
        TransmissionClient::new("http://localhost:9091/rpc", None, None),
        Config::default(),
    );
    let prefix = format!("{}/down", dir.path().to_str().unwrap());
    app.modal = Some(Modal::AddLocation {
        url: "magnet:?xt=test".to_string(),
        location: prefix,
    });

    app.handle_add_input(KeyEvent {
        code: KeyCode::Tab,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    });

    match &app.modal {
        Some(Modal::AddLocation { location, .. }) => {
            assert!(
                location.contains("download_dir"),
                "Tab should complete to the directory"
            );
        }
        _ => panic!("expected AddLocation modal"),
    }
}

