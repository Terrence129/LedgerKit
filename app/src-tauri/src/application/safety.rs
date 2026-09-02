#![forbid(unsafe_code)]

use std::path::Path;

use serde::Serialize;

use super::error::ApplicationResult;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub file_name: String,
    pub backup_id: String,
    pub created_at_utc: String,
    pub schema_version: u32,
    pub verified: bool,
    pub protection_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub backup_id: String,
    pub ledger_id: String,
    pub schema_version: u32,
    pub event_watermark: u64,
    pub settings_locale: String,
    pub pre_restore_backup_verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub protection_state: String,
    pub external_target_configured: bool,
    pub external_target_label: Option<String>,
    pub last_attempt_at_utc: Option<String>,
    pub last_success_at_utc: Option<String>,
    pub last_verified_schema_version: Option<u32>,
    pub last_error_code: Option<String>,
    pub device_loss_protected: bool,
    pub recovery_secret_state: String,
    pub daily_retention: u32,
    pub weekly_retention: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportFormat {
    Xlsx,
    Csv,
    Reconciliation,
    Diagnostics,
}

impl ExportFormat {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Csv => "csv",
            Self::Reconciliation | Self::Diagnostics => "json",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Csv => "csv",
            Self::Reconciliation => "reconciliation",
            Self::Diagnostics => "diagnostics",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "xlsx" => Some(Self::Xlsx),
            "csv" => Some(Self::Csv),
            "reconciliation" => Some(Self::Reconciliation),
            "diagnostics" => Some(Self::Diagnostics),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub file_name: String,
    pub format: &'static str,
    pub row_count: u64,
    pub content_sha256: String,
}

pub trait SafetyPort: Send {
    /// Creates and verifies one password-encrypted portable backup.
    ///
    /// # Errors
    ///
    /// Returns a stable path, password, snapshot, encryption, or verification error.
    fn create_portable_backup(
        &mut self,
        target: &Path,
        password: &str,
        settings_json: &str,
        configure_external_target: bool,
    ) -> ApplicationResult<BackupResult>;

    /// Authenticates and verifies a candidate before replacing the live ledger.
    ///
    /// # Errors
    ///
    /// Returns a stable format, authentication, compatibility, validation, or switch error.
    fn restore_portable_backup(
        &mut self,
        source: &Path,
        password: &str,
    ) -> ApplicationResult<RestoreResult>;

    /// Returns persisted external-backup evidence and current 24-hour protection truth.
    ///
    /// # Errors
    ///
    /// Returns a stable storage error when status cannot be verified.
    fn get_backup_status(&mut self, settings_json: &str) -> ApplicationResult<BackupStatus>;

    /// Writes a standalone normalized export selected by the user.
    ///
    /// # Errors
    ///
    /// Returns a stable format, path, query, or write error.
    fn export_data(&self, target: &Path, format: ExportFormat) -> ApplicationResult<ExportResult>;

    /// Creates a local consistent exit snapshot and an external encrypted backup when unlocked.
    fn create_exit_backup(&mut self, settings_json: &str);
}
