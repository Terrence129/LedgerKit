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

use crate::ipc::{AppState, create_ledger, get_ledger_status, open_ledger, update_settings};

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
            if window.label() == "main"
                && let tauri::WindowEvent::Focused(focused) = event
                && let Some(webview_window) = window.app_handle().get_webview_window("main")
            {
                platform::webview::handle_focus_change(&webview_window, *focused);
            }
        })
        .invoke_handler(tauri::generate_handler![
            create_ledger,
            open_ledger,
            get_ledger_status,
            update_settings
        ])
        .run(tauri::generate_context!())
        .expect("LedgerKit desktop runtime failed");
}
