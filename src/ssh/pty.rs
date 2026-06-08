//! russh-backed SSH sessions for the multi-session terminal.
//!
//! Each session is a tokio task that owns a russh [`Channel`] with a
//! server-allocated PTY (via `request_pty` + `request_shell`), drives a
//! [`vt100::Parser`], and multiplexes I/O over a control channel. The parsed
//! screen state is exposed via an `Arc<Mutex<vt100::Parser>>` that the render
//! loop can snapshot without blocking. Using russh over a plain TCP socket
//! avoids the local pseudo-console entirely, so the same code path works on
//! every OS (notably fixing the dead Windows terminal).
//!
//! [`PtyManager`] owns all active sessions and provides a simple API for the
//! application layer.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use russh::client::Handle;
use russh::ChannelMsg;
use tokio::sync::mpsc;

use crate::event::AppEvent;
use crate::ssh::client::Host;
use crate::ssh::session::{connect_and_auth, KnownHostsHandler};

/// Stable numeric identifier for a PTY session (mirrors [`crate::event::SessionId`]).
pub type SessionId = u64;

// ---------------------------------------------------------------------------
// Control messages — main thread → session task
// ---------------------------------------------------------------------------

/// A command sent to a session's owning task.
enum Ctrl {
    /// Keystrokes or pasted bytes for the remote shell.
    Input(Vec<u8>),
    /// Window resize; forwarded to the server as `window_change`.
    Resize { cols: u16, rows: u16 },
    /// Request the task to end and close the connection.
    Close,
}

// ---------------------------------------------------------------------------
// PtySession — a handle to a running session task
// ---------------------------------------------------------------------------

/// Handle to a session task. Holds the shared parser for rendering and the
/// control sender; dropping the sender (or sending [`Ctrl::Close`]) ends the
/// task, which closes the russh connection.
struct PtySession {
    /// Unique identifier for this session.
    id: SessionId,
    /// Shared VT100 parser. The task writes into it; the render loop takes a
    /// read-side snapshot. Held only for microseconds to avoid blocking.
    parser: Arc<Mutex<vt100::Parser>>,
    /// Control channel to the session task (input / resize / close).
    ctrl_tx: mpsc::UnboundedSender<Ctrl>,
}

/// Feeds bytes to the parser in 256-byte sub-chunks, releasing the lock between
/// chunks so a large burst does not starve the render loop.
fn feed_parser(parser: &Arc<Mutex<vt100::Parser>>, data: &[u8]) {
    const CHUNK: usize = 256;
    let mut off = 0;
    while off < data.len() {
        let end = (off + CHUNK).min(data.len());
        if let Ok(mut p) = parser.lock() {
            p.process(&data[off..end]);
        }
        off = end;
    }
}

/// Opens a channel and requests a remote PTY + shell (the `ssh -t` equivalent).
async fn open_shell(
    handle: &Handle<KnownHostsHandler>,
    cols: u16,
    rows: u16,
) -> Result<russh::Channel<russh::client::Msg>> {
    let channel = handle
        .channel_open_session()
        .await
        .context("open terminal channel")?;
    channel
        .request_pty(false, "xterm-256color", cols as u32, rows as u32, 0, 0, &[])
        .await
        .context("request remote pty")?;
    channel
        .request_shell(false)
        .await
        .context("request remote shell")?;
    Ok(channel)
}

/// Owns the russh channel for one session and multiplexes I/O until close/EOF.
async fn session_task(
    id: SessionId,
    host: Host,
    cols: u16,
    rows: u16,
    parser: Arc<Mutex<vt100::Parser>>,
    mut ctrl_rx: mpsc::UnboundedReceiver<Ctrl>,
    tx: mpsc::Sender<AppEvent>,
) {
    // Phase A/B: connect, authenticate, and open the remote shell. Failures are
    // reported in the status bar and tear the tab down via PtyExited.
    let result = async {
        let handle = connect_and_auth(&host).await?;
        open_shell(&handle, cols, rows).await.map(|ch| (handle, ch))
    }
    .await;
    let (_handle, mut channel) = match result {
        Ok(pair) => pair,
        Err(e) => {
            let _ = tx.send(AppEvent::Error(format!("Terminal: {e}"))).await;
            let _ = tx.send(AppEvent::PtyExited(id)).await;
            return;
        }
    };

    // Phase C: pump loop (official russh interactive idiom). `wait` is the only
    // &mut method, so a single owning task can select over it and the control
    // receiver without aliasing.
    loop {
        tokio::select! {
            msg = channel.wait() => match msg {
                // stdout and remote stderr both render in a real terminal.
                Some(ChannelMsg::Data { data })
                | Some(ChannelMsg::ExtendedData { data, .. }) => {
                    feed_parser(&parser, &data);
                    let _ = tx.send(AppEvent::PtyOutput(id)).await;
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {} // ExitStatus etc.: ignore, wait for Close.
            },
            cmd = ctrl_rx.recv() => match cmd {
                Some(Ctrl::Input(bytes)) => {
                    let _ = channel.data(&bytes[..]).await;
                }
                Some(Ctrl::Resize { cols, rows }) => {
                    let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                }
                Some(Ctrl::Close) | None => break,
            },
        }
    }

    let _ = channel.eof().await;
    let _ = tx.send(AppEvent::PtyExited(id)).await;
    // _handle drops here → russh closes the TCP connection.
}

// ---------------------------------------------------------------------------
// PtyManager
// ---------------------------------------------------------------------------

/// Manages all active terminal sessions.
///
/// Stored in [`crate::app::App`] (not in `AppState`) to avoid reference cycles.
/// Dropping `PtyManager` or calling [`PtyManager::shutdown`] closes all
/// sessions gracefully.
pub struct PtyManager {
    sessions: Vec<PtySession>,
    next_id: u64,
}

impl PtyManager {
    /// Creates an empty manager.
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            next_id: 1,
        }
    }

    /// Opens a new terminal tab for `host` and returns the assigned [`SessionId`].
    ///
    /// Returns immediately; the connection runs in a background task. Connection
    /// or auth errors surface later via `AppEvent::Error` + `PtyExited`.
    ///
    /// # Errors
    /// Currently infallible, but kept fallible so the caller's error handling
    /// (and the public API) stays unchanged.
    pub fn open(
        &mut self,
        host: &Host,
        cols: u16,
        rows: u16,
        tx: mpsc::Sender<AppEvent>,
    ) -> Result<SessionId> {
        // ProxyJump is not yet wired into the russh terminal path. Refuse rather
        // than silently connecting direct to the wrong host.
        if host.proxy_jump.is_some() {
            anyhow::bail!("ProxyJump is not yet supported in the terminal");
        }
        let id = self.next_id;
        self.next_id += 1;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 1000)));
        let (ctrl_tx, ctrl_rx) = mpsc::unbounded_channel();
        tokio::spawn(session_task(
            id,
            host.clone(),
            cols,
            rows,
            Arc::clone(&parser),
            ctrl_rx,
            tx,
        ));
        self.sessions.push(PtySession {
            id,
            parser,
            ctrl_tx,
        });
        tracing::info!("terminal session {} opened for host '{}'", id, host.name);
        Ok(id)
    }

    /// Sends raw bytes to the session identified by `id`. Unknown id is a no-op.
    ///
    /// # Errors
    /// Infallible in practice; a dropped task is ignored like a closed session.
    pub fn write(&mut self, id: SessionId, data: &[u8]) -> Result<()> {
        if let Some(s) = self.sessions.iter().find(|s| s.id == id) {
            let _ = s.ctrl_tx.send(Ctrl::Input(data.to_vec()));
        }
        Ok(())
    }

    /// Forwards a resize to the session identified by `id`. No-op if not found.
    ///
    /// The vt100 parser is resized by the app layer; the task only relays
    /// `window_change` to the server.
    ///
    /// # Errors
    /// Infallible; kept fallible to preserve the public signature.
    pub fn resize(&mut self, id: SessionId, cols: u16, rows: u16) -> Result<()> {
        if let Some(s) = self.sessions.iter().find(|s| s.id == id) {
            let _ = s.ctrl_tx.send(Ctrl::Resize { cols, rows });
        }
        Ok(())
    }

    /// Closes and removes the session with the given `id`.
    pub fn close(&mut self, id: SessionId) {
        if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            let s = self.sessions.remove(pos);
            let _ = s.ctrl_tx.send(Ctrl::Close);
            tracing::info!("terminal session {} closed", id);
        }
    }

    /// Gracefully shuts down all sessions.
    pub fn shutdown(self) {
        for s in self.sessions {
            let _ = s.ctrl_tx.send(Ctrl::Close);
        }
        // Dropping each ctrl_tx also ends its task as a backstop.
    }

    /// Returns the parser `Arc` for the session with the given `id`, if any.
    pub fn parser_for(&self, id: SessionId) -> Option<Arc<Mutex<vt100::Parser>>> {
        self.sessions
            .iter()
            .find(|s| s.id == id)
            .map(|s| Arc::clone(&s.parser))
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// key_to_bytes — convert crossterm KeyEvent to PTY stdin bytes
// ---------------------------------------------------------------------------

/// Converts a crossterm [`KeyEvent`] to the raw byte sequence that should be
/// written to PTY stdin.
///
/// Returns an empty `Vec` for events that have no meaningful byte
/// representation (e.g. lone modifier keys). The caller discards empty vecs.
///
/// This function is `pub` so `app.rs` can call it from the terminal input handler.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_tx() -> mpsc::Sender<AppEvent> {
        mpsc::channel(1).0
    }

    #[tokio::test]
    async fn open_assigns_incrementing_ids() {
        let mut mgr = PtyManager::new();
        let host = Host::default();
        let a = mgr.open(&host, 80, 24, dummy_tx()).unwrap();
        let b = mgr.open(&host, 80, 24, dummy_tx()).unwrap();
        assert_eq!((a, b), (1, 2));
        assert_eq!(mgr.sessions.len(), 2);
    }

    #[tokio::test]
    async fn close_removes_only_the_target() {
        let mut mgr = PtyManager::new();
        let host = Host::default();
        let a = mgr.open(&host, 80, 24, dummy_tx()).unwrap();
        let b = mgr.open(&host, 80, 24, dummy_tx()).unwrap();
        mgr.close(a);
        assert_eq!(mgr.sessions.len(), 1);
        assert_eq!(mgr.sessions[0].id, b);
    }

    #[tokio::test]
    async fn write_and_resize_unknown_id_are_noops() {
        let mut mgr = PtyManager::new();
        assert!(mgr.write(999, b"x").is_ok());
        assert!(mgr.resize(999, 80, 24).is_ok());
    }

    #[tokio::test]
    async fn parser_for_returns_open_session_only() {
        let mut mgr = PtyManager::new();
        let id = mgr.open(&Host::default(), 80, 24, dummy_tx()).unwrap();
        assert!(mgr.parser_for(id).is_some());
        assert!(mgr.parser_for(id + 1).is_none());
    }
}
