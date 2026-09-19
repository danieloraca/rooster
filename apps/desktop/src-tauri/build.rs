fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "editing",
            "choose_draft_source",
            "load_settings",
            "choose_workspace",
            "remove_location",
            "start_scan",
            "scan_status",
            "search_artifacts",
            "cancel_scan",
            "get_inventory",
            "inspect_artifact",
            "assess_context",
        ]),
    ))
    .expect("Tauri application build failed");
}
