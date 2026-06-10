//! Translation of crossterm key events into PTY stdin bytes.
//!
//! This is TUI-side input handling: the SSH core only accepts raw `&[u8]`
//! for PTY input (see [`omnyssh_core::ssh::pty::PtyManager::write`]).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Converts a crossterm [`KeyEvent`] to the raw byte sequence that should be
/// written to PTY stdin.
///
/// Returns an empty `Vec` for events that have no meaningful byte
/// representation (e.g. lone modifier keys). The caller discards empty vecs.
pub fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        // ----------------------------------------------------------------
        // Printable characters
        // ----------------------------------------------------------------
        KeyCode::Char(c) if ctrl => {
            // Control codes: Ctrl+A = 0x01, Ctrl+Z = 0x1A.
            match c {
                'a'..='z' => vec![c as u8 - b'a' + 1],
                'A'..='Z' => vec![c as u8 - b'A' + 1],
                '[' => vec![0x1b], // Ctrl+[ = ESC
                '\\' => vec![0x1c],
                ']' => vec![0x1d],
                '^' => vec![0x1e],
                '_' => vec![0x1f],
                '@' => vec![0x00], // Ctrl+@ = NUL
                _ => vec![c as u8],
            }
        }
        KeyCode::Char(c) if alt => {
            // Alt sequences: ESC + char.
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            let mut bytes = vec![0x1b];
            bytes.extend_from_slice(s.as_bytes());
            bytes
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }

        // ----------------------------------------------------------------
        // Special keys
        // ----------------------------------------------------------------
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                vec![0x1b, b'[', b'Z'] // Shift+Tab (reverse-tab)
            } else {
                vec![0x09]
            }
        }
        KeyCode::Esc => vec![0x1b],

        // ----------------------------------------------------------------
        // Cursor keys (DECCKM off — application mode not yet detected)
        // ----------------------------------------------------------------
        KeyCode::Up => vec![0x1b, b'[', b'A'],
        KeyCode::Down => vec![0x1b, b'[', b'B'],
        KeyCode::Right => vec![0x1b, b'[', b'C'],
        KeyCode::Left => vec![0x1b, b'[', b'D'],
        KeyCode::Home => vec![0x1b, b'[', b'H'],
        KeyCode::End => vec![0x1b, b'[', b'F'],

        // ----------------------------------------------------------------
        // Edit keys
        // ----------------------------------------------------------------
        KeyCode::Insert => vec![0x1b, b'[', b'2', b'~'],
        KeyCode::Delete => vec![0x1b, b'[', b'3', b'~'],
        KeyCode::PageUp => vec![0x1b, b'[', b'5', b'~'],
        KeyCode::PageDown => vec![0x1b, b'[', b'6', b'~'],

        // ----------------------------------------------------------------
        // Function keys (xterm/VT220 encoding)
        // ----------------------------------------------------------------
        KeyCode::F(1) => vec![0x1b, b'O', b'P'],
        KeyCode::F(2) => vec![0x1b, b'O', b'Q'],
        KeyCode::F(3) => vec![0x1b, b'O', b'R'],
        KeyCode::F(4) => vec![0x1b, b'O', b'S'],
        KeyCode::F(5) => vec![0x1b, b'[', b'1', b'5', b'~'],
        KeyCode::F(6) => vec![0x1b, b'[', b'1', b'7', b'~'],
        KeyCode::F(7) => vec![0x1b, b'[', b'1', b'8', b'~'],
        KeyCode::F(8) => vec![0x1b, b'[', b'1', b'9', b'~'],
        KeyCode::F(9) => vec![0x1b, b'[', b'2', b'0', b'~'],
        KeyCode::F(10) => vec![0x1b, b'[', b'2', b'1', b'~'],
        KeyCode::F(11) => vec![0x1b, b'[', b'2', b'3', b'~'],
        KeyCode::F(12) => vec![0x1b, b'[', b'2', b'4', b'~'],

        // Unknown — produce nothing so callers can skip the write.
        _ => vec![],
    }
}
