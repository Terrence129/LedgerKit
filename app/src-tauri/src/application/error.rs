#![forbid(unsafe_code)]

use std::fmt::{Display, Formatter};

use crate::domain::error::DomainError;

pub type ApplicationResult<T> = Result<T, ApplicationError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    Domain(DomainError),
    InvalidLocale,
    InvalidView,
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
    CatalogEntityNotFound,
    CatalogDuplicate,
    CatalogReferenceInvalid,
    ActivityLimitInvalid,
    ActivityCursorInvalid,
    ActivityFilterInvalid,
    ExpenseDateRangeInvalid,
    ResponseTooLarge,
    BackupCancelled,
    BackupPathInvalid,
    BackupPasswordRequired,
    BackupFormatUnsupported,
    BackupKdfUnsupported,
    BackupAuthenticationFailed,
    BackupHashMismatch,
    BackupVerificationFailed,
    BackupWriteFailed,
    BackupRestoreFailed,
    ExportCancelled,
    ExportFormatUnsupported,
    ExportPathInvalid,
    ExportWriteFailed,
    ImportCancelled,
    ImportFileInvalid,
    ImportFileTooLarge,
    ImportTemplateUnsupported,
    ImportBatchNotFound,
    ImportConfirmationRequired,
    ImportBlockersPresent,
    ImportModifiedMergeForbidden,
    ImportCandidateSwitchFailed,
    ImportReconciliationFailed,
    ImportWorkerFailed,
}

impl ApplicationError {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Domain(error) => error.code(),
            Self::InvalidLocale => "SETTINGS_LOCALE_UNSUPPORTED",
            Self::InvalidView => "VIEW_UNSUPPORTED",
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
            Self::CatalogEntityNotFound => "CATALOG_ENTITY_NOT_FOUND",
            Self::CatalogDuplicate => "CATALOG_DUPLICATE",
            Self::CatalogReferenceInvalid => "CATALOG_REFERENCE_INVALID",
            Self::ActivityLimitInvalid => "ACTIVITY_LIMIT_INVALID",
            Self::ActivityCursorInvalid => "ACTIVITY_CURSOR_INVALID",
            Self::ActivityFilterInvalid => "ACTIVITY_FILTER_INVALID",
            Self::ExpenseDateRangeInvalid => "EXPENSE_DATE_RANGE_INVALID",
            Self::ResponseTooLarge => "RESPONSE_TOO_LARGE",
            Self::BackupCancelled => "BACKUP_CANCELLED",
            Self::BackupPathInvalid => "BACKUP_PATH_INVALID",
            Self::BackupPasswordRequired => "BACKUP_PASSWORD_REQUIRED",
            Self::BackupFormatUnsupported => "BACKUP_FORMAT_UNSUPPORTED",
            Self::BackupKdfUnsupported => "BACKUP_KDF_UNSUPPORTED",
            Self::BackupAuthenticationFailed => "BACKUP_AUTHENTICATION_FAILED",
            Self::BackupHashMismatch => "BACKUP_HASH_MISMATCH",
            Self::BackupVerificationFailed => "BACKUP_VERIFICATION_FAILED",
            Self::BackupWriteFailed => "BACKUP_WRITE_FAILED",
            Self::BackupRestoreFailed => "BACKUP_RESTORE_FAILED",
            Self::ExportCancelled => "EXPORT_CANCELLED",
            Self::ExportFormatUnsupported => "EXPORT_FORMAT_UNSUPPORTED",
            Self::ExportPathInvalid => "EXPORT_PATH_INVALID",
            Self::ExportWriteFailed => "EXPORT_WRITE_FAILED",
            Self::ImportCancelled => "IMPORT_CANCELLED",
            Self::ImportFileInvalid => "IMPORT_FILE_INVALID",
            Self::ImportFileTooLarge => "IMPORT_FILE_TOO_LARGE",
            Self::ImportTemplateUnsupported => "IMPORT_TEMPLATE_UNSUPPORTED",
            Self::ImportBatchNotFound => "IMPORT_BATCH_NOT_FOUND",
            Self::ImportConfirmationRequired => "IMPORT_CONFIRMATION_REQUIRED",
            Self::ImportBlockersPresent => "IMPORT_BLOCKERS_PRESENT",
            Self::ImportModifiedMergeForbidden => "IMPORT_MODIFIED_MERGE_FORBIDDEN",
            Self::ImportCandidateSwitchFailed => "IMPORT_CANDIDATE_SWITCH_FAILED",
            Self::ImportReconciliationFailed => "IMPORT_RECONCILIATION_FAILED",
            Self::ImportWorkerFailed => "IMPORT_WORKER_FAILED",
        }
    }

    #[must_use]
    pub const fn field(self) -> Option<&'static str> {
        match self {
            Self::InvalidLocale => Some("uiLocale"),
            Self::InvalidView => Some("view"),
            Self::Domain(DomainError::CurrencyInvalid | DomainError::FxSelfRateImmutable) => {
                Some("currency")
            }
            Self::Domain(DomainError::LocalDateInvalid) => Some("date"),
            Self::Domain(DomainError::UuidV7Invalid) => Some("id"),
            Self::Domain(DomainError::CatalogTextInvalid) => Some("name"),
            Self::Domain(DomainError::BusinessIdInvalid) => Some("businessId"),
            Self::Domain(DomainError::CategoryKindInvalid) => Some("kind"),
            Self::Domain(DomainError::SemanticRoleInvalid) => Some("semanticRole"),
            Self::Domain(DomainError::SortOrderInvalid) => Some("sortOrder"),
            Self::Domain(DomainError::PositiveValueRequired) => Some("value"),
            Self::Domain(DomainError::PriceMustBePositive) => Some("price"),
            Self::Domain(
                DomainError::PortfolioInstitutionMismatch
                | DomainError::TradeCurrencyMismatch
                | DomainError::SettlementInstitutionMismatch,
            ) => Some("settlementAccountId"),
            Self::Domain(DomainError::AccountBalanceNonzero) => Some("enabled"),
            Self::Domain(DomainError::RevisionImmutable) => Some("revisionId"),
            Self::Domain(DomainError::AmountMustBePositive | DomainError::AdjustmentZero) => {
                Some("amount")
            }
            Self::Domain(DomainError::FxOverrideReasonRequired) => Some("fxOverrides"),
            Self::Domain(
                DomainError::RevisionReasonRequired | DomainError::ReversalReasonRequired,
            ) => Some("reason"),
            Self::Domain(DomainError::NegativeHoldingNotAllowed) => Some("quantity"),
            Self::Domain(DomainError::SettlementOverrideReasonRequired) => {
                Some("settlementOverrideReason")
            }
            Self::Domain(
                DomainError::InstrumentRequiredForFeeScope
                | DomainError::PortfolioFeeInstrumentForbidden,
            ) => Some("instrumentId"),
            Self::Domain(DomainError::DividendDeductionsExceedGross) => Some("grossCashAmount"),
            Self::ActivityLimitInvalid => Some("limit"),
            Self::ActivityCursorInvalid => Some("cursor"),
            Self::ActivityFilterInvalid => Some("search"),
            Self::ExpenseDateRangeInvalid => Some("dateRange"),
            Self::BackupPasswordRequired | Self::BackupAuthenticationFailed => {
                Some("backupPassword")
            }
            Self::BackupPathInvalid => Some("backupTarget"),
            Self::ExportFormatUnsupported => Some("exportFormat"),
            Self::ExportPathInvalid => Some("exportTarget"),
            _ => None,
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
