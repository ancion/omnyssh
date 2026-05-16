//! Clipboard-paste handling for the embedded terminal.
//!
//! With bracketed paste enabled, a paste arrives as one atomic block. For the
//! terminal tab it is forwarded to the PTY — wrapped in `ESC[200~`/`ESC[201~`
//! when the foreground application requested bracketed paste, so editors like
//! `vim` insert it verbatim without auto-indent. For other input widgets it is
//! replayed as the key events it would have produced if typed.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Bracketed-paste start marker.
const PASTE_START: &str = "\x1b[200~";
/// Bracketed-paste end marker.
const PASTE_END: &str = "\x1b[201~";

/// Prepares clipboard text for delivery to the PTY.
///
/// Newlines are normalized to CR (a PTY's Enter), embedded bracketed-paste
/// markers are stripped so pasted content cannot break out of the bracket,
/// and the payload is wrapped in `ESC[200~`/`ESC[201~` when `bracketed` is set.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    // Collapse CRLF and bare LF to a single CR.
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    // Escape-injection defense: never let the content carry its own markers.
    let safe = normalized.replace(PASTE_START, "").replace(PASTE_END, "");

    let mut out = Vec::with_capacity(safe.len() + PASTE_START.len() + PASTE_END.len());
    if bracketed {
        out.extend_from_slice(PASTE_START.as_bytes());
    }
    out.extend_from_slice(safe.as_bytes());
    if bracketed {
        out.extend_from_slice(PASTE_END.as_bytes());
    }
    out
}

/// Splits clipboard text into the key events it would have produced if typed.
///
/// Used for non-terminal input widgets so they keep handling pastes exactly as
/// before bracketed paste was enabled.
pub fn paste_to_keys(text: &str) -> Vec<KeyEvent> {
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let code = match c {
            '\r' => {
                // Collapse a CRLF pair into a single Enter.
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                KeyCode::Enter
            }
            '\n' => KeyCode::Enter,
            '\t' => KeyCode::Tab,
            other => KeyCode::Char(other),
        };
        keys.push(KeyEvent::new(code, KeyModifiers::NONE));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_wraps_only_when_bracketed() {
        assert_eq!(encode_paste("abc", false), b"abc".to_vec());
        assert_eq!(encode_paste("abc", true), b"\x1b[200~abc\x1b[201~".to_vec());
    }

    #[test]
    fn encode_normalizes_newlines_to_cr() {
        assert_eq!(encode_paste("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }

    #[test]
    fn encode_strips_embedded_markers() {
        let hostile = "a\x1b[201~rm -rf\x1b[200~b";
        assert_eq!(
            encode_paste(hostile, true),
            b"\x1b[200~arm -rfb\x1b[201~".to_vec()
        );
    }

    #[test]
    fn paste_to_keys_maps_chars_and_newlines() {
        let keys = paste_to_keys("ab\r\nc");
        let codes: Vec<KeyCode> = keys.iter().map(|k| k.code).collect();
        assert_eq!(
            codes,
            vec![
                KeyCode::Char('a'),
                KeyCode::Char('b'),
                KeyCode::Enter,
                KeyCode::Char('c'),
            ]
        );
    }
}
