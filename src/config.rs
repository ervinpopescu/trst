use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Config {
    pub connection: ConnectionConfig,
    pub theme: ThemeConfig,
    pub keys: KeysConfig,
}

#[derive(Deserialize, Serialize, Default)]
#[serde(default)]
pub struct ConnectionConfig {
    pub url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub timeout: Option<u64>,
}

impl Config {
    pub fn load() -> Self {
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &PathBuf) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("warning: bad config {}: {e}", path.display());
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Self::default();
                cfg.save_to(path);
                cfg
            }
            Err(e) => {
                eprintln!("warning: failed to read config {}: {e}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        self.save_to(&config_path());
    }

    pub fn save_to(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(toml) = toml::to_string_pretty(self) {
            #[cfg(unix)]
            {
                use std::io::Write;
                use std::os::unix::fs::OpenOptionsExt;
                let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
                let tmp_path = dir.join(".config.toml.tmp");
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&tmp_path)
                {
                    let _ = f.write_all(toml.as_bytes());
                    let _ = std::fs::rename(&tmp_path, path);
                }
            }
            #[cfg(not(unix))]
            {
                let _ = std::fs::write(&path, toml);
            }
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("trst")
        .join("config.toml")
}

#[derive(Deserialize, Serialize)]
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
            cursor: ColorPair {
                fg: "black".into(),
                bg: "white".into(),
            },
            selected: ColorPair {
                fg: "white".into(),
                bg: "blue".into(),
            },
            selected_cursor: ColorPair {
                fg: "black".into(),
                bg: "light_blue".into(),
            },

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

#[derive(Deserialize, Serialize, Clone)]
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

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct KeysConfig {
    pub quit: String,
    pub help: String,

    pub up: String,
    pub down: String,
    pub top: String,
    pub bottom: String,
    pub select_up: String,
    pub select_down: String,
    pub select_toggle: String,

    pub enter: String,
    pub details: String,
    pub pause: String,
    pub remove: String,
    pub delete: String,
    pub add: String,
    pub reannounce: String,
    pub verify: String,
    pub change_location: String,
    pub queue_up: String,
    pub queue_down: String,
    pub filter: String,
    pub sort: String,
    pub sort_reverse: String,
    pub edit_labels: String,
    pub sequential: String,

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
            verify: "v".into(),
            change_location: "m".into(),
            queue_up: "K".into(),
            queue_down: "J".into(),
            filter: "/".into(),
            sort: "s".into(),
            sort_reverse: "S".into(),
            edit_labels: "L".into(),
            sequential: "e".into(),

            priority_up: "+".into(),
            priority_down: "-".into(),
            toggle_wanted: "x".into(),
            back: "esc".into(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct KeyBind {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBind {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();

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
        match (self.code, code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => {
                a.eq_ignore_ascii_case(&b) && self.modifiers == modifiers
            }
            _ => self.code == code && modifiers.contains(self.modifiers),
        }
    }

    fn split_modifiers<'a>(s: &'a str, modifiers: &mut KeyModifiers) -> Option<&'a str> {
        if !s.contains('+') || s == "+" {
            return Some(s);
        }

        let mut last_split = None;
        let mut pos = 0;
        for (i, part) in s.split('+').enumerate() {
            if i == 0 {
                pos = part.len() + 1;
                continue;
            }
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

        if key_str.is_empty() {
            Some("+")
        } else {
            Some(key_str)
        }
    }
}

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
    pub change_location: KeyBind,
    pub queue_up: KeyBind,
    pub queue_down: KeyBind,
    pub filter: KeyBind,
    pub sort: KeyBind,
    pub sort_reverse: KeyBind,
    pub edit_labels: KeyBind,
    pub sequential: KeyBind,
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
            change_location: bind(&k.change_location, &defaults.change_location),
            queue_up: bind(&k.queue_up, &defaults.queue_up),
            queue_down: bind(&k.queue_down, &defaults.queue_down),
            filter: bind(&k.filter, &defaults.filter),
            sort: bind(&k.sort, &defaults.sort),
            sort_reverse: bind(&k.sort_reverse, &defaults.sort_reverse),
            edit_labels: bind(&k.edit_labels, &defaults.edit_labels),
            sequential: bind(&k.sequential, &defaults.sequential),
            priority_up: bind(&k.priority_up, &defaults.priority_up),
            priority_down: bind(&k.priority_down, &defaults.priority_down),
            toggle_wanted: bind(&k.toggle_wanted, &defaults.toggle_wanted),
            back: bind(&k.back, &defaults.back),
        }
    }
}

#[cfg(test)]
mod tests {
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
    fn test_load_from_missing_file_creates_file_with_defaults() {
        let dir = tempfile::tempdir().expect("tempdir");
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
        let contents = std::fs::read_to_string(&path).expect("read config file");
        let _: Config = toml::from_str(&contents).expect("config file must be valid TOML");
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
}
