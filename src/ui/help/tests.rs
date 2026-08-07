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

#[test]
fn format_bind_names_every_supported_navigation_key() {
    for (code, expected) in [
        (KeyCode::Tab, "tab"),
        (KeyCode::Backspace, "backspace"),
        (KeyCode::Left, "left"),
        (KeyCode::Right, "right"),
        (KeyCode::PageUp, "pageup"),
        (KeyCode::PageDown, "pagedown"),
        (KeyCode::Insert, "ins"),
    ] {
        assert_eq!(format_bind(&kb(code, KeyModifiers::NONE)), expected);
    }
    assert_eq!(format_bind(&kb(KeyCode::F(5), KeyModifiers::NONE)), "?");
}

#[test]
fn format_bind_orders_combined_modifiers_consistently() {
    assert_eq!(
        format_bind(&kb(
            KeyCode::Left,
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT,
        )),
        "ctrl+alt+shift+left"
    );
    assert_eq!(
        format_bind(&kb(KeyCode::Char('x'), KeyModifiers::ALT)),
        "alt+x"
    );
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn prop_format_bind_never_panics(
        char_val in any::<char>(),
        bits in 0..255u8
    ) {
        // Just verify it doesn't crash on any modifier combinations + chars
        let mods = KeyModifiers::from_bits_truncate(bits);
        let _ = format_bind(&KeyBind {
            code: KeyCode::Char(char_val),
            modifiers: mods,
        });
    }
}
