use super::*;

#[test]
fn idle_threshold_parser_accepts_toml_assignment_and_rejects_similar_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let config = dir.path().join("config.toml");

    std::fs::write(
        &config,
        "idle_threshold_extra = 5\nidle_threshold = 900 # fifteen minutes\n",
    )?;
    assert_eq!(idle_threshold_from_config(&config), 900);

    std::fs::write(&config, "idle_threshold = 1_200\n")?;
    assert_eq!(idle_threshold_from_config(&config), 1_200);

    std::fs::write(&config, "[nested]\nidle_threshold = 60\n")?;
    assert_eq!(idle_threshold_from_config(&config), 1800);

    std::fs::write(&config, "idle_threshold = not-a-number\n")?;
    assert_eq!(idle_threshold_from_config(&config), 1800);
    assert_eq!(
        idle_threshold_from_config(&dir.path().join("missing.toml")),
        1800
    );
    Ok(())
}

#[test]
fn hashes_are_trimmed_filtered_and_returned_newest_first() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let hashes = dir.path().join("synced-hashes");
    std::fs::write(&hashes, " old-hash \n\nnew-hash\n")?;

    assert_eq!(read_hashes(&hashes), ["new-hash", "old-hash"]);
    assert!(read_hashes(&dir.path().join("missing")).is_empty());
    Ok(())
}

#[test]
fn log_tail_is_bounded_and_newest_first() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let log = dir.path().join("sync.log");
    std::fs::write(&log, "one\ntwo\nthree\nfour\n")?;

    assert_eq!(tail_log(&log, 2), ["four", "three"]);
    assert_eq!(tail_log(&log, 10), ["four", "three", "two", "one"]);
    assert!(tail_log(&log, 0).is_empty());
    assert!(tail_log(&dir.path().join("missing"), 10).is_empty());
    Ok(())
}

#[test]
fn last_active_timestamp_requires_a_nonnegative_integer() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let state = dir.path().join("last-active");

    std::fs::write(&state, " 1700000000\n")?;
    assert_eq!(read_last_active_ts(&state), Some(1_700_000_000));

    std::fs::write(&state, "-1")?;
    assert_eq!(read_last_active_ts(&state), None);
    std::fs::write(&state, "not a timestamp")?;
    assert_eq!(read_last_active_ts(&state), None);
    assert_eq!(read_last_active_ts(&dir.path().join("missing")), None);
    Ok(())
}

#[test]
fn load_from_combines_all_runtime_files_and_limits_log_history()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let hashes = dir.path().join("synced-hashes");
    let log = dir.path().join("sync.log");
    let state = dir.path().join("last-active");
    let config = dir.path().join("config.toml");

    std::fs::write(&hashes, "first\nsecond\n")?;
    let log_text = (0..105)
        .map(|line| format!("event-{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&log, log_text)?;
    std::fs::write(&state, "12345")?;
    std::fs::write(&config, "idle_threshold = 60")?;

    let loaded = RsyncState::load_from(&hashes, &log, &state, &config);
    assert_eq!(loaded.synced_hashes, ["second", "first"]);
    assert_eq!(loaded.log_lines.len(), LOG_TAIL);
    assert_eq!(
        loaded.log_lines.first().map(String::as_str),
        Some("event-104")
    );
    assert_eq!(loaded.log_lines.last().map(String::as_str), Some("event-5"));
    assert_eq!(loaded.last_active_ts, Some(12345));
    assert_eq!(loaded.idle_threshold, 60);
    Ok(())
}
