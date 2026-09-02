fn main() {
    const COMMANDS: &[&str] = &[
        "create_ledger",
        "open_ledger",
        "get_ledger_status",
        "update_settings",
        "save_institution",
        "save_cash_account",
        "save_category",
        "save_portfolio",
        "save_instrument",
        "save_fx_revision",
        "save_price_revision",
        "preview_event",
        "post_event",
        "revise_event",
        "reverse_event",
        "get_expense_analysis",
        "get_activity",
        "preview_investment_event",
        "post_investment_event",
        "revise_investment_event",
        "get_investment_workspace",
        "get_overview",
        "get_data_quality",
        "analyze_import",
        "commit_import",
    ];
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to build the LedgerKit application manifest");
}
