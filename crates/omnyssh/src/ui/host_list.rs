use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{AppAction, HostForm, HostListView, HostPopup, ViewState};

// ---------------------------------------------------------------------------
// Input handling
// ---------------------------------------------------------------------------

/// Handles host-list-specific key events and returns an optional action for
/// the main loop to execute.
pub fn handle_input(key: KeyEvent, view: &mut ViewState) -> Option<AppAction> {
    let hlv = &mut view.host_list;

    if hlv.popup.is_some() {
        return handle_popup_input(key, hlv);
    }

    if hlv.search_mode {
        return handle_search_input(key, hlv);
    }

    handle_normal_input(key, hlv)
}

fn handle_normal_input(key: KeyEvent, hlv: &mut HostListView) -> Option<AppAction> {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            hlv.select_next();
            None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            hlv.select_prev();
            None
        }
        KeyCode::Enter => hlv.selected_host_idx().map(AppAction::ConnectAt),
        KeyCode::Char('/') => {
            hlv.search_mode = true;
            None
        }
        KeyCode::Char('a') => {
            hlv.popup = Some(HostPopup::Add(HostForm::empty()));
            None
        }
        KeyCode::Char('e') => {
            if hlv.selected_host_idx().is_some() {
                Some(AppAction::OpenEditPopup)
            } else {
                None
            }
        }
        KeyCode::Char('d') => {
            if let Some(idx) = hlv.selected_host_idx() {
                hlv.popup = Some(HostPopup::DeleteConfirm(idx));
            }
            None
        }
        KeyCode::Char('K') => {
            if hlv.selected_host_idx().is_some() {
                Some(AppAction::StartKeySetup)
            } else {
                None
            }
        }
        KeyCode::Char('r') => Some(AppAction::ReloadHosts),
        _ => None,
    }
}

fn handle_search_input(key: KeyEvent, hlv: &mut HostListView) -> Option<AppAction> {
    match key.code {
        KeyCode::Enter => {
            hlv.search_mode = false;
            None
        }
        KeyCode::Esc => {
            hlv.search_mode = false;
            hlv.search_query.clear();
            Some(AppAction::SearchQueryChanged)
        }
        KeyCode::Backspace => {
            hlv.search_query.pop();
            Some(AppAction::SearchQueryChanged)
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            hlv.search_query.push(c);
            Some(AppAction::SearchQueryChanged)
        }
        _ => None,
    }
}

fn handle_popup_input(key: KeyEvent, hlv: &mut HostListView) -> Option<AppAction> {
    // Esc always closes the popup regardless of type.
    if key.code == KeyCode::Esc {
        hlv.popup = None;
        return None;
    }

    match &mut hlv.popup {
        Some(HostPopup::Add(form)) | Some(HostPopup::Edit { form, .. }) => {
            handle_form_input(key, form)
        }
        Some(HostPopup::DeleteConfirm(_)) => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Some(AppAction::ConfirmDelete),
            KeyCode::Char('n') | KeyCode::Char('N') => {
                hlv.popup = None;
                None
            }
            _ => None,
        },
        Some(HostPopup::KeySetupConfirm(idx)) => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                Some(AppAction::ConfirmKeySetup(*idx))
            }
            KeyCode::Char('n') | KeyCode::Char('N') => Some(AppAction::CancelKeySetup),
            _ => None,
        },
        Some(HostPopup::KeySetupProgress { .. }) => {
            // Progress popup is non-interactive, only Esc can close it
            None
        }
        None => None,
    }
}

fn handle_form_input(key: KeyEvent, form: &mut HostForm) -> Option<AppAction> {
    match key.code {
        KeyCode::Enter => Some(AppAction::ConfirmForm),
        // Esc is handled before we reach this function (in handle_popup_input).
        KeyCode::Tab => {
            form.focus_next();
            None
        }
        KeyCode::BackTab => {
            form.focus_prev();
            None
        }
        KeyCode::Backspace => {
            if let Some(field) = form.fields.get_mut(form.focused_field) {
                field.backspace();
            }
            None
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(field) = form.fields.get_mut(form.focused_field) {
                field.insert_char(c);
            }
            None
        }
        _ => None,
    }
}
