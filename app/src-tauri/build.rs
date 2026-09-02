fn main() {
    const COMMANDS: &[&str] = &["get_ledger_status", "update_settings"];
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to build the LedgerKit application manifest");
}
