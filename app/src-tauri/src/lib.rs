#![deny(unsafe_op_in_unsafe_fn)]

mod application;
mod domain;
mod infrastructure;
mod ipc;
mod platform;

use std::sync::Mutex;

use application::settings::SettingsService;
use infrastructure::file_settings::FileSettingsRepository;
use tauri::Manager;

use crate::ipc::{AppState, get_ledger_status, update_settings};

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
            let service = SettingsService::new(FileSettingsRepository::new(settings_path));
            app.manage(AppState::new(Mutex::new(service)));
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
        .invoke_handler(tauri::generate_handler![get_ledger_status, update_settings])
        .run(tauri::generate_context!())
        .expect("LedgerKit desktop runtime failed");
}
