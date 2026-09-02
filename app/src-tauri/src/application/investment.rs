#![forbid(unsafe_code)]

use serde::Serialize;

use crate::domain::decimal::Decimal;
use crate::domain::investment::FeeScope;
use crate::domain::types::{LocalDate, Sequence, UuidV7};

use super::cash::FxOverrideInput;
use super::error::ApplicationResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestmentEventType {
    SecurityBuy,
    SecuritySell,
    Dividend,
    InvestmentExpense,
    OpeningPosition,
    OpeningPerformance,
}

impl InvestmentEventType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SecurityBuy => "SecurityBuy",
            Self::SecuritySell => "SecuritySell",
            Self::Dividend => "Dividend",
            Self::InvestmentExpense => "InvestmentExpense",
            Self::OpeningPosition => "OpeningPosition",
            Self::OpeningPerformance => "OpeningPerformance",
        }
    }

    /// Parses the stable public discriminator.
    ///
    /// # Errors
    ///
    /// Returns an event invariant error for unsupported values.
    pub fn parse(value: &str) -> Result<Self, crate::domain::error::DomainError> {
        match value {
            "SecurityBuy" => Ok(Self::SecurityBuy),
            "SecuritySell" => Ok(Self::SecuritySell),
            "Dividend" => Ok(Self::Dividend),
            "InvestmentExpense" => Ok(Self::InvestmentExpense),
            "OpeningPosition" => Ok(Self::OpeningPosition),
            "OpeningPerformance" => Ok(Self::OpeningPerformance),
            _ => Err(crate::domain::error::DomainError::EventInvariantViolation),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestmentEventInput {
    pub effective_date: LocalDate,
    pub sequence: Sequence,
    pub event_type: InvestmentEventType,
    pub portfolio_id: UuidV7,
    pub instrument_id: Option<UuidV7>,
    pub settlement_account_id: UuidV7,
    pub quantity: Option<Decimal>,
    pub unit_price: Option<Decimal>,
    pub trade_fee: Option<Decimal>,
    pub gross_cash_amount: Option<Decimal>,
    pub withholding_tax: Option<Decimal>,
    pub fee_amount: Option<Decimal>,
    pub amount: Option<Decimal>,
    pub carrying_cost: Option<Decimal>,
    pub realized_trade_pnl: Option<Decimal>,
    pub net_dividend: Option<Decimal>,
    pub independent_expense: Option<Decimal>,
    pub cost_currency: Option<crate::domain::types::Currency>,
    pub cutover_date: Option<LocalDate>,
    pub migration_policy: Option<String>,
    pub fee_scope: Option<FeeScope>,
    pub settlement_override_reason: Option<String>,
    pub fx_overrides: Vec<FxOverrideInput>,
}

/// High-level authoritative event command. UI callers cannot submit postings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentPostingPreview {
    pub posting_kind: &'static str,
    pub account_id: Option<String>,
    pub portfolio_id: String,
    pub instrument_id: Option<String>,
    pub quantity_delta: String,
    pub currency: String,
    pub base_value: Option<String>,
    pub base_currency: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentEventPreview {
    pub event_type: &'static str,
    pub effective_date: String,
    pub sequence: u64,
    pub postings: Vec<InvestmentPostingPreview>,
    pub quantity_after: Option<String>,
    pub carrying_cost_after: Option<String>,
    pub average_cost_after: Option<String>,
    pub realized_trade_pnl_after: Option<String>,
    pub quality_issue_codes: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostedInvestmentEvent {
    pub event_id: String,
    pub event_watermark: u64,
    pub revision: u32,
    pub preview: InvestmentEventPreview,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestmentRevisionInput {
    pub target_event_id: UuidV7,
    pub reason: String,
    pub replacement: InvestmentEventInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingPosition {
    pub portfolio_id: String,
    pub portfolio_name: String,
    pub instrument_id: String,
    pub instrument_name: String,
    pub currency: String,
    pub as_of_date: String,
    pub quantity: String,
    pub carrying_cost: String,
    pub average_cost: Option<String>,
    pub realized_trade_pnl: String,
    pub net_dividend: String,
    pub independent_expense: String,
    pub market_price: Option<String>,
    pub price_revision_id: Option<String>,
    pub price_date: Option<String>,
    pub price_age_days: Option<u32>,
    pub market_value: Option<String>,
    pub fx_rate: Option<String>,
    pub fx_revision_id: Option<String>,
    pub base_market_value: Option<String>,
    pub unrealized_pnl: Option<String>,
    pub total_return: Option<String>,
    pub valuation_state: &'static str,
    pub unvalued_reason: Option<&'static str>,
    pub warning_codes: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioExpenseSummary {
    pub portfolio_id: String,
    pub portfolio_name: String,
    pub amount: String,
    pub currency: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestmentWorkspace {
    pub as_of_date: String,
    pub base_currency: String,
    pub holdings: Vec<HoldingPosition>,
    pub portfolio_expenses: Vec<PortfolioExpenseSummary>,
    pub event_watermark: u64,
    pub projection_version: &'static str,
    pub calculation_version: &'static str,
}

#[allow(clippy::missing_errors_doc)]
pub trait InvestmentPort: Send {
    fn preview_investment_event(
        &self,
        input: &InvestmentEventInput,
    ) -> ApplicationResult<InvestmentEventPreview>;
    fn post_investment_event(
        &mut self,
        input: &InvestmentEventInput,
    ) -> ApplicationResult<PostedInvestmentEvent>;
    fn revise_investment_event(
        &mut self,
        input: &InvestmentRevisionInput,
    ) -> ApplicationResult<PostedInvestmentEvent>;
    fn get_investment_workspace(
        &self,
        as_of_date: &LocalDate,
    ) -> ApplicationResult<InvestmentWorkspace>;
}
