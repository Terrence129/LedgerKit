use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::backup::{
    BackupSummary, RestoreSummary, create_encrypted_backup, restore_encrypted_backup,
};
use crate::canonical::{sha256_hex, sha256_prefixed};
use crate::error::{CommandError, SpikeError, SpikeResult};
use crate::excel::{ExportSummary, ImportSummary, analyze_known_template, export_standardized};
use crate::ledger::{
    ActivityPage, LedgerStatus, LedgerStore, Overview, PostEventRequest, PostEventResponse,
};

const MAX_ATTACHMENT_BYTES: u64 = 5 * 1024 * 1024;

pub struct AppState {
    pub ledger: LedgerStore,
    root: PathBuf,
    authorized_attachments: Mutex<HashMap<String, PathBuf>>,
    authorized_backups: Mutex<HashMap<String, PathBuf>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityRequest {
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpenseAnalysisRequest {
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseChartRow {
    pub bucket_id: String,
    pub label: String,
    pub amount: String,
    pub distinct_event_count: u64,
    pub width_basis_points: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseAnalysisView {
    pub query_result: Value,
    pub chart_rows: Vec<ExpenseChartRow>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentTokenRequest {
    pub authorization_token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentAuthorization {
    pub authorization_token: String,
    pub display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedAttachment {
    pub managed_name: String,
    pub relative_location: String,
    pub byte_count: u64,
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordRequest {
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreRequest {
    pub backup_id: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrontendReadyMetrics {
    pub first_render_ms: f64,
    pub expense_render_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyAcknowledgement {
    pub recorded: bool,
}

impl AppState {
    pub fn new(root: PathBuf) -> SpikeResult<Self> {
        std::fs::create_dir_all(&root)?;
        let ledger = LedgerStore::open(root.join("ledgerkit-spike.sqlite"))?;
        ledger.initialize_demo()?;
        Ok(Self {
            ledger,
            root,
            authorized_attachments: Mutex::new(HashMap::new()),
            authorized_backups: Mutex::new(HashMap::new()),
        })
    }

    fn authorize_attachment_path(&self, path: PathBuf) -> SpikeResult<AttachmentAuthorization> {
        let canonical = path.canonicalize()?;
        if !canonical.is_file() {
            return Err(SpikeError::FileAuthorizationRejected);
        }
        let token = random_token()?;
        let display_name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("selected-file")
            .to_owned();
        self.authorized_attachments
            .lock()
            .map_err(|_| SpikeError::InternalState)?
            .insert(token.clone(), canonical);
        Ok(AttachmentAuthorization {
            authorization_token: token,
            display_name,
        })
    }

    fn copy_authorized_attachment(&self, token: &str) -> SpikeResult<ManagedAttachment> {
        let source = self
            .authorized_attachments
            .lock()
            .map_err(|_| SpikeError::InternalState)?
            .remove(token)
            .ok_or(SpikeError::FileAuthorizationRejected)?;
        let metadata = std::fs::metadata(&source)?;
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(SpikeError::AttachmentTooLarge);
        }
        let bytes = std::fs::read(&source)?;
        let hash = sha256_hex(&bytes);
        let extension = safe_extension(&source);
        let managed_name = if extension.is_empty() {
            hash.clone()
        } else {
            format!("{hash}.{extension}")
        };
        let attachment_root = self.root.join("attachments");
        std::fs::create_dir_all(&attachment_root)?;
        let destination = attachment_root.join(&managed_name);
        if !destination.exists() {
            std::fs::write(&destination, &bytes)?;
        }
        Ok(ManagedAttachment {
            managed_name: managed_name.clone(),
            relative_location: format!("attachments/{managed_name}"),
            byte_count: metadata.len(),
            sha256: sha256_prefixed(&bytes),
        })
    }
}

#[tauri::command]
pub fn get_ledger_status(state: State<'_, AppState>) -> Result<LedgerStatus, CommandError> {
    state.ledger.status().map_err(Into::into)
}

#[tauri::command]
pub fn post_event(
    state: State<'_, AppState>,
    request: PostEventRequest,
) -> Result<PostEventResponse, CommandError> {
    state.ledger.post_event(&request).map_err(Into::into)
}

#[tauri::command]
pub fn get_activity(
    state: State<'_, AppState>,
    request: ActivityRequest,
) -> Result<ActivityPage, CommandError> {
    state
        .ledger
        .activity(request.page, request.page_size)
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_overview(state: State<'_, AppState>) -> Result<Overview, CommandError> {
    state.ledger.overview().map_err(Into::into)
}

#[tauri::command]
pub fn get_expense_analysis(
    state: State<'_, AppState>,
    request: ExpenseAnalysisRequest,
) -> Result<ExpenseAnalysisView, CommandError> {
    let result = state
        .ledger
        .expense_analysis(&request.start_date, &request.end_date)
        .map_err(CommandError::from)?;
    let mut source_rows = result["top10"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !result["top10"]["other"].is_null() {
        source_rows.push(result["top10"]["other"].clone());
    }
    let maximum = source_rows
        .first()
        .and_then(|row| row["amount"].as_str())
        .map(crate::decimal::parse_stored_decimal)
        .transpose()
        .map_err(CommandError::from)?
        .unwrap_or(Decimal::ONE);
    let chart_rows = source_rows
        .into_iter()
        .map(|row| {
            let amount = row["amount"]
                .as_str()
                .ok_or(SpikeError::InternalState)
                .and_then(crate::decimal::parse_stored_decimal)?;
            let basis_points = ((amount * Decimal::from(10_000u32)) / maximum)
                .round_dp(0)
                .to_string()
                .parse::<u32>()
                .map_err(|_| SpikeError::InternalState)?
                .max(200);
            Ok(ExpenseChartRow {
                bucket_id: row["bucket_id"]
                    .as_str()
                    .ok_or(SpikeError::InternalState)?
                    .to_owned(),
                label: row["label"]
                    .as_str()
                    .ok_or(SpikeError::InternalState)?
                    .to_owned(),
                amount: row["amount"]
                    .as_str()
                    .ok_or(SpikeError::InternalState)?
                    .to_owned(),
                distinct_event_count: row["distinct_event_count"]
                    .as_u64()
                    .ok_or(SpikeError::InternalState)?,
                width_basis_points: basis_points,
            })
        })
        .collect::<SpikeResult<Vec<_>>>()
        .map_err(CommandError::from)?;
    Ok(ExpenseAnalysisView {
        query_result: result,
        chart_rows,
    })
}

#[tauri::command]
pub async fn analyze_import() -> Result<ImportSummary, CommandError> {
    let selection = rfd::AsyncFileDialog::new()
        .add_filter("Excel workbook", &["xlsx"])
        .pick_file()
        .await
        .ok_or(SpikeError::Cancelled)?;
    let canonical = selection.path().canonicalize().map_err(SpikeError::from)?;
    if !canonical.is_file() {
        return Err(SpikeError::FileAuthorizationRejected.into());
    }
    tauri::async_runtime::spawn_blocking(move || analyze_known_template(&canonical))
        .await
        .map_err(|_| CommandError::from(SpikeError::InternalState))?
        .map_err(Into::into)
}

#[tauri::command]
pub fn export_data(state: State<'_, AppState>) -> Result<ExportSummary, CommandError> {
    let export_root = state.root.join("exports");
    let file_name = format!("ledgerkit-standardized-{}.xlsx", unique_number());
    let events = state.ledger.all_activity()?;
    export_standardized(&events, &export_root.join(file_name)).map_err(Into::into)
}

#[tauri::command]
pub fn authorize_attachment(
    state: State<'_, AppState>,
) -> Result<AttachmentAuthorization, CommandError> {
    let selection = rfd::FileDialog::new()
        .pick_file()
        .ok_or(SpikeError::Cancelled)?;
    state
        .authorize_attachment_path(selection)
        .map_err(Into::into)
}

#[tauri::command]
pub fn copy_attachment(
    state: State<'_, AppState>,
    request: AttachmentTokenRequest,
) -> Result<ManagedAttachment, CommandError> {
    state
        .copy_authorized_attachment(&request.authorization_token)
        .map_err(Into::into)
}

#[tauri::command]
pub fn create_backup(
    state: State<'_, AppState>,
    request: PasswordRequest,
) -> Result<BackupSummary, CommandError> {
    let backup_root = state.root.join("backups");
    let backup_id = format!("backup-{}", unique_number());
    let path = backup_root.join(format!("{backup_id}.ledgerkit-backup"));
    let summary = state.ledger.with_connection(|connection| {
        create_encrypted_backup(connection, &path, &request.password)
    })?;
    state
        .authorized_backups
        .lock()
        .map_err(|_| SpikeError::InternalState)?
        .insert(backup_id, path);
    Ok(summary)
}

#[tauri::command]
pub fn restore_backup(
    state: State<'_, AppState>,
    request: RestoreRequest,
) -> Result<RestoreSummary, CommandError> {
    let path = state
        .authorized_backups
        .lock()
        .map_err(|_| SpikeError::InternalState)?
        .get(&request.backup_id)
        .cloned()
        .ok_or(SpikeError::BackupAuthorizationRejected)?;
    state
        .ledger
        .with_connection(|connection| {
            restore_encrypted_backup(
                connection,
                &path,
                &request.password,
                &state.root.join("restore-work"),
            )
        })
        .map_err(Into::into)
}

#[tauri::command]
pub fn mark_frontend_ready(
    app: AppHandle,
    metrics: FrontendReadyMetrics,
) -> Result<ReadyAcknowledgement, CommandError> {
    if let Some(window) = app.get_webview_window("main") {
        crate::platform::synchronize_memory_level(&window)
            .map_err(|_| CommandError::from(SpikeError::InternalState))?;
    }
    let mut recorded = false;
    if let Some(path) = std::env::var_os("LEDGERKIT_SPIKE_READY_FILE") {
        let value = serde_json::json!({
            "firstRenderMs": metrics.first_render_ms,
            "expenseRenderMs": metrics.expense_render_ms,
            "recordedAtUnixMs": unique_number(),
        });
        let bytes = serde_json::to_vec(&value)
            .map_err(SpikeError::from)
            .map_err(CommandError::from)?;
        std::fs::write(PathBuf::from(path), bytes)
            .map_err(SpikeError::from)
            .map_err(CommandError::from)?;
        recorded = true;
    }
    if std::env::var_os("LEDGERKIT_SPIKE_AUTOCLOSE").as_deref() == Some("1".as_ref()) {
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            handle.exit(0);
        });
    }
    Ok(ReadyAcknowledgement { recorded })
}

pub fn setup_state(app: &tauri::App) -> SpikeResult<AppState> {
    let root = if let Some(path) = std::env::var_os("LEDGERKIT_SPIKE_DATA_DIR") {
        PathBuf::from(path)
    } else {
        app.path()
            .app_local_data_dir()
            .map_err(|_| SpikeError::InternalState)?
    };
    AppState::new(root)
}

fn random_token() -> SpikeResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| SpikeError::Crypto)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn unique_number() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn safe_extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .take(10)
                .collect::<String>()
                .to_ascii_lowercase()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn attachment_copy_requires_one_time_authorization_and_contains_destination() {
        let directory = tempdir().unwrap();
        let state = AppState::new(directory.path().join("app")).unwrap();
        let source = directory.path().join("synthetic.RECEIPT.txt");
        std::fs::write(&source, b"synthetic receipt").unwrap();

        assert_eq!(
            state
                .copy_authorized_attachment("../../forged")
                .unwrap_err()
                .code(),
            "FILE_AUTHORIZATION_REJECTED"
        );
        let authorization = state.authorize_attachment_path(source).unwrap();
        let copied = state
            .copy_authorized_attachment(&authorization.authorization_token)
            .unwrap();
        assert!(copied.relative_location.starts_with("attachments/"));
        assert!(!copied.relative_location.contains(".."));
        assert_eq!(
            state
                .copy_authorized_attachment(&authorization.authorization_token)
                .unwrap_err()
                .code(),
            "FILE_AUTHORIZATION_REJECTED"
        );
    }
}
