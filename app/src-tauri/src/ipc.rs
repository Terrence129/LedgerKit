#![forbid(unsafe_code)]

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::application::settings::{PRIVILEGED_OPERATION_COUNT, SettingsError, SettingsService};
use crate::infrastructure::file_settings::FileSettingsRepository;

pub struct AppState {
    settings: Mutex<SettingsService<FileSettingsRepository>>,
}

impl AppState {
    pub const fn new(settings: Mutex<SettingsService<FileSettingsRepository>>) -> Self {
        Self { settings }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LedgerStatusRequest {
    system_locale: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStatusResponse {
    app_version: &'static str,
    ui_locale: &'static str,
    ledger_state: &'static str,
    local_only: bool,
    privileged_operation_count: u8,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSettingsRequest {
    ui_locale: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsResponse {
    ui_locale: &'static str,
    persisted: bool,
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    code: &'static str,
}

impl From<SettingsError> for CommandError {
    fn from(value: SettingsError) -> Self {
        let code = match value {
            SettingsError::InvalidLocale => "SETTINGS_LOCALE_UNSUPPORTED",
            SettingsError::StorageUnavailable => "SETTINGS_STORAGE_UNAVAILABLE",
        };
        Self { code }
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes owned command arguments and State.
pub fn get_ledger_status(
    request: LedgerStatusRequest,
    state: State<'_, AppState>,
) -> Result<LedgerStatusResponse, CommandError> {
    let service = state.settings.lock().map_err(|_| CommandError {
        code: "APPLICATION_STATE_UNAVAILABLE",
    })?;
    let status = service.get_ledger_status(request.system_locale.as_deref())?;
    Ok(LedgerStatusResponse {
        app_version: env!("CARGO_PKG_VERSION"),
        ui_locale: status.ui_locale.as_str(),
        ledger_state: "not-created",
        local_only: true,
        privileged_operation_count: PRIVILEGED_OPERATION_COUNT,
    })
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri deserializes owned command arguments and State.
pub fn update_settings(
    request: UpdateSettingsRequest,
    state: State<'_, AppState>,
) -> Result<UpdateSettingsResponse, CommandError> {
    let service = state.settings.lock().map_err(|_| CommandError {
        code: "APPLICATION_STATE_UNAVAILABLE",
    })?;
    let locale = service.update_ui_locale(&request.ui_locale)?;
    Ok(UpdateSettingsResponse {
        ui_locale: locale.as_str(),
        persisted: true,
    })
}
