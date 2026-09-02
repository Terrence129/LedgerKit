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
    CatalogEntityNotFound,
    CatalogDuplicate,
    CatalogReferenceInvalid,
    ActivityLimitInvalid,
    ActivityCursorInvalid,
    ActivityFilterInvalid,
    ExpenseDateRangeInvalid,
    ResponseTooLarge,
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
            Self::CatalogEntityNotFound => "CATALOG_ENTITY_NOT_FOUND",
            Self::CatalogDuplicate => "CATALOG_DUPLICATE",
            Self::CatalogReferenceInvalid => "CATALOG_REFERENCE_INVALID",
            Self::ActivityLimitInvalid => "ACTIVITY_LIMIT_INVALID",
            Self::ActivityCursorInvalid => "ACTIVITY_CURSOR_INVALID",
            Self::ActivityFilterInvalid => "ACTIVITY_FILTER_INVALID",
            Self::ExpenseDateRangeInvalid => "EXPENSE_DATE_RANGE_INVALID",
            Self::ResponseTooLarge => "RESPONSE_TOO_LARGE",
        }
    }

    #[must_use]
    pub const fn field(self) -> Option<&'static str> {
        match self {
            Self::InvalidLocale => Some("uiLocale"),
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
            Self::Domain(DomainError::PortfolioInstitutionMismatch) => Some("settlementAccountId"),
            Self::Domain(DomainError::AccountBalanceNonzero) => Some("enabled"),
            Self::Domain(DomainError::RevisionImmutable) => Some("revisionId"),
            Self::Domain(DomainError::AmountMustBePositive | DomainError::AdjustmentZero) => {
                Some("amount")
            }
            Self::Domain(DomainError::FxOverrideReasonRequired) => Some("fxOverrides"),
            Self::Domain(
                DomainError::RevisionReasonRequired | DomainError::ReversalReasonRequired,
            ) => Some("reason"),
            Self::ActivityLimitInvalid => Some("limit"),
            Self::ActivityCursorInvalid => Some("cursor"),
            Self::ActivityFilterInvalid => Some("search"),
            Self::ExpenseDateRangeInvalid => Some("dateRange"),
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
