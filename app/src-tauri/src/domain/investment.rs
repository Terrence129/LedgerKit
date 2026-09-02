#![forbid(unsafe_code)]

use std::cmp::Ordering;

use super::decimal::{Decimal, DecimalUse};
use super::error::DomainError;
use super::types::{Currency, LocalDate, Sequence, UuidV7};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvestmentAccountFact {
    pub account_id: UuidV7,
    pub institution_id: UuidV7,
    pub currency: Currency,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortfolioFact {
    pub portfolio_id: UuidV7,
    pub institution_id: UuidV7,
    pub settlement_account_id: UuidV7,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstrumentFact {
    pub instrument_id: UuidV7,
    pub trade_currency: Currency,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeeScope {
    Instrument,
    Portfolio,
}

impl FeeScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Instrument => "instrument",
            Self::Portfolio => "portfolio",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvestmentEventKind {
    SecurityTrade {
        side: TradeSide,
        portfolio: PortfolioFact,
        instrument: InstrumentFact,
        settlement: InvestmentAccountFact,
        quantity: Decimal,
        unit_price: Decimal,
        trade_fee: Decimal,
        settlement_override_reason: Option<String>,
    },
    Dividend {
        portfolio: PortfolioFact,
        instrument: InstrumentFact,
        settlement: InvestmentAccountFact,
        gross_cash_amount: Decimal,
        withholding_tax: Decimal,
        fee_amount: Decimal,
        settlement_override_reason: Option<String>,
    },
    InvestmentExpense {
        portfolio: PortfolioFact,
        instrument: Option<InstrumentFact>,
        settlement: InvestmentAccountFact,
        amount: Decimal,
        fee_scope: FeeScope,
        settlement_override_reason: Option<String>,
    },
    OpeningPosition {
        portfolio: PortfolioFact,
        instrument: InstrumentFact,
        quantity: Decimal,
        carrying_cost: Decimal,
        cost_currency: Currency,
        cutover_date: LocalDate,
        migration_policy: String,
    },
    OpeningPerformance {
        portfolio: PortfolioFact,
        instrument: Option<InstrumentFact>,
        realized_trade_pnl: Decimal,
        net_dividend: Decimal,
        independent_expense: Decimal,
        currency: Currency,
        cutover_date: LocalDate,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestmentEventCommand {
    pub effective_date: LocalDate,
    pub sequence: Sequence,
    pub kind: InvestmentEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HoldingState {
    pub quantity: Decimal,
    pub carrying_cost: Decimal,
    pub realized_trade_pnl: Decimal,
    pub net_dividend: Decimal,
    pub independent_expense: Decimal,
}

impl HoldingState {
    #[must_use]
    pub fn zero() -> Self {
        Self {
            quantity: Decimal::zero(DecimalUse::Internal),
            carrying_cost: Decimal::zero(DecimalUse::Internal),
            realized_trade_pnl: Decimal::zero(DecimalUse::Internal),
            net_dividend: Decimal::zero(DecimalUse::Internal),
            independent_expense: Decimal::zero(DecimalUse::Internal),
        }
    }

    /// Returns moving weighted average cost, or `None` for a closed position.
    ///
    /// # Errors
    ///
    /// Returns a Decimal error if the internal quotient cannot be represented.
    pub fn average_cost(&self) -> Result<Option<Decimal>, DomainError> {
        if self.quantity.is_zero() {
            Ok(None)
        } else {
            self.carrying_cost
                .checked_div_internal(&self.quantity)
                .map(Some)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestmentPostingKind {
    SettlementCash,
    SecurityQuantity,
    HoldingCost,
    RealizedPnl,
    NetDividend,
    IndependentExpense,
    PortfolioIndependentExpense,
    OpeningQuantity,
    OpeningCost,
    OpeningRealizedPnl,
    OpeningNetDividend,
    OpeningIndependentExpense,
    OpeningPortfolioIndependentExpense,
}

impl InvestmentPostingKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SettlementCash => "settlement-cash",
            Self::SecurityQuantity => "security-quantity",
            Self::HoldingCost => "holding-cost",
            Self::RealizedPnl => "realized-pnl",
            Self::NetDividend => "net-dividend",
            Self::IndependentExpense => "independent-expense",
            Self::PortfolioIndependentExpense => "portfolio-independent-expense",
            Self::OpeningQuantity => "opening-quantity",
            Self::OpeningCost => "opening-cost",
            Self::OpeningRealizedPnl => "opening-realized-pnl",
            Self::OpeningNetDividend => "opening-net-dividend",
            Self::OpeningIndependentExpense => "opening-independent-expense",
            Self::OpeningPortfolioIndependentExpense => "opening-portfolio-independent-expense",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestmentPostingDraft {
    pub posting_kind: InvestmentPostingKind,
    pub account_id: Option<UuidV7>,
    pub portfolio_id: UuidV7,
    pub instrument_id: Option<UuidV7>,
    pub quantity_delta: Decimal,
    pub currency: Currency,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedInvestmentEvent {
    pub event_type: &'static str,
    pub postings: Vec<InvestmentPostingDraft>,
    pub next_holding: Option<HoldingState>,
    pub portfolio_expense: Option<Decimal>,
}

/// Applies the authoritative moving-average, fee, dividend, and no-short rules.
///
/// # Errors
///
/// Returns a stable domain error without emitting partial postings.
#[allow(clippy::too_many_lines)] // One authoritative match keeps every financial branch auditable.
pub fn prepare_investment_event(
    command: &InvestmentEventCommand,
    current: &HoldingState,
) -> Result<PreparedInvestmentEvent, DomainError> {
    match &command.kind {
        InvestmentEventKind::SecurityTrade {
            side,
            portfolio,
            instrument,
            settlement,
            quantity,
            unit_price,
            trade_fee,
            settlement_override_reason,
        } => {
            validate_relationships(
                *portfolio,
                *instrument,
                *settlement,
                settlement_override_reason.as_deref(),
            )?;
            require_positive(quantity)?;
            require_positive(unit_price)?;
            require_non_negative(trade_fee)?;
            let gross = quantity.checked_mul_internal(unit_price)?;
            let signed_quantity;
            let cost_delta;
            let cash_delta;
            let realized_delta;
            let mut next = current.clone();
            match side {
                TradeSide::Buy => {
                    let total_cost = gross.checked_add(trade_fee, DecimalUse::Internal)?;
                    signed_quantity = quantity.clone();
                    cost_delta = total_cost.clone();
                    cash_delta = total_cost.checked_neg(DecimalUse::Internal)?;
                    realized_delta = Decimal::zero(DecimalUse::Internal);
                    next.quantity = next.quantity.checked_add(quantity, DecimalUse::Internal)?;
                    next.carrying_cost = next
                        .carrying_cost
                        .checked_add(&total_cost, DecimalUse::Internal)?;
                }
                TradeSide::Sell => {
                    if quantity.numeric_cmp(&current.quantity) == Ordering::Greater {
                        return Err(DomainError::NegativeHoldingNotAllowed);
                    }
                    let net_proceeds = gross.checked_add(
                        &trade_fee.checked_neg(DecimalUse::Internal)?,
                        DecimalUse::Internal,
                    )?;
                    if net_proceeds.is_negative() {
                        return Err(DomainError::AmountMustBePositive);
                    }
                    let released = if quantity.numeric_cmp(&current.quantity) == Ordering::Equal {
                        current.carrying_cost.clone()
                    } else {
                        current
                            .average_cost()?
                            .ok_or(DomainError::NegativeHoldingNotAllowed)?
                            .checked_mul_internal(quantity)?
                    };
                    signed_quantity = quantity.checked_neg(DecimalUse::Internal)?;
                    cost_delta = released.checked_neg(DecimalUse::Internal)?;
                    cash_delta = net_proceeds.clone();
                    realized_delta = net_proceeds.checked_add(&cost_delta, DecimalUse::Internal)?;
                    next.quantity = current
                        .quantity
                        .checked_add(&signed_quantity, DecimalUse::Internal)?;
                    next.carrying_cost = current
                        .carrying_cost
                        .checked_add(&cost_delta, DecimalUse::Internal)?;
                    if next.quantity.is_zero() {
                        next.carrying_cost = Decimal::zero(DecimalUse::Internal);
                    }
                    next.realized_trade_pnl = current
                        .realized_trade_pnl
                        .checked_add(&realized_delta, DecimalUse::Internal)?;
                }
            }
            let ids = (portfolio.portfolio_id, Some(instrument.instrument_id));
            let mut postings = vec![
                posting(
                    InvestmentPostingKind::SettlementCash,
                    Some(settlement.account_id),
                    ids,
                    cash_delta,
                    instrument.trade_currency,
                ),
                posting(
                    InvestmentPostingKind::SecurityQuantity,
                    None,
                    ids,
                    signed_quantity,
                    instrument.trade_currency,
                ),
                posting(
                    InvestmentPostingKind::HoldingCost,
                    None,
                    ids,
                    cost_delta,
                    instrument.trade_currency,
                ),
            ];
            if !realized_delta.is_zero() {
                postings.push(posting(
                    InvestmentPostingKind::RealizedPnl,
                    None,
                    ids,
                    realized_delta,
                    instrument.trade_currency,
                ));
            }
            Ok(PreparedInvestmentEvent {
                event_type: "SecurityTrade",
                postings,
                next_holding: Some(next),
                portfolio_expense: None,
            })
        }
        InvestmentEventKind::Dividend {
            portfolio,
            instrument,
            settlement,
            gross_cash_amount,
            withholding_tax,
            fee_amount,
            settlement_override_reason,
        } => {
            validate_relationships(
                *portfolio,
                *instrument,
                *settlement,
                settlement_override_reason.as_deref(),
            )?;
            require_positive(gross_cash_amount)?;
            require_non_negative(withholding_tax)?;
            require_non_negative(fee_amount)?;
            let deductions = withholding_tax.checked_add(fee_amount, DecimalUse::Internal)?;
            let net_dividend = gross_cash_amount.checked_add(
                &deductions.checked_neg(DecimalUse::Internal)?,
                DecimalUse::Internal,
            )?;
            if net_dividend.is_negative() {
                return Err(DomainError::DividendDeductionsExceedGross);
            }
            let mut next = current.clone();
            next.net_dividend = next
                .net_dividend
                .checked_add(&net_dividend, DecimalUse::Internal)?;
            let ids = (portfolio.portfolio_id, Some(instrument.instrument_id));
            Ok(PreparedInvestmentEvent {
                event_type: "Dividend",
                postings: vec![
                    posting(
                        InvestmentPostingKind::SettlementCash,
                        Some(settlement.account_id),
                        ids,
                        net_dividend.clone(),
                        instrument.trade_currency,
                    ),
                    posting(
                        InvestmentPostingKind::NetDividend,
                        None,
                        ids,
                        net_dividend,
                        instrument.trade_currency,
                    ),
                ],
                next_holding: Some(next),
                portfolio_expense: None,
            })
        }
        InvestmentEventKind::InvestmentExpense {
            portfolio,
            instrument,
            settlement,
            amount,
            fee_scope,
            settlement_override_reason,
        } => {
            require_positive(amount)?;
            if settlement.institution_id != portfolio.institution_id {
                return Err(DomainError::SettlementInstitutionMismatch);
            }
            if settlement.account_id != portfolio.settlement_account_id
                && settlement_override_reason
                    .as_deref()
                    .is_none_or(|reason| reason.trim().is_empty())
            {
                return Err(DomainError::SettlementOverrideReasonRequired);
            }
            let instrument_id = match (fee_scope, instrument) {
                (FeeScope::Instrument, Some(value)) => {
                    if value.trade_currency != settlement.currency {
                        return Err(DomainError::TradeCurrencyMismatch);
                    }
                    Some(value.instrument_id)
                }
                (FeeScope::Instrument, None) => {
                    return Err(DomainError::InstrumentRequiredForFeeScope);
                }
                (FeeScope::Portfolio, None) => None,
                (FeeScope::Portfolio, Some(_)) => {
                    return Err(DomainError::PortfolioFeeInstrumentForbidden);
                }
            };
            let ids = (portfolio.portfolio_id, instrument_id);
            let negative = amount.checked_neg(DecimalUse::Internal)?;
            let next_holding = if instrument_id.is_some() {
                let mut next = current.clone();
                next.independent_expense = next
                    .independent_expense
                    .checked_add(amount, DecimalUse::Internal)?;
                Some(next)
            } else {
                None
            };
            Ok(PreparedInvestmentEvent {
                event_type: "InvestmentExpense",
                postings: vec![
                    posting(
                        InvestmentPostingKind::SettlementCash,
                        Some(settlement.account_id),
                        ids,
                        negative,
                        settlement.currency,
                    ),
                    posting(
                        if instrument_id.is_none() {
                            InvestmentPostingKind::PortfolioIndependentExpense
                        } else {
                            InvestmentPostingKind::IndependentExpense
                        },
                        None,
                        ids,
                        amount.clone(),
                        settlement.currency,
                    ),
                ],
                next_holding,
                portfolio_expense: (instrument_id.is_none()).then(|| amount.clone()),
            })
        }
        InvestmentEventKind::OpeningPosition {
            portfolio,
            instrument,
            quantity,
            carrying_cost,
            cost_currency,
            cutover_date,
            migration_policy,
        } => {
            require_non_negative(quantity)?;
            require_non_negative(carrying_cost)?;
            if *cost_currency != instrument.trade_currency
                || cutover_date != &command.effective_date
                || !matches!(
                    migration_policy.as_str(),
                    "full_history" | "explicit_cutover"
                )
                || (quantity.is_zero() && !carrying_cost.is_zero())
                || !holding_is_empty(current)
            {
                return Err(DomainError::EventInvariantViolation);
            }
            let ids = (portfolio.portfolio_id, Some(instrument.instrument_id));
            let mut postings = vec![posting(
                InvestmentPostingKind::OpeningQuantity,
                None,
                ids,
                quantity.clone(),
                *cost_currency,
            )];
            if !carrying_cost.is_zero() {
                postings.push(posting(
                    InvestmentPostingKind::OpeningCost,
                    None,
                    ids,
                    carrying_cost.clone(),
                    *cost_currency,
                ));
            }
            Ok(PreparedInvestmentEvent {
                event_type: "OpeningPosition",
                postings,
                next_holding: Some(HoldingState {
                    quantity: quantity.clone(),
                    carrying_cost: carrying_cost.clone(),
                    realized_trade_pnl: Decimal::zero(DecimalUse::Internal),
                    net_dividend: Decimal::zero(DecimalUse::Internal),
                    independent_expense: Decimal::zero(DecimalUse::Internal),
                }),
                portfolio_expense: None,
            })
        }
        InvestmentEventKind::OpeningPerformance {
            portfolio,
            instrument,
            realized_trade_pnl,
            net_dividend,
            independent_expense,
            currency,
            cutover_date,
        } => {
            if cutover_date != &command.effective_date
                || instrument.is_some_and(|value| value.trade_currency != *currency)
                || realized_trade_pnl.is_negative()
                || net_dividend.is_negative()
                || independent_expense.is_negative()
                || (instrument.is_none()
                    && (!realized_trade_pnl.is_zero() || !net_dividend.is_zero()))
            {
                return Err(DomainError::EventInvariantViolation);
            }
            let ids = (
                portfolio.portfolio_id,
                instrument.as_ref().map(|value| value.instrument_id),
            );
            let mut postings = Vec::new();
            if !realized_trade_pnl.is_zero() {
                postings.push(posting(
                    InvestmentPostingKind::OpeningRealizedPnl,
                    None,
                    ids,
                    realized_trade_pnl.clone(),
                    *currency,
                ));
            }
            if !net_dividend.is_zero() {
                postings.push(posting(
                    InvestmentPostingKind::OpeningNetDividend,
                    None,
                    ids,
                    net_dividend.clone(),
                    *currency,
                ));
            }
            if !independent_expense.is_zero() {
                postings.push(posting(
                    if instrument.is_some() {
                        InvestmentPostingKind::OpeningIndependentExpense
                    } else {
                        InvestmentPostingKind::OpeningPortfolioIndependentExpense
                    },
                    None,
                    ids,
                    independent_expense.clone(),
                    *currency,
                ));
            }
            let next_holding = instrument.map(|_| {
                let mut next = current.clone();
                next.realized_trade_pnl = realized_trade_pnl.clone();
                next.net_dividend = net_dividend.clone();
                next.independent_expense = independent_expense.clone();
                next
            });
            Ok(PreparedInvestmentEvent {
                event_type: "OpeningPerformance",
                postings,
                next_holding,
                portfolio_expense: instrument.is_none().then(|| independent_expense.clone()),
            })
        }
    }
}

fn holding_is_empty(value: &HoldingState) -> bool {
    value.quantity.is_zero()
        && value.carrying_cost.is_zero()
        && value.realized_trade_pnl.is_zero()
        && value.net_dividend.is_zero()
        && value.independent_expense.is_zero()
}

fn validate_relationships(
    portfolio: PortfolioFact,
    instrument: InstrumentFact,
    settlement: InvestmentAccountFact,
    override_reason: Option<&str>,
) -> Result<(), DomainError> {
    if settlement.institution_id != portfolio.institution_id {
        return Err(DomainError::SettlementInstitutionMismatch);
    }
    if settlement.currency != instrument.trade_currency {
        return Err(DomainError::TradeCurrencyMismatch);
    }
    if settlement.account_id != portfolio.settlement_account_id
        && override_reason.is_none_or(|reason| reason.trim().is_empty())
    {
        return Err(DomainError::SettlementOverrideReasonRequired);
    }
    Ok(())
}

fn require_positive(value: &Decimal) -> Result<(), DomainError> {
    if value.is_positive() {
        Ok(())
    } else {
        Err(DomainError::AmountMustBePositive)
    }
}

fn require_non_negative(value: &Decimal) -> Result<(), DomainError> {
    if value.is_negative() {
        Err(DomainError::AmountMustBePositive)
    } else {
        Ok(())
    }
}

fn posting(
    posting_kind: InvestmentPostingKind,
    account_id: Option<UuidV7>,
    ids: (UuidV7, Option<UuidV7>),
    quantity_delta: Decimal,
    currency: Currency,
) -> InvestmentPostingDraft {
    InvestmentPostingDraft {
        posting_kind,
        account_id,
        portfolio_id: ids.0,
        instrument_id: ids.1,
        quantity_delta,
        currency,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u8) -> UuidV7 {
        UuidV7::from_parts(1_777_000_000_000 + u64::from(seed), [seed; 10]).unwrap()
    }

    fn facts() -> (PortfolioFact, InstrumentFact, InvestmentAccountFact) {
        let institution_id = id(1);
        let settlement_account_id = id(2);
        (
            PortfolioFact {
                portfolio_id: id(3),
                institution_id,
                settlement_account_id,
            },
            InstrumentFact {
                instrument_id: id(4),
                trade_currency: Currency::parse("USD").unwrap(),
            },
            InvestmentAccountFact {
                account_id: settlement_account_id,
                institution_id,
                currency: Currency::parse("USD").unwrap(),
            },
        )
    }

    fn trade(side: TradeSide, quantity: &str, price: &str, fee: &str) -> InvestmentEventCommand {
        let (portfolio, instrument, settlement) = facts();
        InvestmentEventCommand {
            effective_date: LocalDate::parse("2026-02-07").unwrap(),
            sequence: Sequence::new(1).unwrap(),
            kind: InvestmentEventKind::SecurityTrade {
                side,
                portfolio,
                instrument,
                settlement,
                quantity: Decimal::parse(quantity, DecimalUse::Quantity).unwrap(),
                unit_price: Decimal::parse(price, DecimalUse::UnitPrice).unwrap(),
                trade_fee: Decimal::parse(fee, DecimalUse::Amount).unwrap(),
                settlement_override_reason: None,
            },
        }
    }

    #[test]
    fn buy_fee_enters_cost_and_partial_sell_locks_realized_pnl() {
        let bought = prepare_investment_event(
            &trade(TradeSide::Buy, "10", "12.34", "1.60"),
            &HoldingState::zero(),
        )
        .unwrap();
        let holding = bought.next_holding.unwrap();
        assert_eq!(holding.carrying_cost.as_str(), "125.00");
        assert_eq!(bought.postings[0].quantity_delta.as_str(), "-125.00");
        let sold =
            prepare_investment_event(&trade(TradeSide::Sell, "4", "15", "1"), &holding).unwrap();
        let remaining = sold.next_holding.unwrap();
        assert_eq!(remaining.quantity.as_str(), "6");
        assert_eq!(remaining.carrying_cost.as_str(), "75.00");
        assert_eq!(remaining.realized_trade_pnl.as_str(), "9.00");
    }

    #[test]
    fn final_sale_clears_residual_and_reopen_starts_fresh() {
        let mut holding = HoldingState::zero();
        holding.quantity = Decimal::parse("3", DecimalUse::Internal).unwrap();
        holding.carrying_cost =
            Decimal::parse("30.000000000000000001", DecimalUse::Internal).unwrap();
        let partial = prepare_investment_event(&trade(TradeSide::Sell, "2", "13", "0"), &holding)
            .unwrap()
            .next_holding
            .unwrap();
        assert_eq!(partial.carrying_cost.as_str(), "10.000000000000000001");
        let closed = prepare_investment_event(&trade(TradeSide::Sell, "1", "14", "0"), &partial)
            .unwrap()
            .next_holding
            .unwrap();
        assert!(closed.quantity.is_zero());
        assert!(closed.carrying_cost.is_zero());
        assert_eq!(closed.average_cost().unwrap(), None);
        let reopened = prepare_investment_event(&trade(TradeSide::Buy, "1", "20", "0"), &closed)
            .unwrap()
            .next_holding
            .unwrap();
        assert_eq!(reopened.average_cost().unwrap().unwrap().as_str(), "20");
    }

    #[test]
    fn oversell_and_invalid_scopes_fail_without_postings() {
        assert_eq!(
            prepare_investment_event(
                &trade(TradeSide::Sell, "0.000000000001", "1", "0"),
                &HoldingState::zero()
            ),
            Err(DomainError::NegativeHoldingNotAllowed)
        );
        let (portfolio, _, settlement) = facts();
        let expense = InvestmentEventCommand {
            effective_date: LocalDate::parse("2026-02-09").unwrap(),
            sequence: Sequence::new(1).unwrap(),
            kind: InvestmentEventKind::InvestmentExpense {
                portfolio,
                instrument: None,
                settlement,
                amount: Decimal::parse("2", DecimalUse::Amount).unwrap(),
                fee_scope: FeeScope::Instrument,
                settlement_override_reason: None,
            },
        };
        assert_eq!(
            prepare_investment_event(&expense, &HoldingState::zero()),
            Err(DomainError::InstrumentRequiredForFeeScope)
        );
    }
}
