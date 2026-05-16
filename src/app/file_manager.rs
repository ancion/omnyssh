//! File Manager screen state and the `App` methods that drive local/remote
//! directory navigation and SFTP transfers.

use std::collections::HashSet;
use std::time::Duration;

use super::*;
use crate::ssh::sftp::{self, FileEntry, SftpCommand, SftpManager, SftpOpKind};

// ---------------------------------------------------------------------------
// File Manager state (ViewState-only)
// ---------------------------------------------------------------------------

/// Which of the two file panels is currently focused.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FmPanel {
    /// Left panel — local filesystem.
    #[default]
    Local,
    /// Right panel — remote filesystem (SFTP).
    Remote,
}

/// Items held in the copy clipboard.
#[derive(Debug, Clone)]
pub struct FmClipboard {
    /// Absolute paths of the copied items.
    pub paths: Vec<String>,
    /// Which panel the items were copied from.
    pub source_panel: FmPanel,
}

/// UI state for a single file panel (local or remote).
#[derive(Debug, Default)]
pub struct FilePanelView {
    /// Current working directory being displayed.
    pub cwd: String,
    /// Directory entries as returned by the last listing.
    pub entries: Vec<FileEntry>,
    /// Absolute cursor index into `entries`.
    pub cursor: usize,
    /// Index of the first visible row (for scrolling).
    /// Uses [`std::cell::Cell`] so the render function can persist the computed
    /// scroll position through a shared `&FilePanelView` reference.
    pub scroll: std::cell::Cell<usize>,
    /// Set of `entry.path` values that are Space-marked.
    pub marked: HashSet<String>,
}

impl FilePanelView {
    /// Returns a reference to the entry under the cursor, if any.
    pub fn cursor_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.cursor)
    }

    /// Returns the paths to operate on: all marked entries, or the cursor entry
    /// if nothing is marked.
    pub fn marked_or_cursor_paths(&self) -> Vec<String> {
        if !self.marked.is_empty() {
            self.marked.iter().cloned().collect()
        } else if let Some(e) = self.cursor_entry() {
            if e.name != ".." {
                vec![e.path.clone()]
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }

    /// Move cursor down by one row, staying in bounds.
    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = (self.cursor + 1).min(self.entries.len() - 1);
        }
    }

    /// Move cursor up by one row, staying in bounds.
    pub fn select_prev(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Adjust `scroll` so that `cursor` is always visible in `visible_rows` rows.
    pub fn clamp_scroll(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        let scroll = self.scroll.get();
        if self.cursor < scroll {
            self.scroll.set(self.cursor);
        } else if self.cursor >= scroll + visible_rows {
            self.scroll
                .set(self.cursor.saturating_sub(visible_rows - 1));
        }
    }
}

/// Active popup on the File Manager screen.
#[derive(Debug)]
pub enum FileManagerPopup {
    /// Pick which host to connect the remote panel to.
    HostPicker { cursor: usize },
    /// Confirm deletion of one or more items.
    DeleteConfirm { paths: Vec<String> },
    /// Creating a new remote or local directory.
    MkDir(FormField),
    /// Renaming the item under the cursor.
    Rename {
        original_name: String,
        field: FormField,
    },
    /// Live file-transfer progress.
    TransferProgress {
        transfer_id: TransferId,
        filename: String,
        done: u64,
        total: u64,
    },
}

/// All UI state for the File Manager screen.
#[derive(Debug, Default)]
pub struct FileManagerView {
    /// Which panel has keyboard focus.
    pub active_panel: FmPanel,
    /// State of the local (left) panel.
    pub local: FilePanelView,
    /// State of the remote (right) panel.
    pub remote: FilePanelView,
    /// Name of the currently connected remote host, if any.
    pub connected_host: Option<String>,
    /// SFTP connection in progress (shows "Connecting..." indicator).
    pub sftp_connecting: bool,
    /// Copy clipboard.
    pub clipboard: Option<FmClipboard>,
    /// Active popup, if any.
    pub popup: Option<FileManagerPopup>,
    /// Text content shown in the preview zone.
    pub preview_content: Option<String>,
    /// Path whose preview is currently shown (avoids redundant re-fetches).
    pub preview_path: Option<String>,
    /// Transfer id of an in-progress transfer (for the progress popup).
    pub active_transfer: Option<TransferId>,
    /// Number of queued transfer operations not yet completed.
    pub pending_ops: usize,
}

impl App {
    // -----------------------------------------------------------------------
    // File Manager private helper methods
    // -----------------------------------------------------------------------

    /// Returns a mutable reference to the active file panel view.
    pub(crate) fn active_fm_panel_mut(&mut self) -> &mut FilePanelView {
        match self.view.file_manager.active_panel {
            FmPanel::Local => &mut self.view.file_manager.local,
            FmPanel::Remote => &mut self.view.file_manager.remote,
        }
    }

    /// Returns a shared reference to the active file panel view.
    pub(crate) fn active_fm_panel_ref(&self) -> &FilePanelView {
        match self.view.file_manager.active_panel {
            FmPanel::Local => &self.view.file_manager.local,
            FmPanel::Remote => &self.view.file_manager.remote,
        }
    }

    /// Initialises the file manager when the user first switches to it.
    ///
    /// - Loads the local panel from `home_dir` (or `/`) if it is empty.
    /// - Opens the host-picker popup if no remote session is active.
    pub(crate) async fn bootstrap_file_manager(&mut self) {
        // Load local panel if not yet populated.
        if self.view.file_manager.local.cwd.is_empty() {
            let start = dirs::home_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".to_string());
            let tx = self.event_tx.clone();
            let path = start.clone();
            tokio::spawn(async move {
                match sftp::list_local_dir(&path).await {
                    Ok(entries) => {
                        let _ = tx.send(AppEvent::LocalDirListed { path, entries }).await;
                    }
                    Err(e) => tracing::warn!("Local bootstrap failed: {e}"),
                }
            });
        }

        // Open the host-picker if no remote connection exists.
        if self.sftp_manager.is_none() && self.view.file_manager.connected_host.is_none() {
            self.view.file_manager.popup = Some(FileManagerPopup::HostPicker { cursor: 0 });
        }
    }

    /// Requests a preview for the entry under the cursor in the active panel.
    ///
    /// Skips the request when the same path is already previewed.
    pub(crate) fn request_preview_for_active(&mut self) {
        let is_remote = self.view.file_manager.active_panel == FmPanel::Remote;

        let (path, already_shown) = {
            let panel = self.active_fm_panel_ref();
            let Some(entry) = panel.cursor_entry() else {
                return;
            };
            if entry.is_dir {
                return;
            }
            let path = entry.path.clone();
            let shown = self.view.file_manager.preview_path.as_deref() == Some(&path);
            (path, shown)
        };

        if already_shown {
            return;
        }

        if is_remote {
            if let Some(mgr) = &self.sftp_manager {
                mgr.send(SftpCommand::ReadPreview(path));
            }
        } else {
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                if let Ok(content) = sftp::preview_local_file(&path).await {
                    let _ = tx.send(AppEvent::FilePreviewReady { path, content }).await;
                }
            });
        }
    }

    /// Re-lists both panels after a mutating operation completes.
    pub(crate) async fn refresh_active_panels(&mut self) {
        let local_path = self.view.file_manager.local.cwd.clone();
        if !local_path.is_empty() {
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                match sftp::list_local_dir(&local_path).await {
                    Ok(entries) => {
                        let _ = tx
                            .send(AppEvent::LocalDirListed {
                                path: local_path,
                                entries,
                            })
                            .await;
                    }
                    Err(e) => tracing::warn!("Local refresh failed: {e}"),
                }
            });
        }

        let remote_path = self.view.file_manager.remote.cwd.clone();
        if !remote_path.is_empty() {
            if let Some(mgr) = &self.sftp_manager {
                mgr.send(SftpCommand::ListDir(remote_path));
            }
        }
    }

    /// Initiates an SFTP connection to the host at `idx` in `AppState.hosts`.
    pub(crate) async fn fm_connect_host(&mut self, idx: usize) {
        let host = {
            let state = self.state.read().await;
            state.hosts.get(idx).cloned()
        };
        let Some(host) = host else {
            self.view.status_message = Some("Host not found.".to_string());
            return;
        };

        // Disconnect any existing session.
        if let Some(old) = self.sftp_manager.take() {
            old.disconnect();
        }
        self.view.file_manager.connected_host = None;
        self.view.file_manager.remote = FilePanelView::default();

        self.view.status_message = Some(format!("Connecting to '{}'… (30s timeout)", host.name));
        self.view.file_manager.sftp_connecting = true;

        // Spawn connection in background with 30s timeout to prevent UI freeze
        let tx = self.event_tx.clone();
        let host_clone = host.clone();
        tokio::spawn(async move {
            let connect_future = SftpManager::connect(&host_clone, tx.clone());
            let timeout_future = tokio::time::sleep(Duration::from_secs(30));

            tokio::select! {
                result = connect_future => {
                    match result {
                        Ok(mgr) => {
                            // Send the manager through a new event type
                            let _ = tx
                                .send(AppEvent::SftpManagerReady {
                                    host_name: host_clone.name.clone(),
                                    manager: Box::new(mgr),
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx
                                .send(AppEvent::SftpDisconnected {
                                    host_name: host_clone.name.clone(),
                                    reason: e.to_string(),
                                })
                                .await;
                        }
                    }
                }
                _ = timeout_future => {
                    let _ = tx
                        .send(AppEvent::SftpDisconnected {
                            host_name: host_clone.name.clone(),
                            reason: "connection timed out (30s)".to_string(),
                        })
                        .await;
                }
            }
        });
    }

    /// Enters the directory under the cursor in the active panel.
    pub(crate) async fn fm_enter_dir(&mut self) {
        let is_remote = self.view.file_manager.active_panel == FmPanel::Remote;
        let entry = self.active_fm_panel_ref().cursor_entry().cloned();
        let Some(entry) = entry else { return };
        if !entry.is_dir {
            return;
        }

        if is_remote {
            if let Some(mgr) = &self.sftp_manager {
                mgr.send(SftpCommand::ListDir(entry.path.clone()));
            }
        } else {
            let path = entry.path.clone();
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                match sftp::list_local_dir(&path).await {
                    Ok(entries) => {
                        let _ = tx.send(AppEvent::LocalDirListed { path, entries }).await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(AppEvent::Error("local".to_string(), e.to_string()))
                            .await;
                    }
                }
            });
        }
    }

    /// Navigates to the parent of the current working directory.
    pub(crate) async fn fm_parent_dir(&mut self) {
        let is_remote = self.view.file_manager.active_panel == FmPanel::Remote;
        let cwd = self.active_fm_panel_ref().cwd.clone();

        let parent = std::path::Path::new(&cwd).parent().map(|p| {
            let s = p.to_string_lossy().into_owned();
            if s.is_empty() {
                "/".to_string()
            } else {
                s
            }
        });

        let Some(parent) = parent else { return };

        if is_remote {
            if let Some(mgr) = &self.sftp_manager {
                mgr.send(SftpCommand::ListDir(parent));
            }
        } else {
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                match sftp::list_local_dir(&parent).await {
                    Ok(entries) => {
                        let _ = tx
                            .send(AppEvent::LocalDirListed {
                                path: parent,
                                entries,
                            })
                            .await;
                    }
                    Err(e) => tracing::warn!("Parent dir failed: {e}"),
                }
            });
        }
    }

    /// Pastes all clipboard contents into the active panel (upload / download).
    ///
    /// All files are queued as individual SFTP commands and processed sequentially
    /// by the background task. `pending_ops` tracks how many are still in flight.
    pub(crate) async fn fm_paste(&mut self) {
        let Some(clipboard) = self.view.file_manager.clipboard.clone() else {
            self.view.status_message = Some("Nothing in clipboard.".to_string());
            return;
        };

        let dst_panel = self.view.file_manager.active_panel.clone();

        if clipboard.source_panel == dst_panel {
            self.view.status_message = Some("Cannot paste to the same panel.".to_string());
            return;
        }

        if clipboard.paths.is_empty() {
            self.view.status_message = Some("Clipboard is empty.".to_string());
            return;
        }

        let dst_cwd = match &dst_panel {
            FmPanel::Local => self.view.file_manager.local.cwd.clone(),
            FmPanel::Remote => self.view.file_manager.remote.cwd.clone(),
        };

        let count = clipboard.paths.len();
        let first_tid = self.next_transfer_id;
        self.next_transfer_id += count as u64;
        self.view.file_manager.pending_ops = count;

        // Show progress popup for the first file; subsequent files update it
        // via FileTransferProgress events.
        let first_name = filename_of(&clipboard.paths[0]);
        let popup_name = if count > 1 {
            format!("{first_name}  (+{} more)", count - 1)
        } else {
            first_name
        };
        self.view.file_manager.active_transfer = Some(first_tid);
        self.view.file_manager.popup = Some(FileManagerPopup::TransferProgress {
            transfer_id: first_tid,
            filename: popup_name,
            done: 0,
            total: 0,
        });

        // Queue every file as a separate SFTP command.
        for (i, src_path) in clipboard.paths.iter().enumerate() {
            let tid = first_tid + i as u64;
            let fname = filename_of(src_path);
            let dst = format!("{}/{}", dst_cwd.trim_end_matches('/'), fname);

            match (&clipboard.source_panel, &dst_panel) {
                (FmPanel::Local, FmPanel::Remote) => {
                    if let Some(mgr) = &self.sftp_manager {
                        mgr.send(SftpCommand::Upload {
                            local: src_path.clone(),
                            remote: dst,
                            transfer_id: tid,
                        });
                    }
                }
                (FmPanel::Remote, FmPanel::Local) => {
                    if let Some(mgr) = &self.sftp_manager {
                        mgr.send(SftpCommand::Download {
                            remote: src_path.clone(),
                            local: dst,
                            transfer_id: tid,
                        });
                    }
                }
                _ => unreachable!("same-panel case handled above"),
            }
        }

        if count > 1 {
            self.view.status_message = Some(format!("Queued {count} files for transfer…"));
        }
    }

    /// Deletes items listed in the `DeleteConfirm` popup.
    pub(crate) async fn fm_delete(&mut self) {
        let popup = self.view.file_manager.popup.take();
        let Some(FileManagerPopup::DeleteConfirm { paths }) = popup else {
            return;
        };

        let is_remote = self.view.file_manager.active_panel == FmPanel::Remote;

        if is_remote {
            // Send delete commands for all paths.
            for path in paths {
                if let Some(mgr) = &self.sftp_manager {
                    mgr.send(SftpCommand::Delete(path));
                }
            }
        } else {
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                let mut last_err: Option<String> = None;
                for path in paths {
                    let result = tokio::fs::remove_file(&path).await.or_else(|_| {
                        // Might be a directory — try remove_dir (empty only).
                        // Using blocking version since remove_dir_all is destructive.
                        std::fs::remove_dir(&path)
                            .map_err(|e| std::io::Error::new(e.kind(), e.to_string()))
                    });
                    if let Err(e) = result {
                        last_err = Some(e.to_string());
                    }
                }
                let result = last_err.map_or(Ok(()), Err);
                let _ = tx
                    .send(AppEvent::SftpOpDone {
                        kind: SftpOpKind::Delete,
                        result: result.map_err(|e: String| e),
                    })
                    .await;
            });
        }
    }

    /// Creates a new directory in the active panel.
    pub(crate) async fn fm_mkdir(&mut self, name: String) {
        self.view.file_manager.popup = None;
        let is_remote = self.view.file_manager.active_panel == FmPanel::Remote;

        if is_remote {
            let cwd = self.view.file_manager.remote.cwd.clone();
            let new_path = format!("{}/{}", cwd.trim_end_matches('/'), name);
            if let Some(mgr) = &self.sftp_manager {
                mgr.send(SftpCommand::MkDir(new_path));
            }
        } else {
            let cwd = self.view.file_manager.local.cwd.clone();
            let new_path = format!("{}/{}", cwd.trim_end_matches('/'), name);
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                let result = tokio::fs::create_dir(&new_path)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx
                    .send(AppEvent::SftpOpDone {
                        kind: SftpOpKind::MkDir,
                        result,
                    })
                    .await;
            });
        }
    }

    /// Renames the file or directory under the cursor.
    pub(crate) async fn fm_rename(&mut self, new_name: String) {
        let popup = self.view.file_manager.popup.take();
        let is_remote = self.view.file_manager.active_panel == FmPanel::Remote;

        let cwd = match is_remote {
            true => self.view.file_manager.remote.cwd.clone(),
            false => self.view.file_manager.local.cwd.clone(),
        };

        let old_name = match &popup {
            Some(FileManagerPopup::Rename { original_name, .. }) => original_name.clone(),
            _ => return,
        };

        let old_path = format!("{}/{}", cwd.trim_end_matches('/'), old_name);
        let new_path = format!("{}/{}", cwd.trim_end_matches('/'), new_name);

        if is_remote {
            if let Some(mgr) = &self.sftp_manager {
                mgr.send(SftpCommand::Rename {
                    from: old_path,
                    to: new_path,
                });
            }
        } else {
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                let result = tokio::fs::rename(&old_path, &new_path)
                    .await
                    .map_err(|e| e.to_string());
                let _ = tx
                    .send(AppEvent::SftpOpDone {
                        kind: SftpOpKind::Rename,
                        result,
                    })
                    .await;
            });
        }
    }
}

/// Extracts the file name from an absolute path string.
fn filename_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string()
}
