pub mod model;
pub mod providers;

use tracing_subscriber::EnvFilter;

/// `RUST_LOG=gaugecode=debug` for verbose logs. Never logs tokens (AGENTS.md).
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("gaugecode=info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting gaugecode");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
