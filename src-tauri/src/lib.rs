pub mod autostart;
pub mod bar;
pub mod commands;
pub mod i18n;
pub mod icon;
pub mod model;
pub mod notch;
pub mod providers;
pub mod scheduler;
pub mod state;
pub mod store;
pub mod tray;
pub mod updater;

use std::sync::Arc;

use tauri::{Manager, WindowEvent};
use tracing_subscriber::EnvFilter;

use crate::i18n::Language;
use crate::providers::Registry;
use crate::state::AppState;
use crate::store::Store;

/// Fixtures only: no network, no credential reads (SPEC §9.4).
const DEMO_ENV: &str = "GAUGECODE_DEMO";

/// `RUST_LOG=gaugecode=debug` for verbose logs. Never logs tokens (AGENTS.md).
fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("gaugecode=info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
}

fn demo_mode() -> bool {
    std::env::var(DEMO_ENV).is_ok_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

/// Windows are created before `setup` runs, so an event can arrive before the
/// tray state is managed. `try_state` makes that a no-op instead of a panic.
fn note_popup_hidden(window: &tauri::Window) {
    if window.label() != tray::POPUP_WINDOW {
        return;
    }
    if let Some(state) = window.app_handle().try_state::<tray::TrayState>() {
        state.note_popup_hidden();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    let demo = demo_mode();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), demo, "starting gaugecode");

    let app = tauri::Builder::default()
        // A second launch must not add a second tray icon; it shows the running
        // app's settings instead. Has to be the first plugin registered.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tracing::debug!("a second instance was launched; showing settings instead");
            tray::show_settings(app);
        }))
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_prefs,
            commands::refresh_now,
            commands::set_pref,
            commands::open_settings,
            commands::hide_popup,
            commands::get_notch,
            commands::toggle_notch_pin,
            commands::get_bar,
            commands::open_popup,
            updater::update_status,
            updater::check_update_now,
            updater::install_update,
        ])
        .on_window_event(|window, event| match event {
            // Closing a window in a tray app means "put it away", not "quit".
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
                note_popup_hidden(window);
            }
            // The popup goes away as soon as it loses focus (SPEC §9.2).
            WindowEvent::Focused(false) if window.label() == tray::POPUP_WINDOW => {
                let _ = window.hide();
                note_popup_hidden(window);
            }
            // The taskbar strip is draggable; remember where it was left. The
            // state check is for a `Moved` that fires before setup has run.
            WindowEvent::Moved(position)
                if window.label() == bar::BAR_WINDOW
                    && window.app_handle().try_state::<bar::BarState>().is_some() =>
            {
                bar::note_moved(window.app_handle(), *position);
            }
            _ => {}
        })
        .setup(move |app| {
            // No Dock icon on macOS: this is a menu bar app.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = app.path().app_data_dir()?;
            tracing::debug!(dir = %dir.display(), "app data dir");

            // Resolved once: the OS language cannot change under a running app,
            // and the tray menu is built before any window could report it.
            let system_language = Language::from_os();
            tracing::debug!(?system_language, "resolved system language");

            let state = Arc::new(AppState::load(Store::new(dir), demo, system_language));
            app.manage(state);
            app.manage(Arc::new(Registry::new(demo)));
            app.manage(notch::NotchState::new());
            app.manage(bar::BarState::default());
            app.manage(updater::UpdateState::default());

            let handle = app.handle().clone();
            autostart::reconcile(&handle);
            tray::build(&handle)?;
            tray::update(&handle);
            notch::apply(&handle, notch::NotchMode::Folded);
            bar::apply(&handle);
            notch::spawn_pointer_watch(&handle);
            scheduler::spawn(&handle);
            updater::spawn_check(&handle);
            // A webview that loaded before this point got an error from
            // `get_prefs`; this is how it catches up.
            commands::publish_prefs(&handle);

            // A tray app with no window is invisible on a first launch — the
            // user has no way to know it started. So the walkthrough opens
            // itself once, and only once.
            if !app.state::<Arc<AppState>>().prefs().onboarded {
                tray::show_settings(&handle);
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building gaugecode");

    app.run(|_app, event| {
        // Hiding the last window must not end the process; only Quit (which sets
        // an exit code) may.
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
