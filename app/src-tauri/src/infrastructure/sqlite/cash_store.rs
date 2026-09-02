#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use crate::application::canonical::canonical_hash;
use crate::application::cash::{
    ActivityAudit, ActivityEventContent, ActivityFxResolution, ActivityItem, ActivityPage,
    ActivityPosting, ActivityQuery, ActivityRelations, CashEventInput, CashPort, DrilldownContext,
    EventInputType, EventPreview, ExpenseAnalysis, ExpenseBucket, ExpenseMeasure,
    ExpenseQueryContract, ExpenseSummary, ExpenseTop10, ExpenseTopItem, ExpenseVersions,
    ExpenseWatermarks, FxResolutionResult, LargestCategory, PostedEvent, PostingPreview,
    RefundMeasures, ReversalInput, RevisionInput, UnvaluedExpense,
};
use crate::application::error::{ApplicationError, ApplicationResult};
use crate::domain::cash::{
    CashAccountFact, CashContributionRole, CashEventCommand, CashEventKind, CategoryFact, FeeInput,
    IncomeExpenseDirection, PreparedCashEvent, prepare_cash_event,
};
use crate::domain::catalog::{CategoryKind, SemanticRole};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::error::DomainError;
use crate::domain::types::{Currency, LocalDate, UuidV7};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use rust_decimal::Decimal as RustDecimal;

use super::store::{CALCULATION_VERSION, LedgerStore, SqliteLedgerManager, map_sqlite_error};

pub const EXPENSE_POLICY_VERSION: &str = "expense-policy-v1";
pub const BUCKET_POLICY_VERSION: &str = "expense-bucket-policy-v1";
pub const REFUND_POLICY_VERSION: &str = "refund-policy-v1";
const CANONICALIZATION_ID: &str = "ledgerkit-canonical-json-v1";
const MAX_RESPONSE_BYTES: usize = 32 * 1024;
const EXPENSE_DAILY_PROJECTION_VERSION: &str = "expense-daily-projection-v1";

struct SqlBucketAccumulator {
    label: String,
    archived: bool,
    amount: RustDecimal,
    count: u64,
}

struct ExpenseSqlAccumulator {
    summary: RustDecimal,
    global_count: u64,
    unvalued_count: u64,
    refund: RustDecimal,
    refund_count: u64,
    refund_unvalued: u64,
    reimbursement: RustDecimal,
    reimbursement_count: u64,
    reimbursement_unvalued: u64,
    buckets: BTreeMap<String, SqlBucketAccumulator>,
}

struct DailyExpenseAccumulator {
    amount: RustDecimal,
    events: BTreeSet<String>,
}

impl DailyExpenseAccumulator {
    fn new() -> Self {
        Self {
            amount: RustDecimal::ZERO,
            events: BTreeSet::new(),
        }
    }
}

fn new_expense_accumulator() -> ExpenseSqlAccumulator {
    ExpenseSqlAccumulator {
        summary: RustDecimal::ZERO,
        global_count: 0,
        unvalued_count: 0,
        refund: RustDecimal::ZERO,
        refund_count: 0,
        refund_unvalued: 0,
        reimbursement: RustDecimal::ZERO,
        reimbursement_count: 0,
        reimbursement_unvalued: 0,
        buckets: BTreeMap::new(),
    }
}

impl CashPort for SqliteLedgerManager {
    fn preview_event(&self, input: &CashEventInput) -> ApplicationResult<EventPreview> {
        self.open_store()?.preview_cash_event(input)
    }

    fn post_event(&mut self, input: &CashEventInput) -> ApplicationResult<PostedEvent> {
        self.open_store_mut()?.post_cash_event(input, None, 1, None)
    }

    fn revise_event(&mut self, input: &RevisionInput) -> ApplicationResult<PostedEvent> {
        self.open_store_mut()?.revise_cash_event(input)
    }

    fn reverse_event(&mut self, input: &ReversalInput) -> ApplicationResult<PostedEvent> {
        self.open_store_mut()?.reverse_cash_event(input)
    }

    fn get_expense_analysis(
        &self,
        start_date: &LocalDate,
        end_date: &LocalDate,
        event_watermark: Option<u64>,
    ) -> ApplicationResult<ExpenseAnalysis> {
        self.open_store()?
            .expense_analysis(start_date, end_date, event_watermark)
    }

    fn get_activity(&self, query: &ActivityQuery) -> ApplicationResult<ActivityPage> {
        self.open_store()?.activity(query)
    }
}

#[derive(Clone)]
struct ResolvedPosting {
    account_id: String,
    quantity_delta: Decimal,
    currency: Currency,
    base_value: Option<Decimal>,
}

#[derive(Clone)]
struct PreparedWrite {
    domain: PreparedCashEvent,
    preview: EventPreview,
    postings: Vec<ResolvedPosting>,
}

impl LedgerStore {
    fn preview_cash_event(&self, input: &CashEventInput) -> ApplicationResult<EventPreview> {
        Ok(self.prepare_write(input)?.preview)
    }

    fn post_cash_event(
        &mut self,
        input: &CashEventInput,
        supersedes: Option<UuidV7>,
        revision: u32,
        reason: Option<&str>,
    ) -> ApplicationResult<PostedEvent> {
        let prepared = self.prepare_write(input)?;
        let event_id = UuidV7::new()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let watermark = insert_prepared_event(
            &transaction,
            event_id,
            input,
            &prepared,
            supersedes,
            revision,
            reason,
        )?;
        rebuild_cash_derived(&transaction, watermark)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(PostedEvent {
            event_id: event_id.to_string(),
            event_watermark: watermark,
            revision,
            preview: prepared.preview,
        })
    }

    fn revise_cash_event(&mut self, input: &RevisionInput) -> ApplicationResult<PostedEvent> {
        if input.reason.trim().is_empty() {
            return Err(DomainError::RevisionReasonRequired.into());
        }
        let target = input.target_event_id.to_string();
        let revision: Option<u32> = self
            .connection
            .query_row(
                "SELECT revision FROM business_events e WHERE event_id=?1 AND status='posted'
                 AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id)
                 AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id)",
                [&target],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let revision = revision.ok_or(DomainError::RevisionTargetNotEffective)?;
        self.post_cash_event(
            &input.replacement,
            Some(input.target_event_id),
            revision.saturating_add(1),
            Some(&input.reason),
        )
    }

    fn reverse_cash_event(&mut self, input: &ReversalInput) -> ApplicationResult<PostedEvent> {
        if input.reason.trim().is_empty() {
            return Err(DomainError::ReversalReasonRequired.into());
        }
        let target = input.target_event_id.to_string();
        let target_revision: Option<u32> = self
            .connection
            .query_row(
                "SELECT revision FROM business_events e WHERE event_id=?1 AND status='posted'
                 AND event_type<>'Reversal'
                 AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id)
                 AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id)",
                [&target],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        if target_revision.is_none() {
            let reversed: bool = self
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM business_events WHERE reverses_event_id=?1)",
                    [&target],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
            return Err(if reversed {
                DomainError::EventAlreadyReversed.into()
            } else {
                DomainError::RevisionTargetNotEffective.into()
            });
        }
        let rows = load_posting_rows(&self.connection, &target)?;
        let base_currency = load_base_currency(&self.connection)?;
        let event_id = UuidV7::new()?;
        let mut preview_postings = Vec::with_capacity(rows.len());
        let mut reversed_rows = Vec::with_capacity(rows.len());
        for row in rows {
            let delta = row.1.checked_neg(DecimalUse::Internal)?;
            let base_value = row
                .3
                .as_ref()
                .map(|value| value.checked_neg(DecimalUse::Internal))
                .transpose()?;
            preview_postings.push(PostingPreview {
                account_id: row.0.clone(),
                quantity_delta: delta.as_str().to_owned(),
                currency: row.2.to_string(),
                base_value: base_value.as_ref().map(|value| value.as_str().to_owned()),
                base_currency: base_currency.to_string(),
                role: "reversal",
            });
            reversed_rows.push((row.0, delta, row.2, base_value));
        }
        let preview = EventPreview {
            event_type: "Reversal",
            effective_date: input.effective_date.as_str().to_owned(),
            sequence: input.sequence.get(),
            category_id: None,
            semantic_role: "normal",
            fee_account_id: None,
            fee_amount: None,
            postings: preview_postings,
            fx_resolutions: Vec::new(),
            quality_issue_codes: Vec::new(),
        };
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .execute(
                "INSERT INTO business_events(event_id,event_type,effective_date,sequence,status,revision,reverses_event_id,revision_reason,created_at_utc,calculation_version)
                 VALUES(?1,'Reversal',?2,?3,'posted',1,?4,?5,CURRENT_TIMESTAMP,?6)",
                params![event_id.to_string(), input.effective_date.as_str(), sequence_i64(input.sequence.get())?, target, input.reason, CALCULATION_VERSION],
            )
            .map_err(map_sqlite_error)?;
        for (index, (account_id, delta, currency, base_value)) in reversed_rows.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO ledger_postings(posting_id,event_id,posting_ordinal,posting_kind,account_id,quantity_delta,currency,base_value,base_currency,calculation_version)
                     VALUES(?1,?2,?3,'cash-reversal',?4,?5,?6,?7,?8,?9)",
                    params![UuidV7::new()?.to_string(), event_id.to_string(), index_i64(index)?, account_id, delta.as_str(), currency.as_str(), base_value.as_ref().map(Decimal::as_str), base_currency.as_str(), CALCULATION_VERSION],
                )
                .map_err(map_sqlite_error)?;
        }
        insert_audit(&transaction, event_id, "reverse", 1, Some(&input.reason))?;
        let watermark = event_order(&transaction, event_id)?;
        rebuild_cash_derived(&transaction, watermark)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(PostedEvent {
            event_id: event_id.to_string(),
            event_watermark: watermark,
            revision: 1,
            preview,
        })
    }

    fn prepare_write(&self, input: &CashEventInput) -> ApplicationResult<PreparedWrite> {
        let command = to_domain_command(&self.connection, input)?;
        let domain = prepare_cash_event(&command)?;
        let base_currency = load_base_currency(&self.connection)?;
        let mut resolutions = BTreeMap::<(Currency, &'static str), FxResolutionResult>::new();
        let mut postings = Vec::with_capacity(domain.postings.len());
        let mut previews = Vec::with_capacity(domain.postings.len());
        let mut quality = BTreeSet::new();
        for draft in &domain.postings {
            let purpose = match draft.role {
                CashContributionRole::Principal => "transaction",
                CashContributionRole::Fee => "fee",
            };
            let resolution = if let Some(existing) = resolutions.get(&(draft.currency, purpose)) {
                existing.clone()
            } else {
                let resolved = resolve_rate(
                    &self.connection,
                    draft.currency,
                    base_currency,
                    &input.effective_date,
                    input
                        .fx_overrides
                        .iter()
                        .find(|item| item.currency == draft.currency),
                    purpose,
                )?;
                resolutions.insert((draft.currency, purpose), resolved.clone());
                resolved
            };
            let base_value = resolution
                .final_rate
                .as_deref()
                .map(|rate| Decimal::parse(rate, DecimalUse::FxRate))
                .transpose()?
                .map(|rate| draft.quantity_delta.checked_mul_internal(&rate))
                .transpose()?;
            if base_value.is_none() {
                quality.insert("MISSING_FX_RATE");
            }
            let role = if purpose == "fee" { "fee" } else { "principal" };
            previews.push(PostingPreview {
                account_id: draft.account_id.to_string(),
                quantity_delta: draft.quantity_delta.as_str().to_owned(),
                currency: draft.currency.to_string(),
                base_value: base_value.as_ref().map(|value| value.as_str().to_owned()),
                base_currency: base_currency.to_string(),
                role,
            });
            postings.push(ResolvedPosting {
                account_id: draft.account_id.to_string(),
                quantity_delta: draft.quantity_delta.clone(),
                currency: draft.currency,
                base_value,
            });
        }
        Ok(PreparedWrite {
            preview: EventPreview {
                event_type: domain.event_type,
                effective_date: input.effective_date.as_str().to_owned(),
                sequence: input.sequence.get(),
                category_id: input.category_id.map(|value| value.to_string()),
                semantic_role: input.semantic_role.as_str(),
                fee_account_id: input.fee_account_id.map(|value| value.to_string()),
                fee_amount: input
                    .fee_amount
                    .as_ref()
                    .map(|value| value.as_str().to_owned()),
                postings: previews,
                fx_resolutions: resolutions.into_values().collect(),
                quality_issue_codes: quality.into_iter().collect(),
            },
            domain,
            postings,
        })
    }

    fn expense_analysis(
        &self,
        start_date: &LocalDate,
        end_date: &LocalDate,
        requested_watermark: Option<u64>,
    ) -> ApplicationResult<ExpenseAnalysis> {
        if start_date > end_date {
            return Err(ApplicationError::ExpenseDateRangeInvalid);
        }
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let current: u64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(event_order),0) FROM business_events",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?
            .try_into()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let watermark = requested_watermark.unwrap_or(current).min(current);
        let master_watermark: u64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE entity_type IN ('institution','cash-account','category','portfolio','security-instrument','fx-rate-revision','security-price-revision')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?
            .try_into()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let cache_key = format!(
            "{}|{}|{watermark}|{master_watermark}",
            start_date.as_str(),
            end_date.as_str()
        );
        if let Some(result) = self
            .expense_cache
            .borrow()
            .iter()
            .find(|(key, _)| key == &cache_key)
            .map(|(_, value)| value.clone())
        {
            transaction
                .commit()
                .map_err(|_| ApplicationError::TransactionFailed)?;
            return Ok(result);
        }
        let projection_ready = watermark == current
            && transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM projection_metadata
                       WHERE projection_name='expense-daily'
                         AND projection_version=?1
                         AND calculation_version=?2
                         AND event_watermark=?3
                         AND available=1
                     )",
                    params![
                        EXPENSE_DAILY_PROJECTION_VERSION,
                        CALCULATION_VERSION,
                        i64::try_from(watermark)
                            .map_err(|_| ApplicationError::TransactionFailed)?
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
        let aggregates = if projection_ready {
            load_projected_expense_aggregates(&transaction, start_date, end_date)?
        } else {
            // Historical watermarks are deliberately answered from immutable facts: the
            // accepted daily projection represents only the current effective state.
            load_expense_aggregates(&transaction, start_date, end_date, watermark)?
        };
        let result = build_aggregated_expense_result(
            start_date,
            end_date,
            load_base_currency(&transaction)?,
            watermark,
            master_watermark,
            aggregates,
        )?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let mut cache = self.expense_cache.borrow_mut();
        if cache.len() == 8 {
            cache.remove(0);
        }
        cache.push((cache_key, result.clone()));
        Ok(result)
    }

    fn activity(&self, query: &ActivityQuery) -> ApplicationResult<ActivityPage> {
        if query.limit == 0 || query.limit > 100 {
            return Err(ApplicationError::ActivityLimitInvalid);
        }
        if query.start_date > query.end_date {
            return Err(ApplicationError::ExpenseDateRangeInvalid);
        }
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let current: u64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(event_order),0) FROM business_events",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?
            .try_into()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let mut ids = if let Some(context) = &query.context {
            if context.start_date != query.start_date.as_str()
                || context.end_date != query.end_date.as_str()
                || context.event_watermark > current
            {
                return Err(ApplicationError::ActivityCursorInvalid);
            }
            let other_members = if context.bucket_id.as_deref() == Some("system:top10-other") {
                Some(load_top_other_member_ids(
                    &transaction,
                    &query.start_date,
                    &query.end_date,
                    context.event_watermark,
                    context.member_rank_gt.unwrap_or(10),
                )?)
            } else {
                None
            };
            let mut seen = BTreeSet::new();
            load_contributions(
                &transaction,
                &query.start_date,
                &query.end_date,
                context.event_watermark,
                query.cursor,
                Some((u64::from(query.limit) + 1).saturating_mul(3)),
                Some(context),
                other_members.as_deref(),
            )?
            .into_iter()
            .map(|row| (row.event_id, row.event_order))
            .filter(|item| seen.insert(item.0.clone()))
            .collect()
        } else {
            load_general_activity_ids(&transaction, query, current)?
        };
        let has_more = ids.len() > query.limit as usize;
        ids.truncate(query.limit as usize);
        let next_cursor = has_more.then(|| ids.last().map_or(0, |item| item.1));
        let items = load_activity_items(&transaction, &ids)?;
        let page = ActivityPage { items, next_cursor };
        if serde_json::to_vec(&page)
            .map_err(|_| ApplicationError::TransactionFailed)?
            .len()
            > MAX_RESPONSE_BYTES
        {
            return Err(ApplicationError::ResponseTooLarge);
        }
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(page)
    }
}

fn to_domain_command(
    connection: &Connection,
    input: &CashEventInput,
) -> ApplicationResult<CashEventCommand> {
    let account = |id: Option<UuidV7>| -> ApplicationResult<CashAccountFact> {
        load_account(connection, id.ok_or(DomainError::EventInvariantViolation)?)
    };
    let fee = match (input.fee_account_id, input.fee_amount.clone()) {
        (None, None) => None,
        (None, Some(amount)) if amount.is_zero() => None,
        (Some(_), Some(amount)) if amount.is_zero() => None,
        (Some(id), Some(amount)) => Some(FeeInput {
            account: load_account(connection, id)?,
            amount: checked_amount(
                Some(amount),
                load_account(connection, id)?.currency,
                input.currency_precision_confirmed,
            )?,
        }),
        _ => return Err(DomainError::EventInvariantViolation.into()),
    };
    let kind = match input.event_type {
        EventInputType::OpeningBalance => CashEventKind::OpeningBalance {
            account: account(input.account_id)?,
            balance: checked_amount(
                input.amount.clone(),
                account(input.account_id)?.currency,
                input.currency_precision_confirmed,
            )?,
            cutover_date: input
                .cutover_date
                .clone()
                .ok_or(DomainError::EventInvariantViolation)?,
            migration_policy: input
                .migration_policy
                .clone()
                .ok_or(DomainError::EventInvariantViolation)?,
        },
        EventInputType::Income | EventInputType::Expense => CashEventKind::IncomeExpense {
            direction: if input.event_type == EventInputType::Income {
                IncomeExpenseDirection::Income
            } else {
                IncomeExpenseDirection::Expense
            },
            account: account(input.account_id)?,
            amount: checked_amount(
                input.amount.clone(),
                account(input.account_id)?.currency,
                input.currency_precision_confirmed,
            )?,
            category: input
                .category_id
                .map(|id| load_category(connection, id))
                .transpose()?,
            semantic_role: input.semantic_role,
            merchant: input.merchant.clone(),
            note: input.note.clone(),
            fee,
        },
        EventInputType::Adjustment => CashEventKind::Adjustment {
            account: account(input.account_id)?,
            delta: checked_amount(
                input.amount.clone(),
                account(input.account_id)?.currency,
                input.currency_precision_confirmed,
            )?,
            note: input.note.clone(),
        },
        EventInputType::Transfer => CashEventKind::Transfer {
            from: account(input.from_account_id)?,
            to: account(input.to_account_id)?,
            amount: checked_amount(
                input.amount.clone(),
                account(input.from_account_id)?.currency,
                input.currency_precision_confirmed,
            )?,
        },
        EventInputType::CurrencyExchange => CashEventKind::CurrencyExchange {
            from: account(input.from_account_id)?,
            to: account(input.to_account_id)?,
            from_amount: checked_amount(
                input.amount.clone(),
                account(input.from_account_id)?.currency,
                input.currency_precision_confirmed,
            )?,
            to_amount: checked_amount(
                input.to_amount.clone(),
                account(input.to_account_id)?.currency,
                input.currency_precision_confirmed,
            )?,
            fee,
        },
    };
    Ok(CashEventCommand {
        effective_date: input.effective_date.clone(),
        sequence: input.sequence,
        kind,
    })
}

fn checked_amount(
    amount: Option<Decimal>,
    currency: Currency,
    precision_confirmed: bool,
) -> ApplicationResult<Decimal> {
    let amount = amount.ok_or(DomainError::EventInvariantViolation)?;
    if amount.scale() > currency.common_scale() && !precision_confirmed {
        return Err(DomainError::CurrencyPrecisionConfirmationRequired.into());
    }
    Ok(amount)
}

fn load_account(connection: &Connection, id: UuidV7) -> ApplicationResult<CashAccountFact> {
    let currency: Option<String> = connection
        .query_row(
            "SELECT currency FROM cash_accounts WHERE account_id=?1 AND enabled=1",
            [id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    Ok(CashAccountFact {
        account_id: id,
        currency: Currency::parse(&currency.ok_or(ApplicationError::CatalogEntityNotFound)?)?,
    })
}

fn load_category(connection: &Connection, id: UuidV7) -> ApplicationResult<CategoryFact> {
    let value: Option<(String, String)> = connection
        .query_row(
            "SELECT category_kind,semantic_role FROM categories WHERE category_id=?1",
            [id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let (kind, role) = value.ok_or(ApplicationError::CatalogEntityNotFound)?;
    Ok(CategoryFact {
        category_id: id,
        kind: CategoryKind::parse(&kind)?,
        semantic_role: SemanticRole::parse(&role)?,
    })
}

fn load_base_currency(connection: &Connection) -> ApplicationResult<Currency> {
    let value: String = connection
        .query_row(
            "SELECT base_currency FROM app_settings WHERE singleton_id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::LedgerNotOpen)?;
    Currency::parse(&value).map_err(Into::into)
}

fn resolve_rate(
    connection: &Connection,
    currency: Currency,
    base_currency: Currency,
    date: &LocalDate,
    override_input: Option<&crate::application::cash::FxOverrideInput>,
    purpose: &'static str,
) -> ApplicationResult<FxResolutionResult> {
    if override_input.is_some_and(|item| item.reason.trim().is_empty()) {
        return Err(DomainError::FxOverrideReasonRequired.into());
    }
    let automatic = if currency == base_currency {
        None
    } else {
        connection
            .query_row(
                "SELECT fx_rate_revision_id,rate_to_base FROM fx_rate_revisions
                 WHERE currency=?1 AND base_currency=?2 AND active=1 AND rate_date<=?3
                 ORDER BY rate_date DESC,revision DESC,fx_rate_revision_id DESC LIMIT 1",
                params![currency.as_str(), base_currency.as_str(), date.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?
    };
    let final_rate = if currency == base_currency {
        Some("1".to_owned())
    } else if let Some(overridden) = override_input {
        Some(overridden.value.as_str().to_owned())
    } else {
        automatic.as_ref().map(|item| item.1.clone())
    };
    Ok(FxResolutionResult {
        purpose,
        currency: currency.to_string(),
        base_currency: base_currency.to_string(),
        target_date: date.as_str().to_owned(),
        automatic_candidate_revision_id: automatic.map(|item| item.0),
        override_value: override_input.map(|item| item.value.as_str().to_owned()),
        override_reason: override_input.map(|item| item.reason.clone()),
        valuation_state: if final_rate.is_some() {
            "valued"
        } else {
            "unvalued"
        },
        final_rate,
        calculation_version: CALCULATION_VERSION,
    })
}

#[allow(clippy::too_many_arguments)]
fn insert_prepared_event(
    transaction: &Transaction<'_>,
    event_id: UuidV7,
    input: &CashEventInput,
    prepared: &PreparedWrite,
    supersedes: Option<UuidV7>,
    revision: u32,
    reason: Option<&str>,
) -> ApplicationResult<u64> {
    transaction
        .execute(
            "INSERT INTO business_events(event_id,event_type,effective_date,sequence,status,revision,supersedes_event_id,revision_reason,created_at_utc,calculation_version)
             VALUES(?1,?2,?3,?4,'posted',?5,?6,?7,CURRENT_TIMESTAMP,?8)",
            params![event_id.to_string(), prepared.domain.event_type, input.effective_date.as_str(), sequence_i64(input.sequence.get())?, revision, supersedes.map(|id| id.to_string()), reason, CALCULATION_VERSION],
        )
        .map_err(map_sqlite_error)?;
    insert_detail(transaction, event_id, input, &prepared.domain)?;
    for (index, posting) in prepared.postings.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO ledger_postings(posting_id,event_id,posting_ordinal,posting_kind,account_id,quantity_delta,currency,base_value,base_currency,calculation_version)
                 VALUES(?1,?2,?3,'cash',?4,?5,?6,?7,?8,?9)",
                params![UuidV7::new()?.to_string(), event_id.to_string(), index_i64(index)?, posting.account_id, posting.quantity_delta.as_str(), posting.currency.as_str(), posting.base_value.as_ref().map(Decimal::as_str), prepared.preview.postings[index].base_currency, CALCULATION_VERSION],
            )
            .map_err(map_sqlite_error)?;
    }
    let mut saved = BTreeSet::new();
    for resolution in &prepared.preview.fx_resolutions {
        if resolution.final_rate.is_none()
            || !saved.insert((resolution.purpose, resolution.currency.as_str()))
        {
            continue;
        }
        transaction
            .execute(
                "INSERT INTO fx_resolutions(fx_resolution_id,owner_type,owner_id,purpose,target_date,currency,base_currency,auto_rate_revision_id,override_value,override_reason,final_rate,calculation_version,created_at_utc)
                 VALUES(?1,'event',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,CURRENT_TIMESTAMP)",
                params![UuidV7::new()?.to_string(), event_id.to_string(), resolution.purpose, resolution.target_date, resolution.currency, resolution.base_currency, resolution.automatic_candidate_revision_id, resolution.override_value, resolution.override_reason, resolution.final_rate, CALCULATION_VERSION],
            )
            .map_err(map_sqlite_error)?;
    }
    insert_audit(
        transaction,
        event_id,
        if supersedes.is_some() {
            "revise"
        } else {
            "post"
        },
        revision,
        reason,
    )?;
    event_order(transaction, event_id)
}

fn insert_detail(
    transaction: &Transaction<'_>,
    event_id: UuidV7,
    input: &CashEventInput,
    prepared: &PreparedCashEvent,
) -> ApplicationResult<()> {
    match &input.event_type {
        EventInputType::OpeningBalance => {
            transaction.execute(
                "INSERT INTO opening_balance_details(event_id,account_id,balance_amount,cutover_date,migration_policy) VALUES(?1,?2,?3,?4,?5)",
                params![event_id.to_string(), input.account_id.map(|id| id.to_string()), input.amount.as_ref().map(Decimal::as_str), input.cutover_date.as_ref().map(LocalDate::as_str), input.migration_policy],
            ).map_err(map_sqlite_error)?;
        }
        EventInputType::Income | EventInputType::Expense | EventInputType::Adjustment => {
            let detail_type = match input.event_type {
                EventInputType::Income => "income",
                EventInputType::Expense => "expense",
                _ => "balance_adjustment",
            };
            transaction.execute(
                "INSERT INTO income_expense_details(event_id,account_id,entry_type,category_id,amount,merchant,note,semantic_role) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![event_id.to_string(), input.account_id.map(|id| id.to_string()), detail_type, input.category_id.map(|id| id.to_string()), input.amount.as_ref().map(Decimal::as_str), input.merchant, input.note, input.semantic_role.as_str()],
            ).map_err(map_sqlite_error)?;
            if prepared
                .postings
                .iter()
                .any(|posting| posting.role == CashContributionRole::Fee)
            {
                transaction.execute(
                    "INSERT INTO cash_event_fees(event_id,fee_account_id,fee_amount) VALUES(?1,?2,?3)",
                    params![event_id.to_string(), input.fee_account_id.map(|id| id.to_string()), input.fee_amount.as_ref().map(Decimal::as_str)],
                ).map_err(map_sqlite_error)?;
            }
        }
        EventInputType::Transfer => {
            transaction.execute(
                "INSERT INTO transfer_details(event_id,from_account_id,to_account_id,amount) VALUES(?1,?2,?3,?4)",
                params![event_id.to_string(), input.from_account_id.map(|id| id.to_string()), input.to_account_id.map(|id| id.to_string()), input.amount.as_ref().map(Decimal::as_str)],
            ).map_err(map_sqlite_error)?;
        }
        EventInputType::CurrencyExchange => {
            transaction.execute(
                "INSERT INTO currency_exchange_details(event_id,from_account_id,to_account_id,from_amount,to_amount,fee_account_id,fee_amount) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![event_id.to_string(), input.from_account_id.map(|id| id.to_string()), input.to_account_id.map(|id| id.to_string()), input.amount.as_ref().map(Decimal::as_str), input.to_amount.as_ref().map(Decimal::as_str), input.fee_account_id.map(|id| id.to_string()), input.fee_amount.as_ref().map(Decimal::as_str)],
            ).map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}

fn insert_audit(
    transaction: &Transaction<'_>,
    event_id: UuidV7,
    action: &str,
    revision: u32,
    reason: Option<&str>,
) -> ApplicationResult<()> {
    transaction
        .execute(
            "INSERT INTO audit_events(audit_event_id,business_event_id,actor,action,entity_type,entity_id,entity_revision,occurred_at_utc,reason)
             VALUES(?1,?2,'local-user',?3,'business-event',?2,?4,CURRENT_TIMESTAMP,?5)",
            params![UuidV7::new()?.to_string(), event_id.to_string(), action, revision, reason],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn event_order(transaction: &Transaction<'_>, event_id: UuidV7) -> ApplicationResult<u64> {
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

type StoredPostingRow = (String, Decimal, Currency, Option<Decimal>);

fn load_posting_rows(
    connection: &Connection,
    event_id: &str,
) -> ApplicationResult<Vec<StoredPostingRow>> {
    let mut statement = connection
        .prepare(
            "SELECT account_id,quantity_delta,currency,base_value FROM ledger_postings WHERE event_id=?1 ORDER BY posting_ordinal",
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    statement
        .query_map([event_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|_| ApplicationError::TransactionFailed)?
        .map(|row| {
            let row = row.map_err(|_| ApplicationError::TransactionFailed)?;
            Ok((
                row.0,
                Decimal::parse(&row.1, DecimalUse::Internal)?,
                Currency::parse(&row.2)?,
                row.3
                    .as_deref()
                    .map(|value| Decimal::parse(value, DecimalUse::Internal))
                    .transpose()?,
            ))
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // Keeps the cash-derived rebuild in one transaction boundary.
pub(super) fn rebuild_cash_derived(
    transaction: &Transaction<'_>,
    watermark: u64,
) -> ApplicationResult<()> {
    transaction
        .execute_batch(
            "DELETE FROM cash_balance_projection;
             DELETE FROM monthly_cash_flow_projection;
             DELETE FROM cash_data_quality_projection;
             DELETE FROM expense_daily_event_bucket_projection;
             DELETE FROM expense_daily_summary_projection;
             DELETE FROM expense_daily_projection;",
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let rows = {
        let mut statement = transaction
            .prepare(
                "SELECT p.account_id,p.quantity_delta,p.currency,p.base_value,e.event_id,e.event_type,e.effective_date,p.posting_ordinal
                 FROM ledger_postings p JOIN business_events e ON e.event_id=p.event_id
                 WHERE p.posting_kind IN ('cash','cash-reversal')
                   AND e.event_order<=?1 AND e.event_type<>'Reversal'
                   AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id AND n.event_order<=?1)
                   AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id AND r.event_order<=?1)
                 ORDER BY e.effective_date,e.sequence,e.event_id,p.posting_ordinal",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        statement
            .query_map(
                [i64::try_from(watermark).map_err(|_| ApplicationError::TransactionFailed)?],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?
    };
    let mut balances = BTreeMap::<String, (Decimal, String)>::new();
    let mut monthly = BTreeMap::<(String, String), (Decimal, Decimal)>::new();
    let mut quality = BTreeSet::<(String, String, String)>::new();
    for (account, delta, currency, base_value, event_id, event_type, date, ordinal) in rows {
        let delta = Decimal::parse(&delta, DecimalUse::Internal)?;
        let entry = balances
            .entry(account)
            .or_insert_with(|| (Decimal::zero(DecimalUse::Internal), currency.clone()));
        entry.0 = entry.0.checked_add(&delta, DecimalUse::Internal)?;
        let is_flow = matches!(event_type.as_str(), "Income" | "Expense")
            || event_type == "CurrencyExchange" && ordinal == 3;
        if is_flow {
            let flow = monthly
                .entry((date[..7].to_owned(), currency.clone()))
                .or_insert_with(|| {
                    (
                        Decimal::zero(DecimalUse::Internal),
                        Decimal::zero(DecimalUse::Internal),
                    )
                });
            if delta.is_positive() {
                flow.0 = flow.0.checked_add(&delta, DecimalUse::Internal)?;
            }
            if delta.is_negative() {
                flow.1 = flow.1.checked_add(
                    &delta.checked_neg(DecimalUse::Internal)?,
                    DecimalUse::Internal,
                )?;
            }
        }
        if base_value.is_none() {
            quality.insert((event_id, currency, date));
        }
    }
    let wm = i64::try_from(watermark).map_err(|_| ApplicationError::TransactionFailed)?;
    for (account, (balance, currency)) in balances {
        transaction.execute(
            "INSERT INTO cash_balance_projection(account_id,balance,currency,event_watermark,calculation_version) VALUES(?1,?2,?3,?4,?5)",
            params![account,balance.as_str(),currency,wm,CALCULATION_VERSION],
        ).map_err(map_sqlite_error)?;
    }
    for ((month, currency), (income, expense)) in monthly {
        transaction.execute(
            "INSERT INTO monthly_cash_flow_projection(month,currency,income,expense,event_watermark,calculation_version) VALUES(?1,?2,?3,?4,?5,?6)",
            params![month,currency,income.as_str(),expense.as_str(),wm,CALCULATION_VERSION],
        ).map_err(map_sqlite_error)?;
    }
    for (event_id, currency, date) in quality {
        transaction.execute(
            "INSERT INTO cash_data_quality_projection(event_id,issue_code,currency,target_date,event_watermark,calculation_version) VALUES(?1,'MISSING_FX_RATE',?2,?3,?4,?5)",
            params![event_id,currency,date,wm,CALCULATION_VERSION],
        ).map_err(map_sqlite_error)?;
    }
    rebuild_expense_daily_projection(transaction, watermark)?;
    for name in [
        "cash-balance",
        "monthly-cash-flow",
        "cash-data-quality",
        "expense-daily",
    ] {
        transaction.execute(
            "UPDATE projection_metadata
             SET event_watermark=?1,
                 calculation_version=?2,
                 projection_version=CASE WHEN ?3='expense-daily' THEN ?4 ELSE projection_version END,
                 available=1,
                 rebuilt_at_utc=CURRENT_TIMESTAMP
             WHERE projection_name=?3",
            params![wm,CALCULATION_VERSION,name,EXPENSE_DAILY_PROJECTION_VERSION],
        ).map_err(map_sqlite_error)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Mirrors the three-table accepted projection contract.
fn rebuild_expense_daily_projection(
    transaction: &Transaction<'_>,
    watermark: u64,
) -> ApplicationResult<()> {
    let watermark_i64 =
        i64::try_from(watermark).map_err(|_| ApplicationError::TransactionFailed)?;
    let contributions = {
        let mut statement = transaction
            .prepare(
                "SELECT e.effective_date,e.event_id,
                        CASE WHEN d.entry_type='expense' THEN COALESCE(d.category_id,'system:uncategorized') ELSE NULL END,
                        CASE WHEN d.entry_type='expense' THEN 'expense' ELSE d.semantic_role END,
                        p.base_value
                 FROM income_expense_details d
                 JOIN business_events e ON e.event_id=d.event_id
                 JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=1
                 WHERE e.event_order<=?1 AND e.status='posted' AND e.event_type<>'Reversal'
                   AND (d.entry_type='expense' OR (d.entry_type='income' AND d.semantic_role IN ('refund','reimbursement')))
                   AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id AND n.event_order<=?1)
                   AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id AND r.event_order<=?1)
                 UNION ALL
                 SELECT e.effective_date,e.event_id,'system:ordinary-fee','expense',p.base_value
                 FROM cash_event_fees f
                 JOIN business_events e ON e.event_id=f.event_id
                 JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=2
                 WHERE e.event_order<=?1 AND e.status='posted' AND e.event_type<>'Reversal'
                   AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id AND n.event_order<=?1)
                   AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id AND r.event_order<=?1)
                 UNION ALL
                 SELECT e.effective_date,e.event_id,'system:fx-fee','expense',p.base_value
                 FROM currency_exchange_details x
                 JOIN business_events e ON e.event_id=x.event_id
                 JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=3
                 WHERE x.fee_amount IS NOT NULL AND e.event_order<=?1 AND e.status='posted' AND e.event_type<>'Reversal'
                   AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id AND n.event_order<=?1)
                   AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id AND r.event_order<=?1)
                 ORDER BY 1,2,3,4",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        statement
            .query_map([watermark_i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?
    };

    let mut summaries = BTreeMap::<(String, String, String), DailyExpenseAccumulator>::new();
    let mut buckets = BTreeMap::<(String, String, String), DailyExpenseAccumulator>::new();
    let mut event_buckets = BTreeSet::<(String, String, String, String)>::new();
    for (date, event_id, bucket_id, measure_role, base_value) in contributions {
        let valuation_state = if base_value.is_some() {
            "valued"
        } else {
            "unvalued"
        };
        let amount = base_value
            .as_deref()
            .map(|value| {
                let value = Decimal::parse(value, DecimalUse::Internal)?;
                let positive = if measure_role == "expense" {
                    value.checked_neg(DecimalUse::Internal)?
                } else {
                    value
                };
                RustDecimal::from_str_exact(positive.as_str())
                    .map_err(|_| ApplicationError::TransactionFailed)
            })
            .transpose()?
            .unwrap_or(RustDecimal::ZERO);
        let summary = summaries
            .entry((
                date.clone(),
                measure_role.clone(),
                valuation_state.to_owned(),
            ))
            .or_insert_with(DailyExpenseAccumulator::new);
        summary.amount = summary
            .amount
            .checked_add(amount)
            .ok_or(ApplicationError::TransactionFailed)?;
        summary.events.insert(event_id.clone());
        if measure_role == "expense" {
            let bucket_id = bucket_id.ok_or(ApplicationError::TransactionFailed)?;
            let bucket = buckets
                .entry((date.clone(), bucket_id.clone(), valuation_state.to_owned()))
                .or_insert_with(DailyExpenseAccumulator::new);
            bucket.amount = bucket
                .amount
                .checked_add(amount)
                .ok_or(ApplicationError::TransactionFailed)?;
            bucket.events.insert(event_id.clone());
            event_buckets.insert((date, event_id, bucket_id, valuation_state.to_owned()));
        }
    }

    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO expense_daily_summary_projection(
                   effective_date,measure_role,valuation_state,amount,distinct_event_count,event_watermark,calculation_version
                 ) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for ((date, role, valuation_state), aggregate) in summaries {
            statement
                .execute(params![
                    date,
                    role,
                    valuation_state,
                    aggregate.amount.normalize().to_string(),
                    i64::try_from(aggregate.events.len())
                        .map_err(|_| ApplicationError::TransactionFailed)?,
                    watermark_i64,
                    CALCULATION_VERSION
                ])
                .map_err(map_sqlite_error)?;
        }
    }
    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO expense_daily_projection(
                   effective_date,bucket_id,semantic_role,valuation_state,amount,distinct_event_count,event_watermark,calculation_version
                 ) VALUES(?1,?2,'normal',?3,?4,?5,?6,?7)",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for ((date, bucket_id, valuation_state), aggregate) in buckets {
            statement
                .execute(params![
                    date,
                    bucket_id,
                    valuation_state,
                    aggregate.amount.normalize().to_string(),
                    i64::try_from(aggregate.events.len())
                        .map_err(|_| ApplicationError::TransactionFailed)?,
                    watermark_i64,
                    CALCULATION_VERSION
                ])
                .map_err(map_sqlite_error)?;
        }
    }
    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO expense_daily_event_bucket_projection(
                   effective_date,event_id,bucket_id,valuation_state,event_watermark,calculation_version
                 ) VALUES(?1,?2,?3,?4,?5,?6)",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for (date, event_id, bucket_id, valuation_state) in event_buckets {
            statement
                .execute(params![
                    date,
                    event_id,
                    bucket_id,
                    valuation_state,
                    watermark_i64,
                    CALCULATION_VERSION
                ])
                .map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}

struct ExpenseAggregateRow {
    kind: String,
    bucket_id: Option<String>,
    label: Option<String>,
    archived: bool,
    signed_amount: Option<String>,
    distinct_count: u64,
    unvalued_count: u64,
}

#[allow(clippy::too_many_lines)] // Maps one snapshot into the versioned expense contract.
fn load_projected_expense_aggregates(
    connection: &Connection,
    start: &LocalDate,
    end: &LocalDate,
) -> ApplicationResult<Vec<ExpenseAggregateRow>> {
    let mut summary = BTreeMap::<(String, String), (RustDecimal, u64)>::new();
    {
        let mut statement = connection
            .prepare(
                "SELECT measure_role,valuation_state,amount,distinct_event_count
                 FROM expense_daily_summary_projection
                 WHERE effective_date BETWEEN ?1 AND ?2
                 ORDER BY measure_role,valuation_state,effective_date",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let rows = statement
            .query_map(params![start.as_str(), end.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for (role, state, amount, count) in rows {
            let entry = summary
                .entry((role, state))
                .or_insert((RustDecimal::ZERO, 0));
            entry.0 = entry
                .0
                .checked_add(
                    RustDecimal::from_str_exact(&amount)
                        .map_err(|_| ApplicationError::TransactionFailed)?,
                )
                .ok_or(ApplicationError::TransactionFailed)?;
            entry.1 = entry
                .1
                .checked_add(u64::try_from(count).map_err(|_| ApplicationError::TransactionFailed)?)
                .ok_or(ApplicationError::TransactionFailed)?;
        }
    }
    let summary_value = |role: &str, state: &str| {
        summary
            .get(&(role.to_owned(), state.to_owned()))
            .copied()
            .unwrap_or((RustDecimal::ZERO, 0))
    };
    let (expense_amount, expense_count) = summary_value("expense", "valued");
    let (_, expense_unvalued) = summary_value("expense", "unvalued");
    let (refund_amount, refund_count) = summary_value("refund", "valued");
    let (_, refund_unvalued) = summary_value("refund", "unvalued");
    let (reimbursement_amount, reimbursement_count) = summary_value("reimbursement", "valued");
    let (_, reimbursement_unvalued) = summary_value("reimbursement", "unvalued");
    let mut rows = vec![
        ExpenseAggregateRow {
            kind: "summary".to_owned(),
            bucket_id: None,
            label: None,
            archived: false,
            signed_amount: Some(negative_decimal_string(expense_amount)?),
            distinct_count: expense_count,
            unvalued_count: expense_unvalued,
        },
        ExpenseAggregateRow {
            kind: "refund".to_owned(),
            bucket_id: None,
            label: None,
            archived: false,
            signed_amount: Some(refund_amount.normalize().to_string()),
            distinct_count: refund_count,
            unvalued_count: refund_unvalued,
        },
        ExpenseAggregateRow {
            kind: "reimbursement".to_owned(),
            bucket_id: None,
            label: None,
            archived: false,
            signed_amount: Some(reimbursement_amount.normalize().to_string()),
            distinct_count: reimbursement_count,
            unvalued_count: reimbursement_unvalued,
        },
    ];

    let mut buckets = BTreeMap::<String, SqlBucketAccumulator>::new();
    {
        let mut statement = connection
            .prepare(
                "SELECT p.bucket_id,
                        CASE p.bucket_id
                          WHEN 'system:uncategorized' THEN 'Uncategorized'
                          WHEN 'system:ordinary-fee' THEN 'Ordinary fees'
                          WHEN 'system:fx-fee' THEN 'FX fees'
                          ELSE COALESCE(c.name,p.bucket_id)
                        END,
                        CASE WHEN c.category_id IS NOT NULL AND c.enabled=0 THEN 1 ELSE 0 END,
                        p.amount,p.distinct_event_count
                 FROM expense_daily_projection p
                 LEFT JOIN categories c ON c.category_id=p.bucket_id
                 WHERE p.effective_date BETWEEN ?1 AND ?2 AND p.valuation_state='valued'
                 ORDER BY p.bucket_id,p.effective_date",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let projected = statement
            .query_map(params![start.as_str(), end.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for (bucket_id, label, archived, amount, count) in projected {
            let bucket = buckets
                .entry(bucket_id)
                .or_insert_with(|| SqlBucketAccumulator {
                    label,
                    archived,
                    amount: RustDecimal::ZERO,
                    count: 0,
                });
            bucket.amount = bucket
                .amount
                .checked_add(
                    RustDecimal::from_str_exact(&amount)
                        .map_err(|_| ApplicationError::TransactionFailed)?,
                )
                .ok_or(ApplicationError::TransactionFailed)?;
            bucket.count = bucket
                .count
                .checked_add(u64::try_from(count).map_err(|_| ApplicationError::TransactionFailed)?)
                .ok_or(ApplicationError::TransactionFailed)?;
        }
    }
    for (bucket_id, bucket) in buckets {
        rows.push(ExpenseAggregateRow {
            kind: "bucket".to_owned(),
            bucket_id: Some(bucket_id),
            label: Some(bucket.label),
            archived: bucket.archived,
            signed_amount: Some(negative_decimal_string(bucket.amount)?),
            distinct_count: bucket.count,
            unvalued_count: 0,
        });
    }
    add_projected_top_other(connection, start, end, &mut rows)?;
    Ok(rows)
}

fn negative_decimal_string(value: RustDecimal) -> ApplicationResult<String> {
    Ok(
        Decimal::parse(&value.normalize().to_string(), DecimalUse::Internal)?
            .checked_neg(DecimalUse::Internal)?
            .normalized()
            .as_str()
            .to_owned(),
    )
}

fn add_projected_top_other(
    connection: &Connection,
    start: &LocalDate,
    end: &LocalDate,
    rows: &mut Vec<ExpenseAggregateRow>,
) -> ApplicationResult<()> {
    let mut ranked = rows
        .iter()
        .filter(|row| row.kind == "bucket")
        .map(|row| {
            Ok((
                row.bucket_id
                    .clone()
                    .ok_or(ApplicationError::TransactionFailed)?,
                Decimal::parse(
                    row.signed_amount
                        .as_deref()
                        .ok_or(ApplicationError::TransactionFailed)?,
                    DecimalUse::Internal,
                )?,
            ))
        })
        .collect::<ApplicationResult<Vec<_>>>()?;
    ranked.sort_by(|left, right| {
        left.1
            .numeric_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    if ranked.len() <= 10 {
        return Ok(());
    }
    let member_ids = ranked
        .iter()
        .skip(10)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let signed_amount = ranked
        .iter()
        .skip(10)
        .try_fold(Decimal::zero(DecimalUse::Internal), |sum, (_, amount)| {
            sum.checked_add(amount, DecimalUse::Internal)
        })?;
    let member_json =
        serde_json::to_string(&member_ids).map_err(|_| ApplicationError::TransactionFailed)?;
    let distinct_count: i64 = connection
        .query_row(
            "SELECT COUNT(DISTINCT event_id)
             FROM expense_daily_event_bucket_projection
             WHERE effective_date BETWEEN ?1 AND ?2
               AND valuation_state='valued'
               AND bucket_id IN (SELECT value FROM json_each(?3))",
            params![start.as_str(), end.as_str(), member_json],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    rows.push(ExpenseAggregateRow {
        kind: "other".to_owned(),
        bucket_id: Some("system:top10-other".to_owned()),
        label: Some("Other categories".to_owned()),
        archived: false,
        signed_amount: Some(signed_amount.normalized().as_str().to_owned()),
        distinct_count: u64::try_from(distinct_count)
            .map_err(|_| ApplicationError::TransactionFailed)?,
        unvalued_count: 0,
    });
    Ok(())
}

#[allow(clippy::too_many_lines)] // Historical-watermark fallback mirrors the expense policy.
fn load_expense_aggregates(
    connection: &Connection,
    start: &LocalDate,
    end: &LocalDate,
    watermark: u64,
) -> ApplicationResult<Vec<ExpenseAggregateRow>> {
    let watermark = i64::try_from(watermark).map_err(|_| ApplicationError::TransactionFailed)?;
    let has_revision_links: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM business_events WHERE event_order<=?1 AND (supersedes_event_id IS NOT NULL OR reverses_event_id IS NOT NULL))",
            [watermark],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let sql = "WITH contributions AS (
               SELECT
                 CASE WHEN d.entry_type='expense' THEN COALESCE(d.category_id,'system:uncategorized') ELSE NULL END bucket_id,
                 CASE WHEN d.entry_type='expense' THEN COALESCE(c.name,'Uncategorized') ELSE NULL END label,
                 CASE WHEN c.category_id IS NOT NULL AND c.enabled=0 THEN 1 ELSE 0 END archived,
                 d.semantic_role,p.base_value,
                 CASE WHEN d.semantic_role='normal' AND p.base_value IS NOT NULL THEN 1 ELSE 0 END global_weight,
                 CASE WHEN d.semantic_role='normal' AND p.base_value IS NULL THEN 1 ELSE 0 END unvalued_weight
               FROM income_expense_details d JOIN business_events e ON e.event_id=d.event_id
                 JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=1
                 LEFT JOIN categories c ON c.category_id=d.category_id
               WHERE e.effective_date BETWEEN ?1 AND ?2 AND e.event_order<=?3 AND e.status='posted' AND e.event_type<>'Reversal'
                 AND (d.entry_type='expense' OR (d.entry_type='income' AND d.semantic_role IN ('refund','reimbursement')))
                 /*EFFECTIVE*/
               UNION ALL
               SELECT 'system:ordinary-fee','Ordinary fees',0,'normal',p.base_value,
                 CASE WHEN p.base_value IS NULL THEN 0 WHEN e.event_type='Expense' AND principal.base_value IS NOT NULL THEN 0 ELSE 1 END,
                 CASE WHEN p.base_value IS NOT NULL THEN 0 WHEN e.event_type='Expense' AND principal.base_value IS NULL THEN 0 ELSE 1 END
               FROM cash_event_fees f JOIN business_events e ON e.event_id=f.event_id
                 JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=2
                 LEFT JOIN ledger_postings principal ON principal.event_id=e.event_id AND principal.posting_ordinal=1
               WHERE e.effective_date BETWEEN ?1 AND ?2 AND e.event_order<=?3 AND e.status='posted' AND e.event_type<>'Reversal'
                 /*EFFECTIVE*/
               UNION ALL
               SELECT 'system:fx-fee','FX fees',0,'normal',p.base_value,
                 CASE WHEN p.base_value IS NOT NULL THEN 1 ELSE 0 END,
                 CASE WHEN p.base_value IS NULL THEN 1 ELSE 0 END
               FROM currency_exchange_details x JOIN business_events e ON e.event_id=x.event_id
                 JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=3
               WHERE x.fee_amount IS NOT NULL AND e.effective_date BETWEEN ?1 AND ?2 AND e.event_order<=?3 AND e.status='posted' AND e.event_type<>'Reversal'
                 /*EFFECTIVE*/
             ) SELECT bucket_id,MAX(label),MAX(archived),semantic_role,base_value,COUNT(*),SUM(global_weight),SUM(unvalued_weight)
               FROM contributions GROUP BY bucket_id,semantic_role,base_value ORDER BY semantic_role,bucket_id,base_value";
    let effective = "AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id AND n.event_order<=?3)
                     AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id AND r.event_order<=?3)";
    let sql = sql.replace(
        "/*EFFECTIVE*/",
        if has_revision_links { effective } else { "" },
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let groups = statement
        .query_map(params![start.as_str(), end.as_str(), watermark], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })
        .map_err(|_| ApplicationError::TransactionFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    drop(statement);
    let mut payload = new_expense_accumulator();
    for (bucket_id, label, archived, role, base_value, count, global, unvalued) in groups {
        let count = u64::try_from(count).map_err(|_| ApplicationError::TransactionFailed)?;
        payload.global_count = payload
            .global_count
            .checked_add(u64::try_from(global).map_err(|_| ApplicationError::TransactionFailed)?)
            .ok_or(ApplicationError::TransactionFailed)?;
        payload.unvalued_count = payload
            .unvalued_count
            .checked_add(u64::try_from(unvalued).map_err(|_| ApplicationError::TransactionFailed)?)
            .ok_or(ApplicationError::TransactionFailed)?;
        let amount = base_value
            .as_deref()
            .map(RustDecimal::from_str_exact)
            .transpose()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let factor = RustDecimal::from_i128_with_scale(i128::from(count), 0);
        let aggregate = amount
            .map(|value| {
                value
                    .checked_mul(factor)
                    .ok_or(ApplicationError::TransactionFailed)
            })
            .transpose()?;
        match role.as_str() {
            "normal" => {
                if let Some(value) = aggregate {
                    payload.summary = payload
                        .summary
                        .checked_add(value)
                        .ok_or(ApplicationError::TransactionFailed)?;
                    if let Some(id) = bucket_id {
                        let bucket =
                            payload
                                .buckets
                                .entry(id)
                                .or_insert_with(|| SqlBucketAccumulator {
                                    label: label.unwrap_or_default(),
                                    archived,
                                    amount: RustDecimal::ZERO,
                                    count: 0,
                                });
                        bucket.amount = bucket
                            .amount
                            .checked_add(value)
                            .ok_or(ApplicationError::TransactionFailed)?;
                        bucket.count = bucket
                            .count
                            .checked_add(count)
                            .ok_or(ApplicationError::TransactionFailed)?;
                    }
                }
            }
            "refund" => {
                if let Some(value) = aggregate {
                    payload.refund = payload
                        .refund
                        .checked_add(value)
                        .ok_or(ApplicationError::TransactionFailed)?;
                    payload.refund_count = payload
                        .refund_count
                        .checked_add(count)
                        .ok_or(ApplicationError::TransactionFailed)?;
                } else {
                    payload.refund_unvalued = payload
                        .refund_unvalued
                        .checked_add(count)
                        .ok_or(ApplicationError::TransactionFailed)?;
                }
            }
            "reimbursement" => {
                if let Some(value) = aggregate {
                    payload.reimbursement = payload
                        .reimbursement
                        .checked_add(value)
                        .ok_or(ApplicationError::TransactionFailed)?;
                    payload.reimbursement_count = payload
                        .reimbursement_count
                        .checked_add(count)
                        .ok_or(ApplicationError::TransactionFailed)?;
                } else {
                    payload.reimbursement_unvalued = payload
                        .reimbursement_unvalued
                        .checked_add(count)
                        .ok_or(ApplicationError::TransactionFailed)?;
                }
            }
            _ => return Err(ApplicationError::TransactionFailed),
        }
    }
    let mut rows = vec![
        ExpenseAggregateRow {
            kind: "summary".to_owned(),
            bucket_id: None,
            label: None,
            archived: false,
            signed_amount: Some(payload.summary.to_string()),
            distinct_count: payload.global_count,
            unvalued_count: payload.unvalued_count,
        },
        ExpenseAggregateRow {
            kind: "refund".to_owned(),
            bucket_id: None,
            label: None,
            archived: false,
            signed_amount: Some(payload.refund.to_string()),
            distinct_count: payload.refund_count,
            unvalued_count: payload.refund_unvalued,
        },
        ExpenseAggregateRow {
            kind: "reimbursement".to_owned(),
            bucket_id: None,
            label: None,
            archived: false,
            signed_amount: Some(payload.reimbursement.to_string()),
            distinct_count: payload.reimbursement_count,
            unvalued_count: payload.reimbursement_unvalued,
        },
    ];
    rows.extend(
        payload
            .buckets
            .into_iter()
            .map(|(bucket_id, bucket)| ExpenseAggregateRow {
                kind: "bucket".to_owned(),
                bucket_id: Some(bucket_id),
                label: Some(bucket.label),
                archived: bucket.archived,
                signed_amount: Some(bucket.amount.to_string()),
                distinct_count: bucket.count,
                unvalued_count: 0,
            }),
    );
    add_top_other(connection, start, end, watermark, &mut rows)?;
    Ok(rows)
}

fn add_top_other(
    connection: &Connection,
    start: &LocalDate,
    end: &LocalDate,
    watermark: i64,
    rows: &mut Vec<ExpenseAggregateRow>,
) -> ApplicationResult<()> {
    let mut ranked = rows
        .iter()
        .filter(|row| row.kind == "bucket")
        .map(|row| {
            Ok((
                row.bucket_id
                    .clone()
                    .ok_or(ApplicationError::TransactionFailed)?,
                Decimal::parse(
                    row.signed_amount
                        .as_deref()
                        .ok_or(ApplicationError::TransactionFailed)?,
                    DecimalUse::Internal,
                )?,
            ))
        })
        .collect::<ApplicationResult<Vec<_>>>()?;
    ranked.sort_by(|left, right| {
        left.1
            .numeric_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    if ranked.len() <= 10 {
        return Ok(());
    }
    let member_ids: BTreeSet<_> = ranked.iter().skip(10).map(|(id, _)| id.clone()).collect();
    let signed_amount = ranked
        .iter()
        .skip(10)
        .try_fold(Decimal::zero(DecimalUse::Internal), |sum, (_, amount)| {
            sum.checked_add(amount, DecimalUse::Internal)
        })?;
    let watermark = u64::try_from(watermark).map_err(|_| ApplicationError::TransactionFailed)?;
    let contributions =
        load_contributions(connection, start, end, watermark, None, None, None, None)?;
    let distinct_count = contributions
        .into_iter()
        .filter(|row| {
            row.base_value.is_some()
                && row
                    .bucket_id
                    .as_ref()
                    .is_some_and(|id| member_ids.contains(id))
        })
        .map(|row| row.event_id)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    rows.push(ExpenseAggregateRow {
        kind: "other".to_owned(),
        bucket_id: Some("system:top10-other".to_owned()),
        label: Some("Other categories".to_owned()),
        archived: false,
        signed_amount: Some(signed_amount.as_str().to_owned()),
        distinct_count,
        unvalued_count: 0,
    });
    Ok(())
}

#[allow(clippy::too_many_lines)] // Mirrors the accepted canonical result contract.
fn build_aggregated_expense_result(
    start: &LocalDate,
    end: &LocalDate,
    base_currency: Currency,
    event_watermark: u64,
    master_data_watermark: u64,
    rows: Vec<ExpenseAggregateRow>,
) -> ApplicationResult<ExpenseAnalysis> {
    let mut summary_amount = Decimal::zero(DecimalUse::Internal);
    let mut global_count = 0;
    let mut unvalued_count = 0;
    let mut refunds = (Decimal::zero(DecimalUse::Internal), 0, 0);
    let mut reimbursements = (Decimal::zero(DecimalUse::Internal), 0, 0);
    let mut buckets = Vec::<(String, BucketAggregate)>::new();
    let mut other: Option<(Decimal, u64)> = None;
    for row in rows {
        let row_distinct_count = row.distinct_count;
        let amount = row
            .signed_amount
            .as_deref()
            .map(|value| Decimal::parse(value, DecimalUse::Internal))
            .transpose()?
            .unwrap_or_else(|| Decimal::zero(DecimalUse::Internal));
        match row.kind.as_str() {
            "summary" => {
                summary_amount = amount.checked_neg(DecimalUse::Internal)?;
                global_count = row.distinct_count;
                unvalued_count = row.unvalued_count;
            }
            "refund" => refunds = (amount, row.distinct_count, row.unvalued_count),
            "reimbursement" => {
                reimbursements = (amount, row.distinct_count, row.unvalued_count);
            }
            "bucket" => buckets.push((
                row.bucket_id.ok_or(ApplicationError::TransactionFailed)?,
                BucketAggregate {
                    label: row.label.ok_or(ApplicationError::TransactionFailed)?,
                    amount: amount.checked_neg(DecimalUse::Internal)?,
                    events: BTreeSet::new(),
                    distinct_count_override: Some(row_distinct_count),
                    archived: row.archived,
                },
            )),
            "other" => {
                other = Some((
                    amount.checked_neg(DecimalUse::Internal)?,
                    row_distinct_count,
                ));
            }
            _ => return Err(ApplicationError::TransactionFailed),
        }
    }
    buckets.sort_by(|left, right| {
        right
            .1
            .amount
            .numeric_cmp(&left.1.amount)
            .then_with(|| left.0.cmp(&right.0))
    });
    let make_bucket = |id: &str, value: &BucketAggregate| ExpenseBucket {
        bucket_id: id.to_owned(),
        bucket_kind: if id.starts_with("system:") {
            "system"
        } else {
            "category"
        },
        label: value.label.clone(),
        archived: value.archived,
        amount: value.amount.as_str().to_owned(),
        distinct_event_count: value.distinct_count(),
        drilldown_context: context(
            start,
            end,
            event_watermark,
            Some(id.to_owned()),
            None,
            None,
            "valued",
        ),
    };
    let bucket_rows: Vec<_> = buckets
        .iter()
        .map(|(id, value)| make_bucket(id, value))
        .collect();
    let positive: Vec<_> = buckets
        .iter()
        .filter(|(_, value)| value.amount.is_positive())
        .collect();
    let top_items = positive
        .iter()
        .take(10)
        .map(|(id, value)| ExpenseTopItem {
            bucket_id: (*id).clone(),
            label: value.label.clone(),
            amount: value.amount.as_str().to_owned(),
            distinct_event_count: value.distinct_count(),
            drilldown_context: context(
                start,
                end,
                event_watermark,
                Some((*id).clone()),
                None,
                None,
                "valued",
            ),
        })
        .collect();
    let other = other.map(|(amount, count)| ExpenseTopItem {
        bucket_id: "system:top10-other".to_owned(),
        label: "Other categories".to_owned(),
        amount: amount.as_str().to_owned(),
        distinct_event_count: count,
        drilldown_context: context(
            start,
            end,
            event_watermark,
            Some("system:top10-other".to_owned()),
            None,
            Some(10),
            "valued",
        ),
    });
    let largest_category = positive
        .iter()
        .find(|(id, _)| !id.starts_with("system:"))
        .map(|(id, value)| LargestCategory {
            bucket_id: (*id).clone(),
            amount: value.amount.as_str().to_owned(),
        });
    let measure = |role: &str, value: &(Decimal, u64, u64)| ExpenseMeasure {
        amount: value.0.as_str().to_owned(),
        distinct_event_count: value.1,
        unvalued_count: value.2,
        drilldown_context: context(
            start,
            end,
            event_watermark,
            None,
            Some(role.to_owned()),
            None,
            "all",
        ),
    };
    let mut result = ExpenseAnalysis {
        contract: "expense-analysis-query-result/v1",
        query: ExpenseQueryContract {
            start_date: start.as_str().to_owned(),
            end_date: end.as_str().to_owned(),
            base_currency: base_currency.to_string(),
        },
        summary: ExpenseSummary {
            label: "Total expense",
            total_expense: (unvalued_count == 0)
                .then(|| summary_amount.normalized().as_str().to_owned()),
            valued_subtotal: summary_amount.normalized().as_str().to_owned(),
            global_distinct_event_count: global_count,
            largest_category,
        },
        buckets: bucket_rows,
        top10: ExpenseTop10 {
            items: top_items,
            other,
        },
        refunds: RefundMeasures {
            refund: measure("refund", &refunds),
            reimbursement: measure("reimbursement", &reimbursements),
        },
        unvalued: UnvaluedExpense {
            expense_count: unvalued_count,
            drilldown_context: context(
                start,
                end,
                event_watermark,
                None,
                Some("expense".to_owned()),
                None,
                "unvalued",
            ),
        },
        watermarks: ExpenseWatermarks {
            event: event_watermark,
            master_data: master_data_watermark,
        },
        versions: ExpenseVersions {
            calculation: CALCULATION_VERSION,
            expense_policy: EXPENSE_POLICY_VERSION,
            bucket_policy: BUCKET_POLICY_VERSION,
            refund_policy: REFUND_POLICY_VERSION,
        },
        canonicalization: CANONICALIZATION_ID,
        canonical_hash: String::new(),
    };
    let canonical =
        serde_json::to_value(&result).map_err(|_| ApplicationError::TransactionFailed)?;
    result.canonical_hash = canonical_hash(&canonical)?;
    if serde_json::to_vec(&result)
        .map_err(|_| ApplicationError::TransactionFailed)?
        .len()
        > MAX_RESPONSE_BYTES
    {
        return Err(ApplicationError::ResponseTooLarge);
    }
    Ok(result)
}

fn load_top_other_member_ids(
    connection: &Connection,
    start: &LocalDate,
    end: &LocalDate,
    watermark: u64,
    member_rank_gt: u32,
) -> ApplicationResult<Vec<String>> {
    let current: u64 = connection
        .query_row(
            "SELECT COALESCE(MAX(event_order),0) FROM business_events",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|_| ApplicationError::TransactionFailed)?
        .try_into()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let projection_ready = watermark == current
        && connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM projection_metadata
                   WHERE projection_name='expense-daily'
                     AND projection_version=?1
                     AND calculation_version=?2
                     AND event_watermark=?3
                     AND available=1
                 )",
                params![
                    EXPENSE_DAILY_PROJECTION_VERSION,
                    CALCULATION_VERSION,
                    i64::try_from(watermark).map_err(|_| ApplicationError::TransactionFailed)?
                ],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
    let rows = if projection_ready {
        load_projected_expense_aggregates(connection, start, end)?
    } else {
        load_expense_aggregates(connection, start, end, watermark)?
    };
    let mut ranked = rows
        .into_iter()
        .filter(|row| row.kind == "bucket")
        .map(|row| {
            Ok((
                row.bucket_id.ok_or(ApplicationError::TransactionFailed)?,
                Decimal::parse(
                    row.signed_amount
                        .as_deref()
                        .ok_or(ApplicationError::TransactionFailed)?,
                    DecimalUse::Internal,
                )?,
            ))
        })
        .collect::<ApplicationResult<Vec<_>>>()?;
    ranked.sort_by(|left, right| {
        left.1
            .numeric_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    Ok(ranked
        .into_iter()
        .skip(usize::try_from(member_rank_gt).map_err(|_| ApplicationError::TransactionFailed)?)
        .map(|(id, _)| id)
        .collect())
}

#[derive(Clone)]
#[allow(dead_code)] // Labels are retained for the fixture-level fact-row oracle below.
struct ContributionRow {
    event_id: String,
    event_order: u64,
    event_type: String,
    effective_date: String,
    bucket_id: Option<String>,
    label: Option<String>,
    category_id: Option<String>,
    archived: bool,
    semantic_role: String,
    base_value: Option<String>,
    native_amount: String,
    currency: String,
}

fn load_general_activity_ids(
    connection: &Connection,
    query: &ActivityQuery,
    watermark: u64,
) -> ApplicationResult<Vec<(String, u64)>> {
    let search = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if search.is_some_and(|value| value.chars().count() > 200) {
        return Err(ApplicationError::ActivityFilterInvalid);
    }
    let search = search.map(|value| {
        let escaped = value
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_");
        format!("%{escaped}%")
    });
    let cursor = i64::try_from(query.cursor.unwrap_or(i64::MAX as u64))
        .map_err(|_| ApplicationError::ActivityCursorInvalid)?;
    let limit = i64::from(query.limit) + 1;
    let watermark =
        i64::try_from(watermark).map_err(|_| ApplicationError::ActivityCursorInvalid)?;
    let event_type = query.event_type.as_deref();
    let account_id = query.account_id.map(|value| value.to_string());
    let category_id = query.category_id.as_deref();
    let mut statement = connection
        .prepare(
            "SELECT e.event_id,e.event_order
             FROM business_events e
             LEFT JOIN income_expense_details d ON d.event_id=e.event_id
             LEFT JOIN cash_event_fees f ON f.event_id=e.event_id
             LEFT JOIN currency_exchange_details x ON x.event_id=e.event_id
             WHERE e.effective_date BETWEEN ?1 AND ?2
               AND e.event_order<=?3 AND e.event_order<?4 AND e.status='posted'
               AND (?5 IS NULL OR e.event_type=?5)
               AND (?6 IS NULL OR EXISTS(
                     SELECT 1 FROM ledger_postings p
                     WHERE p.event_id=e.event_id AND p.account_id=?6
                   ))
               AND (?7 IS NULL
                    OR d.category_id=?7
                    OR (?7='system:uncategorized' AND d.entry_type='expense' AND d.category_id IS NULL)
                    OR (?7='system:ordinary-fee' AND f.event_id IS NOT NULL)
                    OR (?7='system:fx-fee' AND x.fee_amount IS NOT NULL))
               AND (?8 IS NULL OR lower(
                     e.event_type||' '||e.event_id||' '||
                     COALESCE(d.merchant,'')||' '||COALESCE(d.note,'')
                   ) LIKE lower(?8) ESCAPE '!')
             ORDER BY e.event_order DESC
             LIMIT ?9",
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    statement
        .query_map(
            params![
                query.start_date.as_str(),
                query.end_date.as_str(),
                watermark,
                cursor,
                event_type,
                account_id,
                category_id,
                search,
                limit
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                ))
            },
        )
        .map_err(|_| ApplicationError::TransactionFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::TransactionFailed)
}

#[allow(clippy::too_many_lines)] // One bounded hydration query returns the complete activity detail contract.
fn load_activity_items(
    connection: &Connection,
    ids: &[(String, u64)],
) -> ApplicationResult<Vec<ActivityItem>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids_json =
        serde_json::to_string(&ids.iter().map(|(event_id, _)| event_id).collect::<Vec<_>>())
            .map_err(|_| ApplicationError::TransactionFailed)?;
    let mut statement = connection
        .prepare(
            "SELECT e.event_id,e.event_order,e.event_type,e.effective_date,e.sequence,e.revision,
                    COALESCE(ob.account_id,d.account_id),
                    COALESCE(t.from_account_id,x.from_account_id),
                    COALESCE(t.to_account_id,x.to_account_id),
                    COALESCE(ob.balance_amount,d.amount,t.amount,x.from_amount),
                    x.to_amount,d.category_id,COALESCE(d.semantic_role,'normal'),d.merchant,d.note,
                    COALESCE(f.fee_account_id,x.fee_account_id),COALESCE(f.fee_amount,x.fee_amount),
                    ob.cutover_date,ob.migration_policy,
                    e.supersedes_event_id,e.reverses_event_id,
                    (SELECT n.event_id FROM business_events n WHERE n.supersedes_event_id=e.event_id ORDER BY n.event_order LIMIT 1),
                    (SELECT r.event_id FROM business_events r WHERE r.reverses_event_id=e.event_id ORDER BY r.event_order LIMIT 1),
                    COALESCE((SELECT a.action FROM audit_events a WHERE a.business_event_id=e.event_id ORDER BY a.occurred_at_utc DESC,a.audit_event_id DESC LIMIT 1),'post'),
                    COALESCE((SELECT a.occurred_at_utc FROM audit_events a WHERE a.business_event_id=e.event_id ORDER BY a.occurred_at_utc DESC,a.audit_event_id DESC LIMIT 1),e.created_at_utc),
                    COALESCE((SELECT a.reason FROM audit_events a WHERE a.business_event_id=e.event_id ORDER BY a.occurred_at_utc DESC,a.audit_event_id DESC LIMIT 1),e.revision_reason),
                    COALESCE((
                      SELECT json_group_array(json_object(
                        'postingKind',ordered.posting_kind,
                        'accountId',ordered.account_id,
                        'quantityDelta',ordered.quantity_delta,
                        'currency',ordered.currency,
                        'baseValue',ordered.base_value,
                        'baseCurrency',ordered.base_currency
                      )) FROM (
                        SELECT posting_kind,account_id,quantity_delta,currency,base_value,base_currency
                        FROM ledger_postings WHERE event_id=e.event_id ORDER BY posting_ordinal
                      ) ordered
                    ),'[]'),
                    COALESCE((
                      SELECT json_group_array(json_object(
                        'purpose',ordered.purpose,
                        'currency',ordered.currency,
                        'baseCurrency',ordered.base_currency,
                        'targetDate',ordered.target_date,
                        'automaticCandidateRevisionId',ordered.auto_rate_revision_id,
                        'overrideValue',ordered.override_value,
                        'overrideReason',ordered.override_reason,
                        'finalRate',ordered.final_rate,
                        'calculationVersion',ordered.calculation_version
                      )) FROM (
                        SELECT purpose,currency,base_currency,target_date,auto_rate_revision_id,
                               override_value,override_reason,final_rate,calculation_version
                        FROM fx_resolutions WHERE owner_type='event' AND owner_id=e.event_id
                        ORDER BY purpose,currency
                      ) ordered
                    ),'[]')
             FROM business_events e
             LEFT JOIN opening_balance_details ob ON ob.event_id=e.event_id
             LEFT JOIN income_expense_details d ON d.event_id=e.event_id
             LEFT JOIN cash_event_fees f ON f.event_id=e.event_id
             LEFT JOIN transfer_details t ON t.event_id=e.event_id
             LEFT JOIN currency_exchange_details x ON x.event_id=e.event_id
             WHERE e.event_id IN (SELECT value FROM json_each(?1))
             ORDER BY e.event_order DESC",
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let raw = statement
        .query_map([ids_json], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, Option<String>>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, Option<String>>(16)?,
                row.get::<_, Option<String>>(17)?,
                row.get::<_, Option<String>>(18)?,
                row.get::<_, Option<String>>(19)?,
                row.get::<_, Option<String>>(20)?,
                row.get::<_, Option<String>>(21)?,
                row.get::<_, Option<String>>(22)?,
                row.get::<_, String>(23)?,
                row.get::<_, String>(24)?,
                row.get::<_, Option<String>>(25)?,
                row.get::<_, String>(26)?,
                row.get::<_, String>(27)?,
            ))
        })
        .map_err(|_| ApplicationError::TransactionFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    raw.into_iter()
        .map(|row| {
            let postings: Vec<ActivityPosting> =
                serde_json::from_str(&row.26).map_err(|_| ApplicationError::TransactionFailed)?;
            let fx_resolutions: Vec<ActivityFxResolution> =
                serde_json::from_str(&row.27).map_err(|_| ApplicationError::TransactionFailed)?;
            let reversal_preview = if row.2 == "Reversal" {
                Vec::new()
            } else {
                postings
                    .iter()
                    .map(|posting| {
                        Ok(ActivityPosting {
                            posting_kind: "cash-reversal".to_owned(),
                            account_id: posting.account_id.clone(),
                            quantity_delta: Decimal::parse(
                                &posting.quantity_delta,
                                DecimalUse::Internal,
                            )?
                            .checked_neg(DecimalUse::Internal)?
                            .as_str()
                            .to_owned(),
                            currency: posting.currency.clone(),
                            base_value: posting
                                .base_value
                                .as_deref()
                                .map(|value| {
                                    Ok::<String, ApplicationError>(
                                        Decimal::parse(value, DecimalUse::Internal)?
                                            .checked_neg(DecimalUse::Internal)?
                                            .as_str()
                                            .to_owned(),
                                    )
                                })
                                .transpose()?,
                            base_currency: posting.base_currency.clone(),
                        })
                    })
                    .collect::<ApplicationResult<Vec<_>>>()?
            };
            Ok(ActivityItem {
                event_id: row.0,
                event_order: u64::try_from(row.1)
                    .map_err(|_| ApplicationError::TransactionFailed)?,
                event_type: row.2,
                effective_date: row.3,
                sequence: u64::try_from(row.4).map_err(|_| ApplicationError::TransactionFailed)?,
                revision: u32::try_from(row.5).map_err(|_| ApplicationError::TransactionFailed)?,
                content: ActivityEventContent {
                    account_id: row.6,
                    from_account_id: row.7,
                    to_account_id: row.8,
                    amount: row.9,
                    to_amount: row.10,
                    category_id: row.11,
                    semantic_role: row.12,
                    merchant: row.13,
                    note: row.14,
                    fee_account_id: row.15,
                    fee_amount: row.16,
                    cutover_date: row.17,
                    migration_policy: row.18,
                },
                relations: ActivityRelations {
                    supersedes_event_id: row.19,
                    reverses_event_id: row.20,
                    superseded_by_event_id: row.21,
                    reversed_by_event_id: row.22,
                },
                audit: ActivityAudit {
                    action: row.23,
                    occurred_at_utc: row.24,
                    reason: row.25,
                },
                postings,
                reversal_preview,
                fx_resolutions,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)] // Query bounds and drilldown filters remain explicit.
fn load_contributions(
    connection: &Connection,
    start: &LocalDate,
    end: &LocalDate,
    watermark: u64,
    cursor: Option<u64>,
    limit: Option<u64>,
    context: Option<&DrilldownContext>,
    other_members: Option<&[String]>,
) -> ApplicationResult<Vec<ContributionRow>> {
    let wm = i64::try_from(watermark).map_err(|_| ApplicationError::TransactionFailed)?;
    let cursor = i64::try_from(cursor.unwrap_or(i64::MAX as u64))
        .map_err(|_| ApplicationError::ActivityCursorInvalid)?;
    let limit = i64::try_from(limit.unwrap_or(i64::MAX as u64))
        .map_err(|_| ApplicationError::ActivityLimitInvalid)?;
    let bucket_id = context.and_then(|value| value.bucket_id.as_deref());
    let semantic_role = context.and_then(|value| value.semantic_role.as_deref());
    let valuation_state = context.map_or("all", |value| value.valuation_state.as_str());
    let other_members = serde_json::to_string(other_members.unwrap_or_default())
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let mut statement = connection.prepare(
        "WITH effective AS (
           SELECT e.* FROM business_events e WHERE e.event_order<=?3 AND e.event_order<?4 AND e.status='posted' AND e.event_type<>'Reversal'
             AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id AND n.event_order<=?3)
             AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id AND r.event_order<=?3)
         ), contributions AS (
           SELECT e.event_id,e.event_order,e.event_type,e.effective_date,
             CASE WHEN d.entry_type='expense' THEN COALESCE(d.category_id,'system:uncategorized') ELSE NULL END bucket_id,
             c.name,c.category_id,CASE WHEN c.category_id IS NOT NULL AND c.enabled=0 THEN 1 ELSE 0 END archived,
             d.semantic_role,p.base_value,d.amount native_amount,p.currency
           FROM effective e JOIN income_expense_details d ON d.event_id=e.event_id
             JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=1
             LEFT JOIN categories c ON c.category_id=d.category_id
           WHERE e.effective_date BETWEEN ?1 AND ?2 AND (d.entry_type='expense' OR (d.entry_type='income' AND d.semantic_role IN ('refund','reimbursement')))
           UNION ALL
             SELECT e.event_id,e.event_order,e.event_type,e.effective_date,'system:ordinary-fee','Ordinary fees',NULL,0,'normal',p.base_value,f.fee_amount,p.currency
           FROM effective e JOIN cash_event_fees f ON f.event_id=e.event_id
             JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=2
           WHERE e.effective_date BETWEEN ?1 AND ?2
           UNION ALL
             SELECT e.event_id,e.event_order,e.event_type,e.effective_date,'system:fx-fee','FX fees',NULL,0,'normal',p.base_value,x.fee_amount,p.currency
           FROM effective e JOIN currency_exchange_details x ON x.event_id=e.event_id AND x.fee_amount IS NOT NULL
             JOIN ledger_postings p ON p.event_id=e.event_id AND p.posting_ordinal=3
           WHERE e.effective_date BETWEEN ?1 AND ?2
         ), filtered AS (
           SELECT contributions.*,
             ROW_NUMBER() OVER (
               PARTITION BY event_id
               ORDER BY COALESCE(bucket_id,''),semantic_role,currency,native_amount
             ) AS contribution_rank
           FROM contributions
           WHERE (?6 IS NULL OR bucket_id=?6 OR (
                    ?6='system:top10-other'
                    AND bucket_id IN (SELECT value FROM json_each(?9))
                  ))
             AND (?7 IS NULL OR semantic_role=?7)
             AND (?8='all'
                  OR (?8='valued' AND base_value IS NOT NULL)
                  OR (?8='unvalued' AND base_value IS NULL))
         ) SELECT event_id,event_order,event_type,effective_date,bucket_id,name,category_id,archived,semantic_role,base_value,native_amount,currency
           FROM filtered WHERE contribution_rank=1
           ORDER BY event_order DESC,bucket_id LIMIT ?5"
    ).map_err(|_| ApplicationError::TransactionFailed)?;
    statement
        .query_map(
            params![
                start.as_str(),
                end.as_str(),
                wm,
                cursor,
                limit,
                bucket_id,
                semantic_role,
                valuation_state,
                other_members
            ],
            |row| {
                Ok(ContributionRow {
                    event_id: row.get(0)?,
                    event_order: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                    event_type: row.get(2)?,
                    effective_date: row.get(3)?,
                    bucket_id: row.get(4)?,
                    label: row.get(5)?,
                    category_id: row.get(6)?,
                    archived: row.get(7)?,
                    semantic_role: row.get(8)?,
                    base_value: row.get(9)?,
                    native_amount: row.get(10)?,
                    currency: row.get(11)?,
                })
            },
        )
        .map_err(|_| ApplicationError::TransactionFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::TransactionFailed)
}

struct BucketAggregate {
    label: String,
    amount: Decimal,
    events: BTreeSet<String>,
    distinct_count_override: Option<u64>,
    archived: bool,
}

impl BucketAggregate {
    fn distinct_count(&self) -> u64 {
        self.distinct_count_override
            .unwrap_or(self.events.len() as u64)
    }
}

fn context(
    start: &LocalDate,
    end: &LocalDate,
    event_watermark: u64,
    bucket_id: Option<String>,
    semantic_role: Option<String>,
    member_rank_gt: Option<u32>,
    valuation_state: &str,
) -> DrilldownContext {
    DrilldownContext {
        start_date: start.as_str().to_owned(),
        end_date: end.as_str().to_owned(),
        event_watermark,
        calculation_version: CALCULATION_VERSION,
        expense_policy_version: EXPENSE_POLICY_VERSION,
        bucket_id,
        semantic_role,
        member_rank_gt,
        valuation_state: valuation_state.to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines, dead_code)] // Independent fact-row oracle for golden tests.
fn build_expense_result(
    start: &LocalDate,
    end: &LocalDate,
    base_currency: Currency,
    event_watermark: u64,
    master_data_watermark: u64,
    rows: Vec<ContributionRow>,
) -> ApplicationResult<ExpenseAnalysis> {
    let mut buckets = BTreeMap::<String, BucketAggregate>::new();
    let mut global_events = BTreeSet::new();
    let mut unvalued_expense = BTreeSet::new();
    let mut refunds = (
        Decimal::zero(DecimalUse::Internal),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    let mut reimbursements = (
        Decimal::zero(DecimalUse::Internal),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    for row in rows {
        if matches!(row.semantic_role.as_str(), "refund" | "reimbursement") {
            let target = if row.semantic_role == "refund" {
                &mut refunds
            } else {
                &mut reimbursements
            };
            if let Some(base) = row.base_value {
                let amount = Decimal::parse(&base, DecimalUse::Internal)?;
                target.0 = target.0.checked_add(&amount, DecimalUse::Internal)?;
                target.1.insert(row.event_id);
            } else {
                target.2.insert(row.event_id);
            }
            continue;
        }
        let bucket_id = row.bucket_id.ok_or(ApplicationError::TransactionFailed)?;
        let Some(base) = row.base_value else {
            unvalued_expense.insert(row.event_id);
            continue;
        };
        let amount =
            Decimal::parse(&base, DecimalUse::Internal)?.checked_neg(DecimalUse::Internal)?;
        global_events.insert(row.event_id.clone());
        let entry = buckets
            .entry(bucket_id.clone())
            .or_insert_with(|| BucketAggregate {
                label: row.label.unwrap_or_else(|| {
                    if bucket_id == "system:uncategorized" {
                        "Uncategorized".to_owned()
                    } else {
                        bucket_id.clone()
                    }
                }),
                amount: Decimal::zero(DecimalUse::Internal),
                events: BTreeSet::new(),
                distinct_count_override: None,
                archived: row.archived,
            });
        entry.amount = entry.amount.checked_add(&amount, DecimalUse::Internal)?;
        entry.events.insert(row.event_id);
    }
    let valued_subtotal = buckets
        .values()
        .try_fold(Decimal::zero(DecimalUse::Internal), |sum, bucket| {
            sum.checked_add(&bucket.amount, DecimalUse::Internal)
        })?;
    let mut ordered: Vec<_> = buckets.into_iter().collect();
    ordered.sort_by(|left, right| {
        right
            .1
            .amount
            .numeric_cmp(&left.1.amount)
            .then_with(|| left.0.cmp(&right.0))
    });
    let make_bucket = |id: &str, value: &BucketAggregate| ExpenseBucket {
        bucket_id: id.to_owned(),
        bucket_kind: if id.starts_with("system:") {
            "system"
        } else {
            "category"
        },
        label: value.label.clone(),
        amount: value.amount.as_str().to_owned(),
        distinct_event_count: value.distinct_count(),
        archived: value.archived,
        drilldown_context: context(
            start,
            end,
            event_watermark,
            Some(id.to_owned()),
            None,
            None,
            "valued",
        ),
    };
    let bucket_rows: Vec<_> = ordered
        .iter()
        .map(|(id, value)| make_bucket(id, value))
        .collect();
    let positive: Vec<_> = ordered
        .iter()
        .filter(|(_, value)| value.amount.is_positive())
        .collect();
    let make_top =
        |id: &str, value: &BucketAggregate, member_rank_gt: Option<u32>| ExpenseTopItem {
            bucket_id: id.to_owned(),
            label: value.label.clone(),
            amount: value.amount.as_str().to_owned(),
            distinct_event_count: value.distinct_count(),
            drilldown_context: context(
                start,
                end,
                event_watermark,
                Some(id.to_owned()),
                None,
                member_rank_gt,
                "valued",
            ),
        };
    let top_items: Vec<_> = positive
        .iter()
        .take(10)
        .map(|(id, value)| make_top(id, value, None))
        .collect();
    let other = if positive.len() > 10 {
        let mut other = BucketAggregate {
            label: "Other categories".to_owned(),
            amount: Decimal::zero(DecimalUse::Internal),
            events: BTreeSet::new(),
            distinct_count_override: None,
            archived: false,
        };
        for (_, value) in positive.iter().skip(10) {
            other.amount = other
                .amount
                .checked_add(&value.amount, DecimalUse::Internal)?;
            other.events.extend(value.events.iter().cloned());
        }
        Some(make_top("system:top10-other", &other, Some(10)))
    } else {
        None
    };
    let largest_category = positive
        .iter()
        .find(|(id, _)| !id.starts_with("system:"))
        .map(|(id, value)| LargestCategory {
            bucket_id: (*id).clone(),
            amount: value.amount.as_str().to_owned(),
        });
    let measure =
        |role: &str, value: &(Decimal, BTreeSet<String>, BTreeSet<String>)| ExpenseMeasure {
            amount: value.0.as_str().to_owned(),
            distinct_event_count: value.1.len() as u64,
            unvalued_count: value.2.len() as u64,
            drilldown_context: context(
                start,
                end,
                event_watermark,
                None,
                Some(role.to_owned()),
                None,
                "all",
            ),
        };
    let mut result = ExpenseAnalysis {
        contract: "expense-analysis-query-result/v1",
        query: ExpenseQueryContract {
            start_date: start.as_str().to_owned(),
            end_date: end.as_str().to_owned(),
            base_currency: base_currency.to_string(),
        },
        summary: ExpenseSummary {
            label: "Total expense",
            total_expense: unvalued_expense
                .is_empty()
                .then(|| valued_subtotal.normalized().as_str().to_owned()),
            valued_subtotal: valued_subtotal.normalized().as_str().to_owned(),
            global_distinct_event_count: global_events.len() as u64,
            largest_category,
        },
        buckets: bucket_rows,
        top10: ExpenseTop10 {
            items: top_items,
            other,
        },
        refunds: RefundMeasures {
            refund: measure("refund", &refunds),
            reimbursement: measure("reimbursement", &reimbursements),
        },
        unvalued: UnvaluedExpense {
            expense_count: unvalued_expense.len() as u64,
            drilldown_context: context(
                start,
                end,
                event_watermark,
                None,
                Some("expense".to_owned()),
                None,
                "unvalued",
            ),
        },
        watermarks: ExpenseWatermarks {
            event: event_watermark,
            master_data: master_data_watermark,
        },
        versions: ExpenseVersions {
            calculation: CALCULATION_VERSION,
            expense_policy: EXPENSE_POLICY_VERSION,
            bucket_policy: BUCKET_POLICY_VERSION,
            refund_policy: REFUND_POLICY_VERSION,
        },
        canonicalization: CANONICALIZATION_ID,
        canonical_hash: String::new(),
    };
    let mut canonical =
        serde_json::to_value(&result).map_err(|_| ApplicationError::TransactionFailed)?;
    if let Some(object) = canonical.as_object_mut() {
        object.remove("canonical_hash");
    }
    result.canonical_hash = canonical_hash(&canonical)?;
    if serde_json::to_vec(&result)
        .map_err(|_| ApplicationError::TransactionFailed)?
        .len()
        > MAX_RESPONSE_BYTES
    {
        return Err(ApplicationError::ResponseTooLarge);
    }
    Ok(result)
}

fn sequence_i64(value: u64) -> ApplicationResult<i64> {
    value
        .try_into()
        .map_err(|_| ApplicationError::TransactionFailed)
}

fn index_i64(index: usize) -> ApplicationResult<i64> {
    i64::try_from(index + 1).map_err(|_| ApplicationError::TransactionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::catalog::{
        CashAccount, CatalogPort, Category, FxRateRevision, Institution,
    };
    use crate::application::ledger::{CreateLedgerCommand, LedgerPort};
    use crate::domain::catalog::{BusinessId, CatalogText, SortOrder};
    use crate::domain::settings::UiLocale;
    use tempfile::tempdir;

    fn setup() -> (
        tempfile::TempDir,
        SqliteLedgerManager,
        UuidV7,
        UuidV7,
        UuidV7,
    ) {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        manager
            .create_ledger(CreateLedgerCommand {
                base_currency: Currency::parse("CNY").unwrap(),
                ui_locale: UiLocale::EnUs,
            })
            .unwrap();
        let institution_id = UuidV7::new().unwrap();
        manager
            .save_institution(&Institution {
                institution_id,
                business_id: BusinessId::parse("synthetic-bank").unwrap(),
                name: CatalogText::parse("Synthetic Bank").unwrap(),
                region: None,
                institution_type: CatalogText::parse("bank").unwrap(),
                enabled: true,
            })
            .unwrap();
        let cny = UuidV7::new().unwrap();
        let usd = UuidV7::new().unwrap();
        for (id, business, currency) in [
            (cny, "cny", Currency::parse("CNY").unwrap()),
            (usd, "usd", Currency::parse("USD").unwrap()),
        ] {
            manager
                .save_cash_account(&CashAccount {
                    account_id: id,
                    business_id: BusinessId::parse(business).unwrap(),
                    institution_id,
                    name: CatalogText::parse(business).unwrap(),
                    purpose: CatalogText::parse("daily").unwrap(),
                    currency,
                    opened_on: None,
                    enabled: true,
                })
                .unwrap();
        }
        let category = UuidV7::new().unwrap();
        manager
            .save_category(&Category {
                category_id: category,
                name: CatalogText::parse("Food").unwrap(),
                kind: CategoryKind::Expense,
                semantic_role: SemanticRole::Normal,
                sort_order: SortOrder::new(1).unwrap(),
                enabled: true,
            })
            .unwrap();
        (directory, manager, cny, usd, category)
    }

    fn input(kind: EventInputType, account_id: UuidV7, amount: &str) -> CashEventInput {
        CashEventInput {
            effective_date: LocalDate::parse("2026-02-03").unwrap(),
            sequence: crate::domain::types::Sequence::new(1).unwrap(),
            event_type: kind,
            account_id: Some(account_id),
            from_account_id: None,
            to_account_id: None,
            amount: Some(Decimal::parse(amount, DecimalUse::Amount).unwrap()),
            to_amount: None,
            category_id: None,
            semantic_role: SemanticRole::Normal,
            merchant: None,
            note: None,
            fee_account_id: None,
            fee_amount: None,
            cutover_date: None,
            migration_policy: None,
            fx_overrides: Vec::new(),
            currency_precision_confirmed: false,
        }
    }

    #[test]
    fn preview_missing_fx_is_explicit_and_does_not_mutate() {
        let (_dir, manager, _cny, usd, _category) = setup();
        let preview = manager
            .preview_event(&input(EventInputType::Expense, usd, "5.00"))
            .unwrap();
        assert_eq!(preview.postings[0].base_value, None);
        assert_eq!(preview.quality_issue_codes, vec!["MISSING_FX_RATE"]);
        let count: i64 = manager
            .store
            .as_ref()
            .unwrap()
            .connection
            .query_row("SELECT COUNT(*) FROM business_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn expense_fee_distinct_and_revision_reversal_are_consistent() {
        let (_dir, mut manager, cny, _usd, category) = setup();
        let mut event = input(EventInputType::Expense, cny, "25.50");
        event.category_id = Some(category);
        event.fee_account_id = Some(cny);
        event.fee_amount = Some(Decimal::parse("2.00", DecimalUse::Amount).unwrap());
        let first = manager.post_event(&event).unwrap();
        let report = manager
            .get_expense_analysis(
                &LocalDate::parse("2026-02-01").unwrap(),
                &LocalDate::parse("2026-02-28").unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(report.summary.valued_subtotal, "27.5");
        assert_eq!(report.summary.global_distinct_event_count, 1);
        assert_eq!(report.buckets.len(), 2);
        event.amount = Some(Decimal::parse("20.00", DecimalUse::Amount).unwrap());
        event.sequence = crate::domain::types::Sequence::new(2).unwrap();
        let revised = manager
            .revise_event(&RevisionInput {
                target_event_id: UuidV7::parse(&first.event_id).unwrap(),
                reason: "correct amount".to_owned(),
                replacement: event,
            })
            .unwrap();
        let changed = manager
            .get_expense_analysis(
                &LocalDate::parse("2026-02-01").unwrap(),
                &LocalDate::parse("2026-02-28").unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(changed.summary.valued_subtotal, "22");
        let historical = manager
            .get_expense_analysis(
                &LocalDate::parse("2026-02-01").unwrap(),
                &LocalDate::parse("2026-02-28").unwrap(),
                Some(first.event_watermark),
            )
            .unwrap();
        assert_eq!(historical.summary.valued_subtotal, "27.5");
        let reversal = manager
            .reverse_event(&ReversalInput {
                target_event_id: UuidV7::parse(&revised.event_id).unwrap(),
                reason: "void".to_owned(),
                effective_date: LocalDate::parse("2026-02-04").unwrap(),
                sequence: crate::domain::types::Sequence::new(3).unwrap(),
            })
            .unwrap();
        let reversal_kind: String = manager
            .store
            .as_ref()
            .unwrap()
            .connection
            .query_row(
                "SELECT posting_kind FROM ledger_postings WHERE event_id=?1",
                [&reversal.event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reversal_kind, "cash-reversal");
        let zero = manager
            .get_expense_analysis(
                &LocalDate::parse("2026-02-01").unwrap(),
                &LocalDate::parse("2026-02-28").unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(zero.summary.valued_subtotal, "0");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn general_activity_hydrates_details_filters_and_revision_chains() {
        let (_dir, mut manager, cny, _usd, category) = setup();
        let mut original = input(EventInputType::Expense, cny, "25.50");
        original.category_id = Some(category);
        original.merchant = Some("Synthetic Market".to_owned());
        original.note = Some("weekly basket".to_owned());
        let first = manager.post_event(&original).unwrap();

        let mut replacement = original.clone();
        replacement.amount = Some(Decimal::parse("20.00", DecimalUse::Amount).unwrap());
        replacement.sequence = crate::domain::types::Sequence::new(2).unwrap();
        let revised = manager
            .revise_event(&RevisionInput {
                target_event_id: UuidV7::parse(&first.event_id).unwrap(),
                reason: "correct amount".to_owned(),
                replacement,
            })
            .unwrap();
        let reversal = manager
            .reverse_event(&ReversalInput {
                target_event_id: UuidV7::parse(&revised.event_id).unwrap(),
                reason: "void duplicate".to_owned(),
                effective_date: LocalDate::parse("2026-02-04").unwrap(),
                sequence: crate::domain::types::Sequence::new(3).unwrap(),
            })
            .unwrap();

        let base_query = ActivityQuery {
            start_date: LocalDate::parse("2026-02-01").unwrap(),
            end_date: LocalDate::parse("2026-02-28").unwrap(),
            context: None,
            event_type: None,
            account_id: None,
            category_id: None,
            search: None,
            cursor: None,
            limit: 2,
        };
        let first_page = manager.get_activity(&base_query).unwrap();
        assert_eq!(first_page.items.len(), 2);
        assert_eq!(first_page.items[0].event_id, reversal.event_id);
        assert_eq!(first_page.items[0].audit.action, "reverse");
        assert_eq!(
            first_page.items[0].relations.reverses_event_id.as_deref(),
            Some(revised.event_id.as_str())
        );
        assert_eq!(
            first_page.items[0].postings[0].posting_kind,
            "cash-reversal"
        );
        assert!(first_page.items[0].reversal_preview.is_empty());
        assert_eq!(first_page.items[1].revision, 2);
        assert_eq!(
            first_page.items[1].audit.reason.as_deref(),
            Some("correct amount")
        );
        assert_eq!(
            first_page.items[1].relations.supersedes_event_id.as_deref(),
            Some(first.event_id.as_str())
        );
        assert_eq!(
            first_page.items[1]
                .relations
                .reversed_by_event_id
                .as_deref(),
            Some(reversal.event_id.as_str())
        );
        assert_eq!(
            first_page.items[1].reversal_preview[0].quantity_delta,
            "20.00"
        );

        let second_page = manager
            .get_activity(&ActivityQuery {
                cursor: first_page.next_cursor,
                ..base_query.clone()
            })
            .unwrap();
        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.items[0].event_id, first.event_id);
        assert_eq!(second_page.next_cursor, None);
        assert_eq!(
            second_page.items[0]
                .relations
                .superseded_by_event_id
                .as_deref(),
            Some(revised.event_id.as_str())
        );

        for (label, filtered) in [
            (
                "type",
                ActivityQuery {
                    event_type: Some("Expense".to_owned()),
                    ..base_query.clone()
                },
            ),
            (
                "category",
                ActivityQuery {
                    category_id: Some(category.to_string()),
                    ..base_query.clone()
                },
            ),
            (
                "search",
                ActivityQuery {
                    search: Some("weekly BASKET".to_owned()),
                    ..base_query.clone()
                },
            ),
        ] {
            let page = manager
                .get_activity(&filtered)
                .unwrap_or_else(|error| panic!("{label} filter failed: {error:?}"));
            assert_eq!(page.items.len(), 2);
            assert!(page.items.iter().all(|item| item.event_type == "Expense"));
        }
        let account_page = manager
            .get_activity(&ActivityQuery {
                account_id: Some(cny),
                ..base_query.clone()
            })
            .unwrap();
        assert_eq!(account_page.items.len(), 2);
        assert_eq!(account_page.items[0].event_type, "Reversal");
        let error = manager
            .get_activity(&ActivityQuery {
                search: Some("x".repeat(201)),
                ..base_query
            })
            .unwrap_err();
        assert_eq!(error, ApplicationError::ActivityFilterInvalid);
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn every_daily_cash_type_reaches_the_activity_timeline() {
        let (_dir, mut manager, cny, usd, category) = setup();
        let institution_id = manager
            .store
            .as_ref()
            .unwrap()
            .connection
            .query_row(
                "SELECT institution_id FROM institutions LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .map(|value| UuidV7::parse(&value).unwrap())
            .unwrap();
        let cny_second = UuidV7::new().unwrap();
        manager
            .save_cash_account(&CashAccount {
                account_id: cny_second,
                business_id: BusinessId::parse("cny-secondary").unwrap(),
                institution_id,
                name: CatalogText::parse("CNY secondary").unwrap(),
                purpose: CatalogText::parse("daily").unwrap(),
                currency: Currency::parse("CNY").unwrap(),
                opened_on: None,
                enabled: true,
            })
            .unwrap();
        manager
            .save_fx_revision(
                &FxRateRevision::new(
                    UuidV7::new().unwrap(),
                    LocalDate::parse("2026-02-01").unwrap(),
                    Currency::parse("USD").unwrap(),
                    Currency::parse("CNY").unwrap(),
                    "7.1",
                    CatalogText::parse("synthetic").unwrap(),
                    true,
                )
                .unwrap(),
            )
            .unwrap();

        let mut income = input(EventInputType::Income, cny, "30");
        income.merchant = Some("Synthetic employer".to_owned());
        manager.post_event(&income).unwrap();
        let mut expense = input(EventInputType::Expense, cny, "8");
        expense.sequence = crate::domain::types::Sequence::new(2).unwrap();
        expense.category_id = Some(category);
        manager.post_event(&expense).unwrap();
        let mut adjustment = input(EventInputType::Adjustment, cny, "-2");
        adjustment.sequence = crate::domain::types::Sequence::new(3).unwrap();
        manager.post_event(&adjustment).unwrap();
        let mut transfer = input(EventInputType::Transfer, cny, "5");
        transfer.sequence = crate::domain::types::Sequence::new(4).unwrap();
        transfer.account_id = None;
        transfer.from_account_id = Some(cny);
        transfer.to_account_id = Some(cny_second);
        manager.post_event(&transfer).unwrap();
        let mut exchange = input(EventInputType::CurrencyExchange, cny, "71");
        exchange.sequence = crate::domain::types::Sequence::new(5).unwrap();
        exchange.account_id = None;
        exchange.from_account_id = Some(cny);
        exchange.to_account_id = Some(usd);
        exchange.to_amount = Some(Decimal::parse("10", DecimalUse::Amount).unwrap());
        exchange.fee_account_id = Some(usd);
        exchange.fee_amount = Some(Decimal::parse("1", DecimalUse::Amount).unwrap());
        manager.post_event(&exchange).unwrap();

        let page = manager
            .get_activity(&ActivityQuery {
                start_date: LocalDate::parse("2026-02-01").unwrap(),
                end_date: LocalDate::parse("2026-02-28").unwrap(),
                context: None,
                event_type: None,
                account_id: None,
                category_id: None,
                search: None,
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert_eq!(page.items.len(), 5);
        let types = page
            .items
            .iter()
            .map(|item| item.event_type.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            types,
            BTreeSet::from([
                "Income",
                "Expense",
                "BalanceAdjustment",
                "Transfer",
                "CurrencyExchange"
            ])
        );
        let exchange = &page.items[0];
        assert_eq!(exchange.event_type, "CurrencyExchange");
        assert_eq!(exchange.content.fee_amount.as_deref(), Some("1"));
        assert_eq!(exchange.postings.len(), 3);
        assert_eq!(exchange.fx_resolutions.len(), 3);
    }

    #[test]
    fn transfer_exchange_fee_and_frozen_fx_resolution_follow_cash_rules() {
        let (_dir, mut manager, cny, usd, _category) = setup();
        let institution_id = manager
            .store
            .as_ref()
            .unwrap()
            .connection
            .query_row(
                "SELECT institution_id FROM institutions LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .map(|value| UuidV7::parse(&value).unwrap())
            .unwrap();
        let cny_second = UuidV7::new().unwrap();
        manager
            .save_cash_account(&CashAccount {
                account_id: cny_second,
                business_id: BusinessId::parse("cny-second").unwrap(),
                institution_id,
                name: CatalogText::parse("CNY second").unwrap(),
                purpose: CatalogText::parse("daily").unwrap(),
                currency: Currency::parse("CNY").unwrap(),
                opened_on: None,
                enabled: true,
            })
            .unwrap();
        let fx_revision = FxRateRevision::new(
            UuidV7::new().unwrap(),
            LocalDate::parse("2026-02-01").unwrap(),
            Currency::parse("USD").unwrap(),
            Currency::parse("CNY").unwrap(),
            "7.1",
            CatalogText::parse("synthetic").unwrap(),
            true,
        )
        .unwrap();
        manager.save_fx_revision(&fx_revision).unwrap();

        let mut transfer = input(EventInputType::Transfer, cny, "100");
        transfer.account_id = None;
        transfer.from_account_id = Some(cny);
        transfer.to_account_id = Some(cny_second);
        manager.post_event(&transfer).unwrap();

        let mut exchange = input(EventInputType::CurrencyExchange, cny, "710");
        exchange.sequence = crate::domain::types::Sequence::new(2).unwrap();
        exchange.account_id = None;
        exchange.from_account_id = Some(cny);
        exchange.to_account_id = Some(usd);
        exchange.to_amount = Some(Decimal::parse("100", DecimalUse::Amount).unwrap());
        exchange.fee_account_id = Some(usd);
        exchange.fee_amount = Some(Decimal::parse("1", DecimalUse::Amount).unwrap());
        let posted = manager.post_event(&exchange).unwrap();
        assert_eq!(posted.preview.postings.len(), 3);
        assert_eq!(
            posted.preview.postings[2].base_value.as_deref(),
            Some("-7.1")
        );
        let store = manager.store.as_ref().unwrap();
        let resolution_count: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM fx_resolutions WHERE owner_id=?1",
                [&posted.event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolution_count, 3);
        let balances: Vec<(String, String)> = {
            let mut statement = store
                .connection
                .prepare(
                    "SELECT account_id,balance FROM cash_balance_projection ORDER BY account_id",
                )
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert!(balances.contains(&(cny.to_string(), "-810".to_owned())));
        assert!(balances.contains(&(cny_second.to_string(), "100".to_owned())));
        assert!(balances.contains(&(usd.to_string(), "99".to_owned())));
        let report = manager
            .get_expense_analysis(
                &LocalDate::parse("2026-02-01").unwrap(),
                &LocalDate::parse("2026-02-28").unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(report.summary.valued_subtotal, "7.1");
        assert_eq!(report.buckets[0].bucket_id, "system:fx-fee");
    }

    #[test]
    fn refunds_missing_fx_archived_and_uncategorized_are_explicit() {
        let (_dir, mut manager, cny, usd, category) = setup();
        let refund_category = UuidV7::new().unwrap();
        let reimbursement_category = UuidV7::new().unwrap();
        for (id, label, role, order) in [
            (refund_category, "Refund", SemanticRole::Refund, 2),
            (
                reimbursement_category,
                "Reimbursement",
                SemanticRole::Reimbursement,
                3,
            ),
        ] {
            manager
                .save_category(&Category {
                    category_id: id,
                    name: CatalogText::parse(label).unwrap(),
                    kind: CategoryKind::Income,
                    semantic_role: role,
                    sort_order: SortOrder::new(order).unwrap(),
                    enabled: true,
                })
                .unwrap();
        }
        let mut refund = input(EventInputType::Income, cny, "5");
        refund.category_id = Some(refund_category);
        refund.semantic_role = SemanticRole::Refund;
        manager.post_event(&refund).unwrap();
        let mut reimbursement = input(EventInputType::Income, usd, "2");
        reimbursement.sequence = crate::domain::types::Sequence::new(2).unwrap();
        reimbursement.category_id = Some(reimbursement_category);
        reimbursement.semantic_role = SemanticRole::Reimbursement;
        manager.post_event(&reimbursement).unwrap();
        let mut uncategorized = input(EventInputType::Expense, cny, "3");
        uncategorized.sequence = crate::domain::types::Sequence::new(3).unwrap();
        manager.post_event(&uncategorized).unwrap();
        let mut categorized = input(EventInputType::Expense, cny, "9");
        categorized.sequence = crate::domain::types::Sequence::new(4).unwrap();
        categorized.category_id = Some(category);
        manager.post_event(&categorized).unwrap();
        manager
            .save_category(&Category {
                category_id: category,
                name: CatalogText::parse("Food renamed").unwrap(),
                kind: CategoryKind::Expense,
                semantic_role: SemanticRole::Normal,
                sort_order: SortOrder::new(9).unwrap(),
                enabled: false,
            })
            .unwrap();

        let report = manager
            .get_expense_analysis(
                &LocalDate::parse("2026-02-03").unwrap(),
                &LocalDate::parse("2026-02-03").unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(report.summary.total_expense.as_deref(), Some("12"));
        assert_eq!(report.refunds.refund.amount, "5");
        assert_eq!(report.refunds.refund.distinct_event_count, 1);
        assert_eq!(report.refunds.reimbursement.amount, "0");
        assert_eq!(report.refunds.reimbursement.unvalued_count, 1);
        assert!(
            report
                .buckets
                .iter()
                .any(|row| { row.bucket_id == "system:uncategorized" && row.amount == "3" })
        );
        assert!(report.buckets.iter().any(|row| {
            row.bucket_id == category.to_string()
                && row.label == "Food renamed"
                && row.archived
                && row.amount == "9"
        }));
    }

    #[test]
    fn accepted_daily_projection_deletes_and_rebuilds_to_identical_query() {
        let (_dir, mut manager, cny, _usd, category) = setup();
        let mut event = input(EventInputType::Expense, cny, "13");
        event.category_id = Some(category);
        event.fee_account_id = Some(cny);
        event.fee_amount = Some(Decimal::parse("2", DecimalUse::Amount).unwrap());
        let posted = manager.post_event(&event).unwrap();
        let start = LocalDate::parse("2026-02-01").unwrap();
        let end = LocalDate::parse("2026-02-28").unwrap();
        let before = manager.get_expense_analysis(&start, &end, None).unwrap();
        let projection_rows = |connection: &Connection| -> Vec<String> {
            let mut rows = Vec::new();
            for sql in [
                "SELECT effective_date||'|'||bucket_id||'|'||valuation_state||'|'||amount||'|'||distinct_event_count FROM expense_daily_projection ORDER BY 1",
                "SELECT effective_date||'|'||measure_role||'|'||valuation_state||'|'||amount||'|'||distinct_event_count FROM expense_daily_summary_projection ORDER BY 1",
                "SELECT effective_date||'|'||event_id||'|'||bucket_id||'|'||valuation_state FROM expense_daily_event_bucket_projection ORDER BY 1",
            ] {
                let mut statement = connection.prepare(sql).unwrap();
                rows.extend(
                    statement
                        .query_map([], |row| row.get::<_, String>(0))
                        .unwrap()
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap(),
                );
            }
            rows
        };
        let rows_before = projection_rows(&manager.store.as_ref().unwrap().connection);
        {
            let store = manager.store.as_mut().unwrap();
            let transaction = store.connection.transaction().unwrap();
            transaction
                .execute_batch(
                    "DELETE FROM expense_daily_event_bucket_projection;
                     DELETE FROM expense_daily_summary_projection;
                     DELETE FROM expense_daily_projection;
                     UPDATE projection_metadata SET available=0 WHERE projection_name='expense-daily';",
                )
                .unwrap();
            rebuild_cash_derived(&transaction, posted.event_watermark).unwrap();
            transaction.commit().unwrap();
            store.expense_cache.borrow_mut().clear();
        }
        let rows_after = projection_rows(&manager.store.as_ref().unwrap().connection);
        let after = manager.get_expense_analysis(&start, &end, None).unwrap();
        assert_eq!(rows_after, rows_before);
        assert_eq!(after, before);
    }

    #[test]
    fn top10_other_activity_uses_bounded_cursor_and_exact_member_set() {
        let (_dir, mut manager, cny, _usd, _category) = setup();
        for index in 0..12_u32 {
            let category_id = UuidV7::new().unwrap();
            manager
                .save_category(&Category {
                    category_id,
                    name: CatalogText::parse(&format!("Synthetic category {index}")).unwrap(),
                    kind: CategoryKind::Expense,
                    semantic_role: SemanticRole::Normal,
                    sort_order: SortOrder::new(index + 10).unwrap(),
                    enabled: true,
                })
                .unwrap();
            let mut event = input(
                EventInputType::Expense,
                cny,
                &(12_u32.saturating_sub(index)).to_string(),
            );
            event.sequence = crate::domain::types::Sequence::new(u64::from(index) + 1).unwrap();
            event.category_id = Some(category_id);
            manager.post_event(&event).unwrap();
        }
        let report = manager
            .get_expense_analysis(
                &LocalDate::parse("2026-02-01").unwrap(),
                &LocalDate::parse("2026-02-28").unwrap(),
                None,
            )
            .unwrap();
        assert_eq!(report.top10.items.len(), 10);
        let other = report.top10.other.unwrap();
        assert_eq!(other.amount, "3");
        assert_eq!(other.distinct_event_count, 2);
        let start_date = LocalDate::parse("2026-02-01").unwrap();
        let end_date = LocalDate::parse("2026-02-28").unwrap();
        let first = manager
            .get_activity(&ActivityQuery {
                start_date: start_date.clone(),
                end_date: end_date.clone(),
                context: Some(other.drilldown_context.clone()),
                event_type: None,
                account_id: None,
                category_id: None,
                search: None,
                cursor: None,
                limit: 1,
            })
            .unwrap();
        assert_eq!(first.items.len(), 1);
        let second = manager
            .get_activity(&ActivityQuery {
                start_date,
                end_date,
                context: Some(other.drilldown_context),
                event_type: None,
                account_id: None,
                category_id: None,
                search: None,
                cursor: first.next_cursor,
                limit: 1,
            })
            .unwrap();
        assert_eq!(second.items.len(), 1);
        assert_ne!(first.items[0].event_id, second.items[0].event_id);
        assert_eq!(second.next_cursor, None);
    }

    #[test]
    fn fixture_23_normal_query_matches_the_cross_stack_canonical_hash() {
        let rows = vec![
            ExpenseAggregateRow {
                kind: "summary".to_owned(),
                bucket_id: None,
                label: None,
                archived: false,
                signed_amount: Some("-11.00".to_owned()),
                distinct_count: 1,
                unvalued_count: 0,
            },
            ExpenseAggregateRow {
                kind: "refund".to_owned(),
                bucket_id: None,
                label: None,
                archived: false,
                signed_amount: Some("0".to_owned()),
                distinct_count: 0,
                unvalued_count: 0,
            },
            ExpenseAggregateRow {
                kind: "reimbursement".to_owned(),
                bucket_id: None,
                label: None,
                archived: false,
                signed_amount: Some("0".to_owned()),
                distinct_count: 0,
                unvalued_count: 0,
            },
            ExpenseAggregateRow {
                kind: "bucket".to_owned(),
                bucket_id: Some("cat-food".to_owned()),
                label: Some("Food".to_owned()),
                archived: false,
                signed_amount: Some("-10.00".to_owned()),
                distinct_count: 1,
                unvalued_count: 0,
            },
            ExpenseAggregateRow {
                kind: "bucket".to_owned(),
                bucket_id: Some("system:ordinary-fee".to_owned()),
                label: Some("Ordinary fees".to_owned()),
                archived: false,
                signed_amount: Some("-1.00".to_owned()),
                distinct_count: 1,
                unvalued_count: 0,
            },
        ];
        let result = build_aggregated_expense_result(
            &LocalDate::parse("2026-02-01").unwrap(),
            &LocalDate::parse("2026-02-28").unwrap(),
            Currency::parse("CNY").unwrap(),
            1,
            1,
            rows,
        )
        .unwrap();
        assert_eq!(result.summary.valued_subtotal, "11");
        assert_eq!(result.summary.global_distinct_event_count, 1);
        assert_eq!(
            result.canonical_hash,
            "sha256:2d61dc63cec7549ae621ed86bd0e0370e01e0d521e98d89ae5c0a3043979d3f0"
        );
    }

    #[test]
    #[ignore = "explicit 100k performance gate"]
    fn synthetic_100k_query_meets_latency_and_response_gates() {
        let (_dir, mut manager, cny, _usd, category) = setup();
        let store = manager.store.as_mut().unwrap();
        let transaction = store.connection.transaction().unwrap();
        {
            let mut event = transaction.prepare("INSERT INTO business_events(event_id,event_type,effective_date,sequence,status,revision,created_at_utc,calculation_version) VALUES(?1,'Expense','2026-02-10',?2,'posted',1,CURRENT_TIMESTAMP,?3)").unwrap();
            let mut detail = transaction.prepare("INSERT INTO income_expense_details(event_id,account_id,entry_type,category_id,amount,semantic_role) VALUES(?1,?2,'expense',?3,'1.00','normal')").unwrap();
            let mut posting = transaction.prepare("INSERT INTO ledger_postings(posting_id,event_id,posting_ordinal,posting_kind,account_id,quantity_delta,currency,base_value,base_currency,calculation_version) VALUES(?1,?2,1,'cash',?3,'-1.00','CNY','-1.00','CNY',?4)").unwrap();
            for index in 1..=100_000_u64 {
                let event_id = format!("synthetic-event-{index:06}");
                event
                    .execute(params![
                        event_id,
                        i64::try_from(index).unwrap(),
                        CALCULATION_VERSION
                    ])
                    .unwrap();
                detail
                    .execute(params![event_id, cny.to_string(), category.to_string()])
                    .unwrap();
                posting
                    .execute(params![
                        format!("synthetic-posting-{index:06}"),
                        event_id,
                        cny.to_string(),
                        CALCULATION_VERSION
                    ])
                    .unwrap();
            }
        }
        rebuild_cash_derived(&transaction, 100_000).unwrap();
        transaction.commit().unwrap();
        let start = LocalDate::parse("2026-02-01").unwrap();
        let end = LocalDate::parse("2026-02-28").unwrap();
        let cold_started = std::time::Instant::now();
        let result = manager.get_expense_analysis(&start, &end, None).unwrap();
        let cold = cold_started.elapsed();
        let mut warm = Vec::new();
        for _ in 0..20 {
            let started = std::time::Instant::now();
            let next = manager.get_expense_analysis(&start, &end, None).unwrap();
            warm.push(started.elapsed());
            assert_eq!(next.canonical_hash, result.canonical_hash);
        }
        warm.sort();
        let p95 = warm[18];
        let ipc_started = std::time::Instant::now();
        let ipc_result = manager.get_expense_analysis(&start, &end, None).unwrap();
        let size = serde_json::to_vec(&ipc_result).unwrap().len();
        let ipc = ipc_started.elapsed();
        eprintln!(
            "cash-query-100k cold_ms={} warm_p95_ms={} ipc_ms={} bytes={size}",
            cold.as_millis(),
            p95.as_millis(),
            ipc.as_millis()
        );
        assert!(
            cold.as_millis() <= 150,
            "cold query exceeded 150 ms: {cold:?}"
        );
        assert!(p95.as_millis() <= 50, "warm P95 exceeded 50 ms: {p95:?}");
        assert!(
            ipc.as_millis() <= 200,
            "IPC-equivalent query plus serialization exceeded 200 ms: {ipc:?}"
        );
        assert!(size <= MAX_RESPONSE_BYTES);
    }
}
