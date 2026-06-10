//! Startup "update available" popup state and the `App` methods that drive it.

use crossterm::event::{KeyCode, KeyEvent};

use super::*;

// ---------------------------------------------------------------------------
// Update popup
// ---------------------------------------------------------------------------

/// A button in the startup update popup. Buttons are laid out left-to-right
/// in [`UPDATE_BUTTONS`] order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateButton {
    /// Install the update, or — when the app cannot self-update — dismiss the
    /// popup and check again on the next launch.
    Primary,
    /// Never offer this particular version again.
    Skip,
    /// Disable update checks entirely.
    Disable,
}

/// The three update-popup buttons, in display order.
pub const UPDATE_BUTTONS: [UpdateButton; 3] = [
    UpdateButton::Primary,
    UpdateButton::Skip,
    UpdateButton::Disable,
];

/// Lifecycle phase of the update popup.
#[derive(Debug, Clone)]
pub enum UpdatePopupPhase {
    /// Awaiting the user's choice; holds the highlighted button index.
    Prompt { selected: usize },
    /// A download + install is in progress.
    Installing,
    /// Finished; holds a user-facing message and whether it succeeded.
    Done { message: String, ok: bool },
}

/// State of the startup "update available" popup.
#[derive(Debug, Clone)]
pub struct UpdatePopup {
    /// The release that triggered the popup.
    pub info: crate::update::UpdateInfo,
    /// Current lifecycle phase.
    pub phase: UpdatePopupPhase,
}

impl App {
    /// Translates a key event into an optional [`AppAction`].
    /// Handles input while the startup update popup is visible.
    pub(crate) async fn handle_update_popup_key(&mut self, key: KeyEvent) {
        let phase = match &self.view.update_popup {
            Some(popup) => popup.phase.clone(),
            None => return,
        };

        match phase {
            UpdatePopupPhase::Prompt { selected } => match key.code {
                KeyCode::Left | KeyCode::Char('h') => self.move_update_selection(-1),
                KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => self.move_update_selection(1),
                KeyCode::Enter => {
                    self.activate_update_choice(UPDATE_BUTTONS[selected]).await;
                }
                KeyCode::Esc => self.view.update_popup = None,
                _ => {}
            },
            // Input is ignored while the download/install runs.
            UpdatePopupPhase::Installing => {}
            UpdatePopupPhase::Done { .. } => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
                    self.view.update_popup = None;
                }
            }
        }
    }

    /// Moves the highlighted update-popup button by `delta`, wrapping around.
    fn move_update_selection(&mut self, delta: i32) {
        if let Some(UpdatePopup {
            phase: UpdatePopupPhase::Prompt { selected },
            ..
        }) = &mut self.view.update_popup
        {
            let count = UPDATE_BUTTONS.len() as i32;
            *selected = (*selected as i32 + delta).rem_euclid(count) as usize;
        }
    }

    /// Carries out the action bound to the chosen update-popup button.
    async fn activate_update_choice(&mut self, button: UpdateButton) {
        let info = match &self.view.update_popup {
            Some(popup) => popup.info.clone(),
            None => return,
        };

        match button {
            // Self-update: start the download/install in the background.
            UpdateButton::Primary if info.can_self_update => {
                if let Some(popup) = &mut self.view.update_popup {
                    popup.phase = UpdatePopupPhase::Installing;
                }
                let tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let result = crate::update::perform_update(&info)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(AppEvent::UpdateInstalled(result)).await;
                });
            }
            // No self-update available — just dismiss; check again next launch.
            UpdateButton::Primary => self.view.update_popup = None,
            // Never offer this version again.
            UpdateButton::Skip => {
                self.config.update.skip_version = info.latest.clone();
                self.persist_update_config();
                self.view.update_popup = None;
            }
            // Turn off update checks entirely.
            UpdateButton::Disable => {
                self.config.update.check_on_startup = false;
                self.persist_update_config();
                self.view.update_popup = None;
            }
        }
    }

    /// Persists the update preferences. A failure is logged, never surfaced —
    /// it must not block dismissing the popup.
    fn persist_update_config(&self) {
        if let Err(e) = config::app_config::save_update_config(&self.config.update) {
            tracing::warn!("Failed to save update config: {}", e);
        }
    }
}
