fn main() {
    tauri_build::try_build(tauri_build::Attributes::default().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "window_minimize",
            "window_toggle_maximize",
            "window_close",
            "window_toggle_fullscreen",
            "window_start_dragging",
            "menu_action",
        ]),
    ))
    .expect("failed to run tauri build");
}
