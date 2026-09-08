//! Launching with the system (SPEC §9.3).
//!
//! The operating system owns the truth here — a login item can be removed in
//! Task Manager on Windows or in System Settings on macOS, without the app ever
//! being told. So the preference is reconciled against the OS at startup rather
//! than assumed to still hold.

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::state::AppState;

pub fn apply(app: &AppHandle, enabled: bool) -> Result<(), tauri_plugin_autostart::Error> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
}

/// Aligns the stored preference with what the machine actually has registered.
///
/// When the two disagree the OS wins: the user changing it outside the app is a
/// deliberate act, and silently re-adding a login item they removed would be
/// exactly the kind of behaviour this app promises not to have.
pub fn reconcile(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>();
    let wanted = state.prefs().autostart;

    match app.autolaunch().is_enabled() {
        Ok(actual) if actual != wanted => {
            tracing::debug!(
                stored = wanted,
                actual,
                "autostart differs from the preference; trusting the system"
            );
            state.set_autostart(actual);
        }
        Ok(_) => {}
        Err(error) => tracing::debug!(%error, "could not read the autostart entry"),
    }
}
