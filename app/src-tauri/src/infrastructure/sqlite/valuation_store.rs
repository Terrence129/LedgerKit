#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, Transaction, params};

use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::import::ImportIssue;
use crate::application::valuation::{
    CompositionItem, DATA_QUALITY_CONTRACT, DataQualityIssue, DataQualityReport, FixContext,
    OVERVIEW_CONTRACT, Overview, OverviewComposition, OverviewWatermarks, UnvaluedAsset,
    VALUATION_SNAPSHOT_VERSION, ValuationPort, ValuationSnapshot,
};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::types::{Currency, LocalDate, UuidV7};

use super::store::{CALCULATION_VERSION, LedgerStore, SqliteLedgerManager, map_sqlite_error};

#[derive(Clone)]
struct ValuationLine {
    asset_type: &'static str,
    account_id: Option<String>,
    portfolio_id: Option<String>,
    instrument_id: Option<String>,
    institution_id: String,
    label: String,
    native_value: Decimal,
    native_currency: Currency,
    price_revision_id: Option<String>,
    fx_revision_id: Option<String>,
    fx_rate: Option<Decimal>,
    base_value: Option<Decimal>,
    unvalued_reason: Option<&'static str>,
}

impl ValuationPort for SqliteLedgerManager {
    fn get_overview(&self, as_of_date: &LocalDate) -> ApplicationResult<Overview> {
        self.open_store()?.overview(as_of_date)
    }

    fn get_data_quality(&self, as_of_date: &LocalDate) -> ApplicationResult<DataQualityReport> {
        self.open_store()?.data_quality(as_of_date)
    }

    fn confirm_valuation_snapshot(
        &mut self,
        as_of_date: &LocalDate,
    ) -> ApplicationResult<ValuationSnapshot> {
        self.open_store_mut()?.confirm_snapshot(as_of_date)
    }
}

impl LedgerStore {
    pub(super) fn overview(&self, as_of_date: &LocalDate) -> ApplicationResult<Overview> {
        let (overview, _) = self.build_overview(as_of_date)?;
        Ok(overview)
    }

    #[allow(clippy::too_many_lines)] // One snapshot read assembles all mutually consistent overview sections.
    fn build_overview(
        &self,
        as_of_date: &LocalDate,
    ) -> ApplicationResult<(Overview, Vec<ValuationLine>)> {
        let base_currency = self.base_currency()?;
        let mut lines = self.cash_valuation_lines(as_of_date)?;
        let workspace = self.investment_workspace(as_of_date)?;
        for holding in &workspace.holdings {
            let native_value = Decimal::parse(
                holding.market_value.as_deref().unwrap_or("0"),
                DecimalUse::Internal,
            )?;
            let native_currency = Currency::parse(&holding.currency)?;
            let institution_id: String = self
                .connection
                .query_row(
                    "SELECT institution_id FROM portfolios WHERE portfolio_id=?1",
                    [&holding.portfolio_id],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
            lines.push(ValuationLine {
                asset_type: "holding",
                account_id: None,
                portfolio_id: Some(holding.portfolio_id.clone()),
                instrument_id: Some(holding.instrument_id.clone()),
                institution_id,
                label: holding.instrument_name.clone(),
                native_value,
                native_currency,
                price_revision_id: holding.price_revision_id.clone(),
                fx_revision_id: holding.fx_revision_id.clone(),
                fx_rate: holding
                    .fx_rate
                    .as_deref()
                    .map(|value| Decimal::parse(value, DecimalUse::FxRate))
                    .transpose()?,
                base_value: holding
                    .base_market_value
                    .as_deref()
                    .map(|value| Decimal::parse(value, DecimalUse::Internal))
                    .transpose()?,
                unvalued_reason: holding.unvalued_reason,
            });
        }

        let mut valued_cash = Decimal::zero(DecimalUse::Internal);
        let mut valued_holdings = Decimal::zero(DecimalUse::Internal);
        let mut institutions = BTreeMap::<String, Decimal>::new();
        let mut currencies = BTreeMap::<String, Decimal>::new();
        let mut cash_accounts = Vec::new();
        let mut holdings = Vec::new();
        let mut unvalued_assets = Vec::new();
        for line in &lines {
            if let Some(value) = &line.base_value {
                if line.asset_type == "cash-account" {
                    valued_cash = valued_cash.checked_add(value, DecimalUse::Internal)?;
                    cash_accounts.push(CompositionItem {
                        id: line.account_id.clone().unwrap_or_default(),
                        label: line.label.clone(),
                        base_value: value.as_str().to_owned(),
                    });
                } else {
                    valued_holdings = valued_holdings.checked_add(value, DecimalUse::Internal)?;
                    holdings.push(CompositionItem {
                        id: line.instrument_id.clone().unwrap_or_default(),
                        label: line.label.clone(),
                        base_value: value.as_str().to_owned(),
                    });
                }
                add_decimal(&mut institutions, line.institution_id.clone(), value)?;
                add_decimal(&mut currencies, line.native_currency.to_string(), value)?;
            } else if line.asset_type == "holding" || !line.native_value.is_zero() {
                unvalued_assets.push(UnvaluedAsset {
                    asset_type: line.asset_type.to_owned(),
                    entity_id: line
                        .account_id
                        .clone()
                        .or_else(|| line.instrument_id.clone())
                        .unwrap_or_default(),
                    native_value: line.native_value.as_str().to_owned(),
                    native_currency: line.native_currency.to_string(),
                    reason: line
                        .unvalued_reason
                        .unwrap_or("VALUATION_UNAVAILABLE")
                        .to_owned(),
                });
            }
        }
        let valued_net_assets = valued_cash.checked_add(&valued_holdings, DecimalUse::Internal)?;
        let month = &as_of_date.as_str()[..7];
        let mtd_start = LocalDate::parse(&format!("{month}-01"))?;
        let expense = self.expense_analysis(&mtd_start, as_of_date, None)?;
        let mut anomaly_codes = unvalued_assets
            .iter()
            .map(|item| item.reason.clone())
            .collect::<BTreeSet<_>>();
        for holding in &workspace.holdings {
            anomaly_codes.extend(holding.warning_codes.iter().map(ToString::to_string));
        }
        let event_watermark = current_event_watermark(&self.connection)?;
        let market_data_watermark = market_data_watermark(&self.connection)?;
        let composition = OverviewComposition {
            institutions: map_composition(&self.connection, "institutions", institutions)?,
            currencies: currencies
                .into_iter()
                .map(|(id, value)| CompositionItem {
                    label: id.clone(),
                    id,
                    base_value: value.as_str().to_owned(),
                })
                .collect(),
            cash_accounts,
            holdings,
        };
        Ok((
            Overview {
                contract: OVERVIEW_CONTRACT,
                valuation_date: as_of_date.as_str().to_owned(),
                mtd_start_date: mtd_start.as_str().to_owned(),
                mtd_end_date: as_of_date.as_str().to_owned(),
                base_currency: base_currency.to_string(),
                valued_net_assets: valued_net_assets.as_str().to_owned(),
                valued_cash: valued_cash.as_str().to_owned(),
                valued_holdings: valued_holdings.as_str().to_owned(),
                mtd_expense: expense.summary.valued_subtotal,
                mtd_unvalued_expense_count: expense.unvalued.expense_count,
                composition,
                unvalued_assets,
                anomaly_codes: anomaly_codes.into_iter().collect(),
                watermarks: OverviewWatermarks {
                    event: event_watermark,
                    market_data: market_data_watermark,
                },
                calculation_version: CALCULATION_VERSION,
                snapshot_version: VALUATION_SNAPSHOT_VERSION,
            },
            lines,
        ))
    }

    fn cash_valuation_lines(
        &self,
        as_of_date: &LocalDate,
    ) -> ApplicationResult<Vec<ValuationLine>> {
        let mut statement = self.connection.prepare(
            "SELECT a.account_id,a.name,a.institution_id,a.currency,p.quantity_delta
             FROM ledger_postings p
             JOIN business_events e ON e.event_id=p.event_id
             JOIN cash_accounts a ON a.account_id=p.account_id
             WHERE p.posting_kind IN ('cash','opening-cash','settlement-cash')
               AND e.status='posted' AND e.event_type<>'Reversal' AND e.effective_date<=?1
               AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id)
               AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id)
             ORDER BY a.account_id,e.effective_date,e.sequence,e.event_id,p.posting_ordinal",
        ).map_err(|_| ApplicationError::TransactionFailed)?;
        let rows = statement
            .query_map([as_of_date.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        drop(statement);
        let mut balances = BTreeMap::<(String, String, String, String), Decimal>::new();
        for (account_id, name, institution_id, currency, amount) in rows {
            let value = Decimal::parse(&amount, DecimalUse::Internal)?;
            add_decimal(
                &mut balances,
                (account_id, name, institution_id, currency),
                &value,
            )?;
        }
        let mut lines = Vec::with_capacity(balances.len());
        for ((account_id, name, institution_id, currency_text), native_value) in balances {
            let currency = Currency::parse(&currency_text)?;
            let selection = self.resolve_fx_rate(currency, as_of_date)?;
            let base_value = selection
                .as_ref()
                .map(|value| native_value.checked_mul_internal(&value.value))
                .transpose()?;
            lines.push(ValuationLine {
                asset_type: "cash-account",
                account_id: Some(account_id),
                portfolio_id: None,
                instrument_id: None,
                institution_id,
                label: name,
                native_value,
                native_currency: currency,
                price_revision_id: None,
                fx_revision_id: selection
                    .as_ref()
                    .and_then(|value| value.revision_id.clone()),
                fx_rate: selection.as_ref().map(|value| value.value.clone()),
                base_value,
                unvalued_reason: selection.is_none().then_some("FX_MISSING_AS_OF"),
            });
        }
        Ok(lines)
    }

    fn data_quality(&self, as_of_date: &LocalDate) -> ApplicationResult<DataQualityReport> {
        let (overview, _) = self.build_overview(as_of_date)?;
        let workspace = self.investment_workspace(as_of_date)?;
        let mut issues = Vec::new();
        for asset in &overview.unvalued_assets {
            let (operation, field) = match asset.reason.as_str() {
                "PRICE_MISSING_AS_OF" => ("save_price_revision", "priceDate"),
                _ => ("save_fx_revision", "currency"),
            };
            issues.push(quality_issue(
                &asset.reason,
                "blocker",
                &asset.asset_type,
                &asset.entity_id,
                operation,
                field,
                as_of_date,
            ));
        }
        for holding in &workspace.holdings {
            if Decimal::parse(&holding.quantity, DecimalUse::Quantity)?.is_negative() {
                issues.push(quality_issue(
                    "NEGATIVE_HOLDING",
                    "blocker",
                    "holding",
                    &holding.instrument_id,
                    "review_activity",
                    "quantity",
                    as_of_date,
                ));
            }
            for code in &holding.warning_codes {
                issues.push(quality_issue(
                    code,
                    "warning",
                    "security-instrument",
                    &holding.instrument_id,
                    "save_price_revision",
                    "priceDate",
                    as_of_date,
                ));
            }
        }
        let mut statement = self.connection.prepare(
            "SELECT errors_json FROM import_rows WHERE status IN ('error','warning') ORDER BY sheet_name,source_row_number,import_row_id",
        ).map_err(|_| ApplicationError::TransactionFailed)?;
        let import_rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        drop(statement);
        for row in import_rows {
            for issue in serde_json::from_str::<Vec<ImportIssue>>(&row)
                .map_err(|_| ApplicationError::TransactionFailed)?
            {
                issues.push(quality_issue(
                    &issue.code,
                    &issue.severity,
                    "import-row",
                    &format!("{}:{}", issue.sheet, issue.row),
                    "review_import",
                    &issue.field,
                    as_of_date,
                ));
            }
        }
        issues.sort_by(|left, right| left.issue_id.cmp(&right.issue_id));
        issues.dedup_by(|left, right| left.issue_id == right.issue_id);
        let blocker_count = issues
            .iter()
            .filter(|item| item.severity == "blocker")
            .count();
        let warning_count = issues
            .iter()
            .filter(|item| item.severity == "warning")
            .count();
        Ok(DataQualityReport {
            contract: DATA_QUALITY_CONTRACT,
            as_of_date: as_of_date.as_str().to_owned(),
            blocker_count: u64::try_from(blocker_count)
                .map_err(|_| ApplicationError::ResponseTooLarge)?,
            warning_count: u64::try_from(warning_count)
                .map_err(|_| ApplicationError::ResponseTooLarge)?,
            issues,
            event_watermark: overview.watermarks.event,
            calculation_version: CALCULATION_VERSION,
        })
    }

    fn confirm_snapshot(&mut self, as_of_date: &LocalDate) -> ApplicationResult<ValuationSnapshot> {
        let (overview, lines) = self.build_overview(as_of_date)?;
        let snapshot_id = UuidV7::new()?.to_string();
        let supersedes: Option<String> = self.connection.query_row(
            "SELECT valuation_snapshot_id FROM valuation_snapshots WHERE valuation_date=?1 ORDER BY created_at_utc DESC,valuation_snapshot_id DESC LIMIT 1",
            [as_of_date.as_str()],
            |row| row.get(0),
        ).optional().map_err(|_| ApplicationError::TransactionFailed)?;
        let summary_json =
            serde_json::to_string(&overview).map_err(|_| ApplicationError::TransactionFailed)?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction.execute(
            "INSERT INTO valuation_snapshots(valuation_snapshot_id,supersedes_snapshot_id,valuation_date,base_currency,calculation_version,event_watermark,market_data_watermark,summary_json,created_at_utc)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,CURRENT_TIMESTAMP)",
            params![snapshot_id,supersedes,overview.valuation_date,overview.base_currency,CALCULATION_VERSION,to_i64(overview.watermarks.event)?,to_i64(overview.watermarks.market_data)?,summary_json],
        ).map_err(map_sqlite_error)?;
        for line in &lines {
            insert_snapshot_line(
                &transaction,
                &snapshot_id,
                as_of_date,
                &overview.base_currency,
                line,
            )?;
        }
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(ValuationSnapshot {
            snapshot_id,
            supersedes_snapshot_id: supersedes,
            valuation_date: overview.valuation_date,
            base_currency: overview.base_currency,
            line_count: u64::try_from(lines.len())
                .map_err(|_| ApplicationError::ResponseTooLarge)?,
            valued_net_assets: overview.valued_net_assets,
            event_watermark: overview.watermarks.event,
            market_data_watermark: overview.watermarks.market_data,
            calculation_version: CALCULATION_VERSION,
        })
    }
}

fn insert_snapshot_line(
    transaction: &Transaction<'_>,
    snapshot_id: &str,
    as_of_date: &LocalDate,
    base_currency: &str,
    line: &ValuationLine,
) -> ApplicationResult<()> {
    let line_id = UuidV7::new()?.to_string();
    let fx_resolution_id = if let Some(rate) = &line.fx_rate {
        let id = UuidV7::new()?.to_string();
        transaction.execute(
            "INSERT INTO fx_resolutions(fx_resolution_id,owner_type,owner_id,purpose,target_date,currency,base_currency,auto_rate_revision_id,final_rate,calculation_version,created_at_utc)
             VALUES(?1,'valuation',?2,'valuation',?3,?4,?5,?6,?7,?8,CURRENT_TIMESTAMP)",
            params![id,line_id,as_of_date.as_str(),line.native_currency.as_str(),base_currency,line.fx_revision_id,rate.as_str(),CALCULATION_VERSION],
        ).map_err(map_sqlite_error)?;
        Some(id)
    } else {
        None
    };
    let valuation_state = if line.base_value.is_some() {
        "valued"
    } else {
        "unvalued"
    };
    transaction.execute(
        "INSERT INTO valuation_snapshot_lines(valuation_snapshot_line_id,valuation_snapshot_id,asset_type,account_id,portfolio_id,instrument_id,native_value,native_currency,price_revision_id,fx_resolution_id,base_value,base_currency,valuation_state,unvalued_reason)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![line_id,snapshot_id,line.asset_type,line.account_id,line.portfolio_id,line.instrument_id,line.native_value.as_str(),line.native_currency.as_str(),line.price_revision_id,fx_resolution_id,line.base_value.as_ref().map(Decimal::as_str),base_currency,valuation_state,line.unvalued_reason],
    ).map_err(map_sqlite_error)?;
    Ok(())
}

fn quality_issue(
    code: &str,
    severity: &str,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    field: &str,
    as_of_date: &LocalDate,
) -> DataQualityIssue {
    DataQualityIssue {
        issue_id: format!("{code}:{entity_type}:{entity_id}:{}", as_of_date.as_str()),
        code: code.to_owned(),
        severity: severity.to_owned(),
        status: "open".to_owned(),
        context: FixContext {
            operation: operation.to_owned(),
            field: field.to_owned(),
            entity_type: entity_type.to_owned(),
            entity_id: entity_id.to_owned(),
            as_of_date: as_of_date.as_str().to_owned(),
        },
    }
}

fn add_decimal<K: Ord>(
    map: &mut BTreeMap<K, Decimal>,
    key: K,
    value: &Decimal,
) -> ApplicationResult<()> {
    let current = map
        .remove(&key)
        .unwrap_or_else(|| Decimal::zero(DecimalUse::Internal));
    map.insert(key, current.checked_add(value, DecimalUse::Internal)?);
    Ok(())
}

fn map_composition(
    connection: &rusqlite::Connection,
    table: &str,
    values: BTreeMap<String, Decimal>,
) -> ApplicationResult<Vec<CompositionItem>> {
    if table != "institutions" {
        return Err(ApplicationError::TransactionFailed);
    }
    values
        .into_iter()
        .map(|(id, value)| {
            let label = connection
                .query_row(
                    "SELECT name FROM institutions WHERE institution_id=?1",
                    [&id],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
            Ok(CompositionItem {
                id,
                label,
                base_value: value.as_str().to_owned(),
            })
        })
        .collect()
}

fn current_event_watermark(connection: &rusqlite::Connection) -> ApplicationResult<u64> {
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

fn market_data_watermark(connection: &rusqlite::Connection) -> ApplicationResult<u64> {
    connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM fx_rate_revisions)+(SELECT COUNT(*) FROM security_price_revisions)",
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use crate::application::import::ImportPort;
    use crate::application::ledger::LedgerPort;

    use super::*;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/sanitized/m5/full-import-history.xlsx")
    }

    fn imported_manager() -> (tempfile::TempDir, SqliteLedgerManager) {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let analysis = manager.analyze_import(&fixture()).unwrap();
        assert!(analysis.can_commit, "{:?}", analysis.issues);
        manager.commit_import(&analysis.batch_id, true).unwrap();
        (directory, manager)
    }

    #[test]
    fn overview_is_date_bounded_and_mtd_excludes_future_values() {
        let (_directory, manager) = imported_manager();
        let before_price = manager
            .get_overview(&LocalDate::parse("2026-03-04").unwrap())
            .unwrap();
        assert_eq!(before_price.mtd_expense, "0");
        assert!(
            before_price
                .unvalued_assets
                .iter()
                .any(|item| item.reason == "PRICE_MISSING_AS_OF")
        );
        let valued = manager
            .get_overview(&LocalDate::parse("2026-03-15").unwrap())
            .unwrap();
        assert_eq!(valued.mtd_start_date, "2026-03-01");
        assert_eq!(valued.mtd_end_date, "2026-03-15");
        assert_eq!(valued.mtd_expense, "35");
        assert_eq!(valued.valued_net_assets, "7840");
    }

    #[test]
    fn quality_reports_stale_prices_and_missing_fx_with_stable_fix_context() {
        let (_directory, manager) = imported_manager();
        let stale_date = LocalDate::parse("2026-09-01").unwrap();
        let stale = manager.get_data_quality(&stale_date).unwrap();
        let stale_issue = stale
            .issues
            .iter()
            .find(|item| item.code == "STALE_PRICE")
            .unwrap();
        assert_eq!(stale_issue.status, "open");
        assert_eq!(stale_issue.context.operation, "save_price_revision");
        assert_eq!(stale_issue.context.as_of_date, "2026-09-01");

        manager
            .store
            .as_ref()
            .unwrap()
            .connection
            .execute("UPDATE fx_rate_revisions SET active=0", [])
            .unwrap();
        let valuation_date = LocalDate::parse("2026-03-15").unwrap();
        let missing = manager.get_data_quality(&valuation_date).unwrap();
        assert!(
            missing
                .issues
                .iter()
                .any(|item| item.code == "FX_MISSING_AS_OF"
                    && item.context.operation == "save_fx_revision")
        );
        assert!(
            manager
                .get_overview(&valuation_date)
                .unwrap()
                .unvalued_assets
                .iter()
                .all(|item| item.reason == "FX_MISSING_AS_OF")
        );
    }

    #[test]
    fn confirmed_snapshots_are_immutable_and_form_a_supersession_chain() {
        let (_directory, mut manager) = imported_manager();
        let date = LocalDate::parse("2026-03-15").unwrap();
        let first = manager.confirm_valuation_snapshot(&date).unwrap();
        let second = manager.confirm_valuation_snapshot(&date).unwrap();
        assert_ne!(first.snapshot_id, second.snapshot_id);
        assert_eq!(second.supersedes_snapshot_id, Some(first.snapshot_id));
        let store = manager.store.as_ref().unwrap();
        let snapshots: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM valuation_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        let lines: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM valuation_snapshot_lines", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(snapshots, 2);
        assert_eq!(
            lines,
            i64::try_from(first.line_count + second.line_count).unwrap()
        );
    }

    #[test]
    fn mismatched_cash_and_holding_projections_rebuild_before_queries_resume() {
        let (directory, mut manager) = imported_manager();
        let date = LocalDate::parse("2026-03-15").unwrap();
        let before_overview = manager.get_overview(&date).unwrap();
        let before_quality = manager.get_data_quality(&date).unwrap();
        let snapshot = manager.confirm_valuation_snapshot(&date).unwrap();
        let store = manager.store.as_ref().unwrap();
        store
            .connection
            .execute_batch(
                "DELETE FROM holding_projection;
                 DELETE FROM cash_balance_projection;
                 UPDATE projection_metadata SET available=0,event_watermark=0,calculation_version='stale' WHERE projection_name IN ('holdings','cash-balance','monthly-cash-flow','cash-data-quality','expense-daily');",
            )
            .unwrap();
        drop(manager.store.take());
        let mut reopened = SqliteLedgerManager::new(directory.path()).unwrap();
        reopened.open_ledger().unwrap();
        assert_eq!(reopened.get_overview(&date).unwrap(), before_overview);
        assert_eq!(reopened.get_data_quality(&date).unwrap(), before_quality);
        let snapshot_count: i64 = reopened
            .store
            .as_ref()
            .unwrap()
            .connection
            .query_row(
                "SELECT COUNT(*) FROM valuation_snapshots WHERE valuation_snapshot_id=?1",
                [snapshot.snapshot_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(snapshot_count, 1);
    }
}
