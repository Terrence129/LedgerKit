#![forbid(unsafe_code)]

use serde::Serialize;

use crate::domain::catalog::SemanticRole;
use crate::domain::decimal::Decimal;
use crate::domain::types::{Currency, LocalDate, Sequence, UuidV7};

use super::error::ApplicationResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxOverrideInput {
    pub currency: Currency,
    pub value: Decimal,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventInputType {
    OpeningBalance,
    Income,
    Expense,
    Adjustment,
    Transfer,
    CurrencyExchange,
}

impl EventInputType {
    /// Parses the stable command discriminator.
    ///
    /// # Errors
    ///
    /// Returns an event invariant error for unsupported values.
    pub fn parse(value: &str) -> Result<Self, crate::domain::error::DomainError> {
        match value {
            "OpeningBalance" => Ok(Self::OpeningBalance),
            "Income" => Ok(Self::Income),
            "Expense" => Ok(Self::Expense),
            "Adjustment" | "BalanceAdjustment" => Ok(Self::Adjustment),
            "Transfer" => Ok(Self::Transfer),
            "CurrencyExchange" => Ok(Self::CurrencyExchange),
            _ => Err(crate::domain::error::DomainError::EventInvariantViolation),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashEventInput {
    pub effective_date: LocalDate,
    pub sequence: Sequence,
    pub event_type: EventInputType,
    pub account_id: Option<UuidV7>,
    pub from_account_id: Option<UuidV7>,
    pub to_account_id: Option<UuidV7>,
    pub amount: Option<Decimal>,
    pub to_amount: Option<Decimal>,
    pub category_id: Option<UuidV7>,
    pub semantic_role: SemanticRole,
    pub merchant: Option<String>,
    pub note: Option<String>,
    pub fee_account_id: Option<UuidV7>,
    pub fee_amount: Option<Decimal>,
    pub cutover_date: Option<LocalDate>,
    pub migration_policy: Option<String>,
    pub fx_overrides: Vec<FxOverrideInput>,
    pub currency_precision_confirmed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FxResolutionResult {
    pub purpose: &'static str,
    pub currency: String,
    pub base_currency: String,
    pub target_date: String,
    pub automatic_candidate_revision_id: Option<String>,
    pub override_value: Option<String>,
    pub override_reason: Option<String>,
    pub final_rate: Option<String>,
    pub calculation_version: &'static str,
    pub valuation_state: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostingPreview {
    pub account_id: String,
    pub quantity_delta: String,
    pub currency: String,
    pub base_value: Option<String>,
    pub base_currency: String,
    pub role: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPreview {
    pub event_type: &'static str,
    pub effective_date: String,
    pub sequence: u64,
    pub postings: Vec<PostingPreview>,
    pub fx_resolutions: Vec<FxResolutionResult>,
    pub quality_issue_codes: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostedEvent {
    pub event_id: String,
    pub event_watermark: u64,
    pub revision: u32,
    pub preview: EventPreview,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionInput {
    pub target_event_id: UuidV7,
    pub reason: String,
    pub replacement: CashEventInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReversalInput {
    pub target_event_id: UuidV7,
    pub reason: String,
    pub effective_date: LocalDate,
    pub sequence: Sequence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DrilldownContext {
    pub start_date: String,
    pub end_date: String,
    pub event_watermark: u64,
    pub calculation_version: &'static str,
    pub expense_policy_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_rank_gt: Option<u32>,
    pub valuation_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseBucket {
    pub bucket_id: String,
    pub bucket_kind: &'static str,
    pub label: String,
    pub archived: bool,
    pub amount: String,
    pub distinct_event_count: u64,
    pub drilldown_context: DrilldownContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseTopItem {
    pub bucket_id: String,
    pub label: String,
    pub amount: String,
    pub distinct_event_count: u64,
    pub drilldown_context: DrilldownContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseTop10 {
    pub items: Vec<ExpenseTopItem>,
    pub other: Option<ExpenseTopItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseMeasure {
    pub amount: String,
    pub distinct_event_count: u64,
    pub unvalued_count: u64,
    pub drilldown_context: DrilldownContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RefundMeasures {
    pub refund: ExpenseMeasure,
    pub reimbursement: ExpenseMeasure,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseQueryContract {
    pub start_date: String,
    pub end_date: String,
    pub base_currency: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LargestCategory {
    pub bucket_id: String,
    pub amount: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseSummary {
    pub label: &'static str,
    pub total_expense: Option<String>,
    pub valued_subtotal: String,
    pub global_distinct_event_count: u64,
    pub largest_category: Option<LargestCategory>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UnvaluedExpense {
    pub expense_count: u64,
    pub drilldown_context: DrilldownContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseWatermarks {
    pub event: u64,
    pub master_data: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseVersions {
    pub calculation: &'static str,
    pub expense_policy: &'static str,
    pub bucket_policy: &'static str,
    pub refund_policy: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExpenseAnalysis {
    pub contract: &'static str,
    pub query: ExpenseQueryContract,
    pub summary: ExpenseSummary,
    pub buckets: Vec<ExpenseBucket>,
    pub top10: ExpenseTop10,
    pub refunds: RefundMeasures,
    pub unvalued: UnvaluedExpense,
    pub watermarks: ExpenseWatermarks,
    pub versions: ExpenseVersions,
    pub canonicalization: &'static str,
    pub canonical_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityQuery {
    pub context: DrilldownContext,
    pub cursor: Option<u64>,
    pub limit: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub event_id: String,
    pub event_order: u64,
    pub event_type: String,
    pub effective_date: String,
    pub amount: String,
    pub currency: String,
    pub category_id: Option<String>,
    pub semantic_role: String,
    pub valuation_state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPage {
    pub items: Vec<ActivityItem>,
    pub next_cursor: Option<u64>,
}

#[allow(clippy::missing_errors_doc)]
pub trait CashPort: Send {
    fn preview_event(&self, input: &CashEventInput) -> ApplicationResult<EventPreview>;
    fn post_event(&mut self, input: &CashEventInput) -> ApplicationResult<PostedEvent>;
    fn revise_event(&mut self, input: &RevisionInput) -> ApplicationResult<PostedEvent>;
    fn reverse_event(&mut self, input: &ReversalInput) -> ApplicationResult<PostedEvent>;
    fn get_expense_analysis(
        &self,
        start_date: &LocalDate,
        end_date: &LocalDate,
        event_watermark: Option<u64>,
    ) -> ApplicationResult<ExpenseAnalysis>;
    fn get_activity(&self, query: &ActivityQuery) -> ApplicationResult<ActivityPage>;
}
