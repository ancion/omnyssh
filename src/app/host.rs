//! Host list state: add/edit form, list view, popups, and the `App` methods
//! that create, update, delete, and connect to hosts.

use std::io::Stdout;
use std::time::Duration;

use ratatui::{backend::CrosstermBackend, Terminal};

use super::*;
use crate::ssh::client::HostSource;

// ---------------------------------------------------------------------------
// Host form (used in Add / Edit popups)
// ---------------------------------------------------------------------------

/// Labels for every field in the host add/edit form.
pub const FORM_FIELD_LABELS: &[&str] = &[
    "Name",
    "Hostname / IP",
    "User",
    "Port",
    "Identity File",
    "Password (optional)",
    "Tags (comma-sep)",
    "Notes",
];

/// A single editable text field in the host form.
#[derive(Debug, Clone, Default)]
pub struct FormField {
    pub value: String,
    /// Cursor position (byte offset) within `value`.
    pub cursor: usize,
}

impl FormField {
    pub fn with_value(s: impl Into<String>) -> Self {
        let value = s.into();
        let cursor = value.len();
        Self { value, cursor }
    }

    /// Insert a character at the current cursor position.
    pub fn insert_char(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete the character immediately before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        // Find the previous char boundary.
        let prev = self.value[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.value.drain(prev..self.cursor);
        self.cursor = prev;
    }
}

/// The host add/edit form, containing all editable fields.
#[derive(Debug, Clone)]
pub struct HostForm {
    /// Parallel to `FORM_FIELD_LABELS`.
    pub fields: Vec<FormField>,
    /// Index of the currently focused field.
    pub focused_field: usize,
}

impl HostForm {
    /// Creates an empty form for adding a new host.
    pub fn empty() -> Self {
        Self {
            fields: FORM_FIELD_LABELS
                .iter()
                .map(|_| FormField::default())
                .collect(),
            focused_field: 0,
        }
    }

    /// Creates a form pre-filled from an existing host (for editing).
    pub fn from_host(host: &Host) -> Self {
        let mut form = Self::empty();
        form.fields[0] = FormField::with_value(&host.name);
        form.fields[1] = FormField::with_value(&host.hostname);
        form.fields[2] = FormField::with_value(&host.user);
        form.fields[3] = FormField::with_value(host.port.to_string());
        form.fields[4] = FormField::with_value(host.identity_file.as_deref().unwrap_or(""));
        form.fields[5] = FormField::with_value(host.password.as_deref().unwrap_or(""));
        form.fields[6] = FormField::with_value(host.tags.join(", "));
        form.fields[7] = FormField::with_value(host.notes.as_deref().unwrap_or(""));
        form
    }

    /// Validates the form and converts it into a [`Host`].
    ///
    /// # Errors
    /// Returns a human-readable error string if validation fails.
    pub fn to_host(&self, source: HostSource) -> Result<Host, String> {
        let name = self.fields[0].value.trim().to_string();
        if name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }

        let hostname = self.fields[1].value.trim().to_string();
        if hostname.is_empty() {
            return Err("Hostname / IP cannot be empty".to_string());
        }

        let user = {
            let v = self.fields[2].value.trim();
            if v.is_empty() {
                "root".to_string()
            } else {
                v.to_string()
            }
        };

        let port = {
            let v = self.fields[3].value.trim();
            if v.is_empty() {
                22u16
            } else {
                v.parse::<u16>()
                    .map_err(|_| format!("Port must be a number between 1 and 65535, got '{v}'"))?
            }
        };

        let identity_file = {
            let v = self.fields[4].value.trim();
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        };

        let password = {
            let v = self.fields[5].value.trim();
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        };

        let tags: Vec<String> = self.fields[6]
            .value
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        let notes = {
            let v = self.fields[7].value.trim();
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        };

        Ok(Host {
            name,
            hostname,
            user,
            port,
            identity_file,
            password,
            proxy_jump: None,
            tags,
            notes,
            source,
            original_ssh_host: None,
            key_setup_date: None,
            password_auth_disabled: None,
        })
    }

    /// Move focus to the next field (wraps around).
    pub fn focus_next(&mut self) {
        self.focused_field = (self.focused_field + 1) % self.fields.len();
    }

    /// Move focus to the previous field (wraps around).
    pub fn focus_prev(&mut self) {
        if self.focused_field == 0 {
            self.focused_field = self.fields.len() - 1;
        } else {
            self.focused_field -= 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Host list popup variants
// ---------------------------------------------------------------------------

/// Which popup is currently visible over the host list.
#[derive(Debug, Clone)]
pub enum HostPopup {
    /// Adding a new host.
    Add(HostForm),
    /// Editing an existing host (`host_idx` is the index in `AppState.hosts`).
    Edit { host_idx: usize, form: HostForm },
    /// Asking for confirmation before deleting (`host_idx`).
    DeleteConfirm(usize),
    /// Asking for confirmation to set up SSH key authentication.
    KeySetupConfirm(usize),
    /// Showing key setup progress.
    KeySetupProgress {
        host_idx: usize,
        host_name: String,
        current_step: Option<crate::ssh::key_setup::KeySetupStep>,
    },
}

// ---------------------------------------------------------------------------
// Host-list view state (UI-only, not shared with SSH tasks)
// ---------------------------------------------------------------------------

/// UI state specific to the host list / Dashboard screen.
#[derive(Debug, Default)]
pub struct HostListView {
    /// Selected row index within `filtered_indices`.
    pub selected: usize,
    /// True when the user is actively typing a search query.
    pub search_mode: bool,
    /// Current fuzzy-search query.
    pub search_query: String,
    /// Indices into `AppState.hosts` that match the current query, sorted and
    /// tag-filtered according to `sort_order` / `tag_filter`.
    pub filtered_indices: Vec<usize>,
    /// Currently visible popup (if any).
    pub popup: Option<HostPopup>,
    /// Host waiting to be connected (handled before the next render).
    pub pending_connect: Option<Host>,
    // Dashboard additions -----------
    /// Active sort order for the dashboard grid.
    pub sort_order: SortOrder,
    /// Active tag filter. `None` = show all hosts.
    pub tag_filter: Option<String>,
    /// Whether the tag-filter popup is open.
    pub tag_popup_open: bool,
    /// Selected index within the tag picker popup.
    pub tag_popup_selected: usize,
    /// All unique tags across all hosts (used by the tag picker popup).
    pub available_tags: Vec<String>,
}

impl HostListView {
    /// Returns the index into `AppState.hosts` for the selected filtered row.
    pub fn selected_host_idx(&self) -> Option<usize> {
        self.filtered_indices.get(self.selected).copied()
    }

    /// Rebuilds `filtered_indices` applying text search, tag filter, and
    /// sort order.
    ///
    /// Accepts `metrics` so CPU/RAM sorts can compare live values. Pass an
    /// empty map if metrics are not yet available.
    pub fn rebuild_filter(
        &mut self,
        hosts: &[Host],
        metrics: &HashMap<String, Metrics>,
        statuses: &HashMap<String, ConnectionStatus>,
    ) {
        use std::cmp::Ordering;

        // 1. Text filter.
        let mut indices = filter_hosts(hosts, &self.search_query);

        // Guard: drop any stale indices that fell out of bounds due to a host
        // removal that happened before rebuild_filter was called.
        indices.retain(|&i| i < hosts.len());

        // 2. Tag filter.
        if let Some(tag) = &self.tag_filter {
            let tag = tag.clone();
            indices.retain(|&i| hosts[i].tags.contains(&tag));
        }

        // 3. Sort.
        match self.sort_order {
            SortOrder::Name => {
                indices.sort_by(|&a, &b| hosts[a].name.cmp(&hosts[b].name));
            }
            SortOrder::Cpu => {
                indices.sort_by(|&a, &b| {
                    let ca = metrics
                        .get(&hosts[a].name)
                        .and_then(|m| m.cpu_percent)
                        .unwrap_or(-1.0);
                    let cb = metrics
                        .get(&hosts[b].name)
                        .and_then(|m| m.cpu_percent)
                        .unwrap_or(-1.0);
                    cb.partial_cmp(&ca).unwrap_or(Ordering::Equal)
                });
            }
            SortOrder::Ram => {
                indices.sort_by(|&a, &b| {
                    let ra = metrics
                        .get(&hosts[a].name)
                        .and_then(|m| m.ram_percent)
                        .unwrap_or(-1.0);
                    let rb = metrics
                        .get(&hosts[b].name)
                        .and_then(|m| m.ram_percent)
                        .unwrap_or(-1.0);
                    rb.partial_cmp(&ra).unwrap_or(Ordering::Equal)
                });
            }
            SortOrder::Status => {
                indices.sort_by(|&a, &b| {
                    let sa = status_priority(statuses.get(&hosts[a].name));
                    let sb = status_priority(statuses.get(&hosts[b].name));
                    sa.cmp(&sb)
                });
            }
        }

        self.filtered_indices = indices;
        // Clamp selection to the new range.
        if self.filtered_indices.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len() - 1;
        }
    }

    /// Scroll down by one row.
    pub fn select_next(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.filtered_indices.len() - 1);
    }

    /// Scroll up by one row.
    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Rebuild the `available_tags` list from the full host list.
    pub fn rebuild_tags(&mut self, hosts: &[Host]) {
        let mut tags: Vec<String> = hosts.iter().flat_map(|h| h.tags.iter().cloned()).collect();
        tags.sort();
        tags.dedup();
        self.available_tags = tags;
    }
}

/// Lower number = higher priority for status sort.
fn status_priority(status: Option<&ConnectionStatus>) -> u8 {
    match status {
        Some(ConnectionStatus::Connected) => 0,
        Some(ConnectionStatus::Connecting) => 1,
        Some(ConnectionStatus::Unknown) | None => 2,
        Some(ConnectionStatus::Failed(_)) => 3,
    }
}

/// Simple case-insensitive substring filter over name / hostname / tags / notes.
///
/// Returns the matching indices into `hosts`. When `query` is empty every
/// host matches.
pub fn filter_hosts(hosts: &[Host], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..hosts.len()).collect();
    }
    let q = query.to_lowercase();
    hosts
        .iter()
        .enumerate()
        .filter(|(_, h)| {
            h.name.to_lowercase().contains(&q)
                || h.hostname.to_lowercase().contains(&q)
                || h.tags.iter().any(|t| t.to_lowercase().contains(&q))
                || h.notes
                    .as_deref()
                    .map(|n| n.to_lowercase().contains(&q))
                    .unwrap_or(false)
        })
        .map(|(i, _)| i)
        .collect()
}

impl App {
    /// Handles the user confirming the Add or Edit host form.
    pub(crate) async fn handle_confirm_form(&mut self) {
        match self.view.host_list.popup.take() {
            Some(HostPopup::Add(form)) => {
                match form.to_host(HostSource::Manual) {
                    Ok(host) => {
                        let has_password = host.password.is_some();
                        let host_name = host.name.clone();

                        {
                            let mut state = self.state.write().await;
                            state.hosts.push(host);
                        }
                        self.save_manual_hosts().await;

                        // Restart poll_manager to include the new host
                        {
                            let state = self.state.read().await;
                            if let Some(old) = self.poll_manager.take() {
                                old.shutdown();
                            }
                            self.poll_manager = Some(PollManager::start(
                                state.hosts.clone(),
                                self.event_tx.clone(),
                                Duration::from_secs(30),
                            ));
                        }

                        let state = self.state.read().await;
                        self.view.host_list.rebuild_filter(
                            &state.hosts,
                            &state.metrics,
                            &state.connection_statuses,
                        );
                        self.view.host_list.rebuild_tags(&state.hosts);

                        // Suggest SSH key setup if host was added with password
                        if has_password {
                            self.view.status_message = Some(format!(
                                "Host '{}' added. Press 'Shift+K' to set up SSH key authentication (recommended).",
                                host_name
                            ));
                        } else {
                            self.view.status_message = Some("Host added.".to_string());
                        }
                    }
                    Err(e) => {
                        // Restore popup so the user can correct the input.
                        self.view.host_list.popup = Some(HostPopup::Add(form));
                        self.view.status_message = Some(format!("Error: {e}"));
                    }
                }
            }

            Some(HostPopup::Edit { host_idx, form }) => match form.to_host(HostSource::Manual) {
                Ok(mut host) => {
                    let (old_name, _was_ssh_config) = {
                        let mut state = self.state.write().await;
                        let old_host = state.hosts.get(host_idx);
                        let old_name = old_host.map(|h| h.name.clone());
                        let was_ssh_config = old_host
                            .map(|h| h.source == HostSource::SshConfig)
                            .unwrap_or(false);

                        // If editing a SSH config host, preserve original name for duplicate prevention
                        if was_ssh_config && old_name.is_some() {
                            host.original_ssh_host = old_name.clone();
                        }

                        if let Some(slot) = state.hosts.get_mut(host_idx) {
                            *slot = host.clone();
                        }
                        (old_name, was_ssh_config)
                    };

                    // If the host name changed, migrate all associated data
                    if let Some(old_name) = old_name {
                        if old_name != host.name {
                            let mut state = self.state.write().await;

                            // Migrate metrics
                            if let Some(metrics) = state.metrics.remove(&old_name) {
                                state.metrics.insert(host.name.clone(), metrics);
                            }

                            // Migrate connection status
                            if let Some(status) = state.connection_statuses.remove(&old_name) {
                                state.connection_statuses.insert(host.name.clone(), status);
                            }

                            // Migrate services
                            if let Some(services) = state.services.remove(&old_name) {
                                state.services.insert(host.name.clone(), services);
                            }

                            // Migrate alerts
                            if let Some(alerts) = state.alerts.remove(&old_name) {
                                state.alerts.insert(host.name.clone(), alerts);
                            }

                            // Migrate discovery status
                            if let Some(discovery) = state.discovery_status.remove(&old_name) {
                                state.discovery_status.insert(host.name.clone(), discovery);
                            }
                        }
                    }

                    self.save_manual_hosts().await;
                    let state = self.state.read().await;
                    self.view.host_list.rebuild_filter(
                        &state.hosts,
                        &state.metrics,
                        &state.connection_statuses,
                    );
                    self.view.host_list.rebuild_tags(&state.hosts);
                    self.view.status_message = Some("Host updated.".to_string());
                }
                Err(e) => {
                    self.view.host_list.popup = Some(HostPopup::Edit { host_idx, form });
                    self.view.status_message = Some(format!("Error: {e}"));
                }
            },

            other => {
                // Wrong popup type — put it back unchanged.
                self.view.host_list.popup = other;
            }
        }
    }

    /// Handles the user confirming deletion.
    pub(crate) async fn handle_confirm_delete(&mut self) {
        if let Some(HostPopup::DeleteConfirm(idx)) = self.view.host_list.popup.take() {
            {
                let mut state = self.state.write().await;
                if idx < state.hosts.len() {
                    let removed = state.hosts.remove(idx);
                    self.view.status_message = Some(format!("Deleted '{}'.", removed.name));
                }
            }
            self.save_manual_hosts().await;
            let state = self.state.read().await;
            self.view.host_list.rebuild_filter(
                &state.hosts,
                &state.metrics,
                &state.connection_statuses,
            );
            self.view.host_list.rebuild_tags(&state.hosts);
        }
    }

    /// Saves the manually-added hosts to `hosts.toml`.
    pub(crate) async fn save_manual_hosts(&mut self) {
        let hosts = self.state.read().await.hosts.clone();
        if let Err(e) = config::save_hosts(&hosts) {
            self.view.status_message = Some(format!("Save failed: {e}"));
        }
    }

    /// Temporarily restores the terminal, runs the system SSH binary for the
    /// given host, then re-initialises the TUI.
    pub(crate) async fn connect_system_ssh(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        host: &Host,
    ) -> anyhow::Result<()> {
        // 1. Leave TUI mode.
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crate::utils::mouse::DisableMinimalMouseCapture,
        )?;

        // 2. Build SSH command (ConnectTimeout=10).
        let mut cmd = tokio::process::Command::new("ssh");
        cmd.args(["-o", "ConnectTimeout=10"]);
        if host.port != 22 {
            cmd.args(["-p", &host.port.to_string()]);
        }
        if let Some(ref key) = host.identity_file {
            cmd.args(["-i", key]);
        }
        if let Some(ref jump) = host.proxy_jump {
            cmd.args(["-J", jump]);
        }
        cmd.arg(format!("{}@{}", host.user, host.hostname));

        // 3. Hand off terminal control to SSH.
        tracing::info!("Connecting to {} via system SSH", host.name);
        let status = cmd.spawn()?.wait().await?;

        // 4. Re-enter TUI mode.
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crate::utils::mouse::EnableMinimalMouseCapture,
        )?;
        terminal.clear()?;

        // 5. Show connection result.
        self.view.status_message = Some(if status.success() {
            format!("Disconnected from '{}'.", host.name)
        } else {
            format!(
                "SSH to '{}' exited with code {:?}.",
                host.name,
                status.code()
            )
        });

        Ok(())
    }
}
