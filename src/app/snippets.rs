//! Snippets screen state: forms, popups, list view, and the `App` methods that
//! execute snippets, quick-execute commands, and quick views.

use super::*;
use crate::config::snippets::SnippetScope;
use crate::ssh::session::SshSession;

// ---------------------------------------------------------------------------
// Snippets structs
// ---------------------------------------------------------------------------

/// One host's result entry for the snippet execution results popup.
#[derive(Debug, Clone)]
pub struct SnippetResultEntry {
    pub host_name: String,
    pub snippet_name: String,
    /// `Ok(stdout)` when done, `Err(message)` on error, empty `Ok("")` while pending.
    pub output: Result<String, String>,
    /// True while we're still waiting for the SSH task to complete.
    pub pending: bool,
}

/// Which popup is currently shown over the Snippets screen (or as a full-screen
/// overlay from any screen).
#[derive(Debug)]
pub enum SnippetPopup {
    /// Adding a new snippet.
    Add(SnippetForm),
    /// Editing an existing snippet.
    Edit {
        snippet_idx: usize,
        form: SnippetForm,
    },
    /// Asking for confirmation before deleting.
    DeleteConfirm(usize),
    /// Collecting values for `{{placeholder}}` params before executing.
    ParamInput {
        snippet_idx: usize,
        host_names: Vec<String>,
        param_names: Vec<String>,
        param_fields: Vec<FormField>,
        focused_field: usize,
    },
    /// Picking which hosts to broadcast to.
    BroadcastPicker {
        snippet_idx: usize,
        /// Indices into `AppState.hosts` that are checked.
        selected_host_indices: Vec<usize>,
        /// Highlighted row in the list.
        cursor: usize,
    },
    /// Single-line command input for quick-execute.
    QuickExecuteInput {
        host_name: String,
        command_field: FormField,
    },
    /// Scrollable results from one or more hosts.
    Results {
        entries: Vec<SnippetResultEntry>,
        scroll: usize,
    },
}

/// Labels for each field in the snippet add/edit form.
pub const SNIPPET_FORM_FIELD_LABELS: &[&str] = &[
    "Name",
    "Command",
    "Scope (global / host)",
    "Host (if scope=host)",
    "Tags (comma-sep)",
    "Params (comma-sep)",
];

/// The snippet add/edit form.
#[derive(Debug, Clone)]
pub struct SnippetForm {
    /// Parallel to `SNIPPET_FORM_FIELD_LABELS`.
    pub fields: Vec<FormField>,
    pub focused_field: usize,
}

impl SnippetForm {
    /// Creates an empty form for adding a new snippet.
    pub fn empty() -> Self {
        Self {
            fields: SNIPPET_FORM_FIELD_LABELS
                .iter()
                .map(|_| FormField::default())
                .collect(),
            focused_field: 0,
        }
    }

    /// Creates a form pre-filled from an existing snippet (for editing).
    pub fn from_snippet(s: &Snippet) -> Self {
        let mut form = Self::empty();
        form.fields[0] = FormField::with_value(&s.name);
        form.fields[1] = FormField::with_value(&s.command);
        form.fields[2] = FormField::with_value(match s.scope {
            SnippetScope::Global => "global",
            SnippetScope::Host => "host",
        });
        form.fields[3] = FormField::with_value(s.host.as_deref().unwrap_or(""));
        form.fields[4] = FormField::with_value(s.tags.as_deref().unwrap_or(&[]).join(", "));
        form.fields[5] = FormField::with_value(s.params.as_deref().unwrap_or(&[]).join(", "));
        form
    }

    /// Validates the form and converts it into a [`Snippet`].
    ///
    /// # Errors
    /// Returns a human-readable error string if validation fails.
    pub fn to_snippet(&self) -> Result<Snippet, String> {
        let name = self.fields[0].value.trim().to_string();
        if name.is_empty() {
            return Err("Name cannot be empty".to_string());
        }

        let command = self.fields[1].value.trim().to_string();
        if command.is_empty() {
            return Err("Command cannot be empty".to_string());
        }

        let scope_str = self.fields[2].value.trim().to_lowercase();
        let scope = match scope_str.as_str() {
            "host" => SnippetScope::Host,
            _ => SnippetScope::Global, // default to global
        };

        let host = {
            let v = self.fields[3].value.trim();
            if scope == SnippetScope::Host && v.is_empty() {
                return Err("Host cannot be empty when scope is 'host'".to_string());
            }
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        };

        let tags: Vec<String> = self.fields[4]
            .value
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        let params: Vec<String> = self.fields[5]
            .value
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();

        Ok(Snippet {
            name,
            command,
            scope,
            host,
            tags: if tags.is_empty() { None } else { Some(tags) },
            params: if params.is_empty() {
                None
            } else {
                Some(params)
            },
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

/// UI state specific to the Snippets screen.
#[derive(Debug, Default)]
pub struct SnippetsView {
    /// Selected row index within `filtered_indices`.
    pub selected: usize,
    /// True when the user is actively typing a search query.
    pub search_mode: bool,
    /// Current search query.
    pub search_query: String,
    /// Indices into `AppState.snippets` matching the current query.
    pub filtered_indices: Vec<usize>,
    /// Currently visible popup (if any).
    pub popup: Option<SnippetPopup>,
}

impl SnippetsView {
    /// Returns the index into `AppState.snippets` for the selected row.
    pub fn selected_snippet_idx(&self) -> Option<usize> {
        self.filtered_indices.get(self.selected).copied()
    }

    /// Rebuilds `filtered_indices` with case-insensitive substring matching.
    pub fn rebuild_filter(&mut self, snippets: &[Snippet], query: &str) {
        self.filtered_indices = filter_snippets(snippets, query);
        if self.filtered_indices.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.filtered_indices.len() {
            self.selected = self.filtered_indices.len() - 1;
        }
    }

    pub fn select_next(&mut self) {
        if self.filtered_indices.is_empty() {
            return;
        }
        self.selected = (self.selected + 1).min(self.filtered_indices.len() - 1);
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}

/// Case-insensitive substring filter over snippet name / command / tags.
pub fn filter_snippets(snippets: &[Snippet], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..snippets.len()).collect();
    }
    let q = query.to_lowercase();
    snippets
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.name.to_lowercase().contains(&q)
                || s.command.to_lowercase().contains(&q)
                || s.tags
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .any(|t| t.to_lowercase().contains(&q))
        })
        .map(|(i, _)| i)
        .collect()
}

impl App {
    // -----------------------------------------------------------------------
    // Snippet execution private methods
    // -----------------------------------------------------------------------

    /// Checks whether the snippet requires param input; if yes, opens the
    /// `ParamInput` popup, otherwise fires the execution tasks immediately.
    pub(crate) async fn execute_snippet(&mut self, snippet_idx: usize, host_names: Vec<String>) {
        let snippet = {
            let state = self.state.read().await;
            state.snippets.get(snippet_idx).cloned()
        };
        let Some(snippet) = snippet else { return };

        let param_names: Vec<String> = snippet.params.as_deref().unwrap_or(&[]).to_vec();

        if !param_names.is_empty() {
            // Need values for placeholders — open the param input popup.
            let param_fields = param_names.iter().map(|_| FormField::default()).collect();
            self.view.snippets_view.popup = Some(SnippetPopup::ParamInput {
                snippet_idx,
                host_names,
                param_names,
                param_fields,
                focused_field: 0,
            });
        } else {
            self.spawn_snippet_tasks(&snippet, &host_names, &[]).await;
        }
    }

    /// Called when the user confirms the `ParamInput` popup.  Collects the
    /// filled values and fires the execution tasks.
    pub(crate) async fn handle_confirm_param_input(&mut self) {
        let popup = self.view.snippets_view.popup.take();
        match popup {
            Some(SnippetPopup::ParamInput {
                snippet_idx,
                host_names,
                param_names,
                param_fields,
                ..
            }) => {
                let param_values: Vec<String> = param_fields
                    .iter()
                    .map(|f| f.value.trim().to_string())
                    .collect();

                let snippet = {
                    let state = self.state.read().await;
                    state.snippets.get(snippet_idx).cloned()
                };
                if let Some(snippet) = snippet {
                    self.spawn_snippet_tasks(&snippet, &host_names, &param_values)
                        .await;
                }
                // `self.view.snippets_view.popup` is already `None` from `.take()`.
                // spawn_snippet_tasks will replace it with Results.
                let _ = (param_names,); // suppress unused warning
            }
            other => {
                // Wrong popup type — restore.
                self.view.snippets_view.popup = other;
            }
        }
    }

    /// Substitutes `{{placeholder}}` values in the command, opens a `Results`
    /// popup with pending entries, and spawns one tokio task per host.
    async fn spawn_snippet_tasks(
        &mut self,
        snippet: &Snippet,
        host_names: &[String],
        param_values: &[String],
    ) {
        let command = substitute_params(&snippet.command, snippet.params.as_deref(), param_values);

        // Pre-populate the Results popup with pending entries.
        let entries: Vec<SnippetResultEntry> = host_names
            .iter()
            .map(|h| SnippetResultEntry {
                host_name: h.clone(),
                snippet_name: snippet.name.clone(),
                output: Ok(String::new()),
                pending: true,
            })
            .collect();
        self.view.snippets_view.popup = Some(SnippetPopup::Results { entries, scroll: 0 });

        // Collect Host structs for the requested names.
        let hosts: Vec<Host> = {
            let state = self.state.read().await;
            host_names
                .iter()
                .filter_map(|name| state.hosts.iter().find(|h| &h.name == name).cloned())
                .collect()
        };

        // Spawn one task per host.
        for host in hosts {
            let tx = self.event_tx.clone();
            let cmd = command.clone();
            let sname = snippet.name.clone();
            tokio::spawn(async move {
                let result = run_command_on_host(&host, &cmd).await;
                let _ = tx
                    .send(AppEvent::SnippetResult {
                        host_name: host.name.clone(),
                        snippet_name: sname,
                        output: result,
                    })
                    .await;
            });
        }
    }

    /// Confirms the broadcast-picker and runs the snippet on all checked hosts.
    pub(crate) async fn handle_confirm_broadcast(&mut self) {
        let popup = self.view.snippets_view.popup.take();
        match popup {
            Some(SnippetPopup::BroadcastPicker {
                snippet_idx,
                selected_host_indices,
                ..
            }) => {
                if selected_host_indices.is_empty() {
                    self.view.status_message = Some("No hosts selected.".to_string());
                    return;
                }
                let host_names: Vec<String> = {
                    let state = self.state.read().await;
                    selected_host_indices
                        .iter()
                        .filter_map(|&i| state.hosts.get(i))
                        .map(|h| h.name.clone())
                        .collect()
                };
                self.execute_snippet(snippet_idx, host_names).await;
            }
            other => {
                self.view.snippets_view.popup = other;
            }
        }
    }

    /// Runs an ad-hoc quick-execute command.
    pub(crate) async fn run_quick_execute(&mut self, host_name: String, command: String) {
        let host = {
            let state = self.state.read().await;
            state.hosts.iter().find(|h| h.name == host_name).cloned()
        };

        let Some(host) = host else {
            self.view.snippets_view.popup = Some(SnippetPopup::Results {
                entries: vec![SnippetResultEntry {
                    host_name: host_name.clone(),
                    snippet_name: "(quick-execute)".to_string(),
                    output: Err(format!("Host '{}' not found.", host_name)),
                    pending: false,
                }],
                scroll: 0,
            });
            return;
        };

        // Open Results popup with a single pending entry.
        self.view.snippets_view.popup = Some(SnippetPopup::Results {
            entries: vec![SnippetResultEntry {
                host_name: host_name.clone(),
                snippet_name: "(quick-execute)".to_string(),
                output: Ok(String::new()),
                pending: true,
            }],
            scroll: 0,
        });

        let tx = self.event_tx.clone();
        let cmd = command.clone();
        tokio::spawn(async move {
            let result = run_command_on_host(&host, &cmd).await;
            let _ = tx
                .send(AppEvent::SnippetResult {
                    host_name: host.name.clone(),
                    snippet_name: "(quick-execute)".to_string(),
                    output: result,
                })
                .await;
        });
    }

    /// Executes a quick view command for a specific service.
    /// This is similar to quick-execute but uses predefined commands per service type.
    pub(crate) async fn execute_quick_view(&mut self, service_kind: ServiceKind) {
        // Get the currently selected host from the dashboard/detail view
        let (host, host_name) = {
            let state = self.state.read().await;
            match self.view.host_list.selected_host_idx() {
                Some(idx) => match state.hosts.get(idx) {
                    Some(h) => (Some(h.clone()), h.name.clone()),
                    None => (None, "(unknown)".to_string()),
                },
                None => (None, "(no selection)".to_string()),
            }
        };

        let Some(host) = host else {
            self.view.snippets_view.popup = Some(SnippetPopup::Results {
                entries: vec![SnippetResultEntry {
                    host_name: host_name.clone(),
                    snippet_name: format!("Quick View: {:?}", service_kind),
                    output: Err("No host selected.".to_string()),
                    pending: false,
                }],
                scroll: 0,
            });
            return;
        };

        // Determine the command based on service kind
        let (command, service_name) = match service_kind {
            ServiceKind::Docker => (
                "docker compose ps -a 2>/dev/null || docker ps -a",
                "Docker Containers",
            ),
            ServiceKind::Nginx => (
                "echo '=== Nginx Status ===' && systemctl status nginx --no-pager || service nginx status",
                "Nginx Status",
            ),
            ServiceKind::PostgreSQL => (
                "echo '=== PostgreSQL Connections ===' && sudo -u postgres psql -c 'SELECT count(*) as connections, state FROM pg_stat_activity GROUP BY state;' 2>/dev/null || echo 'No access to PostgreSQL'",
                "PostgreSQL Connections",
            ),
            ServiceKind::Redis => (
                "echo '=== Redis Info ===' && redis-cli info server 2>/dev/null | head -20 || echo 'Redis not accessible'",
                "Redis Info",
            ),
            ServiceKind::NodeJS => (
                "echo '=== PM2 Status ===' && pm2 status 2>/dev/null || (echo '=== Node Processes ===' && ps aux | grep -E '[n]ode ' | head -10)",
                "Node.js Processes",
            ),
        };

        // Open Results popup with a single pending entry
        self.view.snippets_view.popup = Some(SnippetPopup::Results {
            entries: vec![SnippetResultEntry {
                host_name: host_name.clone(),
                snippet_name: format!("Quick View: {}", service_name),
                output: Ok(String::new()),
                pending: true,
            }],
            scroll: 0,
        });

        let tx = self.event_tx.clone();
        let cmd = command.to_string();
        let sname = format!("Quick View: {}", service_name);
        tokio::spawn(async move {
            let result = run_command_on_host(&host, &cmd).await;
            let _ = tx
                .send(AppEvent::SnippetResult {
                    host_name: host.name.clone(),
                    snippet_name: sname,
                    output: result,
                })
                .await;
        });
    }

    /// Confirms the snippet add/edit form and saves.
    pub(crate) async fn handle_confirm_snippet_form(&mut self) {
        match self.view.snippets_view.popup.take() {
            Some(SnippetPopup::Add(form)) => match form.to_snippet() {
                Ok(snippet) => {
                    {
                        let mut state = self.state.write().await;
                        state.snippets.push(snippet);
                    }
                    self.save_snippets().await;
                    let state = self.state.read().await;
                    let q = self.view.snippets_view.search_query.clone();
                    self.view.snippets_view.rebuild_filter(&state.snippets, &q);
                    self.view.status_message = Some("Snippet added.".to_string());
                }
                Err(e) => {
                    self.view.snippets_view.popup = Some(SnippetPopup::Add(form));
                    self.view.status_message = Some(format!("Error: {e}"));
                }
            },

            Some(SnippetPopup::Edit { snippet_idx, form }) => match form.to_snippet() {
                Ok(snippet) => {
                    {
                        let mut state = self.state.write().await;
                        if let Some(slot) = state.snippets.get_mut(snippet_idx) {
                            *slot = snippet;
                        }
                    }
                    self.save_snippets().await;
                    let state = self.state.read().await;
                    let q = self.view.snippets_view.search_query.clone();
                    self.view.snippets_view.rebuild_filter(&state.snippets, &q);
                    self.view.status_message = Some("Snippet updated.".to_string());
                }
                Err(e) => {
                    self.view.snippets_view.popup = Some(SnippetPopup::Edit { snippet_idx, form });
                    self.view.status_message = Some(format!("Error: {e}"));
                }
            },

            other => {
                self.view.snippets_view.popup = other;
            }
        }
    }

    /// Confirms snippet deletion.
    pub(crate) async fn handle_confirm_snippet_delete(&mut self) {
        if let Some(SnippetPopup::DeleteConfirm(idx)) = self.view.snippets_view.popup.take() {
            {
                let mut state = self.state.write().await;
                if idx < state.snippets.len() {
                    let removed = state.snippets.remove(idx);
                    self.view.status_message = Some(format!("Deleted snippet '{}'.", removed.name));
                }
            }
            self.save_snippets().await;
            let state = self.state.read().await;
            let q = self.view.snippets_view.search_query.clone();
            self.view.snippets_view.rebuild_filter(&state.snippets, &q);
        }
    }

    /// Persists `AppState.snippets` to `snippets.toml`.
    async fn save_snippets(&mut self) {
        let snippets = self.state.read().await.snippets.clone();
        if let Err(e) = config::snippets::save_snippets(&snippets) {
            self.view.status_message = Some(format!("Save failed: {e}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Free helper functions for snippet execution
// ---------------------------------------------------------------------------

/// Opens a fresh SSH connection, runs `command`, closes the connection, and
/// returns `Ok(stdout)` or `Err(error_message)`.
///
/// A new connection is opened for each invocation.  Connection pooling with
/// the metrics poller is a future optimisation.
async fn run_command_on_host(host: &Host, command: &str) -> Result<String, String> {
    let session = SshSession::connect(host)
        .await
        .map_err(|e| format!("Connect failed: {e}"))?;
    let output = session
        .run_command(command)
        .await
        .map_err(|e| format!("Command failed: {e}"))?;
    let _ = session.disconnect().await;
    Ok(output)
}

/// Replaces `{{param_name}}` placeholders in `command` with the
/// corresponding values from `param_values` (parallel to `param_names`).
fn substitute_params(
    command: &str,
    param_names: Option<&[String]>,
    param_values: &[String],
) -> String {
    let mut result = command.to_string();
    if let Some(names) = param_names {
        for (name, value) in names.iter().zip(param_values.iter()) {
            let placeholder = format!("{{{{{}}}}}", name);
            result = result.replace(&placeholder, value);
        }
    }
    result
}
