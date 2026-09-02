fn main() {
    const COMMANDS: &[&str] = &[
        "get_ledger_status",
        "post_event",
        "get_activity",
        "get_overview",
        "get_expense_analysis",
        "analyze_import",
        "export_data",
        "authorize_attachment",
        "copy_attachment",
        "create_backup",
        "restore_backup",
        "mark_frontend_ready",
    ];

    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to build the Tauri application manifest");
}
