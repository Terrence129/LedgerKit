#![forbid(unsafe_code)]

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::application::error::ApplicationError;
use crate::application::facade::ApplicationFacade;
use crate::application::ledger::LedgerStatus;
use crate::application::settings::PRIVILEGED_OPERATION_COUNT;
use crate::infrastructure::file_settings::FileSettingsRepository;
use crate::infrastructure::sqlite::SqliteLedgerManager;

type DesktopFacade = ApplicationFacade<SqliteLedgerManager, FileSettingsRepository>;

pub struct AppState {
    facade: Mutex<DesktopFacade>,
}

impl AppState {
    pub const fn new(facade: DesktopFacade) -> Self {
        Self {
            facade: Mutex::new(facade),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateLedgerRequest {
    base_currency: String,
    ui_locale: String,
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
    ledger_id: Option<String>,
    schema_version: Option<u32>,
    base_currency: Option<String>,
    event_watermark: u64,
    projection_watermark: u64,
    calculation_version: &'static str,
    blocked_reason: Option<&'static str>,
    local_only: bool,
    privileged_operation_count: u8,
}

impl From<LedgerStatus> for LedgerStatusResponse {
    fn from(value: LedgerStatus) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION"),
            ui_locale: value.ui_locale.as_str(),
            ledger_state: value.state.as_str(),
            ledger_id: value.ledger_id,
            schema_version: value.schema_version,
            base_currency: value.base_currency.map(|currency| currency.to_string()),
            event_watermark: value.event_watermark,
            projection_watermark: value.projection_watermark,
            calculation_version: value.calculation_version,
            blocked_reason: value.blocked_reason,
            local_only: true,
            privileged_operation_count: PRIVILEGED_OPERATION_COUNT,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSettingsRequest {
    ui_locale: String,
    base_currency: Option<String>,
    valuation_defaults: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsResponse {
    ui_locale: &'static str,
    base_currency: Option<String>,
    persisted: bool,
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    code: &'static str,
}

impl From<ApplicationError> for CommandError {
    fn from(value: ApplicationError) -> Self {
        Self { code: value.code() }
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn create_ledger(
    request: CreateLedgerRequest,
    state: State<'_, AppState>,
) -> Result<LedgerStatusResponse, CommandError> {
    let mut facade = lock_facade(&state)?;
    facade
        .create_ledger(&request.base_currency, &request.ui_locale)
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)] // Tauri injects State by value.
pub fn open_ledger(state: State<'_, AppState>) -> Result<LedgerStatusResponse, CommandError> {
    let mut facade = lock_facade(&state)?;
    facade.open_ledger().map(Into::into).map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_ledger_status(
    request: LedgerStatusRequest,
    state: State<'_, AppState>,
) -> Result<LedgerStatusResponse, CommandError> {
    let facade = lock_facade(&state)?;
    facade
        .get_ledger_status(request.system_locale.as_deref())
        .map(Into::into)
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn update_settings(
    request: UpdateSettingsRequest,
    state: State<'_, AppState>,
) -> Result<UpdateSettingsResponse, CommandError> {
    let valuation_defaults = request
        .valuation_defaults
        .map(|value| serde_json::to_string(&value))
        .transpose()
        .map_err(|_| CommandError {
            code: ApplicationError::StorageUnavailable.code(),
        })?;
    let mut facade = lock_facade(&state)?;
    let status = facade.update_settings(
        &request.ui_locale,
        request.base_currency.as_deref(),
        valuation_defaults,
    )?;
    Ok(UpdateSettingsResponse {
        ui_locale: status.ui_locale.as_str(),
        base_currency: status.base_currency.map(|currency| currency.to_string()),
        persisted: true,
    })
}

fn lock_facade<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, DesktopFacade>, CommandError> {
    state.facade.lock().map_err(|_| CommandError {
        code: ApplicationError::ApplicationStateUnavailable.code(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::CreateLedgerRequest;

    #[test]
    fn create_ledger_dto_cannot_request_an_arbitrary_database_path() {
        let forged = json!({
            "baseCurrency": "CNY",
            "uiLocale": "en-US",
            "databasePath": "D:/synced/ledger.sqlite3"
        });
        assert!(serde_json::from_value::<CreateLedgerRequest>(forged).is_err());
    }
}
