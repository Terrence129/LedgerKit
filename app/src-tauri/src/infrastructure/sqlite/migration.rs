#![forbid(unsafe_code)]

use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, MAIN_DB, OpenFlags, Transaction};

use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::ledger::MigrationBackupPort;

use super::schema::{
    APPLICATION_ID, REQUIRED_INDEXES, REQUIRED_TABLES, REQUIRED_TRIGGERS, SCHEMA_V1,
    SCHEMA_VERSION, schema_hash,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MigrationFailpoint {
    None,
    BeforeValidation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DatabaseIdentity {
    pub schema_version: u32,
}

pub struct MigrationRunner;

impl MigrationRunner {
    pub fn create_new(path: &Path) -> ApplicationResult<Connection> {
        if path.exists() {
            return Err(ApplicationError::LedgerAlreadyExists);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| ApplicationError::StorageUnavailable)?;
        }
        let mut connection = Connection::open(path).map_err(map_open_error)?;
        configure_writable(&connection)?;
        let transaction = connection
            .transaction()
            .map_err(|_| ApplicationError::MigrationFailed)?;
        apply_schema_v1(&transaction)?;
        validate_schema(&transaction)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::MigrationFailed)?;
        Ok(connection)
    }

    pub fn open_existing<B: MigrationBackupPort>(
        path: &Path,
        backup: &mut B,
    ) -> ApplicationResult<Connection> {
        Self::open_existing_with_failpoint(path, backup, MigrationFailpoint::None)
    }

    pub fn open_existing_with_failpoint<B: MigrationBackupPort>(
        path: &Path,
        backup: &mut B,
        failpoint: MigrationFailpoint,
    ) -> ApplicationResult<Connection> {
        if !path.is_file() {
            return Err(ApplicationError::LedgerNotFound);
        }
        let identity = inspect_read_only(path)?;
        if identity.schema_version > SCHEMA_VERSION {
            return Err(ApplicationError::SchemaTooNew);
        }
        if identity.schema_version == SCHEMA_VERSION {
            let connection = Connection::open(path).map_err(map_open_error)?;
            configure_writable(&connection)?;
            validate_schema(&connection)?;
            return Ok(connection);
        }

        backup
            .create_verified_backup(path, identity.schema_version)
            .map_err(|_| ApplicationError::MigrationBackupFailed)?;
        let mut connection = Connection::open(path).map_err(map_open_error)?;
        configure_writable(&connection)?;
        let transaction = connection
            .transaction()
            .map_err(|_| ApplicationError::MigrationFailed)?;
        migrate_to_v1(&transaction, identity.schema_version)?;
        if failpoint == MigrationFailpoint::BeforeValidation {
            return Err(ApplicationError::MigrationFailed);
        }
        validate_schema(&transaction)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::MigrationFailed)?;
        Ok(connection)
    }
}

pub fn inspect_read_only(path: &Path) -> ApplicationResult<DatabaseIdentity> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(map_open_error)?;
    let application_id: u32 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(|_| ApplicationError::DatabaseCorrupt)?;
    if application_id != APPLICATION_ID {
        return Err(ApplicationError::DatabaseNotLedgerKit);
    }
    validate_integrity(&connection)?;
    let schema_version: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ApplicationError::DatabaseCorrupt)?;
    if schema_version == SCHEMA_VERSION {
        validate_schema(&connection)?;
    }
    Ok(DatabaseIdentity { schema_version })
}

pub struct VerifiedSqliteMigrationBackup {
    backup_root: PathBuf,
}

impl VerifiedSqliteMigrationBackup {
    pub fn new(backup_root: PathBuf) -> Self {
        Self { backup_root }
    }
}

impl MigrationBackupPort for VerifiedSqliteMigrationBackup {
    fn create_verified_backup(
        &mut self,
        source: &Path,
        source_schema_version: u32,
    ) -> ApplicationResult<PathBuf> {
        std::fs::create_dir_all(&self.backup_root)
            .map_err(|_| ApplicationError::MigrationBackupFailed)?;
        let backup_id = crate::domain::types::UuidV7::new()
            .map_err(|_| ApplicationError::MigrationBackupFailed)?;
        let backup_path = self.backup_root.join(format!(
            "pre-v{source_schema_version}-migration-{backup_id}.sqlite3"
        ));
        let source_connection = Connection::open_with_flags(
            source,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| ApplicationError::MigrationBackupFailed)?;
        source_connection
            .backup(MAIN_DB, &backup_path, None)
            .map_err(|_| ApplicationError::MigrationBackupFailed)?;
        let backup_identity =
            inspect_read_only(&backup_path).map_err(|_| ApplicationError::MigrationBackupFailed)?;
        if backup_identity.schema_version != source_schema_version {
            return Err(ApplicationError::MigrationBackupFailed);
        }
        Ok(backup_path)
    }
}

pub fn validate_schema(connection: &Connection) -> ApplicationResult<()> {
    validate_integrity(connection)?;
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let application_id: u32 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    if version != SCHEMA_VERSION || application_id != APPLICATION_ID {
        return Err(ApplicationError::SchemaValidationFailed);
    }
    validate_objects(connection, "table", REQUIRED_TABLES)?;
    validate_objects(connection, "index", REQUIRED_INDEXES)?;
    validate_objects(connection, "trigger", REQUIRED_TRIGGERS)?;
    Ok(())
}

fn validate_integrity(connection: &Connection) -> ApplicationResult<()> {
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| ApplicationError::DatabaseCorrupt)?;
    if integrity != "ok" {
        return Err(ApplicationError::DatabaseCorrupt);
    }
    let foreign_key_violation: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|_| ApplicationError::DatabaseCorrupt)?;
    if foreign_key_violation.is_some() {
        return Err(ApplicationError::DatabaseCorrupt);
    }
    Ok(())
}

fn validate_objects(
    connection: &Connection,
    object_type: &str,
    required: &[&str],
) -> ApplicationResult<()> {
    let mut statement = connection
        .prepare("SELECT name FROM sqlite_schema WHERE type = ?1")
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let names = statement
        .query_map([object_type], |row| row.get::<_, String>(0))
        .map_err(|_| ApplicationError::SchemaValidationFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    if required
        .iter()
        .any(|required_name| !names.iter().any(|name| name == required_name))
    {
        return Err(ApplicationError::SchemaValidationFailed);
    }
    Ok(())
}

fn configure_writable(connection: &Connection) -> ApplicationResult<()> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA busy_timeout = 5000;",
        )
        .map_err(|_| ApplicationError::StorageUnavailable)
}

fn apply_schema_v1(transaction: &Transaction<'_>) -> ApplicationResult<()> {
    transaction
        .execute_batch(SCHEMA_V1)
        .map_err(|_| ApplicationError::MigrationFailed)?;
    let application_id = APPLICATION_ID;
    transaction
        .execute_batch(&format!(
            "PRAGMA application_id = {application_id}; PRAGMA user_version = {SCHEMA_VERSION};"
        ))
        .map_err(|_| ApplicationError::MigrationFailed)
}

fn migrate_to_v1(transaction: &Transaction<'_>, source_version: u32) -> ApplicationResult<()> {
    if source_version != 0 {
        return Err(ApplicationError::MigrationFailed);
    }
    let legacy_settings = transaction
        .query_row(
            "SELECT base_currency, ui_locale FROM legacy_settings WHERE singleton_id = 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(|_| ApplicationError::MigrationFailed)?;
    transaction
        .execute_batch("ALTER TABLE legacy_settings RENAME TO migration_v0_settings;")
        .map_err(|_| ApplicationError::MigrationFailed)?;
    apply_schema_v1(transaction)?;
    let ledger_id = crate::domain::types::UuidV7::new()?.to_string();
    transaction
        .execute(
            "INSERT INTO ledger_metadata(singleton_id,ledger_id,created_at_utc,schema_created_by) VALUES(1,?1,CURRENT_TIMESTAMP,?2)",
            [&ledger_id, env!("CARGO_PKG_VERSION")],
        )
        .map_err(|_| ApplicationError::MigrationFailed)?;
    transaction
        .execute(
            "INSERT INTO app_settings(singleton_id, base_currency, ui_locale, valuation_defaults_json, updated_at_utc) VALUES (1, ?1, ?2, '{}', CURRENT_TIMESTAMP)",
            [&legacy_settings.0, &legacy_settings.1],
        )
        .map_err(|_| ApplicationError::MigrationFailed)?;
    transaction
        .execute(
            "INSERT INTO migration_history(schema_version, applied_at_utc, application_version, schema_hash) VALUES (1, CURRENT_TIMESTAMP, ?1, ?2)",
            [env!("CARGO_PKG_VERSION"), &schema_hash()],
        )
        .map_err(|_| ApplicationError::MigrationFailed)?;
    transaction
        .execute(
            "INSERT INTO projection_metadata(projection_name,projection_version,calculation_version,event_watermark,available) VALUES('cash-balance','cash-balance-projection-v1','ledger-calculation-v1',0,1),('holdings','holding-projection-v1','ledger-calculation-v1',0,1)",
            [],
        )
        .map_err(|_| ApplicationError::MigrationFailed)?;
    transaction
        .execute(
            "INSERT INTO backup_status(singleton_id,protection_state,external_target_configured) VALUES(1,'not-configured',0)",
            [],
        )
        .map_err(|_| ApplicationError::MigrationFailed)?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)] // `map_err` supplies the owned driver error.
fn map_open_error(error: rusqlite::Error) -> ApplicationError {
    match error {
        rusqlite::Error::SqliteFailure(_, _) => ApplicationError::DatabaseCorrupt,
        _ => ApplicationError::StorageUnavailable,
    }
}

fn is_forbidden_location(path: &Path) -> bool {
    if !path.is_absolute() {
        return true;
    }
    if matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::UNC(_, _) | std::path::Prefix::VerbatimUNC(_, _)))
    {
        return true;
    }
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        ["onedrive", "dropbox", "google drive"]
            .iter()
            .any(|blocked| value.starts_with(blocked))
    })
}

pub fn validate_local_data_root(path: &Path) -> ApplicationResult<()> {
    if is_forbidden_location(path) {
        return Err(ApplicationError::LiveDatabaseLocationRejected);
    }
    Ok(())
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use std::fs;

    use rusqlite::params;
    use tempfile::tempdir;

    use super::*;

    struct RecordingBackup {
        called: bool,
        path: PathBuf,
    }

    struct FailingBackup;

    impl MigrationBackupPort for FailingBackup {
        fn create_verified_backup(
            &mut self,
            _source: &Path,
            _source_schema_version: u32,
        ) -> ApplicationResult<PathBuf> {
            Err(ApplicationError::MigrationBackupFailed)
        }
    }

    impl MigrationBackupPort for RecordingBackup {
        fn create_verified_backup(
            &mut self,
            source: &Path,
            source_schema_version: u32,
        ) -> ApplicationResult<PathBuf> {
            self.called = true;
            assert_eq!(source_schema_version, 0);
            fs::copy(source, &self.path).map_err(|_| ApplicationError::MigrationBackupFailed)?;
            Ok(self.path.clone())
        }
    }

    fn legacy_database(path: &Path, version: u32) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(&format!(
                "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = {version}; CREATE TABLE legacy_settings(singleton_id INTEGER PRIMARY KEY, base_currency TEXT NOT NULL, ui_locale TEXT NOT NULL); INSERT INTO legacy_settings VALUES(1, 'CNY', 'zh-CN');"
            ))
            .unwrap();
    }

    #[test]
    fn old_schema_is_backed_up_then_migrated_and_validated() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("ledger.sqlite3");
        legacy_database(&database, 0);
        let mut backup = RecordingBackup {
            called: false,
            path: directory.path().join("verified.sqlite3"),
        };
        let connection = MigrationRunner::open_existing(&database, &mut backup).unwrap();
        assert!(backup.called);
        validate_schema(&connection).unwrap();
        let settings: (String, String) = connection
            .query_row(
                "SELECT base_currency, ui_locale FROM app_settings WHERE singleton_id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(settings, ("CNY".to_owned(), "zh-CN".to_owned()));
    }

    #[test]
    fn failed_migration_rolls_back_without_changing_old_schema() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("ledger.sqlite3");
        legacy_database(&database, 0);
        let mut backup = RecordingBackup {
            called: false,
            path: directory.path().join("verified.sqlite3"),
        };
        assert!(matches!(
            MigrationRunner::open_existing_with_failpoint(
                &database,
                &mut backup,
                MigrationFailpoint::BeforeValidation,
            ),
            Err(ApplicationError::MigrationFailed)
        ));
        let connection = Connection::open(&database).unwrap();
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let legacy_value: String = connection
            .query_row("SELECT base_currency FROM legacy_settings", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 0);
        assert_eq!(legacy_value, "CNY");
    }

    #[test]
    fn corrupt_and_too_new_databases_are_blocked_without_mutation() {
        let directory = tempdir().unwrap();
        let corrupt = directory.path().join("corrupt.sqlite3");
        fs::write(&corrupt, b"not sqlite").unwrap();
        assert_eq!(
            inspect_read_only(&corrupt),
            Err(ApplicationError::DatabaseCorrupt)
        );

        let too_new = directory.path().join("new.sqlite3");
        legacy_database(&too_new, SCHEMA_VERSION + 1);
        let before = fs::read(&too_new).unwrap();
        let mut backup = RecordingBackup {
            called: false,
            path: directory.path().join("unused.sqlite3"),
        };
        assert!(matches!(
            MigrationRunner::open_existing(&too_new, &mut backup),
            Err(ApplicationError::SchemaTooNew)
        ));
        assert!(!backup.called);
        assert_eq!(fs::read(&too_new).unwrap(), before);
    }

    #[test]
    fn backup_or_current_schema_validation_failure_never_migrates_the_database() {
        let directory = tempdir().unwrap();
        let legacy = directory.path().join("legacy.sqlite3");
        legacy_database(&legacy, 0);
        let before = fs::read(&legacy).unwrap();
        assert!(matches!(
            MigrationRunner::open_existing(&legacy, &mut FailingBackup),
            Err(ApplicationError::MigrationBackupFailed)
        ));
        assert_eq!(fs::read(&legacy).unwrap(), before);

        let invalid_current = directory.path().join("invalid-current.sqlite3");
        legacy_database(&invalid_current, SCHEMA_VERSION);
        let before = fs::read(&invalid_current).unwrap();
        assert_eq!(
            inspect_read_only(&invalid_current),
            Err(ApplicationError::SchemaValidationFailed)
        );
        assert_eq!(fs::read(&invalid_current).unwrap(), before);
    }

    #[test]
    fn local_data_policy_rejects_relative_and_synchronized_roots() {
        assert_eq!(
            validate_local_data_root(Path::new("relative")),
            Err(ApplicationError::LiveDatabaseLocationRejected)
        );
        let directory = tempdir().unwrap();
        let synchronized = directory
            .path()
            .join("OneDrive - Example")
            .join("LedgerKit");
        assert_eq!(
            validate_local_data_root(&synchronized),
            Err(ApplicationError::LiveDatabaseLocationRejected)
        );
    }

    #[test]
    fn schema_enforces_foreign_keys_dates_and_active_revisions() {
        let directory = tempdir().unwrap();
        let database = directory.path().join("ledger.sqlite3");
        let connection = MigrationRunner::create_new(&database).unwrap();
        connection
            .execute(
                "INSERT INTO security_price_revisions(security_price_revision_id,instrument_id,price_date,price,price_currency,source,revision,active,created_at_utc) VALUES('p','missing','2026-01-01','1','CNY','manual',1,1,CURRENT_TIMESTAMP)",
                [],
            )
            .expect_err("foreign key must be enforced");
        connection
            .execute(
                "INSERT INTO institutions VALUES('i','bank','Bank','CN','bank',1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO cash_accounts VALUES('a','cash','i','Cash','daily','CNY','2026-02-29',1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
                [],
            )
            .expect_err("invalid local date must fail");
        connection
            .execute(
                "INSERT INTO fx_rate_revisions VALUES('fx1','2026-01-01','USD','CNY','7.1','manual',1,1,CURRENT_TIMESTAMP)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO fx_rate_revisions VALUES('fx2','2026-01-01','USD','CNY','7.2','manual',2,1,CURRENT_TIMESTAMP)",
                [],
            )
            .expect_err("only one active revision is allowed");
        let count: u32 = connection
            .query_row("SELECT COUNT(*) FROM fx_rate_revisions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        let _: u32 = connection
            .query_row("SELECT ?1", params![count], |row| row.get(0))
            .unwrap();
    }
}
