//! Commands the UI is allowed to call. The UI never touches the network or a
//! credential — it reads snapshots and asks for these (SPEC §4).

use std::sync::Arc;

use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::model::{ProviderAccount, ProviderId, ProviderSnapshot, ProviderStatus, SignInHint};
use crate::notch;
use crate::providers::Registry;
use crate::state::AppState;
use crate::tray;

/// Everything the UI needs about one provider, in one place.
#[derive(Debug, Serialize)]
pub struct ProviderView {
    pub id: ProviderId,
    pub name: &'static str,
    pub enabled: bool,
    /// `None` means "no reading yet" — the UI shows a placeholder, not a zero.
    pub status: Option<ProviderStatus>,
    pub snapshot: Option<ProviderSnapshot>,
    pub account: Option<ProviderAccount>,
    pub sign_in_hint: Option<SignInHint>,
    pub manage_url: Option<&'static str>,
    /// False for providers whose adapter is not in this build yet.
    pub implemented: bool,
    pub is_primary: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "key", rename_all = "snake_case")]
pub enum PrefUpdate {
    ProviderEnabled { provider: ProviderId, enabled: bool },
    Primary { provider: ProviderId },
    NotchVisible { visible: bool },
    NotchEdge { edge: notch::NotchEdge },
}

#[tauri::command]
pub fn get_state(
    state: State<'_, Arc<AppState>>,
    registry: State<'_, Arc<Registry>>,
) -> Vec<ProviderView> {
    let prefs = state.prefs();

    ProviderId::ALL
        .into_iter()
        .map(|id| {
            let provider = registry.get(id);
            let enabled = prefs.is_enabled(id);
            ProviderView {
                id,
                name: id.display_name(),
                enabled,
                status: state.status(id),
                snapshot: state.snapshot(id),
                // Reads the credential only; still no network on this path.
                account: provider.filter(|_| enabled).and_then(|p| p.account()),
                sign_in_hint: provider.map(|p| p.sign_in_hint()),
                manage_url: provider.map(|p| p.manage_url()),
                implemented: provider.is_some(),
                is_primary: prefs.primary == id,
            }
        })
        .collect()
}

/// Asks every provider loop to fetch now. Returns a message when the primary
/// provider is serving a rate-limit penalty, so the UI can say so instead of
/// pretending the click did something.
#[tauri::command]
pub fn refresh_now(state: State<'_, Arc<AppState>>) -> Option<String> {
    let primary = state.prefs().primary;
    if let Some(until) = state.backoff_until(primary) {
        let local = until.with_timezone(&Local).format("%H:%M");
        return Some(format!("Rate limited — waiting until {local}"));
    }
    state.request_refresh();
    None
}

#[tauri::command]
pub fn set_pref(app: AppHandle, state: State<'_, Arc<AppState>>, update: PrefUpdate) {
    match update {
        PrefUpdate::ProviderEnabled { provider, enabled } => {
            if !enabled {
                if let Some(adapter) = app.state::<Arc<Registry>>().get(provider) {
                    adapter.forget_cached_credential();
                }
            }
            state.set_provider_enabled(provider, enabled);
        }
        PrefUpdate::Primary { provider } => state.set_primary(provider),
        PrefUpdate::NotchVisible { visible } => state.set_notch_visible(visible),
        PrefUpdate::NotchEdge { edge } => state.set_notch_edge(edge),
    }
    tray::update(&app);
    // The notch length depends on how many providers are on, and its position on
    // the chosen edge, so any of these changes has to re-anchor it.
    notch::apply(&app, app.state::<notch::NotchState>().mode());
    state.request_refresh();
}

#[tauri::command]
pub fn get_notch(app: AppHandle) -> notch::NotchView {
    notch::view(&app)
}

/// Clicking the notch pins it open; clicking again lets it fold back.
#[tauri::command]
pub fn toggle_notch_pin(app: AppHandle) {
    let next = match app.state::<notch::NotchState>().mode() {
        notch::NotchMode::Pinned => notch::NotchMode::Folded,
        _ => notch::NotchMode::Pinned,
    };
    notch::apply(&app, next);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    tray::show_settings(&app);
}

#[tauri::command]
pub fn hide_popup(app: AppHandle) {
    if let Some(window) = app.get_webview_window(tray::POPUP_WINDOW) {
        let _ = window.hide();
    }
}
