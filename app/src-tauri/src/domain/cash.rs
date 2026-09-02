#![forbid(unsafe_code)]

use super::catalog::{CategoryKind, SemanticRole};
use super::decimal::{Decimal, DecimalUse};
use super::error::DomainError;
use super::types::{Currency, LocalDate, Sequence, UuidV7};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CashAccountFact {
    pub account_id: UuidV7,
    pub currency: Currency,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CategoryFact {
    pub category_id: UuidV7,
    pub kind: CategoryKind,
    pub semantic_role: SemanticRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeInput {
    pub account: CashAccountFact,
    pub amount: Decimal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IncomeExpenseDirection {
    Income,
    Expense,
}

impl IncomeExpenseDirection {
    #[must_use]
    pub const fn event_type(self) -> &'static str {
        match self {
            Self::Income => "Income",
            Self::Expense => "Expense",
        }
    }

    #[must_use]
    pub const fn detail_type(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CashEventKind {
    OpeningBalance {
        account: CashAccountFact,
        balance: Decimal,
        cutover_date: LocalDate,
        migration_policy: String,
    },
    IncomeExpense {
        direction: IncomeExpenseDirection,
        account: CashAccountFact,
        amount: Decimal,
        category: Option<CategoryFact>,
        semantic_role: SemanticRole,
        merchant: Option<String>,
        note: Option<String>,
        fee: Option<FeeInput>,
    },
    Adjustment {
        account: CashAccountFact,
        delta: Decimal,
        note: Option<String>,
    },
    Transfer {
        from: CashAccountFact,
        to: CashAccountFact,
        amount: Decimal,
    },
    CurrencyExchange {
        from: CashAccountFact,
        to: CashAccountFact,
        from_amount: Decimal,
        to_amount: Decimal,
        fee: Option<FeeInput>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashEventCommand {
    pub effective_date: LocalDate,
    pub sequence: Sequence,
    pub kind: CashEventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CashContributionRole {
    Principal,
    Fee,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashPostingDraft {
    pub account_id: UuidV7,
    pub quantity_delta: Decimal,
    pub currency: Currency,
    pub role: CashContributionRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedCashEvent {
    pub event_type: &'static str,
    pub postings: Vec<CashPostingDraft>,
}

/// Applies the authoritative sign, transfer, exchange, fee, and category rules.
///
/// # Errors
///
/// Returns a stable domain error without producing any partial postings.
#[allow(clippy::too_many_lines)] // One exhaustive match keeps all event-sign rules together.
pub fn prepare_cash_event(command: &CashEventCommand) -> Result<PreparedCashEvent, DomainError> {
    let mut postings = Vec::new();
    let event_type = match &command.kind {
        CashEventKind::OpeningBalance {
            account, balance, ..
        } => {
            if balance.is_zero() {
                return Err(DomainError::AdjustmentZero);
            }
            postings.push(posting(
                *account,
                balance.clone(),
                CashContributionRole::Principal,
            ));
            "OpeningBalance"
        }
        CashEventKind::IncomeExpense {
            direction,
            account,
            amount,
            category,
            semantic_role,
            fee,
            ..
        } => {
            require_positive(amount)?;
            validate_category(*direction, *semantic_role, category.as_ref())?;
            let signed = match direction {
                IncomeExpenseDirection::Income => amount.clone(),
                IncomeExpenseDirection::Expense => amount.checked_neg(DecimalUse::Amount)?,
            };
            postings.push(posting(*account, signed, CashContributionRole::Principal));
            if let Some(fee) = fee {
                require_positive(&fee.amount)?;
                postings.push(posting(
                    fee.account,
                    fee.amount.checked_neg(DecimalUse::Amount)?,
                    CashContributionRole::Fee,
                ));
            }
            direction.event_type()
        }
        CashEventKind::Adjustment { account, delta, .. } => {
            if delta.is_zero() {
                return Err(DomainError::AdjustmentZero);
            }
            postings.push(posting(
                *account,
                delta.clone(),
                CashContributionRole::Principal,
            ));
            "BalanceAdjustment"
        }
        CashEventKind::Transfer { from, to, amount } => {
            require_positive(amount)?;
            if from.account_id == to.account_id {
                return Err(DomainError::TransferAccountSame);
            }
            if from.currency != to.currency {
                return Err(DomainError::TransferCurrencyMismatch);
            }
            postings.push(posting(
                *from,
                amount.checked_neg(DecimalUse::Amount)?,
                CashContributionRole::Principal,
            ));
            postings.push(posting(
                *to,
                amount.clone(),
                CashContributionRole::Principal,
            ));
            "Transfer"
        }
        CashEventKind::CurrencyExchange {
            from,
            to,
            from_amount,
            to_amount,
            fee,
        } => {
            require_positive(from_amount)?;
            require_positive(to_amount)?;
            if from.account_id == to.account_id {
                return Err(DomainError::TransferAccountSame);
            }
            if from.currency == to.currency {
                return Err(DomainError::ExchangeCurrencySame);
            }
            postings.push(posting(
                *from,
                from_amount.checked_neg(DecimalUse::Amount)?,
                CashContributionRole::Principal,
            ));
            postings.push(posting(
                *to,
                to_amount.clone(),
                CashContributionRole::Principal,
            ));
            if let Some(fee) = fee {
                require_positive(&fee.amount)?;
                postings.push(posting(
                    fee.account,
                    fee.amount.checked_neg(DecimalUse::Amount)?,
                    CashContributionRole::Fee,
                ));
            }
            "CurrencyExchange"
        }
    };
    Ok(PreparedCashEvent {
        event_type,
        postings,
    })
}

fn validate_category(
    direction: IncomeExpenseDirection,
    role: SemanticRole,
    category: Option<&CategoryFact>,
) -> Result<(), DomainError> {
    if direction == IncomeExpenseDirection::Expense && role != SemanticRole::Normal {
        return Err(DomainError::CategoryDirectionMismatch);
    }
    if let Some(category) = category {
        let expected = match direction {
            IncomeExpenseDirection::Income => CategoryKind::Income,
            IncomeExpenseDirection::Expense => CategoryKind::Expense,
        };
        if category.kind != expected || category.semantic_role != role {
            return Err(DomainError::CategoryDirectionMismatch);
        }
    } else if role != SemanticRole::Normal {
        return Err(DomainError::CategoryDirectionMismatch);
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

fn posting(
    account: CashAccountFact,
    quantity_delta: Decimal,
    role: CashContributionRole,
) -> CashPostingDraft {
    CashPostingDraft {
        account_id: account.account_id,
        quantity_delta,
        currency: account.currency,
        role,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u8) -> UuidV7 {
        UuidV7::from_parts(1_777_000_000_000 + u64::from(seed), [seed; 10]).unwrap()
    }

    fn account(seed: u8, currency: &str) -> CashAccountFact {
        CashAccountFact {
            account_id: id(seed),
            currency: Currency::parse(currency).unwrap(),
        }
    }

    fn base(kind: CashEventKind) -> CashEventCommand {
        CashEventCommand {
            effective_date: LocalDate::parse("2026-02-03").unwrap(),
            sequence: Sequence::new(1).unwrap(),
            kind,
        }
    }

    #[test]
    fn income_expense_and_fee_signs_are_derived() {
        let event = prepare_cash_event(&base(CashEventKind::IncomeExpense {
            direction: IncomeExpenseDirection::Expense,
            account: account(1, "CNY"),
            amount: Decimal::parse("25.50", DecimalUse::Amount).unwrap(),
            category: None,
            semantic_role: SemanticRole::Normal,
            merchant: None,
            note: None,
            fee: Some(FeeInput {
                account: account(1, "CNY"),
                amount: Decimal::parse("2.00", DecimalUse::Amount).unwrap(),
            }),
        }))
        .unwrap();
        assert_eq!(event.postings[0].quantity_delta.as_str(), "-25.50");
        assert_eq!(event.postings[1].quantity_delta.as_str(), "-2.00");
    }

    #[test]
    fn transfer_and_exchange_boundaries_are_authoritative() {
        assert_eq!(
            prepare_cash_event(&base(CashEventKind::Transfer {
                from: account(1, "CNY"),
                to: account(2, "USD"),
                amount: Decimal::parse("1", DecimalUse::Amount).unwrap(),
            })),
            Err(DomainError::TransferCurrencyMismatch)
        );
        let exchange = prepare_cash_event(&base(CashEventKind::CurrencyExchange {
            from: account(1, "CNY"),
            to: account(2, "USD"),
            from_amount: Decimal::parse("710", DecimalUse::Amount).unwrap(),
            to_amount: Decimal::parse("100", DecimalUse::Amount).unwrap(),
            fee: None,
        }))
        .unwrap();
        assert_eq!(exchange.postings[0].quantity_delta.as_str(), "-710");
        assert_eq!(exchange.postings[1].quantity_delta.as_str(), "100");
    }
}
