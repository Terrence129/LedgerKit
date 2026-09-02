#![forbid(unsafe_code)]

use super::decimal::Decimal;
use super::error::DomainError;
use super::types::{CalculationVersion, Currency, LocalDate, Sequence, UuidV7};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostingKind {
    Cash,
    OpeningCash,
    CashReversal,
    SecurityQuantity,
    SecurityCost,
    RealizedTradePnl,
    NetDividend,
    IndependentExpense,
    SettlementCash,
    HoldingCost,
    RealizedPnl,
    PortfolioIndependentExpense,
    OpeningQuantity,
    OpeningCost,
    OpeningRealizedPnl,
    OpeningNetDividend,
    OpeningIndependentExpense,
    OpeningPortfolioIndependentExpense,
}

impl PostingKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::OpeningCash => "opening-cash",
            Self::CashReversal => "cash-reversal",
            Self::SecurityQuantity => "security-quantity",
            Self::SecurityCost => "security-cost",
            Self::RealizedTradePnl => "realized-trade-pnl",
            Self::NetDividend => "net-dividend",
            Self::IndependentExpense => "independent-expense",
            Self::SettlementCash => "settlement-cash",
            Self::HoldingCost => "holding-cost",
            Self::RealizedPnl => "realized-pnl",
            Self::PortfolioIndependentExpense => "portfolio-independent-expense",
            Self::OpeningQuantity => "opening-quantity",
            Self::OpeningCost => "opening-cost",
            Self::OpeningRealizedPnl => "opening-realized-pnl",
            Self::OpeningNetDividend => "opening-net-dividend",
            Self::OpeningIndependentExpense => "opening-independent-expense",
            Self::OpeningPortfolioIndependentExpense => "opening-portfolio-independent-expense",
        }
    }

    /// Parses a stable posting-kind identifier.
    ///
    /// # Errors
    ///
    /// Returns `POSTING_INVARIANT_VIOLATION` for unknown identifiers.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "cash" => Ok(Self::Cash),
            "opening-cash" => Ok(Self::OpeningCash),
            "cash-reversal" => Ok(Self::CashReversal),
            "security-quantity" => Ok(Self::SecurityQuantity),
            "security-cost" => Ok(Self::SecurityCost),
            "realized-trade-pnl" => Ok(Self::RealizedTradePnl),
            "net-dividend" => Ok(Self::NetDividend),
            "independent-expense" => Ok(Self::IndependentExpense),
            "settlement-cash" => Ok(Self::SettlementCash),
            "holding-cost" => Ok(Self::HoldingCost),
            "realized-pnl" => Ok(Self::RealizedPnl),
            "portfolio-independent-expense" => Ok(Self::PortfolioIndependentExpense),
            "opening-quantity" => Ok(Self::OpeningQuantity),
            "opening-cost" => Ok(Self::OpeningCost),
            "opening-realized-pnl" => Ok(Self::OpeningRealizedPnl),
            "opening-net-dividend" => Ok(Self::OpeningNetDividend),
            "opening-independent-expense" => Ok(Self::OpeningIndependentExpense),
            "opening-portfolio-independent-expense" => Ok(Self::OpeningPortfolioIndependentExpense),
            _ => Err(DomainError::PostingInvariantViolation),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LedgerPosting {
    pub posting_id: UuidV7,
    pub event_id: UuidV7,
    pub effective_date: LocalDate,
    pub sequence: Sequence,
    pub posting_kind: PostingKind,
    pub account_id: Option<UuidV7>,
    pub portfolio_id: Option<UuidV7>,
    pub instrument_id: Option<UuidV7>,
    pub quantity_delta: Decimal,
    pub currency: Currency,
    pub base_value: Option<Decimal>,
    pub base_currency: Currency,
    pub calculation_version: CalculationVersion,
}

impl LedgerPosting {
    /// Confirms the posting belongs to the prepared event and has a target.
    ///
    /// # Errors
    ///
    /// Returns `POSTING_INVARIANT_VIOLATION` when any identity or ordering
    /// field differs from the event.
    pub fn validate_for_event(
        &self,
        event_id: UuidV7,
        effective_date: &LocalDate,
        sequence: Sequence,
    ) -> Result<(), DomainError> {
        if self.event_id != event_id
            || &self.effective_date != effective_date
            || self.sequence != sequence
            || self.account_id.is_none() && self.instrument_id.is_none()
        {
            return Err(DomainError::PostingInvariantViolation);
        }
        Ok(())
    }
}
