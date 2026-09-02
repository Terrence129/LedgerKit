#![forbid(unsafe_code)]

use std::fmt::{Display, Formatter};

use crate::domain::error::DomainError;

pub type ApplicationResult<T> = Result<T, ApplicationError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    Domain(DomainError),
    InvalidLocale,
    StorageUnavailable,
    LedgerAlreadyExists,
    LedgerNotFound,
    LedgerNotOpen,
    LedgerAlreadyOpen,
    LiveDatabaseLocationRejected,
    DatabaseNotLedgerKit,
    DatabaseCorrupt,
    SchemaTooNew,
    MigrationBackupFailed,
    MigrationFailed,
    SchemaValidationFailed,
    TransactionFailed,
    ApplicationStateUnavailable,
}

impl ApplicationError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Domain(error) => error.code(),
            Self::InvalidLocale => "SETTINGS_LOCALE_UNSUPPORTED",
            Self::StorageUnavailable => "SETTINGS_STORAGE_UNAVAILABLE",
            Self::LedgerAlreadyExists => "LEDGER_ALREADY_EXISTS",
            Self::LedgerNotFound => "LEDGER_NOT_FOUND",
            Self::LedgerNotOpen => "LEDGER_NOT_OPEN",
            Self::LedgerAlreadyOpen => "LEDGER_ALREADY_OPEN",
            Self::LiveDatabaseLocationRejected => "LIVE_DATABASE_LOCATION_REJECTED",
            Self::DatabaseNotLedgerKit => "DATABASE_NOT_LEDGERKIT",
            Self::DatabaseCorrupt => "DATABASE_CORRUPT",
            Self::SchemaTooNew => "SCHEMA_VERSION_TOO_NEW",
            Self::MigrationBackupFailed => "MIGRATION_BACKUP_FAILED",
            Self::MigrationFailed => "MIGRATION_FAILED",
            Self::SchemaValidationFailed => "SCHEMA_VALIDATION_FAILED",
            Self::TransactionFailed => "TRANSACTION_FAILED",
            Self::ApplicationStateUnavailable => "APPLICATION_STATE_UNAVAILABLE",
        }
    }
}

impl From<DomainError> for ApplicationError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}

impl Display for ApplicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ApplicationError {}
