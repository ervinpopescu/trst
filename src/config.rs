use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Color;
use serde::Deserialize;
use std::path::PathBuf;

// --- Top-level config ---

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    pub keys: KeysConfig,
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("warning: bad config {}: {e}", path.display());
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("trst")
        .join("config.toml")
}

// --- Theme ---

#[derive(Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub cursor: ColorPair,
    pub selected: ColorPair,
    pub selected_cursor: ColorPair,

    pub downloading: String,
    pub seeding: String,
    pub stopped: String,
    pub verifying: String,
    pub queued: String,

    pub status_bar_bg: String,
    pub status_bar_fg: String,

    pub speed_down: String,
    pub speed_up: String,
    pub error: String,

    pub priority_high: String,
    pub priority_normal: String,
    pub priority_low: String,
    pub priority_skip: String,

    pub header: String,
    pub help_key: String,
    pub help_section: String,
    pub detail_label: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            cursor: ColorPair { fg: "black".into(), bg: "white".into() },
            selected: ColorPair { fg: "white".into(), bg: "blue".into() },
            selected_cursor: ColorPair { fg: "black".into(), bg: "light_blue".into() },

            downloading: "green".into(),
            seeding: "cyan".into(),
            stopped: "dark_gray".into(),
            verifying: "magenta".into(),
            queued: "dark_gray".into(),

            status_bar_bg: "dark_gray".into(),
            status_bar_fg: "white".into(),

            speed_down: "green".into(),
            speed_up: "cyan".into(),
            error: "red".into(),

            priority_high: "red".into(),
            priority_normal: "white".into(),
            priority_low: "blue".into(),
            priority_skip: "dark_gray".into(),

            header: "yellow".into(),
            help_key: "cyan".into(),
            help_section: "yellow".into(),
            detail_label: "yellow".into(),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(default)]
pub struct ColorPair {
    pub fg: String,
    pub bg: String,
}

impl Default for ColorPair {
    fn default() -> Self {
        Self {
            fg: "white".into(),
            bg: "reset".into(),
        }
    }
}

pub fn parse_color(s: &str) -> Color {
    match s.to_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "dark_gray" | "dark_grey" | "darkgray" => Color::DarkGray,
        "light_red" | "lightred" => Color::LightRed,
        "light_green" | "lightgreen" => Color::LightGreen,
        "light_yellow" | "lightyellow" => Color::LightYellow,
        "light_blue" | "lightblue" => Color::LightBlue,
        "light_magenta" | "lightmagenta" => Color::LightMagenta,
        "light_cyan" | "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        "reset" | "default" | "" => Color::Reset,
        hex if hex.starts_with('#') && hex.len() == 7 => {
            let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(0);
            Color::Rgb(r, g, b)
        }
        _ => Color::Reset,
    }
}

// --- Keybindings ---

#[derive(Deserialize)]
#[serde(default)]
pub struct KeysConfig {
    // global
    pub quit: String,
    pub help: String,

    // navigation
    pub up: String,
    pub down: String,
    pub top: String,
    pub bottom: String,
    pub select_up: String,
    pub select_down: String,
    pub select_toggle: String,

    // torrent list
    pub enter: String,
    pub details: String,
    pub pause: String,
    pub remove: String,
    pub delete: String,
    pub add: String,
    pub reannounce: String,
    pub verify: String,
    pub queue_up: String,
    pub queue_down: String,
    pub filter: String,
    pub sort: String,
    pub sort_reverse: String,

    // file list
    pub priority_up: String,
    pub priority_down: String,
    pub toggle_wanted: String,
    pub back: String,
}

impl Default for KeysConfig {
    fn default() -> Self {
        Self {
            quit: "q".into(),
            help: "?".into(),

            up: "k".into(),
            down: "j".into(),
            top: "g".into(),
            bottom: "G".into(),
            select_up: "shift+k".into(),
            select_down: "shift+j".into(),
            select_toggle: "space".into(),

            enter: "enter".into(),
            details: "tab".into(),
            pause: "p".into(),
            remove: "d".into(),
            delete: "D".into(),
            add: "a".into(),
            reannounce: "t".into(),
            verify: "c".into(),
            queue_up: "K".into(),
            queue_down: "J".into(),
            filter: "/".into(),
            sort: "s".into(),
            sort_reverse: "S".into(),

            priority_up: "+".into(),
            priority_down: "-".into(),
            toggle_wanted: "x".into(),
            back: "esc".into(),
        }
    }
}

/// Parsed key binding ready for matching against crossterm events.
#[derive(Clone, Copy)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBind {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();

        // split modifiers from the key, handling "+" as a literal key
        let mut modifiers = KeyModifiers::empty();
        let key_part = Self::split_modifiers(s, &mut modifiers)?;

        let code = match key_part.to_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backspace" | "bs" => KeyCode::Backspace,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            s if s.len() == 1 => {
                // use original case so "S" stays uppercase
                let ch = key_part.chars().next().unwrap();
                KeyCode::Char(ch)
            }
            _ => return None,
        };

        if let KeyCode::Char(c) = code
            && c.is_ascii_uppercase()
        {
            modifiers |= KeyModifiers::SHIFT;
        }

        Some(Self { code, modifiers })
    }

    pub fn matches(&self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        // for char keys, compare case-insensitively and check shift via modifiers
        match (self.code, code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => {
                a.eq_ignore_ascii_case(&b)
                    && self.modifiers == modifiers
            }
            _ => self.code == code && modifiers.contains(self.modifiers),
        }
    }
    /// Split "ctrl+shift+x" into modifiers + key part.
    /// Handles "+" as a literal key: bare "+" or "shift++" work correctly.
    fn split_modifiers<'a>(s: &'a str, modifiers: &mut KeyModifiers) -> Option<&'a str> {
        // no "+" at all, or the string is literally "+"
        if !s.contains('+') || s == "+" {
            return Some(s);
        }

        // find the last "+" that has a known modifier before it
        // e.g. "shift++" → modifier "shift", key "+"
        // e.g. "ctrl+a" → modifier "ctrl", key "a"
        let mut last_split = None;
        let mut pos = 0;
        for (i, part) in s.split('+').enumerate() {
            if i == 0 {
                pos = part.len() + 1;
                continue;
            }
            // check if everything before this "+" is valid modifiers
            let prefix = &s[..pos - 1];
            if prefix.split('+').all(|p| {
                matches!(
                    p.to_lowercase().as_str(),
                    "shift" | "ctrl" | "control" | "alt"
                )
            }) {
                last_split = Some(pos - 1);
            }
            pos += part.len() + 1;
        }

        let split_pos = last_split?;
        let mod_str = &s[..split_pos];
        let key_str = &s[split_pos + 1..];

        for p in mod_str.split('+') {
            match p.to_lowercase().as_str() {
                "shift" => *modifiers |= KeyModifiers::SHIFT,
                "ctrl" | "control" => *modifiers |= KeyModifiers::CONTROL,
                "alt" => *modifiers |= KeyModifiers::ALT,
                _ => return None,
            }
        }

        // key part can be empty if the key is "+" itself (e.g. "shift++")
        if key_str.is_empty() {
            Some("+")
        } else {
            Some(key_str)
        }
    }
}

/// All keybindings parsed and ready to match.
pub struct Bindings {
    pub quit: KeyBind,
    pub help: KeyBind,
    pub up: KeyBind,
    pub down: KeyBind,
    pub top: KeyBind,
    pub bottom: KeyBind,
    pub select_up: KeyBind,
    pub select_down: KeyBind,
    pub select_toggle: KeyBind,
    pub enter: KeyBind,
    pub details: KeyBind,
    pub pause: KeyBind,
    pub remove: KeyBind,
    pub delete: KeyBind,
    pub add: KeyBind,
    pub reannounce: KeyBind,
    pub verify: KeyBind,
    pub queue_up: KeyBind,
    pub queue_down: KeyBind,
    pub filter: KeyBind,
    pub sort: KeyBind,
    pub sort_reverse: KeyBind,
    pub priority_up: KeyBind,
    pub priority_down: KeyBind,
    pub toggle_wanted: KeyBind,
    pub back: KeyBind,
}

fn bind(s: &str, fallback: &str) -> KeyBind {
    KeyBind::parse(s).unwrap_or_else(|| {
        eprintln!("warning: invalid keybinding \"{s}\", using default \"{fallback}\"");
        KeyBind::parse(fallback).expect("default keybinding must be valid")
    })
}

impl Bindings {
    pub fn from_config(k: &KeysConfig) -> Self {
        let defaults = KeysConfig::default();
        Self {
            quit: bind(&k.quit, &defaults.quit),
            help: bind(&k.help, &defaults.help),
            up: bind(&k.up, &defaults.up),
            down: bind(&k.down, &defaults.down),
            top: bind(&k.top, &defaults.top),
            bottom: bind(&k.bottom, &defaults.bottom),
            select_up: bind(&k.select_up, &defaults.select_up),
            select_down: bind(&k.select_down, &defaults.select_down),
            select_toggle: bind(&k.select_toggle, &defaults.select_toggle),
            enter: bind(&k.enter, &defaults.enter),
            details: bind(&k.details, &defaults.details),
            pause: bind(&k.pause, &defaults.pause),
            remove: bind(&k.remove, &defaults.remove),
            delete: bind(&k.delete, &defaults.delete),
            add: bind(&k.add, &defaults.add),
            reannounce: bind(&k.reannounce, &defaults.reannounce),
            verify: bind(&k.verify, &defaults.verify),
            queue_up: bind(&k.queue_up, &defaults.queue_up),
            queue_down: bind(&k.queue_down, &defaults.queue_down),
            filter: bind(&k.filter, &defaults.filter),
            sort: bind(&k.sort, &defaults.sort),
            sort_reverse: bind(&k.sort_reverse, &defaults.sort_reverse),
            priority_up: bind(&k.priority_up, &defaults.priority_up),
            priority_down: bind(&k.priority_down, &defaults.priority_down),
            toggle_wanted: bind(&k.toggle_wanted, &defaults.toggle_wanted),
            back: bind(&k.back, &defaults.back),
        }
    }
}
