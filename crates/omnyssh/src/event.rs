use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEventKind};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::config::snippets::Snippet;
use crate::ssh::client::{ConnectionStatus, Host};
use crate::ssh::key_setup::KeySetupStep;
use crate::ssh::sftp::FileEntry;

/// Placeholder type aliases for future stages.
/// `HostId` is the host's `name` field — stable, human-readable key.
pub type HostId = String;
pub type SessionId = u64;
pub type TransferId = u64;

/// A single process entry for the "top processes" panel.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process / command name.
    pub name: String,
    /// CPU usage percentage.
    pub cpu_percent: f64,
    /// Memory usage percentage.
    pub mem_percent: f64,
}

/// Live metrics collected from a remote server.
#[derive(Debug, Clone)]
pub struct Metrics {
    pub cpu_percent: Option<f64>,
    pub ram_percent: Option<f64>,
    pub disk_percent: Option<f64>,
    pub uptime: Option<String>,
    pub load_avg: Option<String>,
    /// OS information (e.g., "Ubuntu 22.04 LTS", "Debian GNU/Linux 11").
    pub os_info: Option<String>,
    /// Top processes by CPU usage (at most 3).
    pub top_processes: Option<Vec<ProcessInfo>>,
    /// When these metrics were last successfully collected.
    pub last_updated: Instant,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            cpu_percent: None,
            ram_percent: None,
            disk_percent: None,
            uptime: None,
            load_avg: None,
            os_info: None,
            top_processes: None,
            last_updated: Instant::now(),
        }
    }
}

/// Central event type consumed by the main loop. Input events are produced by
/// the crossterm event thread; everything domain-side arrives wrapped in
/// [`AppEvent::Core`] via the forwarder task in `App::run`.
#[derive(Debug)]
pub enum AppEvent {
    /// Keyboard or mouse input from the user.
    Key(KeyEvent),
    /// Text pasted into the terminal (bracketed paste).
    Paste(String),
    /// Render tick (~30 FPS).
    Tick,
    /// The terminal window was resized to the given dimensions (cols, rows).
    TerminalResized(u16, u16),
    /// Mouse-wheel scroll in the terminal pane: positive = up, negative = down.
    TermScroll(i16),
    /// A domain event produced by the SSH engine or a background task.
    Core(CoreEvent),
}

/// Domain events produced by the SSH engine, config loaders, and the update
/// checker. Background tasks send these over a dedicated channel; the TUI
/// wraps them into [`AppEvent::Core`].
#[derive(Debug)]
pub enum CoreEvent {
    /// SSH metrics received from a background task.
    MetricsUpdate(HostId, Metrics),
    /// Connection status changed for a host (reported by metrics poller).
    HostStatusChanged(HostId, ConnectionStatus),
    /// File transfer progress: (transfer_id, bytes_done, bytes_total).
    FileTransferProgress(TransferId, u64, u64),
    /// An error message surfaced to the user.
    Error(String),
    /// Host list loaded from disk / SSH config in a background task.
    HostsLoaded(Vec<Host>),
    /// Snippet list loaded from disk in a background task.
    SnippetsLoaded(Vec<Snippet>),
    /// Result of executing a snippet or quick-execute command on one host.
    /// `output` is `Ok(stdout)` or `Err(error_message)`.
    SnippetResult {
        host_name: String,
        snippet_name: String,
        output: Result<String, String>,
    },

    // -----------------------------------------------------------------------
    // File Manager events
    // -----------------------------------------------------------------------
    /// Remote directory listing completed.
    FileDirListed {
        path: String,
        entries: Vec<FileEntry>,
    },
    /// Local directory listing completed.
    LocalDirListed {
        path: String,
        entries: Vec<FileEntry>,
    },
    /// SFTP session successfully established.
    SftpConnected { host_name: String },
    /// SFTP manager ready with established connection (contains SftpManager handle).
    SftpManagerReady {
        host_name: String,
        manager: Box<crate::ssh::sftp::SftpManager>,
    },
    /// SFTP session closed or failed.
    SftpDisconnected { reason: String },
    /// Preview bytes available for a file.
    FilePreviewReady { path: String, content: String },
    /// A mutating SFTP operation (delete, mkdir, rename, upload, download) finished.
    SftpOpDone { result: Result<(), String> },

    // -----------------------------------------------------------------------
    // PTY multi-session terminal events
    // -----------------------------------------------------------------------
    /// A PTY session produced output. The bytes are already parsed into the
    /// session's `Arc<Mutex<vt100::Parser>>`; this event is a lightweight
    /// render-nudge so the main loop can update `has_activity` state without
    /// copying bulk output data through the channel.
    PtyOutput(SessionId),
    /// The PTY child process exited (reader thread reached EOF or I/O error).
    PtyExited(SessionId),

    // -----------------------------------------------------------------------
    // Smart Server Context — Discovery events
    // -----------------------------------------------------------------------
    /// Quick scan completed for a host, services detected.
    DiscoveryQuickScanDone(HostId, Vec<DetectedService>),
    /// Discovery failed for a host with an error message.
    DiscoveryFailed(HostId, String),

    // -----------------------------------------------------------------------
    // Auto SSH Key Setup events
    // -----------------------------------------------------------------------
    /// Progress update from key setup (host_id, current step, total steps).
    KeySetupProgress(HostId, KeySetupStep),
    /// Key setup completed successfully (host_id, private_key_path).
    KeySetupComplete(HostId, std::path::PathBuf),
    /// Key setup failed with an error (host_id, error_message).
    KeySetupFailed(HostId, String),
    /// Emergency rollback was triggered (host_id, rollback_result).
    KeySetupRollback(HostId, String),

    // -----------------------------------------------------------------------
    // Update checker events
    // -----------------------------------------------------------------------
    /// A newer release was found on GitHub at startup.
    UpdateAvailable(omnyssh_core::update::UpdateInfo),
    /// A self-update finished — `Ok` on success, `Err` with a message on failure.
    UpdateInstalled(Result<(), String>),
}

// ---------------------------------------------------------------------------
// Smart Server Context — Data structures
// ---------------------------------------------------------------------------

/// Describes a service detected on a remote server.
#[derive(Debug, Clone)]
pub struct DetectedService {
    pub kind: ServiceKind,
    pub metrics: Vec<ServiceMetric>,
}

/// Type of service detected on the server.
/// Only 5 core services are supported: Docker, Nginx, PostgreSQL, Redis, Node.js.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ServiceKind {
    Docker,
    Nginx,
    PostgreSQL,
    Redis,
    NodeJS,
}

/// A metric collected from a specific service.
#[derive(Debug, Clone)]
pub struct ServiceMetric {
    pub name: String,       // e.g., "containers_running"
    pub value: MetricValue, // Typed value
}

/// Typed metric value.
#[derive(Debug, Clone)]
pub enum MetricValue {
    Integer(i64),
}

/// Whether a key event should be forwarded to the app.
///
/// Windows emits both a `Press` and a `Release` event per keystroke, while
/// Unix terminals emit only `Press`. Forwarding `Release` would process every
/// keystroke twice (e.g. "j" → "jj"), so it is dropped here.
fn should_forward_key(kind: KeyEventKind) -> bool {
    !matches!(kind, KeyEventKind::Release)
}

/// Spawns a background thread that reads crossterm events and forwards them
/// to the provided sender as [`AppEvent`] values. Also sends a `Tick` every
/// ~33 ms so the render loop stays at ≥30 FPS even when there is no input.
///
/// # Errors
/// Returns an error if the background thread fails to spawn.
pub fn spawn_event_thread(tx: mpsc::Sender<AppEvent>) -> anyhow::Result<()> {
    std::thread::spawn(move || {
        let tick = Duration::from_millis(33);
        loop {
            if event::poll(tick).unwrap_or(false) {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if should_forward_key(key.kind)
                            && tx.blocking_send(AppEvent::Key(key)).is_err()
                        {
                            break;
                        }
                    }
                    Ok(Event::Resize(cols, rows)) => {
                        if tx
                            .blocking_send(AppEvent::TerminalResized(cols, rows))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(Event::Mouse(m)) => {
                        let delta: Option<i16> = match m.kind {
                            MouseEventKind::ScrollUp => Some(3),
                            MouseEventKind::ScrollDown => Some(-3),
                            _ => None,
                        };
                        if let Some(d) = delta {
                            if tx.blocking_send(AppEvent::TermScroll(d)).is_err() {
                                break;
                            }
                        }
                    }
                    Ok(Event::Paste(text)) => {
                        if tx.blocking_send(AppEvent::Paste(text)).is_err() {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            } else if tx.blocking_send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_press_and_repeat_but_not_release() {
        assert!(should_forward_key(KeyEventKind::Press));
        assert!(should_forward_key(KeyEventKind::Repeat));
        assert!(!should_forward_key(KeyEventKind::Release));
    }
}
