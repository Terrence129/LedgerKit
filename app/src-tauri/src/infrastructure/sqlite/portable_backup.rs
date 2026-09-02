#![forbid(unsafe_code)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, Payload, consts::U12};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::safety::{
    BackupResult, BackupStatus, ExportFormat, ExportResult, RestoreResult, SafetyPort,
};

use super::migration::{
    MigrationRunner, VerifiedSqliteMigrationBackup, inspect_read_only, validate_schema,
};
use super::schema::SCHEMA_VERSION;
use super::store::{LedgerStore, SqliteLedgerManager};

const BACKUP_FORMAT: &str = "ledgerkit-portable-backup/v1";
const MANIFEST_FORMAT: &str = "ledgerkit-backup-manifest/v1";
const KDF_ALGORITHM: &str = "argon2id";
const KDF_VERSION: u32 = 19;
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_ITERATIONS: u32 = 3;
const KDF_PARALLELISM: u32 = 4;
const AEAD_ALGORITHM: &str = "aes-256-gcm";
const MAX_BACKUP_BYTES: u64 = 1024 * 1024 * 1024;
const DAILY_RETENTION: usize = 7;
const WEEKLY_RETENTION: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestoreSwitchMode {
    Execute,
    #[cfg(test)]
    SimulateFailure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct KdfHeader {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AeadHeader {
    algorithm: String,
    key_wrap_nonce: String,
    payload_nonce: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupHeader {
    format: String,
    kdf: KdfHeader,
    aead: AeadHeader,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupEnvelope {
    header: BackupHeader,
    wrapped_data_key: String,
    ciphertext: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupManifest {
    format: String,
    backup_id: String,
    ledger_id: String,
    created_at_utc: String,
    application_version: String,
    schema_version: u32,
    event_watermark: u64,
    projection_watermark: u64,
    calculation_version: String,
    canonical_posting_sha256: String,
    database_sha256: String,
    database_bytes: u64,
    settings_sha256: String,
    attachment_content_included: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortablePayload {
    manifest: BackupManifest,
    database_base64: String,
    settings_json: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShellSettingsPayload {
    ui_locale: String,
}

struct TemporaryFile(PathBuf);

impl TemporaryFile {
    fn new(path: PathBuf) -> Self {
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl SafetyPort for SqliteLedgerManager {
    fn create_portable_backup(
        &mut self,
        target: &Path,
        password: &str,
        settings_json: &str,
        configure_external_target: bool,
    ) -> ApplicationResult<BackupResult> {
        let result = self.create_backup_internal(target, password, settings_json);
        match result {
            Ok(mut created) => {
                if configure_external_target {
                    let parent = target.parent().ok_or(ApplicationError::BackupPathInvalid)?;
                    let canonical = parent
                        .canonicalize()
                        .map_err(|_| ApplicationError::BackupPathInvalid)?;
                    let store = self.store.as_mut().ok_or(ApplicationError::LedgerNotOpen)?;
                    store.connection.execute(
                        "UPDATE backup_status SET external_target_configured=1,external_target_path=?1,protection_state='protected',last_error_code=NULL WHERE singleton_id=1",
                        [canonical.to_string_lossy().as_ref()],
                    ).map_err(|_| ApplicationError::BackupWriteFailed)?;
                    self.record_backup_success(&created.created_at_utc, created.schema_version)?;
                    self.automatic_backup_password = Some(Zeroizing::new(password.to_owned()));
                    "protected".clone_into(&mut created.protection_state);
                    Self::rotate_external_backups(&canonical)?;
                }
                Ok(created)
            }
            Err(error) => {
                if configure_external_target {
                    self.record_backup_failure(error.code());
                }
                Err(error)
            }
        }
    }

    fn restore_portable_backup(
        &mut self,
        source: &Path,
        password: &str,
    ) -> ApplicationResult<RestoreResult> {
        self.restore_backup_internal(source, password, RestoreSwitchMode::Execute)
    }

    fn get_backup_status(&mut self, settings_json: &str) -> ApplicationResult<BackupStatus> {
        self.run_due_external_backup(settings_json);
        self.backup_status()
    }

    fn export_data(&self, target: &Path, format: ExportFormat) -> ApplicationResult<ExportResult> {
        self.export_internal(target, format)
    }

    fn create_exit_backup(&mut self, settings_json: &str) {
        if self.create_local_exit_snapshot().is_err() {
            self.record_backup_failure("EXIT_BACKUP_FAILED");
        }
        self.run_due_external_backup(settings_json);
    }
}

impl SqliteLedgerManager {
    fn create_backup_internal(
        &mut self,
        target: &Path,
        password: &str,
        settings_json: &str,
    ) -> ApplicationResult<BackupResult> {
        validate_password(password)?;
        validate_output_path(target, "lkbackup", ApplicationError::BackupPathInvalid)?;
        let snapshot = TemporaryFile::new(self.temporary_path("portable-snapshot", "sqlite3")?);
        let payload = self.build_portable_payload(snapshot.path(), settings_json)?;
        let envelope = encrypt_payload(&payload, password)?;
        let bytes =
            serde_json::to_vec(&envelope).map_err(|_| ApplicationError::BackupWriteFailed)?;
        let verified = decrypt_package_bytes(&bytes, password)?;
        if verified != payload {
            return Err(ApplicationError::BackupVerificationFailed);
        }
        self.verify_payload_database(&verified)?;
        write_atomic_new(target, &bytes, ApplicationError::BackupWriteFailed)?;
        Ok(BackupResult {
            file_name: target
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("ledgerkit.lkbackup")
                .to_owned(),
            backup_id: verified.manifest.backup_id,
            created_at_utc: verified.manifest.created_at_utc,
            schema_version: verified.manifest.schema_version,
            verified: true,
            protection_state: self.backup_status()?.protection_state,
        })
    }

    fn build_portable_payload(
        &self,
        snapshot_path: &Path,
        settings_json: &str,
    ) -> ApplicationResult<PortablePayload> {
        let store = self.store.as_ref().ok_or(ApplicationError::LedgerNotOpen)?;
        store
            .connection
            .backup("main", snapshot_path, None)
            .map_err(|_| ApplicationError::BackupWriteFailed)?;
        validate_schema_snapshot(snapshot_path)?;
        let database = fs::read(snapshot_path).map_err(|_| ApplicationError::BackupWriteFailed)?;
        let status = store.status()?;
        let created_at_utc: String = store
            .connection
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%SZ','now')", [], |row| {
                row.get(0)
            })
            .map_err(|_| ApplicationError::BackupWriteFailed)?;
        let backup_id = crate::domain::types::UuidV7::new()?.to_string();
        Ok(PortablePayload {
            manifest: BackupManifest {
                format: MANIFEST_FORMAT.to_owned(),
                backup_id,
                ledger_id: status.ledger_id.ok_or(ApplicationError::LedgerNotOpen)?,
                created_at_utc,
                application_version: env!("CARGO_PKG_VERSION").to_owned(),
                schema_version: SCHEMA_VERSION,
                event_watermark: status.event_watermark,
                projection_watermark: status.projection_watermark,
                calculation_version: status.calculation_version.to_owned(),
                canonical_posting_sha256: store.canonical_posting_hash()?,
                database_sha256: sha256(&database),
                database_bytes: u64::try_from(database.len())
                    .map_err(|_| ApplicationError::ResponseTooLarge)?,
                settings_sha256: sha256(settings_json.as_bytes()),
                attachment_content_included: false,
            },
            database_base64: BASE64.encode(database),
            settings_json: settings_json.to_owned(),
        })
    }

    fn verify_payload_database(&self, payload: &PortablePayload) -> ApplicationResult<()> {
        validate_payload(payload)?;
        let database = BASE64
            .decode(&payload.database_base64)
            .map_err(|_| ApplicationError::BackupFormatUnsupported)?;
        let candidate = TemporaryFile::new(self.temporary_path("backup-verification", "sqlite3")?);
        write_new_file(
            candidate.path(),
            &database,
            ApplicationError::BackupVerificationFailed,
        )?;
        let result = (|| {
            validate_schema_snapshot(candidate.path())?;
            let mut migration_backup = VerifiedSqliteMigrationBackup::new(
                self.temporary_directory("backup-verification-migrations")?,
            );
            let connection =
                MigrationRunner::open_existing(candidate.path(), &mut migration_backup)?;
            let store = LedgerStore::from_open_connection(connection)?;
            let status = store.status()?;
            if status.ledger_id.as_deref() != Some(&payload.manifest.ledger_id)
                || status.event_watermark != payload.manifest.event_watermark
                || status.projection_watermark != payload.manifest.projection_watermark
                || store.canonical_posting_hash()? != payload.manifest.canonical_posting_sha256
            {
                return Err(ApplicationError::BackupVerificationFailed);
            }
            Ok(())
        })();
        result.map_err(|_| ApplicationError::BackupVerificationFailed)
    }

    fn restore_backup_internal(
        &mut self,
        source: &Path,
        password: &str,
        switch_mode: RestoreSwitchMode,
    ) -> ApplicationResult<RestoreResult> {
        validate_password(password)?;
        validate_input_path(source, "lkbackup")?;
        let metadata = fs::metadata(source).map_err(|_| ApplicationError::BackupPathInvalid)?;
        if metadata.len() == 0 || metadata.len() > MAX_BACKUP_BYTES {
            return Err(ApplicationError::BackupFormatUnsupported);
        }
        let bytes = fs::read(source).map_err(|_| ApplicationError::BackupFormatUnsupported)?;
        let payload = decrypt_package_bytes(&bytes, password)?;
        validate_payload(&payload)?;
        let settings: ShellSettingsPayload = serde_json::from_str(&payload.settings_json)
            .map_err(|_| ApplicationError::BackupVerificationFailed)?;
        if !matches!(settings.ui_locale.as_str(), "zh-CN" | "en-US") {
            return Err(ApplicationError::BackupVerificationFailed);
        }
        let database = BASE64
            .decode(&payload.database_base64)
            .map_err(|_| ApplicationError::BackupFormatUnsupported)?;
        let candidate_path =
            TemporaryFile::new(self.temporary_path("restore-candidate", "sqlite3")?);
        write_new_file(
            candidate_path.path(),
            &database,
            ApplicationError::BackupVerificationFailed,
        )?;
        let source_identity = inspect_read_only(candidate_path.path())?;
        if source_identity.schema_version > SCHEMA_VERSION {
            return Err(ApplicationError::SchemaTooNew);
        }
        if source_identity.schema_version != payload.manifest.schema_version {
            return Err(ApplicationError::BackupVerificationFailed);
        }
        let mut candidate_backup = VerifiedSqliteMigrationBackup::new(
            self.temporary_directory("restore-migration-backups")?,
        );
        let candidate_connection =
            MigrationRunner::open_existing(candidate_path.path(), &mut candidate_backup)?;
        let candidate_store = LedgerStore::from_open_connection(candidate_connection)?;
        let candidate_status = candidate_store.status()?;
        if candidate_store.canonical_posting_hash()? != payload.manifest.canonical_posting_sha256
            || candidate_status.ledger_id.as_deref() != Some(&payload.manifest.ledger_id)
            || candidate_status.event_watermark != payload.manifest.event_watermark
            || candidate_status.projection_watermark != payload.manifest.projection_watermark
        {
            return Err(ApplicationError::BackupVerificationFailed);
        }
        candidate_store
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|_| ApplicationError::BackupVerificationFailed)?;
        drop(candidate_store);

        let had_live_database = self.database_path.is_file();
        let pre_restore = had_live_database
            .then(|| self.create_pre_restore_snapshot())
            .transpose()?;
        drop(self.store.take());
        let switch_result = match switch_mode {
            RestoreSwitchMode::Execute => {
                copy_database_with_online_backup(candidate_path.path(), &self.database_path)
            }
            #[cfg(test)]
            RestoreSwitchMode::SimulateFailure => Err(ApplicationError::BackupRestoreFailed),
        };
        if switch_result.is_err() {
            self.reopen_after_failed_restore(pre_restore.as_deref());
            return Err(ApplicationError::BackupRestoreFailed);
        }
        let reopened = MigrationRunner::open_existing(&self.database_path, &mut self.backup)
            .and_then(LedgerStore::from_open_connection);
        let Ok(restored_store) = reopened else {
            self.reopen_after_failed_restore(pre_restore.as_deref());
            return Err(ApplicationError::BackupRestoreFailed);
        };
        self.store = Some(restored_store);
        let restored_status = self.clear_restored_device_state();
        let Ok(restored_status) = restored_status else {
            drop(self.store.take());
            self.reopen_after_failed_restore(pre_restore.as_deref());
            return Err(ApplicationError::BackupRestoreFailed);
        };
        self.automatic_backup_password = None;
        Ok(RestoreResult {
            backup_id: payload.manifest.backup_id,
            ledger_id: restored_status
                .ledger_id
                .ok_or(ApplicationError::BackupRestoreFailed)?,
            schema_version: SCHEMA_VERSION,
            event_watermark: restored_status.event_watermark,
            settings_locale: settings.ui_locale,
            pre_restore_backup_verified: pre_restore
                .as_deref()
                .is_none_or(|path| inspect_read_only(path).is_ok()),
        })
    }

    fn clear_restored_device_state(
        &self,
    ) -> ApplicationResult<crate::application::ledger::LedgerStatus> {
        let store = self
            .store
            .as_ref()
            .ok_or(ApplicationError::BackupRestoreFailed)?;
        store.connection.execute(
            "UPDATE backup_status SET protection_state='not-configured',external_target_configured=0,external_target_path=NULL,last_error_code=NULL WHERE singleton_id=1",
            [],
        ).map_err(|_| ApplicationError::BackupRestoreFailed)?;
        store
            .status()
            .map_err(|_| ApplicationError::BackupRestoreFailed)
    }

    fn reopen_after_failed_restore(&mut self, pre_restore: Option<&Path>) {
        let Some(pre_restore) = pre_restore else {
            let _ = fs::remove_file(&self.database_path);
            let _ = fs::remove_file(self.database_path.with_extension("sqlite3-wal"));
            let _ = fs::remove_file(self.database_path.with_extension("sqlite3-shm"));
            return;
        };
        if copy_database_with_online_backup(pre_restore, &self.database_path).is_ok()
            && let Ok(connection) =
                MigrationRunner::open_existing(&self.database_path, &mut self.backup)
            && let Ok(store) = LedgerStore::from_open_connection(connection)
        {
            self.store = Some(store);
        }
    }

    fn create_pre_restore_snapshot(&self) -> ApplicationResult<PathBuf> {
        let directory = self.temporary_directory("recovery-backups")?;
        let path = directory.join(format!(
            "pre-restore-{}.sqlite3",
            crate::domain::types::UuidV7::new()?
        ));
        if let Some(store) = self.store.as_ref() {
            store
                .connection
                .backup("main", &path, None)
                .map_err(|_| ApplicationError::BackupRestoreFailed)?;
        } else {
            copy_database_with_online_backup(&self.database_path, &path)?;
        }
        validate_schema_snapshot(&path)?;
        Ok(path)
    }

    fn create_local_exit_snapshot(&self) -> ApplicationResult<()> {
        let directory = self.temporary_directory("exit-backups")?;
        let path = directory.join(format!(
            "exit-{}.sqlite3",
            crate::domain::types::UuidV7::new()?
        ));
        let store = self.store.as_ref().ok_or(ApplicationError::LedgerNotOpen)?;
        store
            .connection
            .backup("main", &path, None)
            .map_err(|_| ApplicationError::BackupWriteFailed)?;
        validate_schema_snapshot(&path)?;
        rotate_files(&directory, "exit-", "sqlite3", DAILY_RETENTION)?;
        Ok(())
    }

    fn run_due_external_backup(&mut self, settings_json: &str) {
        let due = self.store.as_ref().and_then(|store| {
            store.connection.query_row(
                "SELECT external_target_path, last_success_at_utc IS NULL OR date(last_success_at_utc) < date('now') FROM backup_status WHERE singleton_id=1 AND external_target_configured=1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
            ).ok()
        });
        let Some((directory, true)) = due else { return };
        let Some(password) = self
            .automatic_backup_password
            .as_ref()
            .map(|value| Zeroizing::new(value.to_string()))
        else {
            self.mark_pending_if_expired();
            return;
        };
        let directory = PathBuf::from(directory);
        let result = self
            .scheduled_target(&directory, "daily")
            .and_then(|target| {
                self.create_backup_internal(&target, &password, settings_json)
                    .map(|created| (target, created))
            })
            .and_then(|(target, created)| {
                self.ensure_weekly_copy(&target, &directory)?;
                Self::rotate_external_backups(&directory)?;
                Ok(created)
            });
        match result {
            Ok(created) => {
                if let Err(error) =
                    self.record_backup_success(&created.created_at_utc, created.schema_version)
                {
                    self.record_backup_failure(error.code());
                }
            }
            Err(error) => self.record_backup_failure(error.code()),
        }
    }

    fn scheduled_target(&self, directory: &Path, kind: &str) -> ApplicationResult<PathBuf> {
        let canonical = directory
            .canonicalize()
            .map_err(|_| ApplicationError::BackupPathInvalid)?;
        if !canonical.is_dir() {
            return Err(ApplicationError::BackupPathInvalid);
        }
        let date: String = self
            .store
            .as_ref()
            .ok_or(ApplicationError::LedgerNotOpen)?
            .connection
            .query_row("SELECT date('now')", [], |row| row.get(0))
            .map_err(|_| ApplicationError::BackupWriteFailed)?;
        Ok(canonical.join(format!(
            "ledgerkit-{kind}-{date}-{}.lkbackup",
            crate::domain::types::UuidV7::new()?
        )))
    }

    fn ensure_weekly_copy(&self, daily: &Path, directory: &Path) -> ApplicationResult<()> {
        let week: String = self
            .store
            .as_ref()
            .ok_or(ApplicationError::LedgerNotOpen)?
            .connection
            .query_row("SELECT strftime('%Y-W%W','now')", [], |row| row.get(0))
            .map_err(|_| ApplicationError::BackupWriteFailed)?;
        let prefix = format!("ledgerkit-weekly-{week}-");
        if matching_files(directory, &prefix, "lkbackup")?.is_empty() {
            let target = directory.join(format!(
                "{prefix}{}.lkbackup",
                crate::domain::types::UuidV7::new()?
            ));
            fs::copy(daily, &target).map_err(|_| ApplicationError::BackupWriteFailed)?;
            if fs::read(daily).ok().map(|bytes| sha256(&bytes))
                != fs::read(&target).ok().map(|bytes| sha256(&bytes))
            {
                let _ = fs::remove_file(target);
                return Err(ApplicationError::BackupVerificationFailed);
            }
        }
        Ok(())
    }

    fn rotate_external_backups(directory: &Path) -> ApplicationResult<()> {
        rotate_files(directory, "ledgerkit-daily-", "lkbackup", DAILY_RETENTION)?;
        rotate_files(directory, "ledgerkit-weekly-", "lkbackup", WEEKLY_RETENTION)
    }

    fn mark_pending_if_expired(&mut self) {
        if let Some(store) = self.store.as_mut() {
            let _ = store.connection.execute(
                "UPDATE backup_status SET protection_state='pending' WHERE singleton_id=1 AND external_target_configured=1 AND (last_success_at_utc IS NULL OR datetime(last_success_at_utc) < datetime('now','-24 hours'))",
                [],
            );
        }
    }

    fn record_backup_success(&mut self, created: &str, schema: u32) -> ApplicationResult<()> {
        let store = self.store.as_mut().ok_or(ApplicationError::LedgerNotOpen)?;
        store.connection.execute(
            "UPDATE backup_status SET last_attempt_at_utc=?1,last_success_at_utc=?1,last_verified_schema_version=?2,last_error_code=NULL,protection_state=CASE WHEN external_target_configured=1 THEN 'protected' ELSE protection_state END WHERE singleton_id=1",
            rusqlite::params![created, schema],
        ).map_err(|_| ApplicationError::BackupWriteFailed)?;
        Ok(())
    }

    fn record_backup_failure(&mut self, code: &str) {
        if let Some(store) = self.store.as_mut() {
            let _ = store.connection.execute(
                "UPDATE backup_status SET last_attempt_at_utc=strftime('%Y-%m-%dT%H:%M:%SZ','now'),last_error_code=?1,protection_state=CASE WHEN external_target_configured=1 THEN 'failed' ELSE protection_state END WHERE singleton_id=1",
                [code],
            );
        }
    }

    fn backup_status(&self) -> ApplicationResult<BackupStatus> {
        let store = self.store.as_ref().ok_or(ApplicationError::LedgerNotOpen)?;
        let row = store.connection.query_row(
            "SELECT CASE
                WHEN external_target_configured=0 THEN 'not-configured'
                WHEN last_error_code IS NOT NULL THEN 'failed'
                WHEN protection_state='protected' AND datetime(last_success_at_utc) >= datetime('now','-24 hours') THEN 'protected'
                ELSE 'pending'
             END,
             external_target_configured,external_target_path,last_attempt_at_utc,last_success_at_utc,last_verified_schema_version,last_error_code,
             (protection_state='protected' AND external_target_configured=1 AND datetime(last_success_at_utc) >= datetime('now','-24 hours') AND last_error_code IS NULL)
             FROM backup_status WHERE singleton_id=1",
            [],
            |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, bool>(1)?, row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, Option<u32>>(5)?,
                row.get::<_, Option<String>>(6)?, row.get::<_, bool>(7)?,
            )),
        ).map_err(|_| ApplicationError::SchemaValidationFailed)?;
        Ok(BackupStatus {
            protection_state: row.0,
            external_target_configured: row.1,
            external_target_label: row.2.and_then(|path| {
                Path::new(&path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned)
            }),
            last_attempt_at_utc: row.3,
            last_success_at_utc: row.4,
            last_verified_schema_version: row.5,
            last_error_code: row.6,
            device_loss_protected: row.7,
            recovery_secret_state: if self.automatic_backup_password.is_some() {
                "unlocked-for-session"
            } else {
                "locked"
            }
            .to_owned(),
            daily_retention: u32::try_from(DAILY_RETENTION).unwrap_or(7),
            weekly_retention: u32::try_from(WEEKLY_RETENTION).unwrap_or(4),
        })
    }

    fn temporary_directory(&self, name: &str) -> ApplicationResult<PathBuf> {
        let root = self
            .database_path
            .parent()
            .ok_or(ApplicationError::StorageUnavailable)?;
        let path = root.join(name);
        fs::create_dir_all(&path).map_err(|_| ApplicationError::StorageUnavailable)?;
        Ok(path)
    }

    fn temporary_path(&self, name: &str, extension: &str) -> ApplicationResult<PathBuf> {
        Ok(self.temporary_directory("temporary")?.join(format!(
            "{name}-{}.{}",
            crate::domain::types::UuidV7::new()?,
            extension
        )))
    }
}

fn encrypt_payload(payload: &PortablePayload, password: &str) -> ApplicationResult<BackupEnvelope> {
    let mut salt = [0_u8; 16];
    let mut key_wrap_nonce = [0_u8; 12];
    let mut payload_nonce = [0_u8; 12];
    let mut data_key = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut salt).map_err(|_| ApplicationError::BackupWriteFailed)?;
    getrandom::fill(&mut key_wrap_nonce).map_err(|_| ApplicationError::BackupWriteFailed)?;
    getrandom::fill(&mut payload_nonce).map_err(|_| ApplicationError::BackupWriteFailed)?;
    getrandom::fill(data_key.as_mut()).map_err(|_| ApplicationError::BackupWriteFailed)?;
    let header = BackupHeader {
        format: BACKUP_FORMAT.to_owned(),
        kdf: KdfHeader {
            algorithm: KDF_ALGORITHM.to_owned(),
            version: KDF_VERSION,
            memory_kib: KDF_MEMORY_KIB,
            iterations: KDF_ITERATIONS,
            parallelism: KDF_PARALLELISM,
            salt: BASE64.encode(salt),
        },
        aead: AeadHeader {
            algorithm: AEAD_ALGORITHM.to_owned(),
            key_wrap_nonce: BASE64.encode(key_wrap_nonce),
            payload_nonce: BASE64.encode(payload_nonce),
        },
    };
    let aad = serde_json::to_vec(&header).map_err(|_| ApplicationError::BackupWriteFailed)?;
    let kek = Zeroizing::new(derive_kek(password, &salt)?);
    let wrapping =
        Aes256Gcm::new_from_slice(&kek[..]).map_err(|_| ApplicationError::BackupWriteFailed)?;
    let wrapped = wrapping
        .encrypt(
            &nonce(&key_wrap_nonce),
            Payload {
                msg: data_key.as_ref(),
                aad: &aad,
            },
        )
        .map_err(|_| ApplicationError::BackupWriteFailed)?;
    let payload_bytes =
        serde_json::to_vec(payload).map_err(|_| ApplicationError::BackupWriteFailed)?;
    let cipher = Aes256Gcm::new_from_slice(data_key.as_ref())
        .map_err(|_| ApplicationError::BackupWriteFailed)?;
    let ciphertext = cipher
        .encrypt(
            &nonce(&payload_nonce),
            Payload {
                msg: &payload_bytes,
                aad: &aad,
            },
        )
        .map_err(|_| ApplicationError::BackupWriteFailed)?;
    Ok(BackupEnvelope {
        header,
        wrapped_data_key: BASE64.encode(wrapped),
        ciphertext: BASE64.encode(ciphertext),
    })
}

fn decrypt_package_bytes(bytes: &[u8], password: &str) -> ApplicationResult<PortablePayload> {
    let envelope: BackupEnvelope =
        serde_json::from_slice(bytes).map_err(|_| ApplicationError::BackupFormatUnsupported)?;
    validate_header(&envelope.header)?;
    let salt = decode_array::<16>(&envelope.header.kdf.salt)?;
    let key_wrap_nonce = decode_array::<12>(&envelope.header.aead.key_wrap_nonce)?;
    let payload_nonce = decode_array::<12>(&envelope.header.aead.payload_nonce)?;
    let wrapped = BASE64
        .decode(&envelope.wrapped_data_key)
        .map_err(|_| ApplicationError::BackupFormatUnsupported)?;
    let ciphertext = BASE64
        .decode(&envelope.ciphertext)
        .map_err(|_| ApplicationError::BackupFormatUnsupported)?;
    let aad = serde_json::to_vec(&envelope.header)
        .map_err(|_| ApplicationError::BackupFormatUnsupported)?;
    let kek = Zeroizing::new(derive_kek(password, &salt)?);
    let wrapping = Aes256Gcm::new_from_slice(&kek[..])
        .map_err(|_| ApplicationError::BackupAuthenticationFailed)?;
    let data_key = Zeroizing::new(
        wrapping
            .decrypt(
                &nonce(&key_wrap_nonce),
                Payload {
                    msg: &wrapped,
                    aad: &aad,
                },
            )
            .map_err(|_| ApplicationError::BackupAuthenticationFailed)?,
    );
    if data_key.len() != 32 {
        return Err(ApplicationError::BackupAuthenticationFailed);
    }
    let cipher = Aes256Gcm::new_from_slice(data_key.as_ref())
        .map_err(|_| ApplicationError::BackupAuthenticationFailed)?;
    let plaintext = cipher
        .decrypt(
            &nonce(&payload_nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| ApplicationError::BackupAuthenticationFailed);
    serde_json::from_slice(&plaintext?).map_err(|_| ApplicationError::BackupVerificationFailed)
}

fn validate_header(header: &BackupHeader) -> ApplicationResult<()> {
    if header.format != BACKUP_FORMAT || header.aead.algorithm != AEAD_ALGORITHM {
        return Err(ApplicationError::BackupFormatUnsupported);
    }
    if header.kdf.algorithm != KDF_ALGORITHM
        || header.kdf.version != KDF_VERSION
        || header.kdf.memory_kib != KDF_MEMORY_KIB
        || header.kdf.iterations != KDF_ITERATIONS
        || header.kdf.parallelism != KDF_PARALLELISM
    {
        return Err(ApplicationError::BackupKdfUnsupported);
    }
    Ok(())
}

fn derive_kek(password: &str, salt: &[u8; 16]) -> ApplicationResult<[u8; 32]> {
    let params = Params::new(KDF_MEMORY_KIB, KDF_ITERATIONS, KDF_PARALLELISM, Some(32))
        .map_err(|_| ApplicationError::BackupKdfUnsupported)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = [0_u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut output)
        .map_err(|_| ApplicationError::BackupAuthenticationFailed)?;
    Ok(output)
}

fn validate_payload(payload: &PortablePayload) -> ApplicationResult<()> {
    if payload.manifest.format != MANIFEST_FORMAT || payload.manifest.attachment_content_included {
        return Err(ApplicationError::BackupFormatUnsupported);
    }
    if payload.manifest.schema_version > SCHEMA_VERSION {
        return Err(ApplicationError::SchemaTooNew);
    }
    let database = BASE64
        .decode(&payload.database_base64)
        .map_err(|_| ApplicationError::BackupFormatUnsupported)?;
    if u64::try_from(database.len()).ok() != Some(payload.manifest.database_bytes)
        || sha256(&database) != payload.manifest.database_sha256
        || sha256(payload.settings_json.as_bytes()) != payload.manifest.settings_sha256
    {
        return Err(ApplicationError::BackupHashMismatch);
    }
    Ok(())
}

fn validate_password(password: &str) -> ApplicationResult<()> {
    if password.chars().count() < 12 || password.chars().count() > 1024 {
        return Err(ApplicationError::BackupPasswordRequired);
    }
    Ok(())
}

fn decode_array<const N: usize>(value: &str) -> ApplicationResult<[u8; N]> {
    BASE64
        .decode(value)
        .map_err(|_| ApplicationError::BackupFormatUnsupported)?
        .try_into()
        .map_err(|_| ApplicationError::BackupFormatUnsupported)
}

fn sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(7 + digest.len() * 2);
    result.push_str("sha256:");
    for byte in digest {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

fn nonce(value: &[u8; 12]) -> Nonce<U12> {
    (*value).into()
}

fn validate_schema_snapshot(path: &Path) -> ApplicationResult<()> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| ApplicationError::BackupVerificationFailed)?;
    validate_schema(&connection).map_err(|_| ApplicationError::BackupVerificationFailed)
}

fn copy_database_with_online_backup(source: &Path, target: &Path) -> ApplicationResult<()> {
    let connection = Connection::open_with_flags(
        source,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| ApplicationError::BackupRestoreFailed)?;
    connection
        .backup("main", target, None)
        .map_err(|_| ApplicationError::BackupRestoreFailed)
}

fn validate_output_path(
    target: &Path,
    extension: &str,
    error: ApplicationError,
) -> ApplicationResult<()> {
    if !target.is_absolute()
        || target.exists()
        || target
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            != Some(extension.to_owned())
    {
        return Err(error);
    }
    let parent = target.parent().ok_or(error)?;
    let canonical = parent.canonicalize().map_err(|_| error)?;
    if !canonical.is_dir() {
        return Err(error);
    }
    Ok(())
}

fn validate_input_path(source: &Path, extension: &str) -> ApplicationResult<()> {
    if !source.is_absolute()
        || !source.is_file()
        || source
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            != Some(extension.to_owned())
    {
        return Err(ApplicationError::BackupPathInvalid);
    }
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8], error: ApplicationError) -> ApplicationResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| error)?;
    file.write_all(bytes).map_err(|_| error)?;
    file.sync_all().map_err(|_| error)
}

fn write_atomic_new(target: &Path, bytes: &[u8], error: ApplicationError) -> ApplicationResult<()> {
    let parent = target.parent().ok_or(error)?;
    let temporary = parent.join(format!(
        ".ledgerkit-{}.tmp",
        crate::domain::types::UuidV7::new().map_err(|_| error)?
    ));
    let temporary = TemporaryFile::new(temporary);
    write_new_file(temporary.path(), bytes, error)?;
    if fs::rename(temporary.path(), target).is_err() {
        return Err(error);
    }
    Ok(())
}

fn matching_files(
    directory: &Path,
    prefix: &str,
    extension: &str,
) -> ApplicationResult<Vec<PathBuf>> {
    let canonical = directory
        .canonicalize()
        .map_err(|_| ApplicationError::BackupPathInvalid)?;
    let mut paths = fs::read_dir(&canonical)
        .map_err(|_| ApplicationError::BackupPathInvalid)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|name| name.starts_with(prefix))
                && path.extension().and_then(|value| value.to_str()) == Some(extension)
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn rotate_files(
    directory: &Path,
    prefix: &str,
    extension: &str,
    keep: usize,
) -> ApplicationResult<()> {
    let paths = matching_files(directory, prefix, extension)?;
    let remove_count = paths.len().saturating_sub(keep);
    for path in paths.into_iter().take(remove_count) {
        fs::remove_file(path).map_err(|_| ApplicationError::BackupWriteFailed)?;
    }
    Ok(())
}

impl SqliteLedgerManager {
    fn export_internal(
        &self,
        target: &Path,
        format: ExportFormat,
    ) -> ApplicationResult<ExportResult> {
        validate_output_path(
            target,
            format.extension(),
            ApplicationError::ExportPathInvalid,
        )?;
        let store = self.store.as_ref().ok_or(ApplicationError::LedgerNotOpen)?;
        validate_schema(&store.connection).map_err(|_| ApplicationError::ExportWriteFailed)?;
        let (bytes, row_count) = match format {
            ExportFormat::Csv => export_csv(&store.connection)?,
            ExportFormat::Reconciliation => export_reconciliation(store)?,
            ExportFormat::Diagnostics => export_diagnostics(store)?,
            ExportFormat::Xlsx => {
                return export_xlsx(target, &store.connection);
            }
        };
        write_atomic_new(target, &bytes, ApplicationError::ExportWriteFailed)?;
        Ok(ExportResult {
            file_name: target
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("ledgerkit-export")
                .to_owned(),
            format: format.as_str(),
            row_count,
            content_sha256: sha256(&bytes),
        })
    }
}

const ACTIVITY_EXPORT_SQL: &str = "SELECT e.event_id,e.event_type,e.effective_date,e.sequence,e.revision,e.status,p.posting_ordinal,p.posting_kind,COALESCE(p.account_id,''),COALESCE(p.portfolio_id,''),COALESCE(p.instrument_id,''),p.quantity_delta,p.currency,COALESCE(p.base_value,''),p.base_currency,p.calculation_version FROM business_events e LEFT JOIN ledger_postings p ON p.event_id=e.event_id ORDER BY e.effective_date,e.sequence,e.event_id,p.posting_ordinal";

fn export_csv(connection: &Connection) -> ApplicationResult<(Vec<u8>, u64)> {
    let headers = [
        "event_id",
        "event_type",
        "effective_date",
        "sequence",
        "revision",
        "status",
        "posting_ordinal",
        "posting_kind",
        "account_id",
        "portfolio_id",
        "instrument_id",
        "quantity_delta",
        "currency",
        "base_value",
        "base_currency",
        "calculation_version",
    ];
    let rows = query_rows(connection, ACTIVITY_EXPORT_SQL)?;
    let mut output = String::new();
    output.push_str(&headers.map(csv_cell).join(","));
    output.push_str("\r\n");
    for row in &rows {
        output.push_str(&row.iter().map(csv_cell).collect::<Vec<_>>().join(","));
        output.push_str("\r\n");
    }
    Ok((
        output.into_bytes(),
        u64::try_from(rows.len()).map_err(|_| ApplicationError::ResponseTooLarge)?,
    ))
}

fn csv_cell(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    let protected = if value.starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", protected.replace('"', "\"\""))
}

fn export_reconciliation(store: &LedgerStore) -> ApplicationResult<(Vec<u8>, u64)> {
    let status = store.status()?;
    let counts = export_counts(&store.connection)?;
    let value = serde_json::json!({
        "contract": "ledgerkit-reconciliation-export/v1",
        "schemaVersion": SCHEMA_VERSION,
        "ledgerId": status.ledger_id,
        "eventWatermark": status.event_watermark,
        "projectionWatermark": status.projection_watermark,
        "calculationVersion": status.calculation_version,
        "canonicalPostingSha256": store.canonical_posting_hash()?,
        "integrityCheck": "ok",
        "foreignKeyViolations": 0,
        "counts": counts,
    });
    let row_count = counts.values().sum();
    Ok((
        serde_json::to_vec_pretty(&value).map_err(|_| ApplicationError::ExportWriteFailed)?,
        row_count,
    ))
}

fn export_diagnostics(store: &LedgerStore) -> ApplicationResult<(Vec<u8>, u64)> {
    let status = store.status()?;
    let counts = export_counts(&store.connection)?;
    let last_error: Option<String> = store
        .connection
        .query_row(
            "SELECT last_error_code FROM backup_status WHERE singleton_id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::ExportWriteFailed)?;
    let value = serde_json::json!({
        "contract": "ledgerkit-privacy-diagnostics/v1",
        "applicationVersion": env!("CARGO_PKG_VERSION"),
        "schemaVersion": SCHEMA_VERSION,
        "ledgerId": status.ledger_id,
        "eventWatermark": status.event_watermark,
        "projectionWatermark": status.projection_watermark,
        "calculationVersion": status.calculation_version,
        "lastErrorCategory": last_error,
        "counts": counts,
        "containsPaths": false,
        "containsFinancialAmounts": false,
        "containsBusinessText": false,
        "containsSecrets": false,
    });
    let row_count = counts.values().sum();
    Ok((
        serde_json::to_vec_pretty(&value).map_err(|_| ApplicationError::ExportWriteFailed)?,
        row_count,
    ))
}

fn export_counts(
    connection: &Connection,
) -> ApplicationResult<std::collections::BTreeMap<String, u64>> {
    let mut counts = std::collections::BTreeMap::new();
    for (label, table) in [
        ("events", "business_events"),
        ("postings", "ledger_postings"),
        ("institutions", "institutions"),
        ("cashAccounts", "cash_accounts"),
        ("categories", "categories"),
        ("portfolios", "portfolios"),
        ("instruments", "security_instruments"),
        ("fxRevisions", "fx_rate_revisions"),
        ("priceRevisions", "security_price_revisions"),
        ("holdings", "holding_projection"),
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let value: i64 = connection
            .query_row(&sql, [], |row| row.get(0))
            .map_err(|_| ApplicationError::ExportWriteFailed)?;
        counts.insert(
            label.to_owned(),
            u64::try_from(value).map_err(|_| ApplicationError::ExportWriteFailed)?,
        );
    }
    Ok(counts)
}

fn export_xlsx(target: &Path, connection: &Connection) -> ApplicationResult<ExportResult> {
    let parent = target.parent().ok_or(ApplicationError::ExportPathInvalid)?;
    let temporary = TemporaryFile::new(parent.join(format!(
        ".ledgerkit-{}.xlsx",
        crate::domain::types::UuidV7::new()?
    )));
    let mut workbook = Workbook::new();
    let sheets = [
        (
            "Ledger",
            "SELECT m.ledger_id,m.created_at_utc,m.schema_created_by,s.base_currency,s.ui_locale,s.valuation_defaults_json FROM ledger_metadata m CROSS JOIN app_settings s",
        ),
        (
            "Institutions",
            "SELECT institution_id,business_id,name,COALESCE(region,''),institution_type,enabled FROM institutions ORDER BY business_id",
        ),
        (
            "Cash Accounts",
            "SELECT account_id,business_id,institution_id,name,purpose,currency,COALESCE(opened_on,''),enabled FROM cash_accounts ORDER BY business_id",
        ),
        (
            "Categories",
            "SELECT category_id,name,category_kind,semantic_role,sort_order,enabled FROM categories ORDER BY sort_order,category_id",
        ),
        (
            "Portfolios",
            "SELECT portfolio_id,business_id,institution_id,settlement_account_id,name,portfolio_type,enabled FROM portfolios ORDER BY business_id",
        ),
        (
            "Instruments",
            "SELECT instrument_id,business_id,code,name,trade_currency,enabled FROM security_instruments ORDER BY business_id",
        ),
        (
            "FX Rates",
            "SELECT fx_rate_revision_id,rate_date,currency,base_currency,rate_to_base,source,revision,active FROM fx_rate_revisions ORDER BY rate_date,currency,revision",
        ),
        (
            "Prices",
            "SELECT security_price_revision_id,instrument_id,price_date,price,price_currency,source,revision,active FROM security_price_revisions ORDER BY price_date,instrument_id,revision",
        ),
        ("Events and Postings", ACTIVITY_EXPORT_SQL),
        (
            "Holdings",
            "SELECT h.portfolio_id,h.instrument_id,h.as_of_date,h.quantity,h.carrying_cost,h.realized_trade_pnl,h.net_dividend,h.independent_expense,i.trade_currency AS cost_currency,h.event_watermark,h.projection_version,h.calculation_version FROM holding_projection h JOIN security_instruments i ON i.instrument_id=h.instrument_id ORDER BY h.portfolio_id,h.instrument_id",
        ),
    ];
    let mut row_count = 0_u64;
    for (name, sql) in sheets {
        row_count = row_count
            .checked_add(write_query_sheet(&mut workbook, connection, name, sql)?)
            .ok_or(ApplicationError::ResponseTooLarge)?;
    }
    workbook
        .save(temporary.path())
        .map_err(|_| ApplicationError::ExportWriteFailed)?;
    let bytes = fs::read(temporary.path()).map_err(|_| ApplicationError::ExportWriteFailed)?;
    if fs::rename(temporary.path(), target).is_err() {
        return Err(ApplicationError::ExportWriteFailed);
    }
    Ok(ExportResult {
        file_name: target
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("ledgerkit.xlsx")
            .to_owned(),
        format: "xlsx",
        row_count,
        content_sha256: sha256(&bytes),
    })
}

fn write_query_sheet(
    workbook: &mut Workbook,
    connection: &Connection,
    name: &str,
    sql: &str,
) -> ApplicationResult<u64> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| ApplicationError::ExportWriteFailed)?;
    let columns = statement
        .column_names()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name(name)
        .map_err(|_| ApplicationError::ExportWriteFailed)?;
    for (column, header) in columns.iter().enumerate() {
        worksheet
            .write_string(
                0,
                u16::try_from(column).map_err(|_| ApplicationError::ResponseTooLarge)?,
                header,
            )
            .map_err(|_| ApplicationError::ExportWriteFailed)?;
    }
    let mut rows = statement
        .query([])
        .map_err(|_| ApplicationError::ExportWriteFailed)?;
    let mut row_index = 1_u32;
    while let Some(row) = rows
        .next()
        .map_err(|_| ApplicationError::ExportWriteFailed)?
    {
        if row_index >= 1_048_576 {
            return Err(ApplicationError::ResponseTooLarge);
        }
        for column in 0..columns.len() {
            let value = value_text(
                row.get_ref(column)
                    .map_err(|_| ApplicationError::ExportWriteFailed)?,
            )?;
            worksheet
                .write_string(
                    row_index,
                    u16::try_from(column).map_err(|_| ApplicationError::ResponseTooLarge)?,
                    &value,
                )
                .map_err(|_| ApplicationError::ExportWriteFailed)?;
        }
        row_index += 1;
    }
    Ok(u64::from(row_index - 1))
}

fn query_rows(connection: &Connection, sql: &str) -> ApplicationResult<Vec<Vec<String>>> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| ApplicationError::ExportWriteFailed)?;
    let column_count = statement.column_count();
    let mut rows = statement
        .query([])
        .map_err(|_| ApplicationError::ExportWriteFailed)?;
    let mut output = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|_| ApplicationError::ExportWriteFailed)?
    {
        let mut values = Vec::with_capacity(column_count);
        for column in 0..column_count {
            values.push(value_text(
                row.get_ref(column)
                    .map_err(|_| ApplicationError::ExportWriteFailed)?,
            )?);
        }
        output.push(values);
    }
    Ok(output)
}

fn value_text(value: ValueRef<'_>) -> ApplicationResult<String> {
    match value {
        ValueRef::Null => Ok(String::new()),
        ValueRef::Integer(value) => Ok(value.to_string()),
        ValueRef::Real(_) => Err(ApplicationError::ExportWriteFailed),
        ValueRef::Text(value) => std::str::from_utf8(value)
            .map(str::to_owned)
            .map_err(|_| ApplicationError::ExportWriteFailed),
        ValueRef::Blob(value) => Ok(BASE64.encode(value)),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use calamine::{Reader, Xlsx, open_workbook};
    use tempfile::{TempDir, tempdir};

    use crate::application::error::ApplicationError;
    use crate::application::ledger::{CreateLedgerCommand, LedgerPort};
    use crate::application::safety::{ExportFormat, SafetyPort};
    use crate::domain::settings::UiLocale;
    use crate::domain::types::Currency;

    use super::{
        BackupEnvelope, RestoreSwitchMode, SCHEMA_VERSION, SqliteLedgerManager,
        decrypt_package_bytes, encrypt_payload, inspect_read_only, matching_files, sha256,
        validate_payload,
    };

    const PASSWORD: &str = "synthetic-backup-password";
    const SETTINGS: &str = r#"{"uiLocale":"en-US"}"#;

    fn open_synthetic_ledger(directory: &TempDir) -> SqliteLedgerManager {
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        manager
            .create_ledger(CreateLedgerCommand {
                base_currency: Currency::parse("CNY").unwrap(),
                ui_locale: UiLocale::EnUs,
            })
            .unwrap();
        manager
    }

    fn live_ledger_id(manager: &SqliteLedgerManager) -> String {
        manager
            .store
            .as_ref()
            .unwrap()
            .status()
            .unwrap()
            .ledger_id
            .unwrap()
    }

    #[test]
    #[allow(clippy::too_many_lines)] // One end-to-end package exercises the expensive Argon2 matrix once.
    fn portable_backup_round_trip_rejects_wrong_password_tamper_and_unknown_contracts() {
        let source_directory = tempdir().unwrap();
        let mut source = open_synthetic_ledger(&source_directory);
        let backup = source_directory.path().join("first.lkbackup");
        let created = source
            .create_portable_backup(&backup, PASSWORD, SETTINGS, false)
            .unwrap();
        assert!(created.verified);
        assert_eq!(created.schema_version, SCHEMA_VERSION);

        let second = source_directory.path().join("second.lkbackup");
        source
            .create_portable_backup(&second, PASSWORD, SETTINGS, false)
            .unwrap();
        let first_envelope: BackupEnvelope =
            serde_json::from_slice(&fs::read(&backup).unwrap()).unwrap();
        let second_envelope: BackupEnvelope =
            serde_json::from_slice(&fs::read(&second).unwrap()).unwrap();
        assert_ne!(
            first_envelope.header.kdf.salt,
            second_envelope.header.kdf.salt
        );
        assert_ne!(
            first_envelope.header.aead.key_wrap_nonce,
            second_envelope.header.aead.key_wrap_nonce
        );
        assert_ne!(
            first_envelope.header.aead.payload_nonce,
            second_envelope.header.aead.payload_nonce
        );
        assert_ne!(first_envelope.ciphertext, second_envelope.ciphertext);
        assert_eq!(
            source.create_portable_backup(&second, PASSWORD, SETTINGS, false),
            Err(ApplicationError::BackupPathInvalid)
        );
        assert_eq!(
            source.create_portable_backup(
                std::path::Path::new("relative.lkbackup"),
                PASSWORD,
                SETTINGS,
                false,
            ),
            Err(ApplicationError::BackupPathInvalid)
        );

        let original_id = live_ledger_id(&source);
        assert_eq!(
            source.restore_portable_backup(&backup, "different-password"),
            Err(ApplicationError::BackupAuthenticationFailed)
        );
        assert_eq!(live_ledger_id(&source), original_id);
        assert_eq!(
            source.restore_backup_internal(&backup, PASSWORD, RestoreSwitchMode::SimulateFailure),
            Err(ApplicationError::BackupRestoreFailed)
        );
        assert_eq!(live_ledger_id(&source), original_id);

        let mut tampered = first_envelope.clone();
        let replacement = if tampered.ciphertext.ends_with('A') {
            'B'
        } else {
            'A'
        };
        tampered.ciphertext.pop();
        tampered.ciphertext.push(replacement);
        let tampered_path = source_directory.path().join("tampered.lkbackup");
        fs::write(&tampered_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert_eq!(
            source.restore_portable_backup(&tampered_path, PASSWORD),
            Err(ApplicationError::BackupAuthenticationFailed)
        );
        assert_eq!(live_ledger_id(&source), original_id);

        let truncated_path = source_directory.path().join("truncated.lkbackup");
        fs::write(&truncated_path, b"{\"header\":" as &[u8]).unwrap();
        assert_eq!(
            source.restore_portable_backup(&truncated_path, PASSWORD),
            Err(ApplicationError::BackupFormatUnsupported)
        );

        let mut unknown_format = first_envelope.clone();
        "ledgerkit-portable-backup/v99".clone_into(&mut unknown_format.header.format);
        let unknown_format_path = source_directory.path().join("unknown-format.lkbackup");
        fs::write(
            &unknown_format_path,
            serde_json::to_vec(&unknown_format).unwrap(),
        )
        .unwrap();
        assert_eq!(
            source.restore_portable_backup(&unknown_format_path, PASSWORD),
            Err(ApplicationError::BackupFormatUnsupported)
        );

        let mut unknown_kdf = first_envelope;
        unknown_kdf.header.kdf.memory_kib += 1;
        let unknown_kdf_path = source_directory.path().join("unknown-kdf.lkbackup");
        fs::write(&unknown_kdf_path, serde_json::to_vec(&unknown_kdf).unwrap()).unwrap();
        assert_eq!(
            source.restore_portable_backup(&unknown_kdf_path, PASSWORD),
            Err(ApplicationError::BackupKdfUnsupported)
        );

        let package = fs::read(&backup).unwrap();
        let mut too_new = decrypt_package_bytes(&package, PASSWORD).unwrap();
        too_new.manifest.schema_version = SCHEMA_VERSION + 1;
        assert_eq!(
            validate_payload(&too_new),
            Err(ApplicationError::SchemaTooNew)
        );
        let encrypted_too_new = encrypt_payload(&too_new, PASSWORD).unwrap();
        let too_new_path = source_directory.path().join("too-new.lkbackup");
        fs::write(
            &too_new_path,
            serde_json::to_vec(&encrypted_too_new).unwrap(),
        )
        .unwrap();
        assert_eq!(
            source.restore_portable_backup(&too_new_path, PASSWORD),
            Err(ApplicationError::SchemaTooNew)
        );

        let fresh_directory = tempdir().unwrap();
        let mut fresh = SqliteLedgerManager::new(fresh_directory.path()).unwrap();
        let started = Instant::now();
        let restored = fresh.restore_portable_backup(&backup, PASSWORD).unwrap();
        assert!(started.elapsed() < Duration::from_secs(600));
        assert_eq!(restored.ledger_id, original_id);
        assert!(restored.pre_restore_backup_verified);
        assert_eq!(
            inspect_read_only(&fresh.database_path)
                .unwrap()
                .schema_version,
            SCHEMA_VERSION
        );

        fresh.restore_portable_backup(&backup, PASSWORD).unwrap();
        let recovery_files = fs::read_dir(fresh_directory.path().join("recovery-backups"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sqlite3"))
            .collect::<Vec<_>>();
        assert_eq!(recovery_files.len(), 1);
        assert_eq!(
            inspect_read_only(&recovery_files[0])
                .unwrap()
                .schema_version,
            SCHEMA_VERSION
        );
    }

    #[test]
    fn external_backup_status_stays_unprotected_after_failure_and_recovers_from_verified_bytes() {
        let directory = tempdir().unwrap();
        let mut manager = open_synthetic_ledger(&directory);
        let external = directory.path().join("external-backups");
        fs::create_dir(&external).unwrap();
        let manual = external.join("manual.lkbackup");
        manager
            .create_portable_backup(&manual, PASSWORD, SETTINGS, true)
            .unwrap();
        manager
            .store
            .as_ref()
            .unwrap()
            .connection
            .execute(
                "UPDATE backup_status SET last_success_at_utc='2000-01-01T00:00:00Z'",
                [],
            )
            .unwrap();

        let unavailable = directory.path().join("external-unavailable");
        fs::rename(&external, &unavailable).unwrap();
        let failed = manager.get_backup_status(SETTINGS).unwrap();
        assert_eq!(failed.protection_state, "failed");
        assert!(!failed.device_loss_protected);
        assert_eq!(
            failed.last_error_code.as_deref(),
            Some("BACKUP_PATH_INVALID")
        );

        fs::rename(&unavailable, &external).unwrap();
        let recovered = manager.get_backup_status(SETTINGS).unwrap();
        assert_eq!(recovered.protection_state, "protected");
        assert!(recovered.device_loss_protected);
        assert_eq!(recovered.recovery_secret_state, "unlocked-for-session");
        assert_eq!(
            recovered.external_target_label.as_deref(),
            Some("external-backups")
        );
        assert_eq!(recovered.daily_retention, 7);
        assert_eq!(recovered.weekly_retention, 4);

        for index in 0..9 {
            fs::write(
                external.join(format!("ledgerkit-daily-2000-01-{index:02}.lkbackup")),
                b"synthetic",
            )
            .unwrap();
            fs::write(
                external.join(format!("ledgerkit-weekly-2000-W{index:02}.lkbackup")),
                b"synthetic",
            )
            .unwrap();
        }
        SqliteLedgerManager::rotate_external_backups(&external).unwrap();
        assert_eq!(
            matching_files(&external, "ledgerkit-daily-", "lkbackup")
                .unwrap()
                .len(),
            7
        );
        assert_eq!(
            matching_files(&external, "ledgerkit-weekly-", "lkbackup")
                .unwrap()
                .len(),
            4
        );
        assert!(manual.is_file());
        manager.automatic_backup_password = None;
        manager.store.as_ref().unwrap().connection.execute(
            "UPDATE backup_status SET last_success_at_utc=strftime('%Y-%m-%dT%H:%M:%SZ','now','-25 hours'),last_error_code=NULL,protection_state='protected'",
            [],
        ).unwrap();
        let expired = manager.get_backup_status(SETTINGS).unwrap();
        assert_eq!(expired.protection_state, "pending");
        assert!(!expired.device_loss_protected);
    }

    #[test]
    fn standalone_exports_are_complete_formula_safe_and_diagnostics_are_redacted() {
        let directory = tempdir().unwrap();
        let manager = open_synthetic_ledger(&directory);
        let connection = &manager.store.as_ref().unwrap().connection;
        connection.execute(
            "INSERT INTO institutions(institution_id,business_id,name,region,institution_type,enabled,created_at_utc,updated_at_utc) VALUES('synthetic-institution','public-fixture','=HYPERLINK(\"https://invalid.example\")','ZZ','bank',1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO business_events(event_id,event_type,effective_date,sequence,status,revision,created_at_utc,calculation_version) VALUES('=1+1','Income','2026-09-03',1,'posted',1,CURRENT_TIMESTAMP,'ledger-calculation-v1')",
            [],
        ).unwrap();

        let csv = directory.path().join("events.csv");
        let csv_result = manager.export_data(&csv, ExportFormat::Csv).unwrap();
        let csv_bytes = fs::read(&csv).unwrap();
        let csv_text = String::from_utf8(csv_bytes.clone()).unwrap();
        assert!(csv_text.contains("\"'=1+1\""));
        assert_eq!(csv_result.content_sha256, sha256(&csv_bytes));
        assert_eq!(csv_result.row_count, 1);

        let xlsx = directory.path().join("ledger.xlsx");
        let xlsx_result = manager.export_data(&xlsx, ExportFormat::Xlsx).unwrap();
        assert!(xlsx_result.row_count >= 2);
        let mut workbook: Xlsx<_> = open_workbook(&xlsx).unwrap();
        let formulas = workbook.worksheet_formula("Institutions").unwrap();
        assert!(
            formulas
                .used_cells()
                .all(|(_, _, formula)| formula.is_empty())
        );
        let values = workbook.worksheet_range("Institutions").unwrap();
        assert!(
            values
                .rows()
                .flatten()
                .any(|value| value.to_string().starts_with("=HYPERLINK"))
        );

        let reconciliation = directory.path().join("reconciliation.json");
        let reconciliation_result = manager
            .export_data(&reconciliation, ExportFormat::Reconciliation)
            .unwrap();
        assert!(reconciliation_result.row_count > 0);
        let reconciliation_json: serde_json::Value =
            serde_json::from_slice(&fs::read(reconciliation).unwrap()).unwrap();
        assert_eq!(
            reconciliation_json["contract"],
            "ledgerkit-reconciliation-export/v1"
        );

        let diagnostics = directory.path().join("diagnostics.json");
        manager
            .export_data(&diagnostics, ExportFormat::Diagnostics)
            .unwrap();
        let diagnostic_text = fs::read_to_string(diagnostics).unwrap();
        assert!(!diagnostic_text.contains("HYPERLINK"));
        assert!(!diagnostic_text.contains("public-fixture"));
        assert!(!diagnostic_text.contains("backupPassword"));
        assert!(diagnostic_text.contains("\"containsSecrets\": false"));

        let existing = directory.path().join("existing.csv");
        fs::write(&existing, b"occupied").unwrap();
        assert_eq!(
            manager.export_data(&existing, ExportFormat::Csv),
            Err(ApplicationError::ExportPathInvalid)
        );
        assert_eq!(
            manager.export_data(std::path::Path::new("relative.csv"), ExportFormat::Csv),
            Err(ApplicationError::ExportPathInvalid)
        );
        let production_source = include_str!("portable_backup.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!production_source.contains("println!"));
        assert!(!production_source.contains("dbg!"));
    }
}
