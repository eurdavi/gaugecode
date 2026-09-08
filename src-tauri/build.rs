fn main() {
    // tauri-build only re-runs when tauri.conf.json or capabilities/ change, so
    // a regenerated icon.ico would otherwise ship inside a stale Windows
    // resource — the installer kept the old logo for exactly that reason.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
