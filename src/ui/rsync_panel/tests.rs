use super::*;
use crate::client::TransmissionClient;
use crate::config::Config;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn app(url: &str) -> App {
    App::new(TransmissionClient::new(url, None, None), Config::default())
}

fn rendered_text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn remote_daemon_renders_an_explicit_unavailable_message() {
    let app = app("http://transmission.example:9091/transmission/rpc");
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();

    terminal
        .draw(|frame| draw(frame, &app, frame.area()))
        .unwrap();

    let text = rendered_text(&terminal);
    assert!(text.contains("rsync-torrents data unavailable for remote daemons"));
    assert!(!text.contains("Synced hashes (newest first)"));
}

#[test]
fn local_daemon_renders_hashes_log_and_idle_timer() {
    let mut app = app("http://localhost:9091/transmission/rpc");
    app.rsync_state = RsyncState {
        synced_hashes: vec!["new-hash".into(), "old-hash".into()],
        log_lines: vec!["latest sync completed".into(), "sync started".into()],
        last_active_ts: Some(1),
        idle_threshold: 60,
    };
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();

    terminal
        .draw(|frame| draw(frame, &app, frame.area()))
        .unwrap();

    let text = rendered_text(&terminal);
    for expected in [
        "Synced hashes (newest first)",
        "new-hash",
        "Sync log (newest first)",
        "latest sync completed",
        "Idle timer",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text:?}");
    }
}

#[test]
fn idle_status_reports_missing_active_warning_and_expired_states() {
    let mut state = RsyncState {
        idle_threshold: 100,
        ..Default::default()
    };

    let missing = idle_line(&state, 1_000);
    assert_eq!(missing.spans[0].style.fg, Some(Color::DarkGray));
    assert!(missing.spans[0].content.contains("no state file"));

    state.last_active_ts = Some(950);
    let active = idle_line(&state, 1_000);
    assert_eq!(active.spans[0].style.fg, Some(Color::Green));
    assert!(active.spans[0].content.contains("shutdown in 50s"));

    state.last_active_ts = Some(920);
    let warning = idle_line(&state, 1_000);
    assert_eq!(warning.spans[0].style.fg, Some(Color::Yellow));
    assert!(warning.spans[0].content.contains("shutdown in 20s"));

    state.last_active_ts = Some(900);
    let expired = idle_line(&state, 1_000);
    assert_eq!(expired.spans[0].style.fg, Some(Color::Red));
    assert!(expired.spans[0].content.contains("shutdown in 0s"));
}

#[test]
fn idle_status_saturates_when_clock_precedes_last_activity() {
    let state = RsyncState {
        last_active_ts: Some(2_000),
        idle_threshold: 60,
        ..Default::default()
    };

    let line = idle_line(&state, 1_000);
    assert_eq!(line.spans[0].style.fg, Some(Color::Green));
    assert!(line.spans[0].content.contains("Idle 0s / 60s"));
    assert!(line.spans[0].content.contains("shutdown in 60s"));
}
