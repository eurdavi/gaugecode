pub mod commands;
pub mod icon;
pub mod model;
pub mod notch;
pub mod providers;
pub mod scheduler;
pub mod state;
pub mod store;
pub mod tray;

use std::sync::Arc;

use tauri::{Manager, WindowEvent};
use tracing_subscriber::EnvFilter;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    let demo = demo_mode();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), demo, "starting gaugecode");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_positioner::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::refresh_now,
            commands::set_pref,
            commands::open_settings,
            commands::hide_popup,
            commands::get_notch,
            commands::toggle_notch_pin,
        ])
        .on_window_event(|window, event| match event {
            // Closing a window in a tray app means "put it away", not "quit".
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
                if window.label() == tray::POPUP_WINDOW {
                    window.app_handle().state::<tray::TrayState>().note_popup_hidden();
                }
            }
            // The popup goes away as soon as it loses focus (SPEC §9.2).
            WindowEvent::Focused(false) if window.label() == tray::POPUP_WINDOW => {
                let _ = window.hide();
                window.app_handle().state::<tray::TrayState>().note_popup_hidden();
            }
            _ => {}
        })
        .setup(move |app| {
            // No Dock icon on macOS: this is a menu bar app.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = app.path().app_data_dir()?;
            tracing::debug!(dir = %dir.display(), "app data dir");

            let state = Arc::new(AppState::load(Store::new(dir), demo));
            app.manage(state);
            app.manage(Arc::new(Registry::new(demo)));
            app.manage(notch::NotchState::new());

            let handle = app.handle().clone();
            tray::build(&handle)?;
            tray::update(&handle);
            notch::apply(&handle, notch::NotchMode::Folded);
            notch::spawn_pointer_watch(&handle);
            scheduler::spawn(&handle);

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
