use super::*;
use std::path::PathBuf;

// ── Config::load_from tests ───────────────────────────────────────────────

/// A path that is a directory (not a regular file) causes a read error that
/// is NOT ErrorKind::NotFound.  load_from must return defaults WITHOUT trying
/// to write a new file at that path.
#[test]
fn test_load_from_non_notfound_error_returns_defaults_without_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    // The path IS the directory itself — reading it as a file is an error
    // other than NotFound.
    let path: PathBuf = dir.path().to_path_buf();

    let cfg = Config::load_from(&path);

    // Should silently return defaults.
    assert!(cfg.connection.url.is_none());
    assert!(cfg.connection.username.is_none());

    // The directory should still be a directory (no file was written there).
    assert!(
        path.is_dir(),
        "load_from must not overwrite the directory path"
    );
}

/// When the config file does not exist load_from must create it and return
/// defaults.
#[test]
fn test_load_from_missing_file_creates_file_with_defaults() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");

    assert!(!path.exists(), "precondition: file must not exist yet");

    let cfg = Config::load_from(&path);

    // Returns defaults.
    assert!(cfg.connection.url.is_none());

    // The file was created.
    assert!(
        path.exists(),
        "load_from must create the config file when missing"
    );

    // The file is valid TOML that round-trips back to Config.
    let contents = std::fs::read_to_string(&path)?;
    let _: Config = toml::from_str(&contents)?;
    Ok(())
}

#[test]
fn malformed_config_returns_defaults_without_overwriting_the_file()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    let malformed = "[connection\nurl = nope";
    std::fs::write(&path, malformed)?;

    let cfg = Config::load_from(&path);

    assert!(cfg.connection.url.is_none());
    assert_eq!(std::fs::read_to_string(path)?, malformed);
    Ok(())
}

#[test]
fn test_parse_color() {
    assert_eq!(parse_color("red"), Color::Red);
    assert_eq!(parse_color("light_blue"), Color::LightBlue);
    assert_eq!(parse_color("lightblue"), Color::LightBlue);
    assert_eq!(parse_color("#123456"), Color::Rgb(0x12, 0x34, 0x56));
    assert_eq!(parse_color(""), Color::Reset);
    assert_eq!(parse_color("invalid"), Color::Reset);
}

#[test]
fn test_keybind_parse() {
    let kb = KeyBind::parse("shift+k").unwrap();
    assert_eq!(kb.code, KeyCode::Char('k'));
    assert_eq!(kb.modifiers, KeyModifiers::SHIFT);

    let kb = KeyBind::parse("ctrl+c").unwrap();
    assert_eq!(kb.code, KeyCode::Char('c'));
    assert_eq!(kb.modifiers, KeyModifiers::CONTROL);

    let kb = KeyBind::parse("+").unwrap();
    assert_eq!(kb.code, KeyCode::Char('+'));
    assert_eq!(kb.modifiers, KeyModifiers::empty());

    let kb = KeyBind::parse("shift++").unwrap();
    assert_eq!(kb.code, KeyCode::Char('+'));
    assert_eq!(kb.modifiers, KeyModifiers::SHIFT);

    assert!(matches!(
        KeyBind::parse("alt+left"),
        Some(KeyBind {
            code: KeyCode::Left,
            modifiers: KeyModifiers::ALT,
        })
    ));
    assert!(matches!(
        KeyBind::parse("ctrl+"),
        Some(KeyBind {
            code: KeyCode::Char('+'),
            modifiers: KeyModifiers::CONTROL,
        })
    ));

    assert!(KeyBind::parse("shift+meta+x").is_none());
    assert!(KeyBind::parse("invalid_key").is_none());
}

#[test]
fn test_keybind_matches() {
    let kb = KeyBind::parse("j").unwrap();
    assert!(kb.matches(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(kb.matches(KeyCode::Char('J'), KeyModifiers::NONE));
    assert!(!kb.matches(KeyCode::Char('k'), KeyModifiers::NONE));

    let ctrl_c = KeyBind::parse("ctrl+c").unwrap();
    assert!(ctrl_c.matches(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(!ctrl_c.matches(KeyCode::Char('c'), KeyModifiers::NONE));

    let enter = KeyBind::parse("enter").unwrap();
    assert!(enter.matches(KeyCode::Enter, KeyModifiers::NONE));
    assert!(!enter.matches(KeyCode::Esc, KeyModifiers::NONE));
}

#[test]
fn test_bind_falls_back_on_invalid() {
    let kb = bind("not_a_real_key_!!!", "j");
    assert_eq!(kb.code, KeyCode::Char('j'));
    assert_eq!(kb.modifiers, KeyModifiers::NONE);
}

#[test]
fn test_bind_uses_provided_when_valid() {
    let kb = bind("ctrl+x", "j");
    assert_eq!(kb.code, KeyCode::Char('x'));
    assert_eq!(kb.modifiers, KeyModifiers::CONTROL);
}

#[test]
fn test_edit_labels_default_binding() {
    let defaults = KeysConfig::default();
    let kb = KeyBind::parse(&defaults.edit_labels).unwrap();
    assert_eq!(kb.code, crossterm::event::KeyCode::Char('L'));
}

#[test]
fn test_default_serde_functions_and_priority() {
    assert!(default_true());
    assert_eq!(default_priority_high(), FilePriorityConfig::High);

    assert_eq!(
        FilePriorityConfig::Skip.to_protocol(),
        crate::protocol::FilePriority::Unwanted
    );
    assert_eq!(
        FilePriorityConfig::Low.to_protocol(),
        crate::protocol::FilePriority::Low
    );
    assert_eq!(
        FilePriorityConfig::Normal.to_protocol(),
        crate::protocol::FilePriority::Normal
    );
    assert_eq!(
        FilePriorityConfig::High.to_protocol(),
        crate::protocol::FilePriority::High
    );

    let toml_seq = "action = \"set_sequential\"";
    let action_seq: ActionConfig = toml::from_str(toml_seq).expect("deserialize set_sequential");
    if let ActionConfig::SetSequential { enabled } = action_seq {
        assert!(enabled);
    } else {
        panic!("expected SetSequential");
    }

    let toml_prio = "action = \"prioritize_files\"";
    let action_prio: ActionConfig =
        toml::from_str(toml_prio).expect("deserialize prioritize_files");
    if let ActionConfig::PrioritizeFiles { priority, .. } = action_prio {
        assert_eq!(priority, FilePriorityConfig::High);
    } else {
        panic!("expected PrioritizeFiles");
    }
}

#[test]
fn test_events_config_toml_deserialization() {
    let toml_str = r#"
        [[on_torrent_added]]
        require_labels = ["tv"]
        require_tracker = "tracker.example"
        name_pattern = "^Show"

        [[on_torrent_added.actions]]
        action = "set_sequential"
        enabled = true

        [[on_torrent_added.actions]]
        action = "prioritize_files"
        first_alphabetical = 2
        pattern = "\\.mkv$"
        priority = "high"

        [[on_torrent_added.actions]]
        action = "set_labels"
        labels = ["auto"]

        [[on_torrent_added.actions]]
        action = "set_location"
        path = "/downloads"

        [[on_torrent_added.actions]]
        action = "execute"
        command = "echo"
        args = ["{name}"]

        [[on_torrent_added.actions]]
        action = "stop"

        [[on_torrent_added.actions]]
        action = "start"

        [[on_torrent_added.actions]]
        action = "remove"
        delete_local_data = false
    "#;

    let cfg: EventsConfig = toml::from_str(toml_str).expect("events config deserializes");
    assert_eq!(cfg.on_torrent_added.len(), 1);
    assert_eq!(cfg.on_torrent_added[0].actions.len(), 8);
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_parse_color_does_not_panic(s in ".*") {
        let _ = parse_color(&s);
    }
}
