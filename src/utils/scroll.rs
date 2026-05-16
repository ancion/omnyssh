//! Mouse-wheel handling for the embedded terminal.
//!
//! On the normal screen a wheel notch scrolls the local vt100 scrollback.
//! On the alternate screen (vim, less, htop, ...) there is no scrollback, so
//! the notch is forwarded to the foreground application instead — as native
//! mouse-wheel sequences when it requested mouse reporting, or as cursor-key
//! presses otherwise (xterm "alternate scroll" behaviour).

use vt100::{MouseProtocolEncoding, MouseProtocolMode, Screen};

/// Resolved effect of a single mouse-wheel notch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollAction {
    /// Scroll the local vt100 scrollback by this signed line delta
    /// (positive = towards older output).
    Scrollback(i16),
    /// Forward these bytes to the PTY (alternate screen is active).
    Forward(Vec<u8>),
}

/// Decides what a wheel `delta` should do given the foreground app's state.
///
/// `delta` is positive for wheel-up and negative for wheel-down; its
/// magnitude is the number of lines per notch.
pub fn resolve_scroll(delta: i16, screen: &Screen) -> ScrollAction {
    if delta == 0 || !screen.alternate_screen() {
        return ScrollAction::Scrollback(delta);
    }
    let up = delta > 0;
    if screen.mouse_protocol_mode() == MouseProtocolMode::None {
        // No mouse reporting: emulate the wheel with cursor-key presses.
        ScrollAction::Forward(arrow_key_seq(
            up,
            delta.unsigned_abs() as usize,
            screen.application_cursor(),
        ))
    } else {
        // Mouse reporting on: send a native wheel event.
        ScrollAction::Forward(wheel_mouse_seq(up, screen.mouse_protocol_encoding()))
    }
}

/// Builds a mouse-wheel report for an application that enabled mouse mode.
/// The event is reported at cell (1, 1); for a wheel notch the exact position
/// is irrelevant to every common consumer (vim, less, htop).
fn wheel_mouse_seq(up: bool, encoding: MouseProtocolEncoding) -> Vec<u8> {
    // Wheel-up = button 64, wheel-down = button 65 (xterm convention).
    let button: u16 = if up { 64 } else { 65 };
    match encoding {
        MouseProtocolEncoding::Sgr => format!("\x1b[<{button};1;1M").into_bytes(),
        // X10 / UTF-8: `ESC [ M` followed by button, column and row, each
        // offset by 32. Cell (1, 1) keeps every value in the ASCII range,
        // so the UTF-8 and X10 encodings coincide here.
        _ => vec![0x1b, b'[', b'M', (button + 32) as u8, 1 + 32, 1 + 32],
    }
}

/// Repeats a cursor up/down escape sequence `count` times. Uses the
/// application-cursor form (`ESC O A/B`) when DECCKM is active.
fn arrow_key_seq(up: bool, count: usize, application_cursor: bool) -> Vec<u8> {
    let seq: &[u8] = match (up, application_cursor) {
        (true, false) => b"\x1b[A",
        (false, false) => b"\x1b[B",
        (true, true) => b"\x1bOA",
        (false, true) => b"\x1bOB",
    };
    seq.repeat(count.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt100::Parser;

    /// Builds a parser and feeds it the given escape sequences.
    fn parser_with(seqs: &[&[u8]]) -> Parser {
        let mut p = Parser::new(24, 80, 1000);
        for s in seqs {
            p.process(s);
        }
        p
    }

    #[test]
    fn normal_screen_scrolls_scrollback() {
        let p = parser_with(&[]);
        assert_eq!(resolve_scroll(3, p.screen()), ScrollAction::Scrollback(3));
        assert_eq!(resolve_scroll(-3, p.screen()), ScrollAction::Scrollback(-3));
    }

    #[test]
    fn zero_delta_is_noop_scrollback() {
        let p = parser_with(&[b"\x1b[?1049h"]);
        assert_eq!(resolve_scroll(0, p.screen()), ScrollAction::Scrollback(0));
    }

    #[test]
    fn alt_screen_without_mouse_sends_arrow_keys() {
        let p = parser_with(&[b"\x1b[?1049h"]);
        assert_eq!(
            resolve_scroll(3, p.screen()),
            ScrollAction::Forward(b"\x1b[A\x1b[A\x1b[A".to_vec())
        );
        assert_eq!(
            resolve_scroll(-3, p.screen()),
            ScrollAction::Forward(b"\x1b[B\x1b[B\x1b[B".to_vec())
        );
    }

    #[test]
    fn alt_screen_with_application_cursor_uses_ss3() {
        let p = parser_with(&[b"\x1b[?1049h", b"\x1b[?1h"]);
        assert_eq!(
            resolve_scroll(3, p.screen()),
            ScrollAction::Forward(b"\x1bOA\x1bOA\x1bOA".to_vec())
        );
    }

    #[test]
    fn alt_screen_with_mouse_sends_x10_wheel() {
        let p = parser_with(&[b"\x1b[?1049h", b"\x1b[?1000h"]);
        assert_eq!(
            resolve_scroll(3, p.screen()),
            ScrollAction::Forward(vec![0x1b, b'[', b'M', 96, 33, 33])
        );
        assert_eq!(
            resolve_scroll(-3, p.screen()),
            ScrollAction::Forward(vec![0x1b, b'[', b'M', 97, 33, 33])
        );
    }

    #[test]
    fn alt_screen_with_sgr_mouse_sends_sgr_wheel() {
        let p = parser_with(&[b"\x1b[?1049h", b"\x1b[?1000h", b"\x1b[?1006h"]);
        assert_eq!(
            resolve_scroll(3, p.screen()),
            ScrollAction::Forward(b"\x1b[<64;1;1M".to_vec())
        );
        assert_eq!(
            resolve_scroll(-3, p.screen()),
            ScrollAction::Forward(b"\x1b[<65;1;1M".to_vec())
        );
    }
}
