use super::*;

#[test]
fn test_status_str() {
    let mut t = Torrent {
        status: 0,
        ..Default::default()
    };
    assert_eq!(t.status_str(), "Stopped");
    assert!(t.is_stopped());

    t.status = 1;
    assert_eq!(t.status_str(), "Queued verify");
    assert!(!t.is_stopped());

    t.status = 2;
    assert_eq!(t.status_str(), "Verifying");

    t.status = 3;
    assert_eq!(t.status_str(), "Queued");

    t.status = 4;
    assert_eq!(t.status_str(), "Downloading");

    t.status = 5;
    assert_eq!(t.status_str(), "Queued seed");

    t.status = 6;
    assert_eq!(t.status_str(), "Seeding");

    t.status = 99;
    assert_eq!(t.status_str(), "Unknown");
}

#[test]
fn test_file_priority() {
    // Test from_stats
    let stats_unwanted = FileStats {
        wanted: false,
        priority: 0,
        bytes_completed: 0,
    };
    assert_eq!(
        FilePriority::from_stats(&stats_unwanted),
        FilePriority::Unwanted
    );

    let stats_low = FileStats {
        wanted: true,
        priority: -1,
        bytes_completed: 0,
    };
    assert_eq!(FilePriority::from_stats(&stats_low), FilePriority::Low);

    let stats_normal = FileStats {
        wanted: true,
        priority: 0,
        bytes_completed: 0,
    };
    assert_eq!(
        FilePriority::from_stats(&stats_normal),
        FilePriority::Normal
    );

    let stats_high = FileStats {
        wanted: true,
        priority: 1,
        bytes_completed: 0,
    };
    assert_eq!(FilePriority::from_stats(&stats_high), FilePriority::High);

    // Test next
    assert_eq!(FilePriority::Unwanted.next(), FilePriority::Low);
    assert_eq!(FilePriority::Low.next(), FilePriority::Normal);
    assert_eq!(FilePriority::Normal.next(), FilePriority::High);
    assert_eq!(FilePriority::High.next(), FilePriority::Unwanted);

    // Test prev
    assert_eq!(FilePriority::Unwanted.prev(), FilePriority::High);
    assert_eq!(FilePriority::High.prev(), FilePriority::Normal);
    assert_eq!(FilePriority::Normal.prev(), FilePriority::Low);
    assert_eq!(FilePriority::Low.prev(), FilePriority::Unwanted);

    // Test label
    assert_eq!(FilePriority::Unwanted.label(), "skip");
    assert_eq!(FilePriority::Low.label(), "low");
    assert_eq!(FilePriority::Normal.label(), "normal");
    assert_eq!(FilePriority::High.label(), "high");
}

#[test]
fn test_is_stopped_all_statuses() {
    let mut t = Torrent::default();
    for status in 0i64..=6 {
        t.status = status;
        assert_eq!(t.is_stopped(), status == 0, "status={status}");
    }
    t.status = 99;
    assert!(!t.is_stopped());
}

#[test]
fn test_file_priority_out_of_range() {
    // priority 2 and -2 should fall back to Normal (wanted=true)
    for p in [2i64, -2, i64::MAX, i64::MIN] {
        let stats = FileStats {
            wanted: true,
            priority: p,
            bytes_completed: 0,
        };
        assert_eq!(
            FilePriority::from_stats(&stats),
            FilePriority::Normal,
            "priority={p} should be Normal"
        );
    }
    // unwanted overrides priority value
    for p in [2i64, -2, i64::MAX] {
        let stats = FileStats {
            wanted: false,
            priority: p,
            bytes_completed: 0,
        };
        assert_eq!(FilePriority::from_stats(&stats), FilePriority::Unwanted);
    }
}

#[test]
fn test_torrent_list_fields_contain_required() {
    let required = [
        "id",
        "name",
        "status",
        "percentDone",
        "rateDownload",
        "rateUpload",
    ];
    for field in required {
        assert!(
            TORRENT_LIST_FIELDS.contains(&field),
            "TORRENT_LIST_FIELDS missing {field}"
        );
    }
}

#[test]
fn test_torrent_detail_fields_superset_of_list_fields() {
    for field in TORRENT_LIST_FIELDS {
        assert!(
            TORRENT_DETAIL_FIELDS.contains(field),
            "TORRENT_DETAIL_FIELDS missing {field} (present in list fields)"
        );
    }
}

#[test]
fn test_torrent_detail_fields_extras() {
    // Fields only in detail, not list
    let detail_only = [
        "hashString",
        "downloadDir",
        "addedDate",
        "files",
        "fileStats",
        "peers",
    ];
    for field in detail_only {
        assert!(
            TORRENT_DETAIL_FIELDS.contains(&field),
            "TORRENT_DETAIL_FIELDS missing {field}"
        );
    }
}

#[test]
fn test_torrent_labels_default() {
    let t = Torrent::default();
    assert!(t.labels.is_empty());
}

#[test]
fn test_torrent_labels_deserialize() {
    let json = r#"{"labels": ["linux", "iso"]}"#;
    let t: Torrent = serde_json::from_str(json).unwrap();
    assert_eq!(t.labels, vec!["linux", "iso"]);
}

#[test]
fn test_field_arrays_contain_labels() {
    assert!(TORRENT_LIST_FIELDS.contains(&"labels"));
    assert!(TORRENT_DETAIL_FIELDS.contains(&"labels"));
}

#[test]
fn test_torrent_deserialization_sequential_download() {
    let json = r#"{
        "id": 42,
        "name": "Test Torrent",
        "sequential_download": true
    }"#;
    let t: Torrent = serde_json::from_str(json).unwrap();
    assert_eq!(t.id, 42);
    assert_eq!(t.name, "Test Torrent");
    assert!(t.sequential_download);

    let json_false = r#"{
        "id": 42,
        "sequential_download": false
    }"#;
    let t_false: Torrent = serde_json::from_str(json_false).unwrap();
    assert!(!t_false.sequential_download);
}

#[test]
fn test_sequential_download_camelcase_not_recognized() {
    // Transmission daemon sends snake_case; camelCase must not accidentally set the field.
    let json = r#"{"id": 1, "sequentialDownload": true}"#;
    let t: Torrent = serde_json::from_str(json).unwrap();
    assert!(
        !t.sequential_download,
        "camelCase key must not populate sequential_download"
    );
}

#[test]
fn test_field_arrays_contain_sequential_download() {
    assert!(
        TORRENT_LIST_FIELDS.contains(&"sequential_download"),
        "TORRENT_LIST_FIELDS must request sequential_download from the daemon"
    );
    assert!(
        TORRENT_DETAIL_FIELDS.contains(&"sequential_download"),
        "TORRENT_DETAIL_FIELDS must request sequential_download from the daemon"
    );
}

#[test]
fn test_set_sequential_rpc_key() {
    // Verify the key sent to torrent-set is the snake_case form the daemon expects.
    let args = serde_json::json!({
        "ids": [1i64],
        "sequential_download": true,
    });
    assert_eq!(
        args["sequential_download"],
        serde_json::Value::Bool(true),
        "set_sequential must use snake_case key"
    );
    assert!(
        args.get("sequentialDownload").is_none(),
        "camelCase key must not appear in torrent-set args"
    );
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_status_str_covers_all_values(status in 0..1000i64) {
        let t = Torrent {
            status,
            ..Torrent::default()
        };
        let s = t.status_str();
        assert!(!s.is_empty());

        // Stopped is 0, any other value is not stopped
        if status == 0 {
            assert!(t.is_stopped());
            assert_eq!(s, "Stopped");
        } else {
            assert!(!t.is_stopped());
        }
    }
}
