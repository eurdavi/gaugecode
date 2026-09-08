//! Commands the UI is allowed to call. The UI never touches the network or a
//! credential — it reads snapshots and asks for these (SPEC §4).

use std::sync::Arc;

use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::i18n::{fill, Language};
use crate::model::{ProviderAccount, ProviderId, ProviderSnapshot, ProviderStatus, SignInHint};
use crate::notch::{self, NotchAnimation, NotchEdge};
use crate::providers::Registry;
use crate::state::AppState;
use crate::store;
use crate::tray;

/// Emitted whenever anything in [`PrefsView`] changes, so every window (notch,
/// popup, settings) redraws together instead of drifting apart.
pub const PREFS_EVENT: &str = "prefs:changed";

/// One limit window as the per-provider gear offers it.
#[derive(Debug, Serialize)]
pub struct WindowChoice {
    pub id: String,
    pub label: String,
    pub hidden: bool,
}

/// Everything the UI needs about one provider, in one place.
#[derive(Debug, Serialize)]
pub struct ProviderView {
    pub id: ProviderId,
    pub name: &'static str,
    pub enabled: bool,
    /// `None` means "no reading yet" — the UI shows a placeholder, not a zero.
    pub status: Option<ProviderStatus>,
    /// Already filtered by the user's choices — this is what to draw.
    pub snapshot: Option<ProviderSnapshot>,
    /// Every window the provider reported, hidden ones included, so the gear
    /// can offer them back.
    pub windows: Vec<WindowChoice>,
    pub account: Option<ProviderAccount>,
    pub sign_in_hint: Option<SignInHint>,
    pub manage_url: Option<&'static str>,
    /// False for providers whose adapter is not in this build yet.
    pub implemented: bool,
    pub is_primary: bool,
}

/// The preferences the UI can see and change.
#[derive(Debug, Clone, Serialize)]
pub struct PrefsView {
    /// The language actually in use, already resolved.
    pub language: Language,
    /// What the user picked; `null` means "follow the system".
    pub language_override: Option<Language>,
    pub primary: ProviderId,
    pub notch_visible: bool,
    pub notch_edge: NotchEdge,
    pub notch_animation: NotchAnimation,
    pub autostart: bool,
    pub auto_update: bool,
    pub poll_seconds: u64,
    /// Bounds the UI offers, so the slider cannot ask for something the Rust
    /// side would clamp anyway.
    pub poll_seconds_min: u64,
    pub poll_seconds_max: u64,
    /// False on Wayland, where an app cannot place its own window. The UI
    /// explains it instead of offering a notch that would drift.
    pub notch_supported: bool,
    /// True when running on fixtures, so the UI can say the numbers are fake.
    pub demo: bool,
    pub version: &'static str,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "key", rename_all = "snake_case")]
pub enum PrefUpdate {
    ProviderEnabled { provider: ProviderId, enabled: bool },
    Primary { provider: ProviderId },
    NotchVisible { visible: bool },
    NotchEdge { edge: NotchEdge },
    NotchAnimation { animation: NotchAnimation },
    /// `null` goes back to following the operating system.
    Language { language: Option<Language> },
    Autostart { enabled: bool },
    AutoUpdate { enabled: bool },
    PollSeconds { seconds: u64 },
    /// Shows or hides one limit window of one provider.
    WindowHidden { provider: ProviderId, window: String, hidden: bool },
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
            let windows = state
                .snapshot(id)
                .map(|snapshot| {
                    snapshot
                        .windows
                        .into_iter()
                        .map(|window| WindowChoice {
                            hidden: prefs.is_window_hidden(id, &window.id),
                            id: window.id,
                            label: window.label,
                        })
                        .collect()
                })
                .unwrap_or_default();

            ProviderView {
                id,
                name: id.display_name(),
                enabled,
                status: state.status(id),
                snapshot: state.visible_snapshot(id),
                windows,
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

#[tauri::command]
pub fn get_prefs(state: State<'_, Arc<AppState>>) -> PrefsView {
    view_of(&state)
}

/// Announces the preferences to every window. Called at the end of `setup` as
/// well, because a webview can load and ask for them before the state exists.
pub fn publish_prefs(app: &AppHandle) {
    let view = view_of(&app.state::<Arc<AppState>>());
    let _ = app.emit(PREFS_EVENT, view);
}

fn view_of(state: &AppState) -> PrefsView {
    let prefs = state.prefs();
    PrefsView {
        language: state.language(),
        language_override: prefs.language,
        primary: prefs.primary,
        notch_visible: prefs.notch_visible,
        notch_edge: prefs.notch_edge,
        notch_animation: prefs.notch_animation,
        autostart: prefs.autostart,
        auto_update: prefs.auto_update,
        poll_seconds: prefs.poll_seconds,
        poll_seconds_min: store::MIN_POLL_SECONDS,
        poll_seconds_max: store::MAX_POLL_SECONDS,
        notch_supported: notch::positioning_supported(),
        demo: state.is_demo(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

/// Asks every provider loop to fetch now. Returns a message when the primary
/// provider is serving a rate-limit penalty, so the UI can say so instead of
/// pretending the click did something.
#[tauri::command]
pub fn refresh_now(state: State<'_, Arc<AppState>>) -> Option<String> {
    let primary = state.prefs().primary;
    if let Some(until) = state.backoff_until(primary) {
        let local = until.with_timezone(&Local).format("%H:%M").to_string();
        return Some(fill(state.language().strings().rate_limited_until, &local));
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
        PrefUpdate::NotchAnimation { animation } => state.set_notch_animation(animation),
        PrefUpdate::Language { language } => state.set_language(language),
        PrefUpdate::Autostart { enabled } => {
            // Ask the OS first: if registering the login item fails there is no
            // point remembering a preference the machine will not honour.
            match crate::autostart::apply(&app, enabled) {
                Ok(()) => state.set_autostart(enabled),
                Err(error) => tracing::warn!(%error, "could not change the autostart entry"),
            }
        }
        PrefUpdate::AutoUpdate { enabled } => state.set_auto_update(enabled),
        PrefUpdate::PollSeconds { seconds } => state.set_poll_seconds(seconds),
        PrefUpdate::WindowHidden { provider, window, hidden } => {
            state.set_window_hidden(provider, window, hidden)
        }
    }

    tray::update(&app);
    // The notch length depends on how many providers are on, and its position on
    // the chosen edge, so any of these changes has to re-anchor it.
    notch::apply(&app, app.state::<notch::NotchState>().mode());
    let _ = app.emit(PREFS_EVENT, view_of(&state));
    state.request_refresh();
}

/// Taking `State` here is deliberate: Tauri turns unmanaged state into a clean
/// command error, and the window recovers from the `prefs:changed` event that
/// `setup` emits a moment later.
#[tauri::command]
pub fn get_notch(app: AppHandle, state: State<'_, Arc<AppState>>) -> notch::NotchView {
    notch::view(&app, &state.prefs())
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
