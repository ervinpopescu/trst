use super::*;
use crossterm::event::{KeyCode, KeyModifiers};

fn kb(code: KeyCode, modifiers: KeyModifiers) -> KeyBind {
    KeyBind { code, modifiers }
}

#[test]
fn test_format_bind_plain_char() {
    assert_eq!(
        format_bind(&kb(KeyCode::Char('q'), KeyModifiers::NONE)),
        "q"
    );
    assert_eq!(
        format_bind(&kb(KeyCode::Char('j'), KeyModifiers::NONE)),
        "j"
    );
}

#[test]
fn test_format_bind_special_keys() {
    assert_eq!(
        format_bind(&kb(KeyCode::Enter, KeyModifiers::NONE)),
        "enter"
    );
    assert_eq!(format_bind(&kb(KeyCode::Esc, KeyModifiers::NONE)), "esc");
    assert_eq!(
        format_bind(&kb(KeyCode::Char(' '), KeyModifiers::NONE)),
        "space"
    );
    assert_eq!(format_bind(&kb(KeyCode::Up, KeyModifiers::NONE)), "up");
    assert_eq!(format_bind(&kb(KeyCode::Down, KeyModifiers::NONE)), "down");
    assert_eq!(format_bind(&kb(KeyCode::Home, KeyModifiers::NONE)), "home");
    assert_eq!(format_bind(&kb(KeyCode::End, KeyModifiers::NONE)), "end");
    assert_eq!(format_bind(&kb(KeyCode::Delete, KeyModifiers::NONE)), "del");
}

#[test]
fn test_format_bind_ctrl() {
    assert_eq!(
        format_bind(&kb(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        "ctrl+c"
    );
}

#[test]
fn test_format_bind_uppercase_no_shift_prefix() {
    // Uppercase char already implies shift; don't add "shift+" prefix
    assert_eq!(
        format_bind(&kb(KeyCode::Char('S'), KeyModifiers::SHIFT)),
        "S"
    );
}

#[test]
fn test_format_bind_shift_non_char() {
    assert_eq!(
        format_bind(&kb(KeyCode::Up, KeyModifiers::SHIFT)),
        "shift+up"
    );
}
