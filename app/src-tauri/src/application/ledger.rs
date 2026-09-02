#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use crate::domain::decimal::Decimal;
use crate::domain::error::DomainError;
use crate::domain::posting::LedgerPosting;
use crate::domain::settings::UiLocale;
use crate::domain::types::{
    CalculationVersion, Currency, LocalDate, ProjectionWatermark, Sequence, UuidV7,
};

use super::error::ApplicationResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LedgerState {
    NotCreated,
    Closed,
    Open,
    Blocked,
}

impl LedgerState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotCreated => "not-created",
            Self::Closed => "closed",
            Self::Open => "open",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerStatus {
    pub state: LedgerState,
    pub ledger_id: Option<String>,
    pub schema_version: Option<u32>,
    pub base_currency: Option<Currency>,
    pub ui_locale: UiLocale,
    pub event_watermark: u64,
    pub projection_watermark: u64,
    pub calculation_version: &'static str,
    pub blocked_reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreateLedgerCommand {
    pub base_currency: Currency,
    pub ui_locale: UiLocale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateLedgerSettingsCommand {
    pub base_currency: Option<Currency>,
    pub ui_locale: UiLocale,
    pub valuation_defaults_json: Option<String>,
}

pub trait LedgerPort: Send {
    /// Creates and opens the fixed ledger.
    ///
    /// # Errors
    ///
    /// Returns a stable error when creation or validation fails.
    fn create_ledger(&mut self, command: CreateLedgerCommand) -> ApplicationResult<LedgerStatus>;
    /// Identifies, migrates when required, validates, and opens the fixed ledger.
    ///
    /// # Errors
    ///
    /// Returns a stable error when backup, migration, validation, or opening fails.
    fn open_ledger(&mut self) -> ApplicationResult<LedgerStatus>;
    /// Inspects current ledger state without opening a closed ledger.
    ///
    /// # Errors
    ///
    /// Returns a stable error when status cannot be determined.
    fn get_ledger_status(&self, fallback_locale: UiLocale) -> ApplicationResult<LedgerStatus>;
    /// Persists settings on an open ledger.
    ///
    /// # Errors
    ///
    /// Returns a stable error when the ledger is closed, a freeze applies, or
    /// persistence fails.
    fn update_settings(
        &mut self,
        command: &UpdateLedgerSettingsCommand,
    ) -> ApplicationResult<LedgerStatus>;
}

pub trait MigrationBackupPort: Send {
    /// Creates and verifies a consistent backup before forward migration.
    ///
    /// # Errors
    ///
    /// Returns a stable error when snapshot creation or verification fails.
    fn create_verified_backup(
        &mut self,
        source: &Path,
        source_schema_version: u32,
    ) -> ApplicationResult<PathBuf>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusinessEventType {
    Income,
    Expense,
    BalanceAdjustment,
}

impl BusinessEventType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Income => "Income",
            Self::Expense => "Expense",
            Self::BalanceAdjustment => "BalanceAdjustment",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IncomeExpenseKind {
    Income,
    Expense,
    BalanceAdjustment,
}

impl IncomeExpenseKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
            Self::BalanceAdjustment => "balance_adjustment",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncomeExpenseDetail {
    pub account_id: UuidV7,
    pub kind: IncomeExpenseKind,
    pub category_id: Option<UuidV7>,
    pub amount: Decimal,
    pub semantic_role: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedEventCommit {
    pub event_id: UuidV7,
    pub event_type: BusinessEventType,
    pub effective_date: LocalDate,
    pub sequence: Sequence,
    pub revision: u32,
    pub created_at_utc: String,
    pub calculation_version: CalculationVersion,
    pub detail: IncomeExpenseDetail,
    pub postings: Vec<LedgerPosting>,
    pub audit_event_id: UuidV7,
    pub audit_reason: Option<String>,
}

impl PreparedEventCommit {
    /// Validates event/detail correspondence and deterministic postings.
    ///
    /// # Errors
    ///
    /// Returns a domain invariant error when the prepared unit is inconsistent.
    pub fn validate(&self) -> Result<(), DomainError> {
        let detail_matches_event = matches!(
            (self.event_type, self.detail.kind),
            (BusinessEventType::Income, IncomeExpenseKind::Income)
                | (BusinessEventType::Expense, IncomeExpenseKind::Expense)
                | (
                    BusinessEventType::BalanceAdjustment,
                    IncomeExpenseKind::BalanceAdjustment
                )
        );
        if self.revision == 0 || !detail_matches_event || self.postings.is_empty() {
            return Err(DomainError::EventInvariantViolation);
        }
        for posting in &self.postings {
            posting.validate_for_event(self.event_id, &self.effective_date, self.sequence)?;
        }
        Ok(())
    }
}

pub trait EventTransactionPort {
    /// Atomically persists an event, detail, postings, audit, and watermark.
    ///
    /// # Errors
    ///
    /// Returns a stable error and leaves no partial state on failure.
    fn commit_event(
        &mut self,
        prepared: &PreparedEventCommit,
    ) -> ApplicationResult<ProjectionWatermark>;
    /// Clears and deterministically rebuilds the cash projection.
    ///
    /// # Errors
    ///
    /// Returns a stable error and rolls back the rebuild on failure.
    fn rebuild_cash_projection(&mut self) -> ApplicationResult<ProjectionWatermark>;
    /// Returns the physical-row-order-independent canonical posting hash.
    ///
    /// # Errors
    ///
    /// Returns a stable error when persisted postings are invalid.
    fn canonical_posting_hash(&self) -> ApplicationResult<String>;
}
