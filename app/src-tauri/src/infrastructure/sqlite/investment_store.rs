#![forbid(unsafe_code)]

use std::cmp::Ordering;
use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::application::cash::FxOverrideInput;
use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::investment::{
    HoldingPosition, InvestmentEventInput, InvestmentEventPreview, InvestmentEventType,
    InvestmentPort, InvestmentPostingPreview, InvestmentRevisionInput, InvestmentWorkspace,
    PortfolioExpenseSummary, PostedInvestmentEvent,
};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::error::DomainError;
use crate::domain::investment::{
    FeeScope, HoldingState, InstrumentFact, InvestmentAccountFact, InvestmentEventCommand,
    InvestmentEventKind, PortfolioFact, PreparedInvestmentEvent, TradeSide,
    prepare_investment_event,
};
use crate::domain::types::{Currency, LocalDate, Sequence, UuidV7};

use super::cash_store::rebuild_cash_derived;
use super::store::{CALCULATION_VERSION, LedgerStore, SqliteLedgerManager, map_sqlite_error};

pub const HOLDING_PROJECTION_VERSION: &str = "holding-projection-v1";

impl InvestmentPort for SqliteLedgerManager {
    fn preview_investment_event(
        &self,
        input: &InvestmentEventInput,
    ) -> ApplicationResult<InvestmentEventPreview> {
        self.open_store()?.preview_investment(input)
    }

    fn post_investment_event(
        &mut self,
        input: &InvestmentEventInput,
    ) -> ApplicationResult<PostedInvestmentEvent> {
        self.open_store_mut()?.post_investment(input, None, 1, None)
    }

    fn revise_investment_event(
        &mut self,
        input: &InvestmentRevisionInput,
    ) -> ApplicationResult<PostedInvestmentEvent> {
        self.open_store_mut()?.revise_investment(input)
    }

    fn get_investment_workspace(
        &self,
        as_of_date: &LocalDate,
    ) -> ApplicationResult<InvestmentWorkspace> {
        self.open_store()?.investment_workspace(as_of_date)
    }
}

#[derive(Clone)]
struct StoredInvestmentEvent {
    event_id: String,
    effective_date: LocalDate,
    sequence: Sequence,
    event_type: String,
    trade_type: Option<String>,
    portfolio_id: String,
    instrument_id: Option<String>,
    settlement_account_id: String,
    quantity: Option<String>,
    unit_price: Option<String>,
    trade_fee: Option<String>,
    settlement_override_reason: Option<String>,
    gross_cash_amount: Option<String>,
    withholding_tax: Option<String>,
    dividend_fee: Option<String>,
    expense_amount: Option<String>,
    fee_scope: Option<String>,
}

struct ReplayOutcome {
    holdings: BTreeMap<(String, String), HoldingState>,
    last_dates: BTreeMap<(String, String), String>,
    portfolio_expenses: BTreeMap<(String, String), Decimal>,
}

impl LedgerStore {
    fn preview_investment(
        &self,
        input: &InvestmentEventInput,
    ) -> ApplicationResult<InvestmentEventPreview> {
        let command = command_from_input(&self.connection, input)?;
        let key = command_key(&command);
        let replay = replay_events(
            &self.connection,
            Some((&input.effective_date, input.sequence)),
        )?;
        let current = key
            .as_ref()
            .and_then(|value| replay.holdings.get(value))
            .cloned()
            .unwrap_or_else(HoldingState::zero);
        let prepared = prepare_investment_event(&command, &current)?;
        build_preview(&self.connection, input, &prepared)
    }

    fn post_investment(
        &mut self,
        input: &InvestmentEventInput,
        supersedes: Option<UuidV7>,
        revision: u32,
        reason: Option<&str>,
    ) -> ApplicationResult<PostedInvestmentEvent> {
        let preview = self.preview_investment(input)?;
        let command = command_from_input(&self.connection, input)?;
        let event_id = UuidV7::new()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .execute(
                "INSERT INTO business_events(event_id,event_type,effective_date,sequence,status,revision,supersedes_event_id,revision_reason,created_at_utc,calculation_version)
                 VALUES(?1,?2,?3,?4,'posted',?5,?6,?7,CURRENT_TIMESTAMP,?8)",
                params![
                    event_id.to_string(),
                    stored_event_type(input.event_type),
                    input.effective_date.as_str(),
                    to_i64(input.sequence.get())?,
                    revision,
                    supersedes.map(|value| value.to_string()),
                    reason,
                    CALCULATION_VERSION
                ],
            )
            .map_err(map_sqlite_error)?;
        insert_investment_detail(&transaction, event_id, input)?;
        save_event_fx_resolution(&transaction, event_id, input, &command)?;
        insert_investment_audit(
            &transaction,
            event_id,
            if supersedes.is_some() {
                "revise"
            } else {
                "post"
            },
            revision,
            reason,
        )?;
        let watermark = event_watermark(&transaction, event_id)?;
        rebuild_investment_derived(&transaction, watermark)?;
        rebuild_cash_derived(&transaction, watermark)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(PostedInvestmentEvent {
            event_id: event_id.to_string(),
            event_watermark: watermark,
            revision,
            preview,
        })
    }

    fn revise_investment(
        &mut self,
        input: &InvestmentRevisionInput,
    ) -> ApplicationResult<PostedInvestmentEvent> {
        if input.reason.trim().is_empty() {
            return Err(DomainError::RevisionReasonRequired.into());
        }
        let target = input.target_event_id.to_string();
        let revision: Option<u32> = self
            .connection
            .query_row(
                "SELECT revision FROM business_events e WHERE event_id=?1 AND status='posted'
                 AND e.event_type IN ('SecurityTrade','Dividend','InvestmentExpense')
                 AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id)
                 AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id)",
                [&target],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let revision = revision.ok_or(DomainError::RevisionTargetNotEffective)?;
        self.post_investment(
            &input.replacement,
            Some(input.target_event_id),
            revision.saturating_add(1),
            Some(&input.reason),
        )
    }

    fn investment_workspace(
        &self,
        as_of_date: &LocalDate,
    ) -> ApplicationResult<InvestmentWorkspace> {
        let replay = replay_events(
            &self.connection,
            Some((as_of_date, Sequence::new(u64::MAX >> 12)?)),
        )?;
        let base_currency = self.base_currency()?;
        let mut holdings = Vec::with_capacity(replay.holdings.len());
        for ((portfolio_id, instrument_id), state) in replay.holdings {
            holdings.push(self.value_holding(as_of_date, &portfolio_id, &instrument_id, &state)?);
        }
        let mut portfolio_expenses = Vec::with_capacity(replay.portfolio_expenses.len());
        for ((portfolio_id, currency), amount) in replay.portfolio_expenses {
            let name = load_name(
                &self.connection,
                "portfolios",
                "portfolio_id",
                &portfolio_id,
            )?;
            portfolio_expenses.push(PortfolioExpenseSummary {
                portfolio_id,
                portfolio_name: name,
                amount: amount.as_str().to_owned(),
                currency,
            });
        }
        let event_watermark = current_event_watermark(&self.connection)?;
        Ok(InvestmentWorkspace {
            as_of_date: as_of_date.as_str().to_owned(),
            base_currency: base_currency.to_string(),
            holdings,
            portfolio_expenses,
            event_watermark,
            projection_version: HOLDING_PROJECTION_VERSION,
            calculation_version: CALCULATION_VERSION,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn value_holding(
        &self,
        as_of_date: &LocalDate,
        portfolio_id: &str,
        instrument_id: &str,
        state: &HoldingState,
    ) -> ApplicationResult<HoldingPosition> {
        let portfolio_name =
            load_name(&self.connection, "portfolios", "portfolio_id", portfolio_id)?;
        let instrument_name = load_name(
            &self.connection,
            "security_instruments",
            "instrument_id",
            instrument_id,
        )?;
        let instrument_uuid = UuidV7::parse(instrument_id)?;
        let currency_text: String = self
            .connection
            .query_row(
                "SELECT trade_currency FROM security_instruments WHERE instrument_id=?1",
                [instrument_id],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::CatalogEntityNotFound)?;
        let currency = Currency::parse(&currency_text)?;
        let average_cost = state.average_cost()?.map(|value| value.as_str().to_owned());
        if state.quantity.is_zero() {
            let zero = Decimal::zero(DecimalUse::Internal);
            let total = investment_total_return(state, &zero)?;
            return Ok(HoldingPosition {
                portfolio_id: portfolio_id.to_owned(),
                portfolio_name,
                instrument_id: instrument_id.to_owned(),
                instrument_name,
                currency: currency_text,
                as_of_date: as_of_date.as_str().to_owned(),
                quantity: state.quantity.as_str().to_owned(),
                carrying_cost: state.carrying_cost.as_str().to_owned(),
                average_cost,
                realized_trade_pnl: state.realized_trade_pnl.as_str().to_owned(),
                net_dividend: state.net_dividend.as_str().to_owned(),
                independent_expense: state.independent_expense.as_str().to_owned(),
                market_price: None,
                price_revision_id: None,
                price_date: None,
                price_age_days: None,
                market_value: Some("0".to_owned()),
                fx_rate: Some("1".to_owned()),
                fx_revision_id: None,
                base_market_value: Some("0".to_owned()),
                unrealized_pnl: Some("0".to_owned()),
                total_return: Some(total.as_str().to_owned()),
                valuation_state: "valued",
                unvalued_reason: None,
                warning_codes: Vec::new(),
            });
        }
        let Some(price) = self.resolve_price(instrument_uuid, as_of_date)? else {
            return Ok(unvalued_holding(
                portfolio_id,
                portfolio_name,
                instrument_id,
                instrument_name,
                currency_text,
                as_of_date,
                state,
                average_cost,
                "PRICE_MISSING_AS_OF",
            ));
        };
        let market_value = state.quantity.checked_mul_internal(&price.value)?;
        let Some(fx) = self.resolve_fx_rate(currency, as_of_date)? else {
            let mut value = unvalued_holding(
                portfolio_id,
                portfolio_name,
                instrument_id,
                instrument_name,
                currency_text,
                as_of_date,
                state,
                average_cost,
                "FX_MISSING_AS_OF",
            );
            value.market_price = Some(price.value.as_str().to_owned());
            value.price_revision_id = price.revision_id;
            value.price_date = Some(price.source_date.as_str().to_owned());
            value.market_value = Some(market_value.as_str().to_owned());
            return Ok(value);
        };
        let age = date_age_days(&self.connection, as_of_date, &price.source_date)?;
        let unrealized = market_value.checked_add(
            &state.carrying_cost.checked_neg(DecimalUse::Internal)?,
            DecimalUse::Internal,
        )?;
        let total = investment_total_return(state, &unrealized)?;
        let base_market_value = market_value.checked_mul_internal(&fx.value)?;
        Ok(HoldingPosition {
            portfolio_id: portfolio_id.to_owned(),
            portfolio_name,
            instrument_id: instrument_id.to_owned(),
            instrument_name,
            currency: currency_text,
            as_of_date: as_of_date.as_str().to_owned(),
            quantity: state.quantity.as_str().to_owned(),
            carrying_cost: state.carrying_cost.as_str().to_owned(),
            average_cost,
            realized_trade_pnl: state.realized_trade_pnl.as_str().to_owned(),
            net_dividend: state.net_dividend.as_str().to_owned(),
            independent_expense: state.independent_expense.as_str().to_owned(),
            market_price: Some(price.value.as_str().to_owned()),
            price_revision_id: price.revision_id,
            price_date: Some(price.source_date.as_str().to_owned()),
            price_age_days: Some(age),
            market_value: Some(market_value.as_str().to_owned()),
            fx_rate: Some(fx.value.as_str().to_owned()),
            fx_revision_id: fx.revision_id,
            base_market_value: Some(base_market_value.as_str().to_owned()),
            unrealized_pnl: Some(unrealized.as_str().to_owned()),
            total_return: Some(total.as_str().to_owned()),
            valuation_state: "valued",
            unvalued_reason: None,
            warning_codes: if age > 7 {
                vec!["STALE_PRICE"]
            } else {
                Vec::new()
            },
        })
    }
}

fn build_preview(
    connection: &Connection,
    input: &InvestmentEventInput,
    prepared: &PreparedInvestmentEvent,
) -> ApplicationResult<InvestmentEventPreview> {
    let base_currency = load_base_currency(connection)?;
    let currency = command_currency(connection, input)?;
    let rate = resolve_preview_rate(
        connection,
        currency,
        base_currency,
        &input.effective_date,
        &input.fx_overrides,
    )?;
    let mut quality_issue_codes = Vec::new();
    if rate.is_none() {
        quality_issue_codes.push("MISSING_FX_RATE");
    }
    let postings = prepared
        .postings
        .iter()
        .map(|posting| {
            let base_value = if posting.posting_kind.as_str() == "settlement-cash" {
                rate.as_ref()
                    .map(|value| posting.quantity_delta.checked_mul_internal(value))
                    .transpose()?
            } else {
                None
            };
            Ok(InvestmentPostingPreview {
                posting_kind: posting.posting_kind.as_str(),
                account_id: posting.account_id.map(|value| value.to_string()),
                portfolio_id: posting.portfolio_id.to_string(),
                instrument_id: posting.instrument_id.map(|value| value.to_string()),
                quantity_delta: posting.quantity_delta.as_str().to_owned(),
                currency: posting.currency.to_string(),
                base_value: base_value.map(|value| value.as_str().to_owned()),
                base_currency: base_currency.to_string(),
            })
        })
        .collect::<ApplicationResult<Vec<_>>>()?;
    let holding = prepared.next_holding.as_ref();
    Ok(InvestmentEventPreview {
        event_type: input.event_type.as_str(),
        effective_date: input.effective_date.as_str().to_owned(),
        sequence: input.sequence.get(),
        postings,
        quantity_after: holding.map(|value| value.quantity.as_str().to_owned()),
        carrying_cost_after: holding.map(|value| value.carrying_cost.as_str().to_owned()),
        average_cost_after: holding
            .map(HoldingState::average_cost)
            .transpose()?
            .flatten()
            .map(|value| value.as_str().to_owned()),
        realized_trade_pnl_after: holding.map(|value| value.realized_trade_pnl.as_str().to_owned()),
        quality_issue_codes,
    })
}

fn command_from_input(
    connection: &Connection,
    input: &InvestmentEventInput,
) -> ApplicationResult<InvestmentEventCommand> {
    let portfolio = load_portfolio(connection, input.portfolio_id)?;
    let settlement = load_investment_account(connection, input.settlement_account_id)?;
    let instrument = input
        .instrument_id
        .map(|value| load_instrument(connection, value))
        .transpose()?;
    let zero = || Decimal::zero(DecimalUse::Amount);
    let kind = match input.event_type {
        InvestmentEventType::SecurityBuy | InvestmentEventType::SecuritySell => {
            InvestmentEventKind::SecurityTrade {
                side: if input.event_type == InvestmentEventType::SecurityBuy {
                    TradeSide::Buy
                } else {
                    TradeSide::Sell
                },
                portfolio,
                instrument: instrument.ok_or(DomainError::InstrumentRequiredForFeeScope)?,
                settlement,
                quantity: input
                    .quantity
                    .clone()
                    .ok_or(DomainError::EventInvariantViolation)?,
                unit_price: input
                    .unit_price
                    .clone()
                    .ok_or(DomainError::EventInvariantViolation)?,
                trade_fee: input.trade_fee.clone().unwrap_or_else(zero),
                settlement_override_reason: input.settlement_override_reason.clone(),
            }
        }
        InvestmentEventType::Dividend => InvestmentEventKind::Dividend {
            portfolio,
            instrument: instrument.ok_or(DomainError::InstrumentRequiredForFeeScope)?,
            settlement,
            gross_cash_amount: input
                .gross_cash_amount
                .clone()
                .ok_or(DomainError::EventInvariantViolation)?,
            withholding_tax: input.withholding_tax.clone().unwrap_or_else(zero),
            fee_amount: input.fee_amount.clone().unwrap_or_else(zero),
            settlement_override_reason: input.settlement_override_reason.clone(),
        },
        InvestmentEventType::InvestmentExpense => InvestmentEventKind::InvestmentExpense {
            portfolio,
            instrument,
            settlement,
            amount: input
                .amount
                .clone()
                .ok_or(DomainError::EventInvariantViolation)?,
            fee_scope: input
                .fee_scope
                .ok_or(DomainError::EventInvariantViolation)?,
            settlement_override_reason: input.settlement_override_reason.clone(),
        },
    };
    Ok(InvestmentEventCommand {
        effective_date: input.effective_date.clone(),
        sequence: input.sequence,
        kind,
    })
}

fn command_from_stored(
    connection: &Connection,
    stored: &StoredInvestmentEvent,
) -> ApplicationResult<InvestmentEventCommand> {
    let event_type = match (stored.event_type.as_str(), stored.trade_type.as_deref()) {
        ("SecurityTrade", Some("BUY")) => InvestmentEventType::SecurityBuy,
        ("SecurityTrade", Some("SELL")) => InvestmentEventType::SecuritySell,
        ("Dividend", _) => InvestmentEventType::Dividend,
        ("InvestmentExpense", _) => InvestmentEventType::InvestmentExpense,
        _ => return Err(DomainError::EventInvariantViolation.into()),
    };
    let fee_scope = stored
        .fee_scope
        .as_deref()
        .map(|value| match value {
            "instrument" => Ok(FeeScope::Instrument),
            "portfolio" => Ok(FeeScope::Portfolio),
            _ => Err(DomainError::EventInvariantViolation),
        })
        .transpose()?;
    command_from_input(
        connection,
        &InvestmentEventInput {
            effective_date: stored.effective_date.clone(),
            sequence: stored.sequence,
            event_type,
            portfolio_id: UuidV7::parse(&stored.portfolio_id)?,
            instrument_id: stored
                .instrument_id
                .as_deref()
                .map(UuidV7::parse)
                .transpose()?,
            settlement_account_id: UuidV7::parse(&stored.settlement_account_id)?,
            quantity: parse_optional(stored.quantity.as_ref(), DecimalUse::Quantity)?,
            unit_price: parse_optional(stored.unit_price.as_ref(), DecimalUse::UnitPrice)?,
            trade_fee: parse_optional(stored.trade_fee.as_ref(), DecimalUse::Amount)?,
            gross_cash_amount: parse_optional(
                stored.gross_cash_amount.as_ref(),
                DecimalUse::Amount,
            )?,
            withholding_tax: parse_optional(stored.withholding_tax.as_ref(), DecimalUse::Amount)?,
            fee_amount: parse_optional(stored.dividend_fee.as_ref(), DecimalUse::Amount)?,
            amount: parse_optional(stored.expense_amount.as_ref(), DecimalUse::Amount)?,
            fee_scope,
            settlement_override_reason: stored.settlement_override_reason.clone(),
            fx_overrides: Vec::new(),
        },
    )
}

fn load_effective_events(connection: &Connection) -> ApplicationResult<Vec<StoredInvestmentEvent>> {
    let mut statement = connection
        .prepare(
            "SELECT e.event_id,e.event_type,e.effective_date,e.sequence,
                    st.trade_type,COALESCE(st.portfolio_id,dv.portfolio_id,ie.portfolio_id),
                    COALESCE(st.instrument_id,dv.instrument_id,ie.instrument_id),
                    COALESCE(st.settlement_account_id,dv.settlement_account_id,ie.settlement_account_id),
                    st.quantity,st.unit_price,st.trade_fee,
                    COALESCE(st.settlement_override_reason,dv.settlement_override_reason,ie.settlement_override_reason),
                    dv.gross_cash_amount,dv.withholding_tax,dv.fee_amount,ie.amount,ie.fee_scope
             FROM business_events e
             LEFT JOIN security_trade_details st ON st.event_id=e.event_id
             LEFT JOIN dividend_details dv ON dv.event_id=e.event_id
             LEFT JOIN investment_expense_details ie ON ie.event_id=e.event_id
             WHERE e.status='posted' AND e.event_type IN ('SecurityTrade','Dividend','InvestmentExpense')
               AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id)
               AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id)
             ORDER BY e.effective_date,e.sequence,e.event_id",
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, Option<String>>(16)?,
            ))
        })
        .map_err(|_| ApplicationError::TransactionFailed)?
        .map(|row| {
            let row = row.map_err(|_| ApplicationError::TransactionFailed)?;
            Ok(StoredInvestmentEvent {
                event_id: row.0,
                event_type: row.1,
                effective_date: LocalDate::parse(&row.2)?,
                sequence: Sequence::new(
                    u64::try_from(row.3).map_err(|_| ApplicationError::TransactionFailed)?,
                )?,
                trade_type: row.4,
                portfolio_id: row.5,
                instrument_id: row.6,
                settlement_account_id: row.7,
                quantity: row.8,
                unit_price: row.9,
                trade_fee: row.10,
                settlement_override_reason: row.11,
                gross_cash_amount: row.12,
                withholding_tax: row.13,
                dividend_fee: row.14,
                expense_amount: row.15,
                fee_scope: row.16,
            })
        })
        .collect()
}

fn replay_events(
    connection: &Connection,
    before_or_at: Option<(&LocalDate, Sequence)>,
) -> ApplicationResult<ReplayOutcome> {
    let mut outcome = ReplayOutcome {
        holdings: BTreeMap::new(),
        last_dates: BTreeMap::new(),
        portfolio_expenses: BTreeMap::new(),
    };
    for stored in load_effective_events(connection)? {
        if before_or_at.is_some_and(|(date, sequence)| {
            stored.effective_date > *date
                || stored.effective_date == *date
                    && stored.sequence.numeric_cmp(sequence) != Ordering::Less
        }) {
            continue;
        }
        let command = command_from_stored(connection, &stored)?;
        let key = command_key(&command);
        let current = key
            .as_ref()
            .and_then(|value| outcome.holdings.get(value))
            .cloned()
            .unwrap_or_else(HoldingState::zero);
        let prepared = prepare_investment_event(&command, &current)?;
        if let (Some(key), Some(next)) = (key, prepared.next_holding) {
            outcome
                .last_dates
                .insert(key.clone(), stored.effective_date.as_str().to_owned());
            outcome.holdings.insert(key, next);
        }
        if let Some(expense) = prepared.portfolio_expense {
            let key = (
                stored.portfolio_id,
                command_currency_from_domain(&command).to_string(),
            );
            let current = outcome
                .portfolio_expenses
                .get(&key)
                .cloned()
                .unwrap_or_else(|| Decimal::zero(DecimalUse::Internal));
            outcome
                .portfolio_expenses
                .insert(key, current.checked_add(&expense, DecimalUse::Internal)?);
        }
    }
    Ok(outcome)
}

trait SequenceCompare {
    fn numeric_cmp(self, other: Self) -> Ordering;
}

impl SequenceCompare for Sequence {
    fn numeric_cmp(self, other: Self) -> Ordering {
        self.get().cmp(&other.get())
    }
}

pub(super) fn rebuild_investment_derived(
    transaction: &Transaction<'_>,
    watermark: u64,
) -> ApplicationResult<()> {
    transaction
        .execute("DELETE FROM holding_projection", [])
        .map_err(map_sqlite_error)?;
    transaction
        .execute(
            "DELETE FROM ledger_postings WHERE event_id IN (
               SELECT e.event_id FROM business_events e
               WHERE e.event_type IN ('SecurityTrade','Dividend','InvestmentExpense')
                 AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id)
                 AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id)
             )",
            [],
        )
        .map_err(map_sqlite_error)?;
    let events = load_effective_events(transaction)?;
    let mut holdings = BTreeMap::<(String, String), HoldingState>::new();
    let mut last_dates = BTreeMap::<(String, String), String>::new();
    let base_currency = load_base_currency(transaction)?;
    for stored in events {
        let command = command_from_stored(transaction, &stored)?;
        let key = command_key(&command);
        let current = key
            .as_ref()
            .and_then(|value| holdings.get(value))
            .cloned()
            .unwrap_or_else(HoldingState::zero);
        let prepared = prepare_investment_event(&command, &current)?;
        let rate = resolved_event_rate(
            transaction,
            &stored.event_id,
            command_currency_from_domain(&command),
            base_currency,
            &stored.effective_date,
        )?;
        for (index, posting) in prepared.postings.iter().enumerate() {
            let base_value = if posting.posting_kind.as_str() == "settlement-cash" {
                rate.as_ref()
                    .map(|value| posting.quantity_delta.checked_mul_internal(value))
                    .transpose()?
            } else {
                None
            };
            transaction
                .execute(
                    "INSERT INTO ledger_postings(posting_id,event_id,posting_ordinal,posting_kind,account_id,portfolio_id,instrument_id,quantity_delta,currency,base_value,base_currency,calculation_version)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                    params![
                        UuidV7::new()?.to_string(),
                        stored.event_id,
                        index_i64(index)?,
                        posting.posting_kind.as_str(),
                        posting.account_id.map(|value| value.to_string()),
                        posting.portfolio_id.to_string(),
                        posting.instrument_id.map(|value| value.to_string()),
                        posting.quantity_delta.as_str(),
                        posting.currency.as_str(),
                        base_value.as_ref().map(Decimal::as_str),
                        base_currency.as_str(),
                        CALCULATION_VERSION
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
        if let (Some(key), Some(next)) = (key, prepared.next_holding) {
            last_dates.insert(key.clone(), stored.effective_date.as_str().to_owned());
            holdings.insert(key, next);
        }
    }
    let wm = to_i64(watermark)?;
    for ((portfolio_id, instrument_id), state) in holdings {
        transaction
            .execute(
                "INSERT INTO holding_projection(portfolio_id,instrument_id,as_of_date,quantity,carrying_cost,realized_trade_pnl,net_dividend,independent_expense,unrealized_pnl,event_watermark,projection_version,calculation_version)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL,?9,?10,?11)",
                params![
                    portfolio_id,
                    instrument_id,
                    last_dates[&(portfolio_id.clone(), instrument_id.clone())],
                    state.quantity.as_str(),
                    state.carrying_cost.as_str(),
                    state.realized_trade_pnl.as_str(),
                    state.net_dividend.as_str(),
                    state.independent_expense.as_str(),
                    wm,
                    HOLDING_PROJECTION_VERSION,
                    CALCULATION_VERSION
                ],
            )
            .map_err(map_sqlite_error)?;
    }
    transaction
        .execute(
            "UPDATE projection_metadata SET event_watermark=?1,projection_version=?2,calculation_version=?3,available=1,rebuilt_at_utc=CURRENT_TIMESTAMP WHERE projection_name='holdings'",
            params![wm, HOLDING_PROJECTION_VERSION, CALCULATION_VERSION],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn insert_investment_detail(
    transaction: &Transaction<'_>,
    event_id: UuidV7,
    input: &InvestmentEventInput,
) -> ApplicationResult<()> {
    match input.event_type {
        InvestmentEventType::SecurityBuy | InvestmentEventType::SecuritySell => {
            transaction.execute(
                "INSERT INTO security_trade_details(event_id,trade_type,portfolio_id,instrument_id,settlement_account_id,quantity,unit_price,trade_fee,settlement_override_reason)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![event_id.to_string(), if input.event_type == InvestmentEventType::SecurityBuy { "BUY" } else { "SELL" }, input.portfolio_id.to_string(), input.instrument_id.map(|value| value.to_string()), input.settlement_account_id.to_string(), input.quantity.as_ref().map(Decimal::as_str), input.unit_price.as_ref().map(Decimal::as_str), input.trade_fee.as_ref().map_or("0", Decimal::as_str), input.settlement_override_reason],
            ).map_err(map_sqlite_error)?;
        }
        InvestmentEventType::Dividend => {
            transaction.execute(
                "INSERT INTO dividend_details(event_id,portfolio_id,instrument_id,settlement_account_id,gross_cash_amount,withholding_tax,fee_amount,settlement_override_reason) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![event_id.to_string(), input.portfolio_id.to_string(), input.instrument_id.map(|value| value.to_string()), input.settlement_account_id.to_string(), input.gross_cash_amount.as_ref().map(Decimal::as_str), input.withholding_tax.as_ref().map_or("0", Decimal::as_str), input.fee_amount.as_ref().map_or("0", Decimal::as_str), input.settlement_override_reason],
            ).map_err(map_sqlite_error)?;
        }
        InvestmentEventType::InvestmentExpense => {
            transaction.execute(
                "INSERT INTO investment_expense_details(event_id,portfolio_id,instrument_id,settlement_account_id,amount,fee_scope,settlement_override_reason) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![event_id.to_string(), input.portfolio_id.to_string(), input.instrument_id.map(|value| value.to_string()), input.settlement_account_id.to_string(), input.amount.as_ref().map(Decimal::as_str), input.fee_scope.map(FeeScope::as_str), input.settlement_override_reason],
            ).map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}

fn save_event_fx_resolution(
    transaction: &Transaction<'_>,
    event_id: UuidV7,
    input: &InvestmentEventInput,
    command: &InvestmentEventCommand,
) -> ApplicationResult<()> {
    let base_currency = load_base_currency(transaction)?;
    let currency = command_currency_from_domain(command);
    let override_input = input
        .fx_overrides
        .iter()
        .find(|value| value.currency == currency);
    if override_input.is_some_and(|value| value.reason.trim().is_empty()) {
        return Err(DomainError::FxOverrideReasonRequired.into());
    }
    let automatic: Option<(String, String)> = if currency == base_currency {
        None
    } else {
        transaction.query_row(
            "SELECT fx_rate_revision_id,rate_to_base FROM fx_rate_revisions WHERE currency=?1 AND base_currency=?2 AND active=1 AND rate_date<=?3 ORDER BY rate_date DESC,revision DESC,fx_rate_revision_id LIMIT 1",
            params![currency.as_str(), base_currency.as_str(), input.effective_date.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(|_| ApplicationError::TransactionFailed)?
    };
    let final_rate = override_input
        .map(|value| value.value.as_str().to_owned())
        .or_else(|| automatic.as_ref().map(|value| value.1.clone()))
        .or_else(|| (currency == base_currency).then(|| "1".to_owned()));
    if let Some(final_rate) = final_rate {
        transaction.execute(
            "INSERT INTO fx_resolutions(fx_resolution_id,owner_type,owner_id,purpose,target_date,currency,base_currency,auto_rate_revision_id,override_value,override_reason,final_rate,calculation_version,created_at_utc)
             VALUES(?1,'event',?2,'transaction',?3,?4,?5,?6,?7,?8,?9,?10,CURRENT_TIMESTAMP)",
            params![UuidV7::new()?.to_string(),event_id.to_string(),input.effective_date.as_str(),currency.as_str(),base_currency.as_str(),automatic.as_ref().map(|value| value.0.as_str()),override_input.map(|value| value.value.as_str()),override_input.map(|value| value.reason.as_str()),final_rate,CALCULATION_VERSION],
        ).map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn resolved_event_rate(
    connection: &Connection,
    event_id: &str,
    currency: Currency,
    base_currency: Currency,
    date: &LocalDate,
) -> ApplicationResult<Option<Decimal>> {
    let frozen: Option<String> = connection
        .query_row(
            "SELECT final_rate FROM fx_resolutions WHERE owner_type='event' AND owner_id=?1 AND purpose='transaction' AND currency=?2 AND base_currency=?3",
            params![event_id, currency.as_str(), base_currency.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    if let Some(value) = frozen {
        return Decimal::parse(&value, DecimalUse::FxRate)
            .map(Some)
            .map_err(Into::into);
    }
    resolve_preview_rate(connection, currency, base_currency, date, &[])
}

fn resolve_preview_rate(
    connection: &Connection,
    currency: Currency,
    base_currency: Currency,
    date: &LocalDate,
    overrides: &[FxOverrideInput],
) -> ApplicationResult<Option<Decimal>> {
    if let Some(value) = overrides.iter().find(|value| value.currency == currency) {
        if value.reason.trim().is_empty() {
            return Err(DomainError::FxOverrideReasonRequired.into());
        }
        return Ok(Some(value.value.clone()));
    }
    if currency == base_currency {
        return Ok(Some(Decimal::parse("1", DecimalUse::FxRate)?));
    }
    connection
        .query_row(
            "SELECT rate_to_base FROM fx_rate_revisions WHERE currency=?1 AND base_currency=?2 AND active=1 AND rate_date<=?3 ORDER BY rate_date DESC,revision DESC,fx_rate_revision_id LIMIT 1",
            params![currency.as_str(), base_currency.as_str(), date.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| ApplicationError::TransactionFailed)?
        .map(|value| Decimal::parse(&value, DecimalUse::FxRate).map_err(Into::into))
        .transpose()
}

fn load_portfolio(connection: &Connection, id: UuidV7) -> ApplicationResult<PortfolioFact> {
    connection
        .query_row(
            "SELECT institution_id,settlement_account_id FROM portfolios WHERE portfolio_id=?1",
            [id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| ApplicationError::TransactionFailed)?
        .map(|(institution, settlement)| {
            Ok::<PortfolioFact, ApplicationError>(PortfolioFact {
                portfolio_id: id,
                institution_id: UuidV7::parse(&institution)?,
                settlement_account_id: UuidV7::parse(&settlement)?,
            })
        })
        .transpose()?
        .ok_or(ApplicationError::CatalogEntityNotFound)
}

fn load_instrument(connection: &Connection, id: UuidV7) -> ApplicationResult<InstrumentFact> {
    connection
        .query_row(
            "SELECT trade_currency FROM security_instruments WHERE instrument_id=?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| ApplicationError::TransactionFailed)?
        .map(|currency| {
            Ok::<InstrumentFact, ApplicationError>(InstrumentFact {
                instrument_id: id,
                trade_currency: Currency::parse(&currency)?,
            })
        })
        .transpose()?
        .ok_or(ApplicationError::CatalogEntityNotFound)
}

fn load_investment_account(
    connection: &Connection,
    id: UuidV7,
) -> ApplicationResult<InvestmentAccountFact> {
    connection
        .query_row(
            "SELECT institution_id,currency FROM cash_accounts WHERE account_id=?1",
            [id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(|_| ApplicationError::TransactionFailed)?
        .map(|(institution, currency)| {
            Ok::<InvestmentAccountFact, ApplicationError>(InvestmentAccountFact {
                account_id: id,
                institution_id: UuidV7::parse(&institution)?,
                currency: Currency::parse(&currency)?,
            })
        })
        .transpose()?
        .ok_or(ApplicationError::CatalogEntityNotFound)
}

fn command_key(command: &InvestmentEventCommand) -> Option<(String, String)> {
    match &command.kind {
        InvestmentEventKind::SecurityTrade {
            portfolio,
            instrument,
            ..
        }
        | InvestmentEventKind::Dividend {
            portfolio,
            instrument,
            ..
        }
        | InvestmentEventKind::InvestmentExpense {
            portfolio,
            instrument: Some(instrument),
            ..
        } => Some((
            portfolio.portfolio_id.to_string(),
            instrument.instrument_id.to_string(),
        )),
        InvestmentEventKind::InvestmentExpense {
            instrument: None, ..
        } => None,
    }
}

fn command_currency_from_domain(command: &InvestmentEventCommand) -> Currency {
    match &command.kind {
        InvestmentEventKind::SecurityTrade { instrument, .. }
        | InvestmentEventKind::Dividend { instrument, .. } => instrument.trade_currency,
        InvestmentEventKind::InvestmentExpense { settlement, .. } => settlement.currency,
    }
}

fn command_currency(
    connection: &Connection,
    input: &InvestmentEventInput,
) -> ApplicationResult<Currency> {
    match input.instrument_id {
        Some(id) => Ok(load_instrument(connection, id)?.trade_currency),
        None => Ok(load_investment_account(connection, input.settlement_account_id)?.currency),
    }
}

fn investment_total_return(
    state: &HoldingState,
    unrealized: &Decimal,
) -> ApplicationResult<Decimal> {
    state
        .realized_trade_pnl
        .checked_add(&state.net_dividend, DecimalUse::Internal)?
        .checked_add(
            &state
                .independent_expense
                .checked_neg(DecimalUse::Internal)?,
            DecimalUse::Internal,
        )?
        .checked_add(unrealized, DecimalUse::Internal)
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn unvalued_holding(
    portfolio_id: &str,
    portfolio_name: String,
    instrument_id: &str,
    instrument_name: String,
    currency: String,
    as_of_date: &LocalDate,
    state: &HoldingState,
    average_cost: Option<String>,
    reason: &'static str,
) -> HoldingPosition {
    HoldingPosition {
        portfolio_id: portfolio_id.to_owned(),
        portfolio_name,
        instrument_id: instrument_id.to_owned(),
        instrument_name,
        currency,
        as_of_date: as_of_date.as_str().to_owned(),
        quantity: state.quantity.as_str().to_owned(),
        carrying_cost: state.carrying_cost.as_str().to_owned(),
        average_cost,
        realized_trade_pnl: state.realized_trade_pnl.as_str().to_owned(),
        net_dividend: state.net_dividend.as_str().to_owned(),
        independent_expense: state.independent_expense.as_str().to_owned(),
        market_price: None,
        price_revision_id: None,
        price_date: None,
        price_age_days: None,
        market_value: None,
        fx_rate: None,
        fx_revision_id: None,
        base_market_value: None,
        unrealized_pnl: None,
        total_return: None,
        valuation_state: "unvalued",
        unvalued_reason: Some(reason),
        warning_codes: Vec::new(),
    }
}

fn parse_optional(value: Option<&String>, usage: DecimalUse) -> ApplicationResult<Option<Decimal>> {
    value
        .map(String::as_str)
        .map(|text| Decimal::parse(text, usage))
        .transpose()
        .map_err(Into::into)
}

fn stored_event_type(value: InvestmentEventType) -> &'static str {
    match value {
        InvestmentEventType::SecurityBuy | InvestmentEventType::SecuritySell => "SecurityTrade",
        InvestmentEventType::Dividend => "Dividend",
        InvestmentEventType::InvestmentExpense => "InvestmentExpense",
    }
}

fn load_base_currency(connection: &Connection) -> ApplicationResult<Currency> {
    let value: String = connection
        .query_row(
            "SELECT base_currency FROM app_settings WHERE singleton_id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    Currency::parse(&value).map_err(Into::into)
}

fn load_name(
    connection: &Connection,
    table: &str,
    id_column: &str,
    id: &str,
) -> ApplicationResult<String> {
    if !matches!(
        (table, id_column),
        ("portfolios", "portfolio_id") | ("security_instruments", "instrument_id")
    ) {
        return Err(ApplicationError::CatalogReferenceInvalid);
    }
    connection
        .query_row(
            &format!("SELECT name FROM {table} WHERE {id_column}=?1"),
            [id],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::CatalogEntityNotFound)
}

fn insert_investment_audit(
    transaction: &Transaction<'_>,
    event_id: UuidV7,
    action: &str,
    revision: u32,
    reason: Option<&str>,
) -> ApplicationResult<()> {
    transaction.execute(
        "INSERT INTO audit_events(audit_event_id,business_event_id,actor,action,entity_type,entity_id,entity_revision,occurred_at_utc,reason)
         VALUES(?1,?2,'local-user',?3,'business-event',?2,?4,CURRENT_TIMESTAMP,?5)",
        params![UuidV7::new()?.to_string(),event_id.to_string(),action,revision,reason],
    ).map_err(map_sqlite_error)?;
    Ok(())
}

fn event_watermark(transaction: &Transaction<'_>, event_id: UuidV7) -> ApplicationResult<u64> {
    transaction
        .query_row(
            "SELECT event_order FROM business_events WHERE event_id=?1",
            [event_id.to_string()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(map_sqlite_error)?
        .try_into()
        .map_err(|_| ApplicationError::TransactionFailed)
}

fn current_event_watermark(connection: &Connection) -> ApplicationResult<u64> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(event_order),0) FROM business_events",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| ApplicationError::TransactionFailed)?
        .try_into()
        .map_err(|_| ApplicationError::TransactionFailed)
}

fn to_i64(value: u64) -> ApplicationResult<i64> {
    i64::try_from(value).map_err(|_| ApplicationError::TransactionFailed)
}

fn index_i64(index: usize) -> ApplicationResult<i64> {
    i64::try_from(index + 1).map_err(|_| ApplicationError::TransactionFailed)
}

fn date_age_days(
    connection: &Connection,
    as_of: &LocalDate,
    source: &LocalDate,
) -> ApplicationResult<u32> {
    let value: i64 = connection
        .query_row(
            "SELECT CAST(julianday(?1)-julianday(?2) AS INTEGER)",
            params![as_of.as_str(), source.as_str()],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    u32::try_from(value).map_err(|_| ApplicationError::TransactionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::cash::{ActivityQuery, CashPort, ReversalInput};
    use crate::application::catalog::{
        CashAccount, CatalogPort, Institution, Portfolio, SecurityInstrument, SecurityPriceRevision,
    };
    use crate::application::ledger::{CreateLedgerCommand, LedgerPort};
    use crate::domain::catalog::{BusinessId, CatalogText};
    use crate::domain::settings::UiLocale;

    fn id(seed: u8) -> UuidV7 {
        UuidV7::from_parts(1_777_000_000_000 + u64::from(seed), [seed; 10]).unwrap()
    }

    fn setup() -> (
        tempfile::TempDir,
        SqliteLedgerManager,
        UuidV7,
        UuidV7,
        UuidV7,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        manager
            .create_ledger(CreateLedgerCommand {
                base_currency: Currency::parse("USD").unwrap(),
                ui_locale: UiLocale::EnUs,
            })
            .unwrap();
        let institution_id = id(1);
        let account_id = id(2);
        let portfolio_id = id(3);
        let instrument_id = id(4);
        manager
            .save_institution(&Institution {
                institution_id,
                business_id: BusinessId::parse("broker").unwrap(),
                name: CatalogText::parse("Broker").unwrap(),
                region: None,
                institution_type: CatalogText::parse("brokerage").unwrap(),
                enabled: true,
            })
            .unwrap();
        manager
            .save_cash_account(&CashAccount {
                account_id,
                business_id: BusinessId::parse("cash-usd").unwrap(),
                institution_id,
                name: CatalogText::parse("USD cash").unwrap(),
                purpose: CatalogText::parse("settlement").unwrap(),
                currency: Currency::parse("USD").unwrap(),
                opened_on: None,
                enabled: true,
            })
            .unwrap();
        manager
            .save_portfolio(&Portfolio {
                portfolio_id,
                business_id: BusinessId::parse("portfolio-a").unwrap(),
                institution_id,
                settlement_account_id: account_id,
                name: CatalogText::parse("Portfolio A").unwrap(),
                portfolio_type: CatalogText::parse("brokerage").unwrap(),
                enabled: true,
            })
            .unwrap();
        manager
            .save_instrument(&SecurityInstrument {
                instrument_id,
                business_id: BusinessId::parse("alpha").unwrap(),
                code: CatalogText::parse("ALPHA").unwrap(),
                name: CatalogText::parse("Alpha").unwrap(),
                trade_currency: Currency::parse("USD").unwrap(),
                enabled: true,
            })
            .unwrap();
        (directory, manager, account_id, portfolio_id, instrument_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn trade(
        account: UuidV7,
        portfolio: UuidV7,
        instrument: UuidV7,
        side: InvestmentEventType,
        date: &str,
        sequence: u64,
        quantity: &str,
        price: &str,
        fee: &str,
    ) -> InvestmentEventInput {
        InvestmentEventInput {
            effective_date: LocalDate::parse(date).unwrap(),
            sequence: Sequence::new(sequence).unwrap(),
            event_type: side,
            portfolio_id: portfolio,
            instrument_id: Some(instrument),
            settlement_account_id: account,
            quantity: Some(Decimal::parse(quantity, DecimalUse::Quantity).unwrap()),
            unit_price: Some(Decimal::parse(price, DecimalUse::UnitPrice).unwrap()),
            trade_fee: Some(Decimal::parse(fee, DecimalUse::Amount).unwrap()),
            gross_cash_amount: None,
            withholding_tax: None,
            fee_amount: None,
            amount: None,
            fee_scope: None,
            settlement_override_reason: None,
            fx_overrides: Vec::new(),
        }
    }

    #[test]
    fn sqlite_replay_handles_partial_close_reopen_and_historical_insert() {
        let (_directory, mut manager, account, portfolio, instrument) = setup();
        manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-06",
                10,
                "10",
                "12.34",
                "1.60",
            ))
            .unwrap();
        manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecuritySell,
                "2026-02-07",
                10,
                "4",
                "15",
                "1",
            ))
            .unwrap();
        let view = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-07").unwrap())
            .unwrap();
        assert_eq!(view.holdings[0].quantity, "6");
        assert_eq!(view.holdings[0].carrying_cost, "75.00");
        assert_eq!(view.holdings[0].realized_trade_pnl, "9.00");
        manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-06",
                5,
                "2",
                "10",
                "0",
            ))
            .unwrap();
        let replayed = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-07").unwrap())
            .unwrap();
        assert_eq!(replayed.holdings[0].quantity, "8");
    }

    #[test]
    fn failed_oversell_rolls_back_event_and_projection() {
        let (_directory, mut manager, account, portfolio, instrument) = setup();
        manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-06",
                1,
                "1",
                "10",
                "0",
            ))
            .unwrap();
        let error = manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecuritySell,
                "2026-02-07",
                1,
                "1.000000000001",
                "10",
                "0",
            ))
            .unwrap_err();
        assert_eq!(error.code(), "NEGATIVE_HOLDING_NOT_ALLOWED");
        let view = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-07").unwrap())
            .unwrap();
        assert_eq!(view.event_watermark, 1);
        assert_eq!(view.holdings[0].quantity, "1");
    }

    #[test]
    fn dividends_and_both_expense_scopes_follow_the_single_return_formula() {
        let (_directory, mut manager, account, portfolio, instrument) = setup();
        manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-06",
                1,
                "2",
                "10",
                "0",
            ))
            .unwrap();
        manager
            .post_investment_event(&InvestmentEventInput {
                effective_date: LocalDate::parse("2026-02-09").unwrap(),
                sequence: Sequence::new(1).unwrap(),
                event_type: InvestmentEventType::Dividend,
                portfolio_id: portfolio,
                instrument_id: Some(instrument),
                settlement_account_id: account,
                quantity: None,
                unit_price: None,
                trade_fee: None,
                gross_cash_amount: Some(Decimal::parse("10", DecimalUse::Amount).unwrap()),
                withholding_tax: Some(Decimal::parse("1.5", DecimalUse::Amount).unwrap()),
                fee_amount: Some(Decimal::parse("0.5", DecimalUse::Amount).unwrap()),
                amount: None,
                fee_scope: None,
                settlement_override_reason: None,
                fx_overrides: Vec::new(),
            })
            .unwrap();
        manager
            .post_investment_event(&InvestmentEventInput {
                effective_date: LocalDate::parse("2026-02-09").unwrap(),
                sequence: Sequence::new(2).unwrap(),
                event_type: InvestmentEventType::InvestmentExpense,
                portfolio_id: portfolio,
                instrument_id: Some(instrument),
                settlement_account_id: account,
                quantity: None,
                unit_price: None,
                trade_fee: None,
                gross_cash_amount: None,
                withholding_tax: None,
                fee_amount: None,
                amount: Some(Decimal::parse("2", DecimalUse::Amount).unwrap()),
                fee_scope: Some(FeeScope::Instrument),
                settlement_override_reason: None,
                fx_overrides: Vec::new(),
            })
            .unwrap();
        manager
            .post_investment_event(&InvestmentEventInput {
                effective_date: LocalDate::parse("2026-02-09").unwrap(),
                sequence: Sequence::new(3).unwrap(),
                event_type: InvestmentEventType::InvestmentExpense,
                portfolio_id: portfolio,
                instrument_id: None,
                settlement_account_id: account,
                quantity: None,
                unit_price: None,
                trade_fee: None,
                gross_cash_amount: None,
                withholding_tax: None,
                fee_amount: None,
                amount: Some(Decimal::parse("0.01", DecimalUse::Amount).unwrap()),
                fee_scope: Some(FeeScope::Portfolio),
                settlement_override_reason: None,
                fx_overrides: Vec::new(),
            })
            .unwrap();
        manager
            .save_price_revision(
                &SecurityPriceRevision::new(
                    id(5),
                    instrument,
                    LocalDate::parse("2026-02-09").unwrap(),
                    "12",
                    Currency::parse("USD").unwrap(),
                    CatalogText::parse("synthetic").unwrap(),
                    true,
                )
                .unwrap(),
            )
            .unwrap();
        let view = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-09").unwrap())
            .unwrap();
        assert_eq!(view.holdings[0].net_dividend, "8.0");
        assert_eq!(view.holdings[0].independent_expense, "2");
        assert_eq!(view.holdings[0].unrealized_pnl.as_deref(), Some("4"));
        assert_eq!(view.holdings[0].total_return.as_deref(), Some("10.0"));
        assert_eq!(view.portfolio_expenses[0].amount, "0.01");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn portfolio_keys_override_audit_and_as_of_valuation_are_explicit() {
        let (_directory, mut manager, account, portfolio, instrument) = setup();
        let institution = id(1);
        let alternate = id(6);
        let portfolio_b = id(7);
        manager
            .save_cash_account(&CashAccount {
                account_id: alternate,
                business_id: BusinessId::parse("cash-alt").unwrap(),
                institution_id: institution,
                name: CatalogText::parse("Alternate").unwrap(),
                purpose: CatalogText::parse("settlement").unwrap(),
                currency: Currency::parse("USD").unwrap(),
                opened_on: None,
                enabled: true,
            })
            .unwrap();
        manager
            .save_portfolio(&Portfolio {
                portfolio_id: portfolio_b,
                business_id: BusinessId::parse("portfolio-b").unwrap(),
                institution_id: institution,
                settlement_account_id: account,
                name: CatalogText::parse("Portfolio B").unwrap(),
                portfolio_type: CatalogText::parse("brokerage").unwrap(),
                enabled: true,
            })
            .unwrap();
        manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-10",
                1,
                "2",
                "10",
                "0",
            ))
            .unwrap();
        manager
            .post_investment_event(&trade(
                account,
                portfolio_b,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-10",
                2,
                "3",
                "20",
                "0",
            ))
            .unwrap();
        let missing = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-10").unwrap())
            .unwrap();
        assert!(
            missing
                .holdings
                .iter()
                .all(|holding| holding.valuation_state == "unvalued")
        );
        assert!(
            missing
                .holdings
                .iter()
                .all(|holding| holding.unvalued_reason == Some("PRICE_MISSING_AS_OF"))
        );
        let mut overridden = trade(
            alternate,
            portfolio,
            instrument,
            InvestmentEventType::SecurityBuy,
            "2026-02-10",
            3,
            "1",
            "11",
            "0",
        );
        assert_eq!(
            manager
                .preview_investment_event(&overridden)
                .unwrap_err()
                .code(),
            "SETTLEMENT_OVERRIDE_REASON_REQUIRED"
        );
        overridden.settlement_override_reason =
            Some("Synthetic segregated cash account".to_owned());
        manager.post_investment_event(&overridden).unwrap();
        manager
            .save_price_revision(
                &SecurityPriceRevision::new(
                    id(8),
                    instrument,
                    LocalDate::parse("2026-02-10").unwrap(),
                    "10",
                    Currency::parse("USD").unwrap(),
                    CatalogText::parse("old").unwrap(),
                    true,
                )
                .unwrap(),
            )
            .unwrap();
        manager
            .save_price_revision(
                &SecurityPriceRevision::new(
                    id(9),
                    instrument,
                    LocalDate::parse("2026-02-16").unwrap(),
                    "99",
                    Currency::parse("USD").unwrap(),
                    CatalogText::parse("future inactive").unwrap(),
                    false,
                )
                .unwrap(),
            )
            .unwrap();
        let view = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-15").unwrap())
            .unwrap();
        assert_eq!(view.holdings.len(), 2);
        assert_eq!(view.holdings[0].price_revision_id, Some(id(8).to_string()));
        let stale = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-20").unwrap())
            .unwrap();
        assert_eq!(stale.holdings[0].price_age_days, Some(10));
        assert_eq!(stale.holdings[0].warning_codes, vec!["STALE_PRICE"]);
    }

    #[test]
    fn investment_revision_and_generic_reversal_rebuild_to_zero() {
        let (_directory, mut manager, account, portfolio, instrument) = setup();
        let original = manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-10",
                1,
                "2",
                "10",
                "0",
            ))
            .unwrap();
        let revised = manager
            .revise_investment_event(&InvestmentRevisionInput {
                target_event_id: UuidV7::parse(&original.event_id).unwrap(),
                reason: "Synthetic correction".to_owned(),
                replacement: trade(
                    account,
                    portfolio,
                    instrument,
                    InvestmentEventType::SecurityBuy,
                    "2026-02-10",
                    2,
                    "3",
                    "10",
                    "0",
                ),
            })
            .unwrap();
        assert_eq!(
            manager
                .get_investment_workspace(&LocalDate::parse("2026-02-10").unwrap())
                .unwrap()
                .holdings[0]
                .quantity,
            "3"
        );
        manager
            .reverse_event(&ReversalInput {
                target_event_id: UuidV7::parse(&revised.event_id).unwrap(),
                reason: "Synthetic duplicate".to_owned(),
                effective_date: LocalDate::parse("2026-02-10").unwrap(),
                sequence: Sequence::new(3).unwrap(),
            })
            .unwrap();
        let view = manager
            .get_investment_workspace(&LocalDate::parse("2026-02-10").unwrap())
            .unwrap();
        assert!(view.holdings.is_empty());
    }

    #[test]
    fn investment_activity_exposes_typed_details_postings_and_override_audit() {
        let (_directory, mut manager, account, portfolio, instrument) = setup();
        let posted = manager
            .post_investment_event(&trade(
                account,
                portfolio,
                instrument,
                InvestmentEventType::SecurityBuy,
                "2026-02-10",
                1,
                "2",
                "10",
                "0.25",
            ))
            .unwrap();
        let page = manager
            .get_activity(&ActivityQuery {
                start_date: LocalDate::parse("2026-02-10").unwrap(),
                end_date: LocalDate::parse("2026-02-10").unwrap(),
                context: None,
                event_type: Some("SecurityTrade".to_owned()),
                account_id: None,
                category_id: None,
                search: None,
                cursor: None,
                limit: 25,
            })
            .unwrap();
        assert_eq!(page.items.len(), 1);
        let item = &page.items[0];
        assert_eq!(item.event_id, posted.event_id);
        assert_eq!(item.content.trade_type.as_deref(), Some("BUY"));
        assert_eq!(
            item.content.portfolio_id.as_deref(),
            Some(portfolio.to_string().as_str())
        );
        assert_eq!(
            item.content.instrument_id.as_deref(),
            Some(instrument.to_string().as_str())
        );
        assert_eq!(item.postings[0].posting_kind, "settlement-cash");
        assert_eq!(item.postings[1].posting_kind, "security-quantity");
        assert_eq!(item.reversal_preview[0].quantity_delta, "20.25");
    }
}
