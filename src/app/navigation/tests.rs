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
fn test_target_ids_cursor() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    let t1 = Torrent {
        id: 1,
        ..Default::default()
    };
    let t2 = Torrent {
        id: 2,
        ..Default::default()
    };
    app.torrents = vec![t1, t2];
    app.rebuild_filter();

    app.cursor = 0;
    assert_eq!(app.target_ids(), vec![1]);

    app.cursor = 1;
    assert_eq!(app.target_ids(), vec![2]);
}

#[test]
fn test_target_ids_selected() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    let t1 = Torrent {
        id: 10,
        ..Default::default()
    };
    let t2 = Torrent {
        id: 20,
        ..Default::default()
    };
    let t3 = Torrent {
        id: 30,
        ..Default::default()
    };
    app.torrents = vec![t1, t2, t3];
    app.rebuild_filter();

    app.selected.insert(0);
    app.selected.insert(2);
    let mut ids = app.target_ids();
    ids.sort();
    assert_eq!(ids, vec![10, 30]);
}

#[test]
fn test_clamp_cursor() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    // Empty list: cursor stays 0
    app.cursor = 5;
    app.clamp_cursor();
    assert_eq!(app.cursor, 0);

    let t1 = Torrent {
        id: 1,
        ..Default::default()
    };
    let t2 = Torrent {
        id: 2,
        ..Default::default()
    };
    app.torrents = vec![t1, t2];
    app.rebuild_filter();

    app.cursor = 10;
    app.clamp_cursor();
    assert_eq!(app.cursor, 1);

    app.cursor = 1;
    app.clamp_cursor();
    assert_eq!(app.cursor, 1);
}

#[test]
fn test_file_target_indices_cursor() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.file_cursor = 3;
    assert_eq!(app.file_target_indices(), vec![3]);
}

#[test]
fn test_target_ids_from_labels() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    let t = Torrent {
        id: 10,
        labels: vec!["foo".into(), "bar".into()],
        ..Default::default()
    };
    app.torrents = vec![t];
    app.rebuild_filter();

    assert!(!app.label_editing);
    assert!(app.label_input.is_empty());

    app.cursor = 0;
    app.selected.insert(0);

    // Edit labels trigger
    app.label_input = "foo, bar, baz".into();
    app.label_editing = true;

    // Actually submit the labels
    app.handle_label_input();

    // Should close label editing
    assert!(!app.label_editing);
    assert!(app.label_input.is_empty());

    // The dummy client will return an error because it's a dummy HTTP agent
    assert!(app.last_error.is_some());
}

#[test]
fn test_file_target_indices_selected() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.file_selected.insert(1);
    app.file_selected.insert(4);
    let mut idxs = app.file_target_indices();
    idxs.sort();
    assert_eq!(idxs, vec![1, 4]);
}

#[test]
fn test_move_down_empty_list_does_not_populate_selected() {
    let mut cursor: usize = 0;
    let mut selected = BTreeSet::new();
    App::move_down(&mut cursor, &mut selected, 0, true);
    assert!(
        selected.is_empty(),
        "move_down on empty list must not insert into selected"
    );
    assert_eq!(cursor, 0);
}

#[test]
fn test_move_up_empty_list_does_not_populate_selected() {
    let mut cursor: usize = 0;
    let mut selected = BTreeSet::new();
    App::move_up(&mut cursor, &mut selected, 0, true);
    assert!(
        selected.is_empty(),
        "move_up on empty list must not insert into selected"
    );
    assert_eq!(cursor, 0);
}

#[test]
fn test_clamp_file_cursor_no_torrent() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
    app.file_cursor = 5;
    app.clamp_file_cursor();
    assert_eq!(app.file_cursor, 0);
}

#[test]
fn test_clamp_file_cursor_with_files() {
    use crate::protocol::TorrentFile;
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );
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
        ],
        ..Default::default()
    });
    app.file_cursor = 10;
    app.clamp_file_cursor();
    assert_eq!(app.file_cursor, 1);
}

