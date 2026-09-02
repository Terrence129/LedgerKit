pub mod application;
pub mod backup;
pub mod canonical;
pub mod decimal;
pub mod error;
pub mod excel;
pub mod ledger;
pub mod platform;

use tauri::Manager;

use application::{
    analyze_import, authorize_attachment, copy_attachment, create_backup, export_data,
    get_activity, get_expense_analysis, get_ledger_status, get_overview, mark_frontend_ready,
    post_event, restore_backup, setup_state,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = setup_state(app)?;
            app.manage(state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::Focused(focused) = event
                && let Some(webview_window) = window.app_handle().get_webview_window("main")
            {
                platform::handle_focus_change(&webview_window, *focused);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_ledger_status,
            post_event,
            get_activity,
            get_overview,
            get_expense_analysis,
            analyze_import,
            export_data,
            authorize_attachment,
            copy_attachment,
            create_backup,
            restore_backup,
            mark_frontend_ready,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
