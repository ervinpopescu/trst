#![allow(unused_imports)]
use super::*;
use crate::client::TransmissionClient;
use crate::config::Config;
use crate::protocol::{FreeSpace, SessionStats, Torrent, TrackerStats};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_sort_column() {
    assert_eq!(SortColumn::Name.label(), "name");
    assert_eq!(SortColumn::Name.next(), SortColumn::Size);
    assert_eq!(SortColumn::Queue.next(), SortColumn::Name);
}

#[test]
fn test_sort_column_full_cycle() {
    // Cycling through all variants starting from Name must return to Name
    let variants = [
        SortColumn::Name,
        SortColumn::Size,
        SortColumn::Progress,
        SortColumn::Down,
        SortColumn::Up,
        SortColumn::Eta,
        SortColumn::Ratio,
        SortColumn::Status,
        SortColumn::Queue,
    ];
    let n = variants.len();
    for (i, &col) in variants.iter().enumerate() {
        assert_eq!(col.next(), variants[(i + 1) % n]);
    }
}

#[test]
fn test_filtering() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    let t1 = Torrent {
        name: "ubuntu.iso".into(),
        status: 4, // Downloading
        tracker_stats: vec![TrackerStats {
            host: "tracker.ubuntu.com".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    let t2 = Torrent {
        name: "debian.iso".into(),
        status: 6, // Seeding
        tracker_stats: vec![TrackerStats {
            host: "tracker.debian.org".into(),
            ..Default::default()
        }],
        ..Default::default()
    };

    app.torrents = vec![t1.clone(), t2.clone()];
    app.rebuild_filter();

    assert_eq!(app.filtered_torrents().len(), 2);

    app.filter_input = "ubuntu".into();
    app.rebuild_filter();
    assert_eq!(app.filtered_torrents().len(), 1);

    app.filter_input = "status:seeding".into();
    app.rebuild_filter();
    assert_eq!(app.filtered_torrents().len(), 1);
    assert_eq!(app.filtered_torrents()[0].name, "debian.iso");

    app.filter_input = "tracker:ubuntu.com".into();
    app.rebuild_filter();
    assert_eq!(app.filtered_torrents().len(), 1);
    assert_eq!(app.filtered_torrents()[0].name, "ubuntu.iso");

    let t3 = Torrent {
        name: "arch.iso".into(),
        labels: vec!["linux".into(), "iso".into()],
        ..Default::default()
    };
    app.torrents = vec![t1.clone(), t2.clone(), t3.clone()];

    app.filter_input = "label:linux".into();
    app.rebuild_filter();
    assert_eq!(app.filtered_torrents().len(), 1);
    assert_eq!(app.filtered_torrents()[0].name, "arch.iso");

    app.filter_input = "label:ISO".into(); // case-insensitive
    app.rebuild_filter();
    assert_eq!(app.filtered_torrents().len(), 1);

    app.filter_input = "label:nonexistent".into();
    app.rebuild_filter();
    assert_eq!(app.filtered_torrents().len(), 0);
}

#[test]
fn test_sorting() {
    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    let t1 = Torrent {
        name: "B".into(),
        total_size: 100,
        percent_done: 0.5,
        rate_download: 50,
        rate_upload: 10,
        upload_ratio: 2.0,
        eta: 10,
        queue_position: 2,
        status: 1,
        ..Default::default()
    };

    let t2 = Torrent {
        name: "A".into(),
        total_size: 200,
        percent_done: 0.8,
        rate_download: 20,
        rate_upload: 30,
        upload_ratio: 1.0,
        eta: 20,
        queue_position: 1,
        status: 2,
        ..Default::default()
    };

    let mut list = vec![t1, t2];
    app.torrents = list.clone();
    app.rebuild_filter();

    // Sort by Name, asc
    app.sort_column = SortColumn::Name;
    app.sort_ascending = true;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "A");

    // Sort by Name, desc
    app.sort_ascending = false;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "B");

    // Sort by Size, asc
    app.sort_column = SortColumn::Size;
    app.sort_ascending = true;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "B");

    // Sort by Progress, desc
    app.sort_column = SortColumn::Progress;
    app.sort_ascending = false;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "A");

    // Sort by Down, desc
    app.sort_column = SortColumn::Down;
    app.sort_ascending = false;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "B");

    // Sort by Up, desc
    app.sort_column = SortColumn::Up;
    app.sort_ascending = false;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "A");

    // Sort by Ratio, asc
    app.sort_column = SortColumn::Ratio;
    app.sort_ascending = true;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "A");

    // Sort by ETA, asc
    app.sort_column = SortColumn::Eta;
    app.sort_ascending = true;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "B");

    // Sort by Queue, asc
    app.sort_column = SortColumn::Queue;
    app.sort_ascending = true;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "A");

    // Sort by Status, asc
    app.sort_column = SortColumn::Status;
    app.sort_ascending = true;
    app.sort_torrents(&mut list);
    assert_eq!(list[0].name, "B");
}

#[test]
fn test_sort_clears_selection() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut app = App::new(
        TransmissionClient::new("http://dummy", None, None),
        Config::default(),
    );

    app.torrents = vec![
        Torrent {
            id: 1,
            name: "alpha".into(),
            ..Default::default()
        },
        Torrent {
            id: 2,
            name: "beta".into(),
            ..Default::default()
        },
        Torrent {
            id: 3,
            name: "gamma".into(),
            ..Default::default()
        },
    ];
    app.rebuild_filter();

    // pre-select some indices
    app.selected.insert(0);
    app.selected.insert(2);
    assert!(!app.selected.is_empty());

    // press 's' — the default sort key
    let key = KeyEvent {
        code: KeyCode::Char('s'),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };
    app.handle_torrent_list_key(key);

    assert!(
        app.selected.is_empty(),
        "selection must be cleared when sort order changes"
    );
    // List must also be immediately re-sorted (name asc = alpha, beta, gamma)
    assert_eq!(
        app.torrents
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta", "gamma"],
        "torrents must be re-sorted immediately on keypress, not deferred to next tick"
    );
}

#[test]
fn test_sort_column_index() {
    assert_eq!(SortColumn::Status.column_index(), Some(0));
    assert_eq!(SortColumn::Name.column_index(), Some(1));
    assert_eq!(SortColumn::Size.column_index(), Some(2));
    assert_eq!(SortColumn::Progress.column_index(), Some(3));
    assert_eq!(SortColumn::Down.column_index(), Some(4));
    assert_eq!(SortColumn::Up.column_index(), Some(5));
    assert_eq!(SortColumn::Eta.column_index(), Some(6));
    assert_eq!(SortColumn::Ratio.column_index(), Some(7));
    assert_eq!(SortColumn::Queue.column_index(), None);
}
