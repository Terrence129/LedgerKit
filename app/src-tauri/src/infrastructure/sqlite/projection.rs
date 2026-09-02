#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use rusqlite::{Transaction, params};

use crate::application::error::{ApplicationError, ApplicationResult};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::posting::{LedgerPosting, PostingKind};
use crate::domain::types::ProjectionWatermark;

pub const CASH_PROJECTION_NAME: &str = "cash-balance";
pub const CASH_PROJECTION_VERSION: &str = "cash-balance-projection-v1";

pub trait ProjectionRebuilder {
    fn projection_name(&self) -> &'static str;
    fn rebuild(
        &self,
        transaction: &Transaction<'_>,
        calculation_version: &str,
    ) -> ApplicationResult<ProjectionWatermark>;
}

pub struct CashBalanceProjectionRebuilder;

impl ProjectionRebuilder for CashBalanceProjectionRebuilder {
    fn projection_name(&self) -> &'static str {
        CASH_PROJECTION_NAME
    }

    fn rebuild(
        &self,
        transaction: &Transaction<'_>,
        calculation_version: &str,
    ) -> ApplicationResult<ProjectionWatermark> {
        transaction
            .execute("DELETE FROM cash_balance_projection", [])
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .execute(
                "UPDATE projection_metadata SET available=0 WHERE projection_name=?1",
                [self.projection_name()],
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let rows = {
            let mut statement = transaction
                .prepare(
                    "SELECT p.account_id, p.quantity_delta, p.currency
                     FROM ledger_postings p
                     JOIN business_events e ON e.event_id=p.event_id
                     WHERE p.posting_kind='cash'
                     ORDER BY e.effective_date, e.sequence, e.event_id, p.posting_id",
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|_| ApplicationError::TransactionFailed)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ApplicationError::TransactionFailed)?
        };
        let mut balances: BTreeMap<String, (Decimal, String)> = BTreeMap::new();
        for (account_id, amount, currency) in rows {
            let delta = Decimal::parse(&amount, DecimalUse::Amount)?;
            let entry = balances
                .entry(account_id)
                .or_insert_with(|| (Decimal::zero(DecimalUse::Amount), currency.clone()));
            if entry.1 != currency {
                return Err(ApplicationError::TransactionFailed);
            }
            entry.0 = entry.0.checked_add(&delta, DecimalUse::Amount)?;
        }
        let event_watermark: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(event_order),0) FROM business_events",
                [],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let watermark = ProjectionWatermark::new(
            u64::try_from(event_watermark).map_err(|_| ApplicationError::TransactionFailed)?,
        )?;
        for (account_id, (balance, currency)) in balances {
            transaction
                .execute(
                    "INSERT INTO cash_balance_projection(account_id,balance,currency,event_watermark,calculation_version) VALUES(?1,?2,?3,?4,?5)",
                    params![account_id, balance.as_str(), currency, watermark_to_i64(watermark)?, calculation_version],
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
        }
        transaction
            .execute(
                "UPDATE projection_metadata SET projection_version=?1, calculation_version=?2, event_watermark=?3, available=1, rebuilt_at_utc=CURRENT_TIMESTAMP WHERE projection_name=?4",
                params![CASH_PROJECTION_VERSION, calculation_version, watermark_to_i64(watermark)?, self.projection_name()],
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(watermark)
    }
}

pub fn apply_cash_postings(
    transaction: &Transaction<'_>,
    postings: &[LedgerPosting],
    watermark: ProjectionWatermark,
) -> ApplicationResult<()> {
    for posting in postings
        .iter()
        .filter(|posting| posting.posting_kind == PostingKind::Cash)
    {
        let account_id = posting
            .account_id
            .ok_or(ApplicationError::TransactionFailed)?
            .to_string();
        let current = transaction
            .query_row(
                "SELECT balance,currency FROM cash_balance_projection WHERE account_id=?1",
                [&account_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let balance = if let Some((balance, currency)) = current {
            if currency != posting.currency.as_str() {
                return Err(ApplicationError::TransactionFailed);
            }
            Decimal::parse(&balance, DecimalUse::Amount)?
                .checked_add(&posting.quantity_delta, DecimalUse::Amount)?
        } else {
            posting.quantity_delta.clone()
        };
        transaction
            .execute(
                "INSERT INTO cash_balance_projection(account_id,balance,currency,event_watermark,calculation_version)
                 VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(account_id) DO UPDATE SET balance=excluded.balance,event_watermark=excluded.event_watermark,calculation_version=excluded.calculation_version",
                params![account_id, balance.as_str(), posting.currency.as_str(), watermark_to_i64(watermark)?, posting.calculation_version.as_str()],
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
    }
    Ok(())
}

use rusqlite::OptionalExtension;

fn watermark_to_i64(watermark: ProjectionWatermark) -> ApplicationResult<i64> {
    i64::try_from(watermark.get()).map_err(|_| ApplicationError::TransactionFailed)
}
