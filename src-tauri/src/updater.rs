//! Signed auto-update (SPEC §10).
//!
//! Every artifact is verified against the public key embedded in
//! `tauri.conf.json` before anything is installed; a build not signed with the
//! matching private key simply will not apply. That check is Tauri's and cannot
//! be turned off.
//!
//! Nothing installs itself behind the user's back: the check only *reports* that
//! a version exists, and installing is an explicit click in Settings.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::state::AppState;

pub const UPDATE_EVENT: &str = "update:available";

/// Waited out before the first check so launch is not competing with it.
const STARTUP_DELAY: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize)]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: Option<String>,
}

#[derive(Default)]
pub struct UpdateState {
    available: Mutex<Option<AvailableUpdate>>,
}

impl UpdateState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<AvailableUpdate>> {
        self.available.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn get(&self) -> Option<AvailableUpdate> {
        self.lock().clone()
    }
}

/// Looks for a newer signed release. `Ok(None)` means "already current".
async fn check(app: &AppHandle) -> Result<Option<AvailableUpdate>, String> {
    let updater = app.updater().map_err(|error| error.to_string())?;
    let update = updater.check().await.map_err(|error| error.to_string())?;

    let found = update.map(|update| AvailableUpdate {
        version: update.version.clone(),
        notes: update.body.clone(),
    });

    *app.state::<UpdateState>().lock() = found.clone();
    if let Some(update) = found.as_ref() {
        tracing::info!(version = %update.version, "an update is available");
        let _ = app.emit(UPDATE_EVENT, update.clone());
    }
    Ok(found)
}

/// Checks once, shortly after launch, if the user left auto-update on.
pub fn spawn_check(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        if !app.state::<Arc<AppState>>().prefs().auto_update {
            return;
        }
        if let Err(error) = check(&app).await {
            // An update server that is unreachable is not worth bothering the
            // user about; the app works fine without it.
            tracing::debug!(%error, "update check failed");
        }
    });
}

#[tauri::command]
pub fn update_status(state: tauri::State<'_, UpdateState>) -> Option<AvailableUpdate> {
    state.get()
}

#[tauri::command]
pub async fn check_update_now(app: AppHandle) -> Result<Option<AvailableUpdate>, String> {
    check(&app).await
}

/// Downloads and installs the pending update, then restarts. On Windows the
/// installer requires the app to exit first, which Tauri handles.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Err("no update available".into());
    };

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;

    app.restart();
}
