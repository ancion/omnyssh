//! Keybindings resolved from config strings into crossterm key codes.
//!
//! The raw [`KeybindingsConfig`] (plain strings such as `"Ctrl+T"`) lives in
//! the config layer; turning those strings into [`KeyCode`] values is a
//! TUI concern and lives here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::app_config::KeybindingsConfig;

/// A resolved key binding that may optionally require the `Ctrl` modifier.
#[derive(Debug, Clone, Copy)]
pub struct KeyBind {
    pub code: KeyCode,
    /// If true, the `Ctrl` modifier must be pressed for this binding to match.
    pub ctrl: bool,
}

impl KeyBind {
    pub fn matches(&self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        key.code == self.code && ctrl == self.ctrl
    }
}

/// Keybindings resolved from [`KeybindingsConfig`] into concrete
/// [`crossterm::event::KeyCode`] values used by the event loop.
#[derive(Debug, Clone)]
pub struct ParsedKeybindings {
    /// Key that exits the application (default: `q`).
    pub quit: KeyCode,
    /// Key that activates fuzzy search (default: `/`).
    pub search: KeyCode,
    /// Key that switches to the Dashboard screen (default: `F1`).
    pub dashboard: KeyCode,
    /// Key that switches to the File Manager screen (default: `F2`).
    pub file_manager: KeyCode,
    /// Key that switches to the Snippets screen (default: `F3`).
    pub snippets: KeyCode,
    /// Key that cycles to the next screen / switches FM panels (default: `Tab`).
    pub next_screen: KeyBind,
    /// Key that cycles terminal tabs / split panes (default: `Tab`).
    pub next_tab: KeyBind,
}

impl ParsedKeybindings {
    /// Parses a [`KeybindingsConfig`] into concrete key codes.
    ///
    /// Unknown key names fall back to the default binding so the application
    /// never becomes unusable due to a misconfiguration.
    pub fn from_config(cfg: &KeybindingsConfig) -> Self {
        let defaults = KeybindingsConfig::default();
        Self {
            quit: parse_keycode(&cfg.quit)
                .unwrap_or_else(|| parse_keycode(&defaults.quit).expect("default quit")),
            search: parse_keycode(&cfg.search)
                .unwrap_or_else(|| parse_keycode(&defaults.search).expect("default search")),
            dashboard: parse_keycode(&cfg.dashboard)
                .unwrap_or_else(|| parse_keycode(&defaults.dashboard).expect("default dashboard")),
            file_manager: parse_keycode(&cfg.file_manager).unwrap_or_else(|| {
                parse_keycode(&defaults.file_manager).expect("default file_manager")
            }),
            snippets: parse_keycode(&cfg.snippets)
                .unwrap_or_else(|| parse_keycode(&defaults.snippets).expect("default snippets")),
            next_screen: parse_keybind(&cfg.next_screen).unwrap_or_else(|| {
                parse_keybind(&defaults.next_screen).expect("default next_screen")
            }),
            next_tab: parse_keybind(&cfg.next_tab)
                .unwrap_or_else(|| parse_keybind(&defaults.next_tab).expect("default next_tab")),
        }
    }
}

impl Default for ParsedKeybindings {
    fn default() -> Self {
        Self::from_config(&KeybindingsConfig::default())
    }
}

/// Parses a key name string (from config TOML) into a [`KeyCode`].
///
/// Supported formats:
/// - Single printable character: `"q"`, `"/"`, `" "` → `KeyCode::Char(_)`
/// - `"Enter"` → `KeyCode::Enter`
/// - `"Esc"` / `"Escape"` → `KeyCode::Esc`
/// - `"Tab"` → `KeyCode::Tab`
/// - `"Backspace"` / `"BS"` → `KeyCode::Backspace`
/// - `"F1"` … `"F12"` → `KeyCode::F(_)`
/// - `"Up"`, `"Down"`, `"Left"`, `"Right"` → directional keys
///
/// Returns `None` for unrecognised strings.
pub fn parse_keycode(s: &str) -> Option<KeyCode> {
    match s {
        "Enter" => Some(KeyCode::Enter),
        "Esc" | "Escape" => Some(KeyCode::Esc),
        "Tab" => Some(KeyCode::Tab),
        "Backtab" | "BackTab" | "ShiftTab" => Some(KeyCode::BackTab),
        "Backspace" | "BS" => Some(KeyCode::Backspace),
        "Delete" | "Del" => Some(KeyCode::Delete),
        "Up" => Some(KeyCode::Up),
        "Down" => Some(KeyCode::Down),
        "Left" => Some(KeyCode::Left),
        "Right" => Some(KeyCode::Right),
        "Home" => Some(KeyCode::Home),
        "End" => Some(KeyCode::End),
        "PageUp" => Some(KeyCode::PageUp),
        "PageDown" => Some(KeyCode::PageDown),
        f if f.starts_with('F') || f.starts_with('f') => f[1..].parse::<u8>().ok().map(KeyCode::F),
        c if c.chars().count() == 1 => c.chars().next().map(KeyCode::Char),
        _ => None,
    }
}

/// Parses a key binding string into a [`KeyBind`].
///
/// Supports two formats:
/// - `"Ctrl+<key>"` — requires the `Ctrl` modifier (e.g. `"Ctrl+T"`, `"Ctrl+W"`).
///   For printable characters the key portion is lower-cased automatically.
/// - Plain key names — passed through to [`parse_keycode`] with `ctrl: false`.
///
/// # Examples
/// ```
/// # use omnyssh::keybindings::parse_keybind;
/// // Ctrl+T for screen cycling, freeing Tab for shell completion.
/// let kb = parse_keybind("Ctrl+T").unwrap();
/// assert!(kb.ctrl);
/// ```
pub fn parse_keybind(s: &str) -> Option<KeyBind> {
    // "Ctrl+<key>" format (case-insensitive prefix).
    if let Some(rest) = s
        .strip_prefix("Ctrl+")
        .or_else(|| s.strip_prefix("ctrl+"))
        .or_else(|| s.strip_prefix("CTRL+"))
    {
        // Ctrl+<char>: always lower-case so "Ctrl+T" and "Ctrl+t" both work.
        if rest.chars().count() == 1 {
            return rest.chars().next().map(|c| KeyBind {
                code: KeyCode::Char(c.to_ascii_lowercase()),
                ctrl: true,
            });
        }
        // Named key e.g. "Ctrl+Enter", "Ctrl+Tab".
        if let Some(code) = parse_keycode(rest) {
            return Some(KeyBind { code, ctrl: true });
        }
        return None;
    }
    // Plain key name — no modifier required.
    parse_keycode(s).map(|code| KeyBind { code, ctrl: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that every hard-coded default key string parses successfully.
    /// This converts a potential runtime panic in `.expect("default …")` into a
    /// compile-time-visible test failure.
    #[test]
    fn default_keybindings_parse() {
        let _kb = ParsedKeybindings::default();
    }

    #[test]
    fn parse_ctrl_combo() {
        let kb = parse_keybind("Ctrl+T").unwrap();
        assert!(kb.ctrl);
        assert_eq!(kb.code, KeyCode::Char('t'));

        let kb = parse_keybind("ctrl+w").unwrap();
        assert!(kb.ctrl);
        assert_eq!(kb.code, KeyCode::Char('w'));

        let kb = parse_keybind("CTRL+q").unwrap();
        assert!(kb.ctrl);
        assert_eq!(kb.code, KeyCode::Char('q'));
    }

    #[test]
    fn parse_plain_key() {
        let kb = parse_keybind("Tab").unwrap();
        assert!(!kb.ctrl);
        assert_eq!(kb.code, KeyCode::Tab);

        let kb = parse_keybind("F5").unwrap();
        assert!(!kb.ctrl);
    }
}
