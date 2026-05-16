//! Terminal multi-session screen state and the `App` methods that open,
//! switch, scroll, and paste into PTY-backed tabs.

use std::sync::{Arc, Mutex};

use super::*;
use crate::event::SessionId;
use crate::ssh::pty::PtyManager;

// ---------------------------------------------------------------------------
// Terminal multi-session view state
// ---------------------------------------------------------------------------

/// Direction of the split-view layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitDirection {
    /// Two panes side-by-side (left | right).
    Vertical,
    /// Two panes stacked (top / bottom).
    Horizontal,
}

/// Which pane currently has keyboard focus in split-view mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SplitFocus {
    #[default]
    Primary,
    Secondary,
}

/// Layout description when split-view is active.
#[derive(Debug, Clone)]
pub struct SplitView {
    pub direction: SplitDirection,
    /// Index into [`TerminalView::tabs`] shown in the secondary pane.
    pub secondary_tab: usize,
}

/// A single open SSH/PTY tab.
pub struct TermTab {
    /// Unique identifier matching the [`PtyManager`] session.
    pub session_id: SessionId,
    /// Display name (= `host.name`).
    pub host_name: String,
    /// Set to `true` when new output arrives while this tab is not focused,
    /// providing an unread-output indicator in the tab bar.
    pub has_activity: bool,
    /// Shared VT100 parser — written by the PTY reader thread, snapshotted by
    /// the render loop.  Stored here (in ViewState) so rendering does not
    /// require access to the PtyManager.
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// Number of lines scrolled back from the live screen (0 = at the bottom).
    /// Set via mouse-wheel; reset to 0 when the user types anything.
    pub scroll_offset: usize,
}

/// Host-picker popup for opening a new terminal tab.
#[derive(Debug, Clone, Default)]
pub struct TermHostPicker {
    /// Index of the currently highlighted host in `AppState.hosts`.
    pub cursor: usize,
    /// If `true`, the picker is in "switch pane mode" — selecting a host replaces
    /// the focused pane's tab rather than creating a new tab.
    pub switch_pane_mode: bool,
}

/// All UI state for the Terminal screen.
#[derive(Default)]
pub struct TerminalView {
    /// Ordered list of open tabs.
    pub tabs: Vec<TermTab>,
    /// Index of the focused (primary) tab.
    pub active_tab: usize,
    /// Active split-view layout, if any.
    pub split: Option<SplitView>,
    /// Which pane has keyboard focus when split-view is active.
    pub split_focus: SplitFocus,
    /// Host-picker popup for creating a new tab (Ctrl+T).
    pub host_picker: Option<TermHostPicker>,
    /// When `true`, the next digit key 1–9 jumps directly to that tab.
    /// Activated by pressing Tab (which also cycles to the next tab).
    pub tab_select_mode: bool,
}

impl TerminalView {
    /// Returns the [`SessionId`] of the currently focused pane, or `None` if
    /// there are no open tabs.
    pub fn active_session_id(&self) -> Option<SessionId> {
        if self.tabs.is_empty() {
            return None;
        }
        let idx = match &self.split {
            Some(sv) if self.split_focus == SplitFocus::Secondary => sv.secondary_tab,
            _ => self.active_tab,
        };
        self.tabs.get(idx).map(|t| t.session_id)
    }
}

impl App {
    /// Handles a mouse-wheel notch in the terminal screen.
    ///
    /// On the normal screen the focused tab's local scrollback is moved. On the
    /// alternate screen (vim, less, htop, ...) the notch is forwarded to the
    /// foreground application instead, since the local scrollback is empty.
    pub(crate) fn handle_term_scroll(&mut self, delta: i16) {
        let tv = &mut self.view.terminal_view;
        let focused_idx = match &tv.split {
            Some(sv) if tv.split_focus == SplitFocus::Secondary => sv.secondary_tab,
            _ => tv.active_tab,
        };
        let Some(tab) = tv.tabs.get_mut(focused_idx) else {
            return;
        };
        // Inspect the foreground app under a brief parser lock, then release it.
        let action = match tab.parser.lock() {
            Ok(parser) => crate::utils::scroll::resolve_scroll(delta, parser.screen()),
            Err(_) => return,
        };
        match action {
            crate::utils::scroll::ScrollAction::Scrollback(d) => {
                if d > 0 {
                    // Cap at the vt100 scrollback capacity (1000 lines, see pty.rs).
                    tab.scroll_offset = tab.scroll_offset.saturating_add(d as usize).min(1000);
                } else {
                    tab.scroll_offset = tab.scroll_offset.saturating_sub((-d) as usize);
                }
            }
            crate::utils::scroll::ScrollAction::Forward(bytes) => {
                let id = tab.session_id;
                if let Some(mgr) = &mut self.pty_manager {
                    if let Err(e) = mgr.write(id, &bytes) {
                        tracing::warn!("PTY scroll-forward write error for session {id}: {e}");
                    }
                }
            }
        }
    }

    /// Forwards pasted text to the focused terminal tab's PTY.
    ///
    /// The payload is wrapped in bracketed-paste markers when the foreground
    /// application requested them (so `vim` inserts it verbatim without
    /// auto-indent), otherwise it is sent as plain input.
    pub(crate) fn handle_term_paste(&mut self, text: &str) {
        let tv = &mut self.view.terminal_view;
        let focused_idx = match &tv.split {
            Some(sv) if tv.split_focus == SplitFocus::Secondary => sv.secondary_tab,
            _ => tv.active_tab,
        };
        let Some(tab) = tv.tabs.get_mut(focused_idx) else {
            return;
        };
        // Paste is input — jump back to the live screen, like typing.
        tab.scroll_offset = 0;
        let id = tab.session_id;
        // Read the foreground app's bracketed-paste mode under a brief lock.
        let bracketed = tab
            .parser
            .lock()
            .map(|p| p.screen().bracketed_paste())
            .unwrap_or(false);
        let bytes = crate::utils::paste::encode_paste(text, bracketed);
        if let Some(mgr) = &mut self.pty_manager {
            if let Err(e) = mgr.write(id, &bytes) {
                tracing::warn!("PTY paste write error for session {id}: {e}");
            }
        }
    }

    /// Opens a new PTY terminal tab for `AppState.hosts[host_idx]`.
    ///
    /// Switches to the Terminal screen and sets `active_tab` to the new tab.
    /// Reports errors in the status bar without panicking.
    pub(crate) async fn open_term_tab(&mut self, host_idx: usize) {
        let host = {
            let state = self.state.read().await;
            state.hosts.get(host_idx).cloned()
        };
        let Some(host) = host else {
            self.view.status_message = Some("No such host.".to_string());
            return;
        };
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        // Reserve rows: status bar (1) + tab bar (1) + pane border top+bottom (2) = 4.
        // Reserve cols: pane border left+right (2) = 2.
        let pty_rows = rows.saturating_sub(4);
        let pty_cols = cols.saturating_sub(2);
        let mgr = self.pty_manager.get_or_insert_with(PtyManager::new);
        match mgr.open(&host, pty_cols, pty_rows, self.event_tx.clone()) {
            Ok(session_id) => {
                let Some(parser) = mgr.parser_for(session_id) else {
                    tracing::error!(
                        session = session_id,
                        "parser not found for freshly created session"
                    );
                    return;
                };
                self.view.terminal_view.tabs.push(TermTab {
                    session_id,
                    host_name: host.name.clone(),
                    has_activity: false,
                    parser,
                    scroll_offset: 0,
                });
                self.view.terminal_view.active_tab =
                    self.view.terminal_view.tabs.len().saturating_sub(1);
                self.state.write().await.screen = Screen::Terminal;
                tracing::info!(
                    "Opened terminal tab for '{}' (session {})",
                    host.name,
                    session_id
                );
            }
            Err(e) => {
                self.view.status_message = Some(format!("PTY error: {e}"));
            }
        }
    }

    /// Switches the focused pane's host connection to a new host.
    /// Closes the existing session for that pane and opens a new one.
    pub(crate) async fn switch_focused_pane_host(&mut self, host_idx: usize) {
        let host = {
            let state = self.state.read().await;
            state.hosts.get(host_idx).cloned()
        };
        let Some(host) = host else {
            self.view.status_message = Some("No such host.".to_string());
            return;
        };

        let tv = &mut self.view.terminal_view;

        // Determine which tab index to replace based on split focus
        let tab_idx = match &tv.split {
            Some(sv) if tv.split_focus == SplitFocus::Secondary => sv.secondary_tab,
            _ => tv.active_tab,
        };

        // Close the old session
        if let Some(old_tab) = tv.tabs.get(tab_idx) {
            if let Some(mgr) = &mut self.pty_manager {
                mgr.close(old_tab.session_id);
                tracing::info!(
                    "Closed terminal session {} for '{}'",
                    old_tab.session_id,
                    old_tab.host_name
                );
            }
        }

        // Open new session
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        let pty_rows = rows.saturating_sub(4);
        let pty_cols = cols.saturating_sub(2);
        let mgr = self.pty_manager.get_or_insert_with(PtyManager::new);

        match mgr.open(&host, pty_cols, pty_rows, self.event_tx.clone()) {
            Ok(session_id) => {
                let Some(parser) = mgr.parser_for(session_id) else {
                    tracing::error!(
                        session = session_id,
                        "parser not found for freshly created session"
                    );
                    return;
                };

                // Replace the tab at the current position
                let new_tab = TermTab {
                    session_id,
                    host_name: host.name.clone(),
                    has_activity: false,
                    parser,
                    scroll_offset: 0,
                };

                if let Some(slot) = tv.tabs.get_mut(tab_idx) {
                    *slot = new_tab;
                }

                tracing::info!(
                    "Switched pane {} to host '{}' (session {})",
                    tab_idx,
                    host.name,
                    session_id
                );
            }
            Err(e) => {
                self.view.status_message = Some(format!("PTY error: {e}"));
            }
        }
    }
}
