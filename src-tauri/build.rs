fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_bootstrap",
            "get_top_processes",
            "save_preferences",
            "request_cleanup",
            "set_preferences_open",
        ]),
    ))
    .expect("Tauri build configuration failed");
}
