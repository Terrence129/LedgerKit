#![deny(unsafe_op_in_unsafe_fn)]

pub mod application;
pub mod domain;
mod infrastructure;
mod ipc;
mod platform;

use application::facade::ApplicationFacade;
use infrastructure::file_settings::FileSettingsRepository;
use infrastructure::sqlite::SqliteLedgerManager;
use tauri::Manager;

use crate::ipc::{
    AppState, analyze_import, commit_import, create_backup, create_ledger, export_data,
    get_activity, get_backup_status, get_data_quality, get_expense_analysis, get_ledger_status,
    get_overview, open_ledger, post_event, preview_event, restore_backup, reverse_event,
    revise_event, save_cash_account, save_category, save_fx_revision, save_institution,
    save_instrument, save_portfolio, save_price_revision, update_settings,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the single `LedgerKit` desktop process tree.
///
/// # Panics
///
/// Panics when Tauri cannot initialize or run the desktop application. There
/// is no safe degraded mode when the trusted shell itself cannot start.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let local_data_root = app.path().app_local_data_dir()?;
            let ledger = SqliteLedgerManager::new(&local_data_root)?;
            let facade = ApplicationFacade::new(ledger, FileSettingsRepository::new(settings_path));
            app.manage(AppState::new(facade));
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                match event {
                    tauri::WindowEvent::Focused(focused) => {
                        if let Some(webview_window) = window.app_handle().get_webview_window("main")
                        {
                            platform::webview::handle_focus_change(&webview_window, *focused);
                        }
                    }
                    tauri::WindowEvent::Destroyed => {
                        window.state::<AppState>().create_exit_backup();
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            create_ledger,
            open_ledger,
            get_ledger_status,
            update_settings,
            save_institution,
            save_cash_account,
            save_category,
            save_portfolio,
            save_instrument,
            save_fx_revision,
            save_price_revision,
            preview_event,
            post_event,
            revise_event,
            reverse_event,
            get_expense_analysis,
            get_activity,
            get_overview,
            get_data_quality,
            analyze_import,
            commit_import,
            create_backup,
            restore_backup,
            get_backup_status,
            export_data
        ])
        .run(tauri::generate_context!())
        .expect("LedgerKit desktop runtime failed");
}
