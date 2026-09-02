#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde_json::json;

use crate::application::cash::{CashEventInput, EventInputType, FxOverrideInput};
use crate::application::catalog::{
    CashAccount, Category, FxRateRevision, Institution, Portfolio, SecurityInstrument,
    SecurityPriceRevision,
};
use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::import::{
    IMPORTER_VERSION, ImportAnalysis, ImportBalance, ImportCommitResult, ImportDifference,
    ImportIssue, ImportMapping, ImportMetric, ImportPort, ImportPosting, ImportProposedEvent,
    ImportReconciliation,
};
use crate::application::investment::{InvestmentEventInput, InvestmentEventType};
use crate::application::ledger::MigrationBackupPort;
use crate::domain::catalog::{BusinessId, CatalogText, CategoryKind, SemanticRole, SortOrder};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::investment::FeeScope;
use crate::domain::settings::UiLocale;
use crate::domain::types::{Currency, LocalDate, Sequence, UuidV7};
use crate::infrastructure::excel::{ParsedRow, ParsedWorkbook, parse_workbook, sha256, value};

use super::SqliteLedgerManager;
use super::cash_store::rebuild_cash_derived;
use super::investment_store::rebuild_investment_derived;
use super::migration::{MigrationRunner, inspect_read_only, validate_schema};
use super::schema::SCHEMA_VERSION;
use super::store::LedgerStore;

const STAGING_DIRECTORY: &str = "import-staging";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImportFailpoint {
    None,
    BeforeSwitch,
}

struct ImportPlan {
    mappings: Vec<ImportMapping>,
    proposed_events: Vec<ImportProposedEvent>,
    balances: Vec<ImportBalance>,
    metrics: Vec<ImportMetric>,
    difference_items: Vec<ImportDifference>,
    issues: Vec<ImportIssue>,
}

impl ImportPort for SqliteLedgerManager {
    fn analyze_import(&mut self, path: &Path) -> ApplicationResult<ImportAnalysis> {
        let parsed = parse_workbook(path)?;
        if let Some(mut existing) = self.find_analysis(&parsed.source_sha256)? {
            existing.reused_staging = true;
            return Ok(existing);
        }
        let batch_id = UuidV7::new()?.to_string();
        let candidate_path = self.candidate_path(&batch_id)?;
        let connection = MigrationRunner::create_new(&candidate_path)?;
        let base_currency = Currency::parse(&parsed.base_currency)
            .unwrap_or_else(|_| Currency::parse("CNY").expect("CNY is a valid currency"));
        let locale = UiLocale::parse(&parsed.ui_locale).unwrap_or(UiLocale::ZhCn);
        let mut candidate = LedgerStore::initialize(connection, base_currency, locale)?;
        let plan = build_plan(&mut candidate, &parsed);
        let blocker_count = count_severity(&plan.issues, "blocker");
        let warning_count = count_severity(&plan.issues, "warning");
        let status = if blocker_count == 0 {
            "ready"
        } else {
            "needs-review"
        };
        let canonical_result_sha256 = reconciliation_hash(
            &plan.proposed_events,
            &plan.balances,
            &plan.metrics,
            &plan.difference_items,
        )?;
        let balanced = plan
            .balances
            .iter()
            .all(|balance| balance.difference == "0")
            && plan.metrics.iter().all(|metric| metric.difference == "0")
            && plan
                .difference_items
                .iter()
                .all(|item| item.difference == "0" || !item.explanation.is_empty());
        let row_count =
            u32::try_from(parsed.rows.len()).map_err(|_| ApplicationError::ImportFileTooLarge)?;
        let invalid_rows = plan
            .issues
            .iter()
            .filter(|issue| issue.severity == "blocker" && issue.row > 0)
            .map(|issue| (&issue.sheet, issue.row))
            .collect::<BTreeSet<_>>()
            .len();
        let valid_row_count = row_count.saturating_sub(
            u32::try_from(invalid_rows).map_err(|_| ApplicationError::ImportFileTooLarge)?,
        );
        let analysis = ImportAnalysis {
            batch_id: batch_id.clone(),
            source_sha256: parsed.source_sha256.clone(),
            template_version: parsed.template_version.clone(),
            importer_version: IMPORTER_VERSION.to_owned(),
            target_schema_version: SCHEMA_VERSION,
            status: status.to_owned(),
            row_count,
            valid_row_count,
            blocker_count,
            warning_count,
            issues: plan.issues,
            mappings: plan.mappings,
            proposed_events: plan.proposed_events,
            reconciliation: ImportReconciliation {
                balances: plan.balances,
                metrics: plan.metrics,
                difference_bridge: vec![
                    "opening + income - expense + adjustment + transfers + exchanges - fees"
                        .to_owned(),
                    "derived/status/display formulas are evidence-only".to_owned(),
                ],
                difference_items: plan.difference_items,
                canonical_result_sha256,
                balanced,
            },
            can_commit: blocker_count == 0 && balanced,
            reused_staging: false,
        };
        save_staging(&mut candidate, &parsed, &analysis)?;
        candidate
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(analysis)
    }

    fn commit_import(
        &mut self,
        batch_id: &str,
        confirmed: bool,
    ) -> ApplicationResult<ImportCommitResult> {
        self.commit_import_with_failpoint(batch_id, confirmed, ImportFailpoint::None)
    }
}

impl SqliteLedgerManager {
    #[allow(clippy::too_many_lines)] // The ordered verification and atomic switch boundary is intentionally contiguous.
    fn commit_import_with_failpoint(
        &mut self,
        batch_id: &str,
        confirmed: bool,
        failpoint: ImportFailpoint,
    ) -> ApplicationResult<ImportCommitResult> {
        UuidV7::parse(batch_id).map_err(|_| ApplicationError::ImportBatchNotFound)?;
        if !confirmed {
            return Err(ApplicationError::ImportConfirmationRequired);
        }
        if self.database_path.exists() {
            if let Some(result) = self.committed_result(batch_id)? {
                return Ok(result);
            }
            return Err(ApplicationError::ImportModifiedMergeForbidden);
        }
        let candidate_path = self.candidate_path(batch_id)?;
        if !candidate_path.is_file() {
            return Err(ApplicationError::ImportBatchNotFound);
        }
        let connection =
            Connection::open(&candidate_path).map_err(|_| ApplicationError::ImportBatchNotFound)?;
        let mut candidate = LedgerStore::from_open_connection(connection)?;
        let analysis = load_analysis(&candidate.connection, batch_id)?;
        if !analysis.can_commit || analysis.blocker_count > 0 {
            return Err(ApplicationError::ImportBlockersPresent);
        }
        let rows = load_rows(&candidate.connection, batch_id)?;
        let parsed = ParsedWorkbook {
            source_sha256: analysis.source_sha256.clone(),
            template_version: analysis.template_version.clone(),
            base_currency: candidate
                .connection
                .query_row(
                    "SELECT base_currency FROM app_settings WHERE singleton_id=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::ImportBatchNotFound)?,
            ui_locale: candidate
                .connection
                .query_row(
                    "SELECT ui_locale FROM app_settings WHERE singleton_id=1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::ImportBatchNotFound)?,
            rows,
            issues: Vec::new(),
        };
        let transaction = candidate
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .execute(
                "UPDATE business_events SET import_batch_id=?1 WHERE import_batch_id IS NULL",
                [batch_id],
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        apply_import_account_states(&transaction, &parsed, &analysis.mappings)?;
        let watermark = transaction
            .query_row(
                "SELECT COALESCE(MAX(event_order),0) FROM business_events",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?
            .try_into()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        rebuild_investment_derived(&transaction, watermark)?;
        rebuild_cash_derived(&transaction, watermark)?;
        transaction
            .execute(
                "UPDATE import_batches SET status='committed',committed_at_utc=CURRENT_TIMESTAMP WHERE import_batch_id=?1",
                [batch_id],
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        validate_schema(&candidate.connection)?;
        verify_balances(&candidate.connection, &analysis.reconciliation.balances)?;
        verify_metrics(&candidate, &analysis.reconciliation.metrics)?;
        let canonical_posting_sha256 = candidate.canonical_posting_hash()?;
        self.backup
            .create_verified_backup(&candidate_path, SCHEMA_VERSION)
            .map_err(|_| ApplicationError::MigrationBackupFailed)?;
        candidate
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(|_| ApplicationError::TransactionFailed)?;
        if failpoint == ImportFailpoint::BeforeSwitch {
            return Err(ApplicationError::ImportCandidateSwitchFailed);
        }
        let status = candidate.status()?;
        drop(candidate);
        fs::rename(&candidate_path, &self.database_path)
            .map_err(|_| ApplicationError::ImportCandidateSwitchFailed)?;
        let connection = MigrationRunner::open_existing(&self.database_path, &mut self.backup)?;
        self.store = Some(LedgerStore::from_open_connection(connection)?);
        Ok(ImportCommitResult {
            batch_id: batch_id.to_owned(),
            source_sha256: analysis.source_sha256,
            status: "committed".to_owned(),
            ledger_id: status
                .ledger_id
                .ok_or(ApplicationError::SchemaValidationFailed)?,
            event_watermark: watermark,
            canonical_posting_sha256,
            already_committed: false,
        })
    }

    fn candidate_path(&self, batch_id: &str) -> ApplicationResult<PathBuf> {
        let root = self
            .database_path
            .parent()
            .ok_or(ApplicationError::StorageUnavailable)?
            .join(STAGING_DIRECTORY);
        fs::create_dir_all(&root).map_err(|_| ApplicationError::StorageUnavailable)?;
        Ok(root.join(format!("candidate-{batch_id}.sqlite3")))
    }

    fn find_analysis(&self, source_sha256: &str) -> ApplicationResult<Option<ImportAnalysis>> {
        if self.database_path.is_file() {
            let connection = Connection::open_with_flags(
                &self.database_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|_| ApplicationError::StorageUnavailable)?;
            if inspect_read_only(&self.database_path).is_ok()
                && let Some(value) = query_analysis(&connection, source_sha256)?
            {
                return Ok(Some(value));
            }
        }
        let root = self
            .database_path
            .parent()
            .ok_or(ApplicationError::StorageUnavailable)?
            .join(STAGING_DIRECTORY);
        if !root.is_dir() {
            return Ok(None);
        }
        let mut candidates = fs::read_dir(root)
            .map_err(|_| ApplicationError::StorageUnavailable)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sqlite3"))
            .collect::<Vec<_>>();
        candidates.sort();
        for path in candidates {
            let Ok(connection) = Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) else {
                continue;
            };
            if let Some(value) = query_analysis(&connection, source_sha256)? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    fn committed_result(&self, batch_id: &str) -> ApplicationResult<Option<ImportCommitResult>> {
        let Some(store) = &self.store else {
            return Ok(None);
        };
        let row: Option<(String, String)> = store
            .connection
            .query_row(
                "SELECT source_sha256,status FROM import_batches WHERE import_batch_id=?1",
                [batch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let Some((source_sha256, status)) = row.filter(|(_, status)| status == "committed") else {
            return Ok(None);
        };
        let ledger_status = store.status()?;
        Ok(Some(ImportCommitResult {
            batch_id: batch_id.to_owned(),
            source_sha256,
            status,
            ledger_id: ledger_status
                .ledger_id
                .ok_or(ApplicationError::SchemaValidationFailed)?,
            event_watermark: ledger_status.event_watermark,
            canonical_posting_sha256: store.canonical_posting_hash()?,
            already_committed: true,
        }))
    }
}

#[allow(clippy::too_many_lines)] // The known-template mapping pass shares one located issue collector.
fn build_plan(candidate: &mut LedgerStore, parsed: &ParsedWorkbook) -> ImportPlan {
    let mut issues = parsed.issues.clone();
    let mut mappings = Vec::new();
    let mut ids = BTreeMap::new();
    validate_settings(parsed, &mut issues);
    for row in parsed.rows.iter().filter(|row| row.sheet == "机构") {
        let result = (|| -> ApplicationResult<()> {
            let id = UuidV7::new()?;
            let institution = Institution {
                institution_id: id,
                business_id: BusinessId::parse(value(row, "legacy_id"))?,
                name: CatalogText::parse(value(row, "name"))?,
                region: optional_text(value(row, "region"))?,
                institution_type: CatalogText::parse(value(row, "institution_type"))?,
                enabled: parse_bool(value(row, "enabled"))?,
            };
            candidate.save_institution(&institution)?;
            ids.insert(("institution", value(row, "legacy_id").to_owned()), id);
            mappings.push(ImportMapping {
                entity_type: "institution".to_owned(),
                legacy_id: value(row, "legacy_id").to_owned(),
                target_id: id.to_string(),
                migration_policy: None,
            });
            Ok(())
        })();
        if result.is_err() {
            issues.push(row_issue("IMPORT_FIELD_INVALID", row, "institution"));
        }
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "资金子账户") {
        let result = (|| -> ApplicationResult<()> {
            let id = UuidV7::new()?;
            let institution_id = mapped(&ids, "institution", value(row, "institution_legacy_id"))?;
            let policy = value(row, "migration_policy");
            if !matches!(policy, "full_history" | "explicit_cutover") {
                return Err(ApplicationError::ImportFileInvalid);
            }
            if policy == "explicit_cutover" {
                LocalDate::parse(value(row, "cutover_date"))?;
            }
            if policy == "full_history" && !matches!(value(row, "opening_balance"), "" | "0") {
                return Err(ApplicationError::ImportFileInvalid);
            }
            let account = CashAccount {
                account_id: id,
                business_id: BusinessId::parse(value(row, "legacy_id"))?,
                institution_id,
                name: CatalogText::parse(value(row, "name"))?,
                purpose: CatalogText::parse(value(row, "purpose"))?,
                currency: Currency::parse(value(row, "currency"))?,
                opened_on: optional_date(value(row, "opened_on"))?,
                // Historical rows are replayed while the account is temporarily
                // active; commit restores the source lifecycle state atomically.
                enabled: true,
            };
            candidate.save_cash_account(&account)?;
            ids.insert(("account", value(row, "legacy_id").to_owned()), id);
            mappings.push(ImportMapping {
                entity_type: "account".to_owned(),
                legacy_id: value(row, "legacy_id").to_owned(),
                target_id: id.to_string(),
                migration_policy: Some(policy.to_owned()),
            });
            Ok(())
        })();
        if result.is_err() {
            issues.push(row_issue("IMPORT_FIELD_INVALID", row, "account"));
        }
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "投资组合") {
        let result = (|| -> ApplicationResult<()> {
            let id = UuidV7::new()?;
            let policy = value(row, "migration_policy");
            if !matches!(policy, "full_history" | "explicit_cutover") {
                return Err(ApplicationError::ImportFileInvalid);
            }
            if policy == "explicit_cutover" {
                LocalDate::parse(value(row, "cutover_date"))?;
            }
            let portfolio = Portfolio {
                portfolio_id: id,
                business_id: BusinessId::parse(value(row, "legacy_id"))?,
                institution_id: mapped(&ids, "institution", value(row, "institution_legacy_id"))?,
                settlement_account_id: mapped(
                    &ids,
                    "account",
                    value(row, "settlement_account_legacy_id"),
                )?,
                name: CatalogText::parse(value(row, "name"))?,
                portfolio_type: CatalogText::parse(value(row, "portfolio_type"))?,
                enabled: parse_bool(value(row, "enabled"))?,
            };
            candidate.save_portfolio(&portfolio)?;
            ids.insert(("portfolio", value(row, "legacy_id").to_owned()), id);
            mappings.push(ImportMapping {
                entity_type: "portfolio".to_owned(),
                legacy_id: value(row, "legacy_id").to_owned(),
                target_id: id.to_string(),
                migration_policy: Some(policy.to_owned()),
            });
            Ok(())
        })();
        if result.is_err() {
            issues.push(row_issue("IMPORT_FIELD_INVALID", row, "portfolio"));
        }
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "证券") {
        let result = (|| -> ApplicationResult<()> {
            let id = UuidV7::new()?;
            let instrument = SecurityInstrument {
                instrument_id: id,
                business_id: BusinessId::parse(value(row, "legacy_id"))?,
                code: CatalogText::parse(value(row, "code"))?,
                name: CatalogText::parse(value(row, "name"))?,
                trade_currency: Currency::parse(value(row, "trade_currency"))?,
                enabled: parse_bool(value(row, "enabled"))?,
            };
            candidate.save_instrument(&instrument)?;
            ids.insert(("instrument", value(row, "legacy_id").to_owned()), id);
            mappings.push(ImportMapping {
                entity_type: "instrument".to_owned(),
                legacy_id: value(row, "legacy_id").to_owned(),
                target_id: id.to_string(),
                migration_policy: None,
            });
            Ok(())
        })();
        if result.is_err() {
            issues.push(row_issue("IMPORT_FIELD_INVALID", row, "instrument"));
        }
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "分类") {
        let result = (|| -> ApplicationResult<()> {
            let id = UuidV7::new()?;
            let category = Category {
                category_id: id,
                name: CatalogText::parse(value(row, "name"))?,
                kind: CategoryKind::parse(value(row, "kind"))?,
                semantic_role: SemanticRole::parse(value(row, "semantic_role"))?,
                sort_order: SortOrder::new(parse_u32(value(row, "sort_order"))?)?,
                enabled: parse_bool(value(row, "enabled"))?,
            };
            candidate.save_category(&category)?;
            ids.insert(("category", value(row, "legacy_id").to_owned()), id);
            mappings.push(ImportMapping {
                entity_type: "category".to_owned(),
                legacy_id: value(row, "legacy_id").to_owned(),
                target_id: id.to_string(),
                migration_policy: None,
            });
            Ok(())
        })();
        if result.is_err() {
            issues.push(row_issue("IMPORT_FIELD_INVALID", row, "category"));
        }
    }
    let base_currency = Currency::parse(&parsed.base_currency)
        .unwrap_or_else(|_| Currency::parse("CNY").expect("CNY is a valid currency"));
    for row in parsed.rows.iter().filter(|row| row.sheet == "汇率") {
        let result = (|| -> ApplicationResult<()> {
            let revision = FxRateRevision::new(
                UuidV7::new()?,
                LocalDate::parse(value(row, "rate_date"))?,
                Currency::parse(value(row, "currency"))?,
                base_currency,
                value(row, "rate_to_base"),
                CatalogText::parse(value(row, "source"))?,
                parse_bool(value(row, "active"))?,
            )?;
            candidate.save_fx_revision(&revision)
        })();
        if result.is_err() {
            issues.push(row_issue("IMPORT_FIELD_INVALID", row, "rate_to_base"));
        }
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "证券价格") {
        let result = (|| -> ApplicationResult<()> {
            let instrument_id = mapped(&ids, "instrument", value(row, "instrument_legacy_id"))?;
            let revision = SecurityPriceRevision::new(
                UuidV7::new()?,
                instrument_id,
                LocalDate::parse(value(row, "price_date"))?,
                value(row, "price"),
                Currency::parse(value(row, "price_currency"))?,
                CatalogText::parse(value(row, "source"))?,
                parse_bool(value(row, "active"))?,
            )?;
            candidate.save_price_revision(&revision)
        })();
        if result.is_err() {
            issues.push(row_issue("IMPORT_FIELD_INVALID", row, "price"));
        }
    }
    let events = build_event_inputs(parsed, &mappings, &mut issues);
    let mut proposed_events = Vec::new();
    for (sheet, row, input) in &events {
        match candidate.post_cash_event(input, None, 1, Some("initial-xlsx-import")) {
            Ok(posted) => {
                if posted
                    .preview
                    .quality_issue_codes
                    .contains(&"MISSING_FX_RATE")
                {
                    issues.push(ImportIssue {
                        code: "IMPORT_MISSING_FX".to_owned(),
                        severity: "blocker".to_owned(),
                        sheet: sheet.clone(),
                        row: *row,
                        field: "currency".to_owned(),
                    });
                }
                proposed_events.push(ImportProposedEvent {
                    source_sheet: sheet.clone(),
                    source_row: *row,
                    event_type: posted.preview.event_type.to_owned(),
                    effective_date: posted.preview.effective_date,
                    sequence: posted.preview.sequence,
                    postings: posted
                        .preview
                        .postings
                        .into_iter()
                        .map(|posting| ImportPosting {
                            account_id: posting.account_id.unwrap_or_default(),
                            portfolio_id: None,
                            instrument_id: None,
                            quantity_delta: posting.quantity_delta,
                            currency: posting.currency,
                            base_value: posting.base_value,
                            role: posting.role.to_owned(),
                        })
                        .collect(),
                });
            }
            Err(_) => issues.push(ImportIssue {
                code: "IMPORT_EVENT_INVALID".to_owned(),
                severity: "blocker".to_owned(),
                sheet: sheet.clone(),
                row: *row,
                field: "event".to_owned(),
            }),
        }
    }
    let investment_events = build_investment_inputs(parsed, &mappings, &mut issues);
    for (sheet, row, input) in &investment_events {
        match candidate.post_investment(input, None, 1, Some("initial-xlsx-import")) {
            Ok(posted) => {
                if posted
                    .preview
                    .quality_issue_codes
                    .contains(&"MISSING_FX_RATE")
                {
                    issues.push(ImportIssue {
                        code: "IMPORT_MISSING_FX".to_owned(),
                        severity: "blocker".to_owned(),
                        sheet: sheet.clone(),
                        row: *row,
                        field: "currency".to_owned(),
                    });
                }
                proposed_events.push(ImportProposedEvent {
                    source_sheet: sheet.clone(),
                    source_row: *row,
                    event_type: posted.preview.event_type.to_owned(),
                    effective_date: posted.preview.effective_date,
                    sequence: posted.preview.sequence,
                    postings: posted
                        .preview
                        .postings
                        .into_iter()
                        .map(|posting| ImportPosting {
                            account_id: posting.account_id.unwrap_or_default(),
                            portfolio_id: Some(posting.portfolio_id),
                            instrument_id: posting.instrument_id,
                            quantity_delta: posting.quantity_delta,
                            currency: posting.currency,
                            base_value: posting.base_value,
                            role: posting.posting_kind.to_owned(),
                        })
                        .collect(),
                });
            }
            Err(_) => issues.push(ImportIssue {
                code: "IMPORT_EVENT_INVALID".to_owned(),
                severity: "blocker".to_owned(),
                sheet: sheet.clone(),
                row: *row,
                field: "event".to_owned(),
            }),
        }
    }
    let balances = reconcile_balances(parsed, &mappings, &proposed_events);
    if balances.iter().any(|balance| balance.difference != "0") {
        issues.push(ImportIssue {
            code: "IMPORT_RECONCILIATION_DIFFERENCE".to_owned(),
            severity: "blocker".to_owned(),
            sheet: "设置".to_owned(),
            row: 2,
            field: "cash_balances".to_owned(),
        });
    }
    let mut metrics = reconcile_investment_metrics(candidate, parsed, &mappings, &mut issues);
    metrics.extend(reconcile_check_rows(
        parsed,
        &mappings,
        &proposed_events,
        &mut issues,
    ));
    metrics.extend(reconcile_valuation_rows(candidate, parsed, &mut issues));
    metrics.sort_by(|left, right| {
        (&left.scope, &left.entity_id, &left.metric).cmp(&(
            &right.scope,
            &right.entity_id,
            &right.metric,
        ))
    });
    if metrics.iter().any(|metric| metric.difference != "0") {
        issues.push(ImportIssue {
            code: "IMPORT_RECONCILIATION_DIFFERENCE".to_owned(),
            severity: "blocker".to_owned(),
            sheet: "检查".to_owned(),
            row: 2,
            field: "investment_metrics".to_owned(),
        });
    }
    let difference_items = build_expense_difference_bridge(parsed, &proposed_events);
    if difference_items
        .iter()
        .any(|item| item.difference != "0" && item.explanation.is_empty())
    {
        issues.push(ImportIssue {
            code: "IMPORT_DIFFERENCE_UNEXPLAINED".to_owned(),
            severity: "blocker".to_owned(),
            sheet: "支出分析".to_owned(),
            row: 2,
            field: "source_amount".to_owned(),
        });
    }
    issues.sort_by(|left, right| {
        (&left.sheet, left.row, &left.field, &left.code).cmp(&(
            &right.sheet,
            right.row,
            &right.field,
            &right.code,
        ))
    });
    ImportPlan {
        mappings,
        proposed_events,
        balances,
        metrics,
        difference_items,
        issues,
    }
}

#[allow(clippy::too_many_lines)] // One ordered pass preserves cash event source-row evidence.
fn build_event_inputs(
    parsed: &ParsedWorkbook,
    mappings: &[ImportMapping],
    issues: &mut Vec<ImportIssue>,
) -> Vec<(String, u32, CashEventInput)> {
    let lookup = mappings
        .iter()
        .map(|item| {
            (
                (item.entity_type.as_str(), item.legacy_id.as_str()),
                item.target_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut events = Vec::new();
    for row in parsed.rows.iter().filter(|row| row.sheet == "资金子账户") {
        let amount = value(row, "opening_balance");
        if amount.is_empty()
            || amount == "0"
            || value(row, "migration_policy") != "explicit_cutover"
        {
            continue;
        }
        let result = (|| -> ApplicationResult<CashEventInput> {
            let account_id = target(&lookup, "account", value(row, "legacy_id"))?;
            Ok(CashEventInput {
                effective_date: LocalDate::parse(value(row, "cutover_date"))?,
                sequence: Sequence::new(8_000_000 + u64::from(row.row))?,
                event_type: EventInputType::OpeningBalance,
                account_id: Some(account_id),
                from_account_id: None,
                to_account_id: None,
                amount: Some(Decimal::parse(amount, DecimalUse::Amount)?),
                to_amount: None,
                category_id: None,
                semantic_role: SemanticRole::Normal,
                merchant: None,
                note: Some("Imported opening balance".to_owned()),
                fee_account_id: None,
                fee_amount: None,
                cutover_date: Some(LocalDate::parse(value(row, "cutover_date"))?),
                migration_policy: Some(value(row, "migration_policy").to_owned()),
                fx_overrides: Vec::new(),
                currency_precision_confirmed: true,
            })
        })();
        push_event_result(row, result, issues, &mut events);
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "收支流水") {
        if !cash_row_in_scope(
            parsed,
            row,
            &[
                value(row, "account_legacy_id"),
                value(row, "fee_account_legacy_id"),
            ],
            issues,
        ) {
            continue;
        }
        let result = (|| -> ApplicationResult<CashEventInput> {
            Ok(CashEventInput {
                effective_date: LocalDate::parse(value(row, "date"))?,
                sequence: Sequence::new(parse_u64(value(row, "sequence"))?)?,
                event_type: EventInputType::parse(value(row, "type"))?,
                account_id: Some(target(&lookup, "account", value(row, "account_legacy_id"))?),
                from_account_id: None,
                to_account_id: None,
                amount: Some(Decimal::parse(value(row, "amount"), DecimalUse::Amount)?),
                to_amount: None,
                category_id: optional_target(
                    &lookup,
                    "category",
                    value(row, "category_legacy_id"),
                )?,
                semantic_role: SemanticRole::parse(value(row, "semantic_role"))?,
                merchant: optional_string(value(row, "merchant")),
                note: optional_string(value(row, "note")),
                fee_account_id: optional_target(
                    &lookup,
                    "account",
                    value(row, "fee_account_legacy_id"),
                )?,
                fee_amount: optional_decimal(value(row, "fee_amount"))?,
                cutover_date: None,
                migration_policy: None,
                fx_overrides: parse_fx_overrides(
                    row,
                    &[
                        (
                            "fx_override_currency",
                            "fx_override_value",
                            "fx_override_reason",
                        ),
                        (
                            "fee_fx_override_currency",
                            "fee_fx_override_value",
                            "fee_fx_override_reason",
                        ),
                    ],
                )?,
                currency_precision_confirmed: true,
            })
        })();
        push_event_result(row, result, issues, &mut events);
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "资金调拨") {
        if !cash_row_in_scope(
            parsed,
            row,
            &[
                value(row, "from_account_legacy_id"),
                value(row, "to_account_legacy_id"),
            ],
            issues,
        ) {
            continue;
        }
        let result = transfer_input(row, &lookup);
        push_event_result(row, result, issues, &mut events);
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "换汇流水") {
        if !cash_row_in_scope(
            parsed,
            row,
            &[
                value(row, "from_account_legacy_id"),
                value(row, "to_account_legacy_id"),
                value(row, "fee_account_legacy_id"),
            ],
            issues,
        ) {
            continue;
        }
        let result = exchange_input(row, &lookup);
        push_event_result(row, result, issues, &mut events);
    }
    events.sort_by(|left, right| {
        (left.2.effective_date.as_str(), left.2.sequence.get())
            .cmp(&(right.2.effective_date.as_str(), right.2.sequence.get()))
    });
    events
}

#[allow(clippy::too_many_lines)]
fn build_investment_inputs(
    parsed: &ParsedWorkbook,
    mappings: &[ImportMapping],
    issues: &mut Vec<ImportIssue>,
) -> Vec<(String, u32, InvestmentEventInput)> {
    let lookup = mappings
        .iter()
        .map(|item| {
            (
                (item.entity_type.as_str(), item.legacy_id.as_str()),
                item.target_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut events = Vec::new();
    for row in parsed.rows.iter().filter(|row| row.sheet == "持仓基线") {
        let portfolio_legacy_id = value(row, "portfolio_legacy_id");
        let Some(portfolio_row) = parsed.rows.iter().find(|candidate| {
            candidate.sheet == "投资组合" && value(candidate, "legacy_id") == portfolio_legacy_id
        }) else {
            continue;
        };
        if value(portfolio_row, "migration_policy") != "explicit_cutover" {
            continue;
        }
        let result = (|| -> ApplicationResult<Vec<InvestmentEventInput>> {
            let cutover = LocalDate::parse(value(portfolio_row, "cutover_date"))?;
            if value(row, "as_of_date") != cutover.as_str() {
                return Err(ApplicationError::ImportFileInvalid);
            }
            let portfolio_id = target(&lookup, "portfolio", portfolio_legacy_id)?;
            let settlement_account_id = target(
                &lookup,
                "account",
                value(portfolio_row, "settlement_account_legacy_id"),
            )?;
            let instrument_id =
                optional_target(&lookup, "instrument", value(row, "instrument_legacy_id"))?;
            let currency = Currency::parse(value(row, "currency"))?;
            let mut opening = Vec::new();
            if instrument_id.is_some() {
                opening.push(InvestmentEventInput {
                    effective_date: cutover.clone(),
                    sequence: Sequence::new(6_000_000 + u64::from(row.row) * 2)?,
                    event_type: InvestmentEventType::OpeningPosition,
                    portfolio_id,
                    instrument_id,
                    settlement_account_id,
                    quantity: Some(Decimal::parse(
                        value(row, "quantity"),
                        DecimalUse::Quantity,
                    )?),
                    unit_price: None,
                    trade_fee: None,
                    gross_cash_amount: None,
                    withholding_tax: None,
                    fee_amount: None,
                    amount: None,
                    carrying_cost: Some(Decimal::parse(
                        value(row, "carrying_cost"),
                        DecimalUse::Internal,
                    )?),
                    realized_trade_pnl: None,
                    net_dividend: None,
                    independent_expense: None,
                    cost_currency: Some(currency),
                    cutover_date: Some(cutover.clone()),
                    migration_policy: Some("explicit_cutover".to_owned()),
                    fee_scope: None,
                    settlement_override_reason: None,
                    fx_overrides: Vec::new(),
                });
            }
            opening.push(InvestmentEventInput {
                effective_date: cutover.clone(),
                sequence: Sequence::new(6_000_001 + u64::from(row.row) * 2)?,
                event_type: InvestmentEventType::OpeningPerformance,
                portfolio_id,
                instrument_id,
                settlement_account_id,
                quantity: None,
                unit_price: None,
                trade_fee: None,
                gross_cash_amount: None,
                withholding_tax: None,
                fee_amount: None,
                amount: None,
                carrying_cost: None,
                realized_trade_pnl: Some(Decimal::parse(
                    value(row, "realized_trade_pnl"),
                    DecimalUse::Internal,
                )?),
                net_dividend: Some(Decimal::parse(
                    value(row, "net_dividend"),
                    DecimalUse::Internal,
                )?),
                independent_expense: Some(Decimal::parse(
                    value(row, "independent_expense"),
                    DecimalUse::Internal,
                )?),
                cost_currency: Some(currency),
                cutover_date: Some(cutover),
                migration_policy: Some("explicit_cutover".to_owned()),
                fee_scope: None,
                settlement_override_reason: None,
                fx_overrides: Vec::new(),
            });
            Ok(opening)
        })();
        match result {
            Ok(values) => events.extend(
                values
                    .into_iter()
                    .map(|input| (row.sheet.clone(), row.row, input)),
            ),
            Err(_) => issues.push(row_issue("IMPORT_FIELD_INVALID", row, "opening")),
        }
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "投资流水") {
        if !investment_row_in_scope(parsed, row, issues) {
            continue;
        }
        let result = (|| -> ApplicationResult<InvestmentEventInput> {
            let event_type = InvestmentEventType::parse(value(row, "type"))?;
            Ok(InvestmentEventInput {
                effective_date: LocalDate::parse(value(row, "date"))?,
                sequence: Sequence::new(parse_u64(value(row, "sequence"))?)?,
                event_type,
                portfolio_id: target(&lookup, "portfolio", value(row, "portfolio_legacy_id"))?,
                instrument_id: optional_target(
                    &lookup,
                    "instrument",
                    value(row, "instrument_legacy_id"),
                )?,
                settlement_account_id: target(
                    &lookup,
                    "account",
                    value(row, "settlement_account_legacy_id"),
                )?,
                quantity: optional_decimal_use(value(row, "quantity"), DecimalUse::Quantity)?,
                unit_price: optional_decimal_use(value(row, "unit_price"), DecimalUse::UnitPrice)?,
                trade_fee: optional_decimal(value(row, "trade_fee"))?,
                gross_cash_amount: optional_decimal(value(row, "gross_cash_amount"))?,
                withholding_tax: optional_decimal(value(row, "withholding_tax"))?,
                fee_amount: optional_decimal(value(row, "fee_amount"))?,
                amount: optional_decimal(value(row, "amount"))?,
                carrying_cost: None,
                realized_trade_pnl: None,
                net_dividend: None,
                independent_expense: None,
                cost_currency: None,
                cutover_date: None,
                migration_policy: None,
                fee_scope: optional_fee_scope(value(row, "fee_scope"))?,
                settlement_override_reason: optional_string(value(
                    row,
                    "settlement_override_reason",
                )),
                fx_overrides: parse_fx_overrides(
                    row,
                    &[(
                        "fx_override_currency",
                        "fx_override_value",
                        "fx_override_reason",
                    )],
                )?,
            })
        })();
        match result {
            Ok(input) => events.push((row.sheet.clone(), row.row, input)),
            Err(_) => issues.push(row_issue("IMPORT_FIELD_INVALID", row, "event")),
        }
    }
    events.sort_by(|left, right| {
        (left.2.effective_date.as_str(), left.2.sequence.get())
            .cmp(&(right.2.effective_date.as_str(), right.2.sequence.get()))
    });
    events
}

fn cash_row_in_scope(
    parsed: &ParsedWorkbook,
    row: &ParsedRow,
    account_ids: &[&str],
    issues: &mut Vec<ImportIssue>,
) -> bool {
    let Ok(date) = LocalDate::parse(value(row, "date")) else {
        return true;
    };
    let decisions = account_ids
        .iter()
        .filter(|id| !id.is_empty())
        .filter_map(|id| {
            parsed.rows.iter().find(|candidate| {
                candidate.sheet == "资金子账户" && value(candidate, "legacy_id") == *id
            })
        })
        .map(|account| {
            (
                value(account, "migration_policy"),
                value(account, "cutover_date"),
            )
        })
        .collect::<BTreeSet<_>>();
    if decisions.len() != 1 {
        issues.push(row_issue(
            "IMPORT_MIGRATION_POLICY_MISMATCH",
            row,
            "migration_policy",
        ));
        return false;
    }
    let Some((policy, cutover)) = decisions.into_iter().next() else {
        return true;
    };
    policy != "explicit_cutover" || LocalDate::parse(cutover).is_ok_and(|value| date > value)
}

fn investment_row_in_scope(
    parsed: &ParsedWorkbook,
    row: &ParsedRow,
    issues: &mut Vec<ImportIssue>,
) -> bool {
    let portfolio = parsed.rows.iter().find(|candidate| {
        candidate.sheet == "投资组合"
            && value(candidate, "legacy_id") == value(row, "portfolio_legacy_id")
    });
    let account = parsed.rows.iter().find(|candidate| {
        candidate.sheet == "资金子账户"
            && value(candidate, "legacy_id") == value(row, "settlement_account_legacy_id")
    });
    let Some((portfolio, account)) = portfolio.zip(account) else {
        return true;
    };
    let policy = value(portfolio, "migration_policy");
    let cutover = value(portfolio, "cutover_date");
    if policy != value(account, "migration_policy")
        || (policy == "explicit_cutover" && cutover != value(account, "cutover_date"))
    {
        issues.push(row_issue(
            "IMPORT_MIGRATION_POLICY_MISMATCH",
            row,
            "migration_policy",
        ));
        return false;
    }
    if policy == "full_history" {
        return true;
    }
    let date = LocalDate::parse(value(row, "date"));
    let cutover = LocalDate::parse(cutover);
    matches!((date, cutover), (Ok(date), Ok(cutover)) if date > cutover)
}

fn transfer_input(
    row: &ParsedRow,
    lookup: &BTreeMap<(&str, &str), &str>,
) -> ApplicationResult<CashEventInput> {
    Ok(CashEventInput {
        effective_date: LocalDate::parse(value(row, "date"))?,
        sequence: Sequence::new(parse_u64(value(row, "sequence"))?)?,
        event_type: EventInputType::Transfer,
        account_id: None,
        from_account_id: Some(target(
            lookup,
            "account",
            value(row, "from_account_legacy_id"),
        )?),
        to_account_id: Some(target(
            lookup,
            "account",
            value(row, "to_account_legacy_id"),
        )?),
        amount: Some(Decimal::parse(value(row, "amount"), DecimalUse::Amount)?),
        to_amount: None,
        category_id: None,
        semantic_role: SemanticRole::Normal,
        merchant: None,
        note: optional_string(value(row, "note")),
        fee_account_id: None,
        fee_amount: None,
        cutover_date: None,
        migration_policy: None,
        fx_overrides: parse_fx_overrides(
            row,
            &[(
                "fx_override_currency",
                "fx_override_value",
                "fx_override_reason",
            )],
        )?,
        currency_precision_confirmed: true,
    })
}

fn exchange_input(
    row: &ParsedRow,
    lookup: &BTreeMap<(&str, &str), &str>,
) -> ApplicationResult<CashEventInput> {
    Ok(CashEventInput {
        effective_date: LocalDate::parse(value(row, "date"))?,
        sequence: Sequence::new(parse_u64(value(row, "sequence"))?)?,
        event_type: EventInputType::CurrencyExchange,
        account_id: None,
        from_account_id: Some(target(
            lookup,
            "account",
            value(row, "from_account_legacy_id"),
        )?),
        to_account_id: Some(target(
            lookup,
            "account",
            value(row, "to_account_legacy_id"),
        )?),
        amount: Some(Decimal::parse(
            value(row, "from_amount"),
            DecimalUse::Amount,
        )?),
        to_amount: Some(Decimal::parse(value(row, "to_amount"), DecimalUse::Amount)?),
        category_id: None,
        semantic_role: SemanticRole::Normal,
        merchant: None,
        note: optional_string(value(row, "note")),
        fee_account_id: optional_target(lookup, "account", value(row, "fee_account_legacy_id"))?,
        fee_amount: optional_decimal(value(row, "fee_amount"))?,
        cutover_date: None,
        migration_policy: None,
        fx_overrides: parse_fx_overrides(
            row,
            &[
                (
                    "from_fx_override_currency",
                    "from_fx_override_value",
                    "from_fx_override_reason",
                ),
                (
                    "to_fx_override_currency",
                    "to_fx_override_value",
                    "to_fx_override_reason",
                ),
                (
                    "fee_fx_override_currency",
                    "fee_fx_override_value",
                    "fee_fx_override_reason",
                ),
            ],
        )?,
        currency_precision_confirmed: true,
    })
}

fn push_event_result(
    row: &ParsedRow,
    result: ApplicationResult<CashEventInput>,
    issues: &mut Vec<ImportIssue>,
    events: &mut Vec<(String, u32, CashEventInput)>,
) {
    match result {
        Ok(input) => events.push((row.sheet.clone(), row.row, input)),
        Err(_) => issues.push(row_issue("IMPORT_FIELD_INVALID", row, "event")),
    }
}

#[allow(clippy::too_many_lines)] // Source semantics and Core previews stay visibly side-by-side for audit.
fn reconcile_balances(
    parsed: &ParsedWorkbook,
    mappings: &[ImportMapping],
    events: &[ImportProposedEvent],
) -> Vec<ImportBalance> {
    let mut proposed = BTreeMap::<(String, String), Decimal>::new();
    for event in events {
        for posting in &event.postings {
            if posting.account_id.is_empty() {
                continue;
            }
            if let Ok(amount) = Decimal::parse(&posting.quantity_delta, DecimalUse::Amount) {
                let key = (posting.account_id.clone(), posting.currency.clone());
                add_balance(&mut proposed, key, &amount);
            }
        }
    }
    let mut source = BTreeMap::<(String, String), Decimal>::new();
    let account_ids = mappings
        .iter()
        .filter(|mapping| mapping.entity_type == "account")
        .map(|mapping| (mapping.legacy_id.as_str(), mapping.target_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let currencies = parsed
        .rows
        .iter()
        .filter(|row| row.sheet == "资金子账户")
        .map(|row| (value(row, "legacy_id"), value(row, "currency")))
        .collect::<BTreeMap<_, _>>();
    for row in parsed.rows.iter().filter(|row| row.sheet == "资金子账户") {
        if value(row, "migration_policy") != "explicit_cutover" {
            continue;
        }
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "legacy_id"),
            value(row, "opening_balance"),
            false,
        );
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "收支流水") {
        if !cash_row_in_scope(
            parsed,
            row,
            &[
                value(row, "account_legacy_id"),
                value(row, "fee_account_legacy_id"),
            ],
            &mut Vec::new(),
        ) {
            continue;
        }
        let negative = value(row, "type") == "Expense";
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "account_legacy_id"),
            value(row, "amount"),
            negative,
        );
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "fee_account_legacy_id"),
            value(row, "fee_amount"),
            true,
        );
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "资金调拨") {
        if !cash_row_in_scope(
            parsed,
            row,
            &[
                value(row, "from_account_legacy_id"),
                value(row, "to_account_legacy_id"),
            ],
            &mut Vec::new(),
        ) {
            continue;
        }
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "from_account_legacy_id"),
            value(row, "amount"),
            true,
        );
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "to_account_legacy_id"),
            value(row, "amount"),
            false,
        );
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "换汇流水") {
        if !cash_row_in_scope(
            parsed,
            row,
            &[
                value(row, "from_account_legacy_id"),
                value(row, "to_account_legacy_id"),
                value(row, "fee_account_legacy_id"),
            ],
            &mut Vec::new(),
        ) {
            continue;
        }
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "from_account_legacy_id"),
            value(row, "from_amount"),
            true,
        );
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "to_account_legacy_id"),
            value(row, "to_amount"),
            false,
        );
        add_source_value(
            &mut source,
            &account_ids,
            &currencies,
            value(row, "fee_account_legacy_id"),
            value(row, "fee_amount"),
            true,
        );
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "投资流水") {
        if !investment_row_in_scope(parsed, row, &mut Vec::new()) {
            continue;
        }
        let settlement = value(row, "settlement_account_legacy_id");
        let amount = match value(row, "type") {
            "SecurityBuy" => investment_trade_cash(row, true),
            "SecuritySell" => investment_trade_cash(row, false),
            "Dividend" => investment_dividend_cash(row),
            "InvestmentExpense" => Decimal::parse(value(row, "amount"), DecimalUse::Amount)
                .map_err(ApplicationError::from)
                .and_then(|value| value.checked_neg(DecimalUse::Amount).map_err(Into::into)),
            _ => continue,
        };
        if let Ok(amount) = amount
            && let Some((target_id, currency)) =
                account_ids.get(settlement).zip(currencies.get(settlement))
        {
            add_balance(
                &mut source,
                ((*target_id).to_owned(), (*currency).to_owned()),
                &amount,
            );
        }
    }
    let keys = source
        .keys()
        .chain(proposed.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    keys.iter()
        .map(|(account_id, currency)| {
            let source_balance = source
                .get(&(account_id.clone(), currency.clone()))
                .cloned()
                .unwrap_or_else(|| Decimal::zero(DecimalUse::Amount));
            let proposed_balance = proposed
                .get(&(account_id.clone(), currency.clone()))
                .cloned()
                .unwrap_or_else(|| Decimal::zero(DecimalUse::Amount));
            let difference = proposed_balance
                .checked_add(
                    &source_balance
                        .checked_neg(DecimalUse::Amount)
                        .unwrap_or_else(|_| Decimal::zero(DecimalUse::Amount)),
                    DecimalUse::Amount,
                )
                .unwrap_or_else(|_| Decimal::zero(DecimalUse::Amount))
                .normalized();
            ImportBalance {
                account_id: account_id.clone(),
                currency: currency.clone(),
                source_balance: source_balance.normalized().as_str().to_owned(),
                proposed_balance: proposed_balance.normalized().as_str().to_owned(),
                difference: difference.as_str().to_owned(),
            }
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // The matrix keeps each source metric adjacent to its canonical comparator.
fn reconcile_investment_metrics(
    candidate: &LedgerStore,
    parsed: &ParsedWorkbook,
    mappings: &[ImportMapping],
    issues: &mut Vec<ImportIssue>,
) -> Vec<ImportMetric> {
    let lookup = mappings
        .iter()
        .map(|item| {
            (
                (item.entity_type.as_str(), item.legacy_id.as_str()),
                item.target_id.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut metrics = Vec::new();
    for row in parsed.rows.iter().filter(|row| row.sheet == "持仓基线") {
        let result = (|| -> ApplicationResult<Vec<ImportMetric>> {
            let as_of = LocalDate::parse(value(row, "as_of_date"))?;
            let workspace = candidate.investment_workspace(&as_of)?;
            let portfolio_id =
                target(&lookup, "portfolio", value(row, "portfolio_legacy_id"))?.to_string();
            let instrument_id =
                optional_target(&lookup, "instrument", value(row, "instrument_legacy_id"))?
                    .map(|value| value.to_string());
            let source = |field: &str| {
                Decimal::parse(value(row, field), DecimalUse::Internal)
                    .map(|value| value.normalized())
            };
            let mut values = Vec::new();
            if let Some(instrument_id) = instrument_id {
                let holding = workspace
                    .holdings
                    .iter()
                    .find(|item| {
                        item.portfolio_id == portfolio_id && item.instrument_id == instrument_id
                    })
                    .ok_or(ApplicationError::ImportReconciliationFailed)?;
                for (name, source_value, proposed_value) in [
                    ("quantity", source("quantity")?, &holding.quantity),
                    (
                        "carrying_cost",
                        source("carrying_cost")?,
                        &holding.carrying_cost,
                    ),
                    (
                        "realized_trade_pnl",
                        source("realized_trade_pnl")?,
                        &holding.realized_trade_pnl,
                    ),
                    (
                        "net_dividend",
                        source("net_dividend")?,
                        &holding.net_dividend,
                    ),
                    (
                        "independent_expense",
                        source("independent_expense")?,
                        &holding.independent_expense,
                    ),
                ] {
                    values.push(import_metric(
                        "holding",
                        &format!("{portfolio_id}:{instrument_id}"),
                        name,
                        &source_value,
                        proposed_value,
                        Some(as_of.as_str()),
                    )?);
                }
            } else {
                let proposed = workspace
                    .portfolio_expenses
                    .iter()
                    .filter(|item| item.portfolio_id == portfolio_id)
                    .try_fold(Decimal::zero(DecimalUse::Internal), |current, item| {
                        current.checked_add(
                            &Decimal::parse(&item.amount, DecimalUse::Internal)?,
                            DecimalUse::Internal,
                        )
                    })?;
                values.push(import_metric(
                    "portfolio",
                    &portfolio_id,
                    "independent_expense",
                    &source("independent_expense")?,
                    proposed.as_str(),
                    Some(as_of.as_str()),
                )?);
            }
            Ok(values)
        })();
        match result {
            Ok(values) => metrics.extend(values),
            Err(_) => issues.push(row_issue(
                "IMPORT_RECONCILIATION_DIFFERENCE",
                row,
                "holding_baseline",
            )),
        }
    }
    metrics.sort_by(|left, right| {
        (&left.scope, &left.entity_id, &left.metric).cmp(&(
            &right.scope,
            &right.entity_id,
            &right.metric,
        ))
    });
    metrics
}

fn reconcile_check_rows(
    parsed: &ParsedWorkbook,
    mappings: &[ImportMapping],
    events: &[ImportProposedEvent],
    issues: &mut Vec<ImportIssue>,
) -> Vec<ImportMetric> {
    let mut output = Vec::new();
    for row in parsed
        .rows
        .iter()
        .filter(|row| row.sheet == "检查" && value(row, "scope") != "valuation")
    {
        let result = (|| -> ApplicationResult<ImportMetric> {
            let scope = value(row, "scope");
            let entity = value(row, "legacy_id");
            let metric = value(row, "metric");
            let proposed = match (scope, metric) {
                ("mapping", "count") => u64::try_from(
                    mappings
                        .iter()
                        .filter(|item| item.entity_type == entity)
                        .count(),
                )
                .map_err(|_| ApplicationError::ResponseTooLarge)?,
                ("events", "count") => {
                    u64::try_from(events.len()).map_err(|_| ApplicationError::ResponseTooLarge)?
                }
                ("source", "row_count") => u64::try_from(parsed.rows.len())
                    .map_err(|_| ApplicationError::ResponseTooLarge)?,
                ("currency", "count") => u64::try_from(
                    parsed
                        .rows
                        .iter()
                        .filter_map(|candidate| match candidate.sheet.as_str() {
                            "资金子账户" => Some(value(candidate, "currency")),
                            "证券" => Some(value(candidate, "trade_currency")),
                            _ => None,
                        })
                        .filter(|value| !value.is_empty())
                        .collect::<BTreeSet<_>>()
                        .len(),
                )
                .map_err(|_| ApplicationError::ResponseTooLarge)?,
                _ => return Err(ApplicationError::ImportFileInvalid),
            };
            let source = parse_u64(value(row, "source_value"))?;
            Ok(ImportMetric {
                scope: scope.to_owned(),
                entity_id: entity.to_owned(),
                metric: metric.to_owned(),
                source_value: source.to_string(),
                proposed_value: proposed.to_string(),
                difference: i128::from(proposed)
                    .saturating_sub(i128::from(source))
                    .to_string(),
                as_of_date: None,
            })
        })();
        match result {
            Ok(value) => output.push(value),
            Err(_) => issues.push(row_issue("IMPORT_CHECK_INVALID", row, "metric")),
        }
    }
    output
}

fn reconcile_valuation_rows(
    candidate: &LedgerStore,
    parsed: &ParsedWorkbook,
    issues: &mut Vec<ImportIssue>,
) -> Vec<ImportMetric> {
    let mut output = Vec::new();
    for row in parsed
        .rows
        .iter()
        .filter(|row| row.sheet == "检查" && value(row, "scope") == "valuation")
    {
        let result = (|| -> ApplicationResult<ImportMetric> {
            if value(row, "metric") != "valued_net_assets" {
                return Err(ApplicationError::ImportFileInvalid);
            }
            let as_of = LocalDate::parse(value(row, "as_of_date"))?;
            let source = Decimal::parse(value(row, "source_value"), DecimalUse::Internal)?;
            let overview = candidate.overview(&as_of)?;
            import_metric(
                "valuation",
                value(row, "legacy_id"),
                "valued_net_assets",
                &source,
                &overview.valued_net_assets,
                Some(as_of.as_str()),
            )
        })();
        match result {
            Ok(value) => output.push(value),
            Err(_) => issues.push(row_issue("IMPORT_CHECK_INVALID", row, "valued_net_assets")),
        }
    }
    output
}

fn import_metric(
    scope: &str,
    entity_id: &str,
    metric: &str,
    source: &Decimal,
    proposed: &str,
    as_of_date: Option<&str>,
) -> ApplicationResult<ImportMetric> {
    let proposed = Decimal::parse(proposed, DecimalUse::Internal)?.normalized();
    let source = source.normalized();
    let difference = proposed.checked_add(
        &source.checked_neg(DecimalUse::Internal)?,
        DecimalUse::Internal,
    )?;
    Ok(ImportMetric {
        scope: scope.to_owned(),
        entity_id: entity_id.to_owned(),
        metric: metric.to_owned(),
        source_value: source.as_str().to_owned(),
        proposed_value: proposed.as_str().to_owned(),
        difference: difference.normalized().as_str().to_owned(),
        as_of_date: as_of_date.map(ToOwned::to_owned),
    })
}

fn build_expense_difference_bridge(
    parsed: &ParsedWorkbook,
    events: &[ImportProposedEvent],
) -> Vec<ImportDifference> {
    let mut items = Vec::new();
    for row in parsed.rows.iter().filter(|row| row.sheet == "支出分析") {
        let result = (|| -> ApplicationResult<ImportDifference> {
            let start = LocalDate::parse(value(row, "start_date"))?;
            let end = LocalDate::parse(value(row, "end_date"))?;
            if start > end {
                return Err(ApplicationError::ExpenseDateRangeInvalid);
            }
            let bucket = value(row, "bucket_id");
            let mut amount = Decimal::zero(DecimalUse::Internal);
            let mut event_ids = BTreeSet::<(String, u32)>::new();
            for event in events.iter().filter(|event| {
                event.source_sheet == "收支流水"
                    && event.effective_date.as_str() >= start.as_str()
                    && event.effective_date.as_str() <= end.as_str()
            }) {
                let Some(source_row) = parsed.rows.iter().find(|source| {
                    source.sheet == event.source_sheet && source.row == event.source_row
                }) else {
                    continue;
                };
                if value(source_row, "semantic_role") != "normal"
                    || (bucket != "all" && bucket != value(source_row, "category_legacy_id"))
                {
                    continue;
                }
                let mut contributed = false;
                for posting in &event.postings {
                    let included = posting.role == "fee"
                        || (posting.role == "principal" && event.event_type == "Expense");
                    if !included {
                        continue;
                    }
                    if let Some(base) = &posting.base_value {
                        let value = Decimal::parse(base, DecimalUse::Internal)?;
                        let expense = if value.is_negative() {
                            value.checked_neg(DecimalUse::Internal)?
                        } else {
                            value
                        };
                        amount = amount.checked_add(&expense, DecimalUse::Internal)?;
                        contributed = true;
                    }
                }
                if contributed {
                    event_ids.insert((event.source_sheet.clone(), event.source_row));
                }
            }
            let source_amount =
                Decimal::parse(value(row, "source_amount"), DecimalUse::Internal)?.normalized();
            let difference = amount.checked_add(
                &source_amount.checked_neg(DecimalUse::Internal)?,
                DecimalUse::Internal,
            )?;
            let application_count =
                u64::try_from(event_ids.len()).map_err(|_| ApplicationError::ResponseTooLarge)?;
            let source_count = parse_u64(value(row, "source_count"))?;
            let count_difference = i128::from(application_count) - i128::from(source_count);
            Ok(ImportDifference {
                scope: "expense-bucket".to_owned(),
                key: format!("{}:{}:{bucket}", start.as_str(), end.as_str()),
                excel_value: format!("{}|{source_count}", source_amount.as_str()),
                application_value: format!("{}|{application_count}", amount.normalized().as_str()),
                difference: if difference.is_zero() && count_difference == 0 {
                    "0".to_owned()
                } else {
                    format!("{}|{count_difference}", difference.normalized().as_str())
                },
                explanation: value(row, "explanation").to_owned(),
            })
        })();
        if let Ok(item) = result {
            items.push(item);
        }
    }
    items
}

fn investment_trade_cash(row: &ParsedRow, buy: bool) -> ApplicationResult<Decimal> {
    let quantity = Decimal::parse(value(row, "quantity"), DecimalUse::Quantity)?;
    let price = Decimal::parse(value(row, "unit_price"), DecimalUse::UnitPrice)?;
    let gross = quantity.checked_mul_internal(&price)?;
    let fee = optional_decimal(value(row, "trade_fee"))?
        .unwrap_or_else(|| Decimal::zero(DecimalUse::Amount));
    if buy {
        Ok(gross
            .checked_add(&fee, DecimalUse::Internal)?
            .checked_neg(DecimalUse::Internal)?)
    } else {
        Ok(gross.checked_add(
            &fee.checked_neg(DecimalUse::Internal)?,
            DecimalUse::Internal,
        )?)
    }
}

fn investment_dividend_cash(row: &ParsedRow) -> ApplicationResult<Decimal> {
    let gross = Decimal::parse(value(row, "gross_cash_amount"), DecimalUse::Amount)?;
    let tax = optional_decimal(value(row, "withholding_tax"))?
        .unwrap_or_else(|| Decimal::zero(DecimalUse::Amount));
    let fee = optional_decimal(value(row, "fee_amount"))?
        .unwrap_or_else(|| Decimal::zero(DecimalUse::Amount));
    Ok(gross
        .checked_add(
            &tax.checked_neg(DecimalUse::Internal)?,
            DecimalUse::Internal,
        )?
        .checked_add(
            &fee.checked_neg(DecimalUse::Internal)?,
            DecimalUse::Internal,
        )?)
}

fn add_source_value(
    balances: &mut BTreeMap<(String, String), Decimal>,
    account_ids: &BTreeMap<&str, &str>,
    currencies: &BTreeMap<&str, &str>,
    legacy_id: &str,
    amount: &str,
    negative: bool,
) {
    let Some((target_id, currency)) = account_ids.get(legacy_id).zip(currencies.get(legacy_id))
    else {
        return;
    };
    let Ok(mut value) = Decimal::parse(amount, DecimalUse::Amount) else {
        return;
    };
    if negative {
        let Ok(negated) = value.checked_neg(DecimalUse::Amount) else {
            return;
        };
        value = negated;
    }
    add_balance(
        balances,
        ((*target_id).to_owned(), (*currency).to_owned()),
        &value,
    );
}

fn add_balance(
    balances: &mut BTreeMap<(String, String), Decimal>,
    key: (String, String),
    amount: &Decimal,
) {
    let current = balances
        .remove(&key)
        .unwrap_or_else(|| Decimal::zero(DecimalUse::Amount));
    if let Ok(total) = current.checked_add(amount, DecimalUse::Amount) {
        balances.insert(key, total.normalized());
    }
}

fn save_staging(
    candidate: &mut LedgerStore,
    parsed: &ParsedWorkbook,
    analysis: &ImportAnalysis,
) -> ApplicationResult<()> {
    let transaction = candidate
        .connection
        .transaction()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let analysis_json =
        serde_json::to_string(analysis).map_err(|_| ApplicationError::TransactionFailed)?;
    transaction
        .execute(
            "INSERT INTO import_batches(import_batch_id,source_sha256,importer_version,source_schema_version,target_schema_version,status,created_at_utc,analysis_json)
             VALUES(?1,?2,?3,?4,?5,?6,CURRENT_TIMESTAMP,?7)",
            params![analysis.batch_id, analysis.source_sha256, IMPORTER_VERSION, analysis.template_version, SCHEMA_VERSION, analysis.status, analysis_json],
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    for row in &parsed.rows {
        let raw =
            serde_json::to_string(&row.raw).map_err(|_| ApplicationError::TransactionFailed)?;
        let target_id = analysis
            .mappings
            .iter()
            .find(|mapping| mapping.legacy_id == value(row, "legacy_id"))
            .map(|mapping| mapping.target_id.as_str());
        let normalized = serde_json::to_string(&json!({
            "fields": row.raw,
            "targetId": target_id,
            "targetSchemaVersion": SCHEMA_VERSION,
        }))
        .map_err(|_| ApplicationError::TransactionFailed)?;
        let formulas = serde_json::to_string(&row.formulas)
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let located_issues = analysis
            .issues
            .iter()
            .filter(|issue| issue.sheet == row.sheet && issue.row == row.row)
            .collect::<Vec<_>>();
        let errors = serde_json::to_string(&located_issues)
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let status = if located_issues
            .iter()
            .any(|issue| issue.severity == "blocker")
        {
            "error"
        } else if located_issues.is_empty() {
            "valid"
        } else {
            "warning"
        };
        transaction
            .execute(
                "INSERT INTO import_rows(import_row_id,import_batch_id,sheet_name,source_row_number,raw_values_json,normalized_values_json,formula_evidence_json,content_sha256,status,errors_json)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![UuidV7::new()?.to_string(), analysis.batch_id, row.sheet, row.row, raw, normalized, formulas, row.content_sha256, status, errors],
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
    }
    transaction
        .commit()
        .map_err(|_| ApplicationError::TransactionFailed)
}

fn load_rows(connection: &Connection, batch_id: &str) -> ApplicationResult<Vec<ParsedRow>> {
    let mut statement = connection
        .prepare(
            "SELECT sheet_name,source_row_number,raw_values_json,formula_evidence_json,content_sha256,errors_json
             FROM import_rows WHERE import_batch_id=?1 ORDER BY sheet_name,source_row_number",
        )
        .map_err(|_| ApplicationError::ImportBatchNotFound)?;
    statement
        .query_map([batch_id], |row| {
            let raw_json: String = row.get(2)?;
            let formula_json: String = row.get(3)?;
            let errors_json: String = row.get(5)?;
            Ok(ParsedRow {
                sheet: row.get(0)?,
                row: row.get(1)?,
                raw: serde_json::from_str(&raw_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        raw_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                formulas: serde_json::from_str(&formula_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        formula_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                content_sha256: row.get(4)?,
                issues: serde_json::from_str(&errors_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        errors_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
            })
        })
        .map_err(|_| ApplicationError::ImportBatchNotFound)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::ImportBatchNotFound)
}

fn load_analysis(connection: &Connection, batch_id: &str) -> ApplicationResult<ImportAnalysis> {
    let json: String = connection
        .query_row(
            "SELECT analysis_json FROM import_batches WHERE import_batch_id=?1",
            [batch_id],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::ImportBatchNotFound)?;
    serde_json::from_str(&json).map_err(|_| ApplicationError::ImportBatchNotFound)
}

fn query_analysis(
    connection: &Connection,
    source_sha256: &str,
) -> ApplicationResult<Option<ImportAnalysis>> {
    let json: Option<String> = connection
        .query_row(
            "SELECT analysis_json FROM import_batches
             WHERE source_sha256=?1 AND importer_version=?2 AND target_schema_version=?3
             ORDER BY created_at_utc LIMIT 1",
            params![source_sha256, IMPORTER_VERSION, SCHEMA_VERSION],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ApplicationError::StorageUnavailable)?;
    json.map(|value| serde_json::from_str(&value).map_err(|_| ApplicationError::StorageUnavailable))
        .transpose()
}

fn verify_balances(connection: &Connection, expected: &[ImportBalance]) -> ApplicationResult<()> {
    for balance in expected {
        let actual: Option<(String, String)> = connection
            .query_row(
                "SELECT balance,currency FROM cash_balance_projection WHERE account_id=?1",
                [&balance.account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::ImportReconciliationFailed)?;
        if actual
            .as_ref()
            .map(|value| (value.0.as_str(), value.1.as_str()))
            != Some((balance.proposed_balance.as_str(), balance.currency.as_str()))
        {
            return Err(ApplicationError::ImportReconciliationFailed);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)] // Verification mirrors every supported reconciliation scope explicitly.
fn verify_metrics(store: &LedgerStore, expected: &[ImportMetric]) -> ApplicationResult<()> {
    let connection = &store.connection;
    for metric in expected {
        let actual = if metric.scope == "holding" {
            let Some((portfolio_id, instrument_id)) = metric.entity_id.split_once(':') else {
                return Err(ApplicationError::ImportReconciliationFailed);
            };
            let as_of = LocalDate::parse(
                metric
                    .as_of_date
                    .as_deref()
                    .ok_or(ApplicationError::ImportReconciliationFailed)?,
            )?;
            let workspace = store.investment_workspace(&as_of)?;
            let holding = workspace
                .holdings
                .iter()
                .find(|item| {
                    item.portfolio_id == portfolio_id && item.instrument_id == instrument_id
                })
                .ok_or(ApplicationError::ImportReconciliationFailed)?;
            match metric.metric.as_str() {
                "quantity" => holding.quantity.clone(),
                "carrying_cost" => holding.carrying_cost.clone(),
                "realized_trade_pnl" => holding.realized_trade_pnl.clone(),
                "net_dividend" => holding.net_dividend.clone(),
                "independent_expense" => holding.independent_expense.clone(),
                _ => return Err(ApplicationError::ImportReconciliationFailed),
            }
        } else if metric.scope == "portfolio" && metric.metric == "independent_expense" {
            let as_of = LocalDate::parse(
                metric
                    .as_of_date
                    .as_deref()
                    .ok_or(ApplicationError::ImportReconciliationFailed)?,
            )?;
            store
                .investment_workspace(&as_of)?
                .portfolio_expenses
                .iter()
                .filter(|item| item.portfolio_id == metric.entity_id)
                .try_fold(Decimal::zero(DecimalUse::Internal), |current, item| {
                    current.checked_add(
                        &Decimal::parse(&item.amount, DecimalUse::Internal)?,
                        DecimalUse::Internal,
                    )
                })?
                .normalized()
                .as_str()
                .to_owned()
        } else if metric.scope == "valuation" && metric.metric == "valued_net_assets" {
            let as_of = LocalDate::parse(
                metric
                    .as_of_date
                    .as_deref()
                    .ok_or(ApplicationError::ImportReconciliationFailed)?,
            )?;
            store.overview(&as_of)?.valued_net_assets
        } else if metric.scope == "mapping" && metric.metric == "count" {
            let table = match metric.entity_id.as_str() {
                "institution" => "institutions",
                "account" => "cash_accounts",
                "category" => "categories",
                "portfolio" => "portfolios",
                "instrument" => "security_instruments",
                _ => return Err(ApplicationError::ImportReconciliationFailed),
            };
            connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|_| ApplicationError::ImportReconciliationFailed)?
                .to_string()
        } else if metric.scope == "events" && metric.metric == "count" {
            connection
                .query_row("SELECT COUNT(*) FROM business_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(|_| ApplicationError::ImportReconciliationFailed)?
                .to_string()
        } else if metric.scope == "currency" && metric.metric == "count" {
            connection
                .query_row(
                    "SELECT COUNT(*) FROM (SELECT currency AS value FROM cash_accounts UNION SELECT trade_currency FROM security_instruments)",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|_| ApplicationError::ImportReconciliationFailed)?
                .to_string()
        } else {
            return Err(ApplicationError::ImportReconciliationFailed);
        };
        if Decimal::parse(&actual, DecimalUse::Internal)?
            .normalized()
            .as_str()
            != Decimal::parse(&metric.proposed_value, DecimalUse::Internal)?
                .normalized()
                .as_str()
        {
            return Err(ApplicationError::ImportReconciliationFailed);
        }
    }
    Ok(())
}

fn apply_import_account_states(
    transaction: &rusqlite::Transaction<'_>,
    parsed: &ParsedWorkbook,
    mappings: &[ImportMapping],
) -> ApplicationResult<()> {
    let lookup = mappings
        .iter()
        .filter(|item| item.entity_type == "account")
        .map(|item| (item.legacy_id.as_str(), item.target_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    for row in parsed.rows.iter().filter(|row| row.sheet == "资金子账户") {
        let enabled = if value(row, "enabled").is_empty() {
            true
        } else {
            parse_bool(value(row, "enabled"))?
        };
        if let Some(account_id) = lookup.get(value(row, "legacy_id")) {
            transaction
                .execute(
                    "UPDATE cash_accounts SET enabled=?1,updated_at_utc=CURRENT_TIMESTAMP WHERE account_id=?2",
                    params![enabled, account_id],
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
        }
    }
    Ok(())
}

fn reconciliation_hash(
    events: &[ImportProposedEvent],
    balances: &[ImportBalance],
    metrics: &[ImportMetric],
    differences: &[ImportDifference],
) -> ApplicationResult<String> {
    let bytes = serde_json::to_vec(&(events, balances, metrics, differences))
        .map_err(|_| ApplicationError::TransactionFailed)?;
    Ok(sha256(&bytes))
}

fn validate_settings(parsed: &ParsedWorkbook, issues: &mut Vec<ImportIssue>) {
    if Currency::parse(&parsed.base_currency).is_err() {
        issues.push(setting_issue("base_currency"));
    }
    if UiLocale::parse(&parsed.ui_locale).is_none() {
        issues.push(setting_issue("ui_locale"));
    }
}

fn setting_issue(field: &str) -> ImportIssue {
    ImportIssue {
        code: "IMPORT_FIELD_INVALID".to_owned(),
        severity: "blocker".to_owned(),
        sheet: "设置".to_owned(),
        row: 2,
        field: field.to_owned(),
    }
}

fn row_issue(code: &str, row: &ParsedRow, field: &str) -> ImportIssue {
    ImportIssue {
        code: code.to_owned(),
        severity: "blocker".to_owned(),
        sheet: row.sheet.clone(),
        row: row.row,
        field: field.to_owned(),
    }
}

fn mapped(
    ids: &BTreeMap<(&'static str, String), UuidV7>,
    entity_type: &'static str,
    legacy_id: &str,
) -> ApplicationResult<UuidV7> {
    ids.get(&(entity_type, legacy_id.to_owned()))
        .copied()
        .ok_or(ApplicationError::CatalogReferenceInvalid)
}

fn target(
    lookup: &BTreeMap<(&str, &str), &str>,
    entity_type: &str,
    legacy_id: &str,
) -> ApplicationResult<UuidV7> {
    lookup
        .get(&(entity_type, legacy_id))
        .ok_or(ApplicationError::CatalogReferenceInvalid)
        .and_then(|id| UuidV7::parse(id).map_err(Into::into))
}

fn optional_target(
    lookup: &BTreeMap<(&str, &str), &str>,
    entity_type: &str,
    legacy_id: &str,
) -> ApplicationResult<Option<UuidV7>> {
    if legacy_id.is_empty() {
        Ok(None)
    } else {
        target(lookup, entity_type, legacy_id).map(Some)
    }
}

fn optional_text(value: &str) -> ApplicationResult<Option<CatalogText>> {
    if value.is_empty() {
        Ok(None)
    } else {
        CatalogText::parse(value).map(Some).map_err(Into::into)
    }
}

fn optional_date(value: &str) -> ApplicationResult<Option<LocalDate>> {
    if value.is_empty() {
        Ok(None)
    } else {
        LocalDate::parse(value).map(Some).map_err(Into::into)
    }
}

fn optional_decimal(value: &str) -> ApplicationResult<Option<Decimal>> {
    if value.is_empty() {
        Ok(None)
    } else {
        Decimal::parse(value, DecimalUse::Amount)
            .map(Some)
            .map_err(Into::into)
    }
}

fn optional_decimal_use(value: &str, usage: DecimalUse) -> ApplicationResult<Option<Decimal>> {
    if value.is_empty() {
        Ok(None)
    } else {
        Decimal::parse(value, usage).map(Some).map_err(Into::into)
    }
}

fn optional_fee_scope(value: &str) -> ApplicationResult<Option<FeeScope>> {
    match value {
        "" => Ok(None),
        "instrument" => Ok(Some(FeeScope::Instrument)),
        "portfolio" => Ok(Some(FeeScope::Portfolio)),
        _ => Err(ApplicationError::ImportFileInvalid),
    }
}

fn parse_fx_overrides(
    row: &ParsedRow,
    fields: &[(&str, &str, &str)],
) -> ApplicationResult<Vec<FxOverrideInput>> {
    let mut overrides = Vec::new();
    let mut currencies = BTreeSet::new();
    for (currency_field, value_field, reason_field) in fields {
        let currency = value(row, currency_field);
        let override_value = value(row, value_field);
        let reason = value(row, reason_field);
        if currency.is_empty() && override_value.is_empty() && reason.is_empty() {
            continue;
        }
        if currency.is_empty() || override_value.is_empty() || reason.trim().is_empty() {
            return Err(ApplicationError::ImportFileInvalid);
        }
        let currency = Currency::parse(currency)?;
        if !currencies.insert(currency) {
            return Err(ApplicationError::ImportFileInvalid);
        }
        overrides.push(FxOverrideInput {
            currency,
            value: Decimal::parse(override_value, DecimalUse::FxRate)?,
            reason: reason.to_owned(),
        });
    }
    Ok(overrides)
}

fn optional_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_bool(value: &str) -> ApplicationResult<bool> {
    match value {
        "1" | "true" | "TRUE" => Ok(true),
        "0" | "false" | "FALSE" => Ok(false),
        _ => Err(ApplicationError::ImportFileInvalid),
    }
}

fn parse_u32(value: &str) -> ApplicationResult<u32> {
    value
        .parse()
        .map_err(|_| ApplicationError::ImportFileInvalid)
}

fn parse_u64(value: &str) -> ApplicationResult<u64> {
    value
        .parse()
        .map_err(|_| ApplicationError::ImportFileInvalid)
}

fn count_severity(issues: &[ImportIssue], severity: &str) -> u32 {
    u32::try_from(
        issues
            .iter()
            .filter(|issue| issue.severity == severity)
            .count(),
    )
    .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use tempfile::tempdir;

    use crate::application::cash::{ActivityQuery, CashPort};
    use crate::application::investment::InvestmentPort;
    use crate::application::valuation::ValuationPort;

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/sanitized/m3")
            .join(name)
    }

    fn m5_fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/sanitized/m5")
            .join(name)
    }

    fn parsed_row(fields: &[(&str, &str)]) -> ParsedRow {
        ParsedRow {
            sheet: "测试".to_owned(),
            row: 2,
            raw: fields
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
            formulas: Vec::new(),
            content_sha256: "sha256:test".to_owned(),
            issues: Vec::new(),
        }
    }

    #[test]
    fn fx_override_triplets_fail_closed_when_incomplete_or_duplicated() {
        let fields = &[("currency", "value", "reason")];
        let incomplete = parsed_row(&[("currency", "USD"), ("value", "7")]);
        assert_eq!(
            parse_fx_overrides(&incomplete, fields),
            Err(ApplicationError::ImportFileInvalid)
        );

        let duplicate = parsed_row(&[
            ("currency", "USD"),
            ("value", "7"),
            ("reason", "Reviewed source override"),
            ("fee_currency", "USD"),
            ("fee_value", "7.1"),
            ("fee_reason", "Reviewed fee override"),
        ]);
        assert_eq!(
            parse_fx_overrides(
                &duplicate,
                &[
                    ("currency", "value", "reason"),
                    ("fee_currency", "fee_value", "fee_reason"),
                ],
            ),
            Err(ApplicationError::ImportFileInvalid)
        );
    }

    #[test]
    fn valid_cash_fixture_is_staged_idempotently_then_atomically_switched() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let started = Instant::now();
        let first = manager
            .analyze_import(&fixture("cash-import-valid.xlsx"))
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(first.can_commit, "issues: {:?}", first.issues);
        assert!(!first.proposed_events.is_empty());
        let second = manager
            .analyze_import(&fixture("cash-import-valid.xlsx"))
            .unwrap();
        assert_eq!(first.batch_id, second.batch_id);
        assert!(second.reused_staging);
        let committed = manager.commit_import(&first.batch_id, true).unwrap();
        assert_eq!(committed.status, "committed");
        assert!(directory.path().join("ledger.sqlite3").is_file());
        let repeated = manager.commit_import(&first.batch_id, true).unwrap();
        assert!(repeated.already_committed);
        assert_eq!(
            committed.canonical_posting_sha256,
            repeated.canonical_posting_sha256
        );
    }

    #[test]
    fn invalid_fixture_reports_located_blockers_and_cannot_commit() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let analysis = manager
            .analyze_import(&fixture("cash-import-invalid.xlsx"))
            .unwrap();
        assert!(!analysis.can_commit);
        assert!(
            analysis
                .issues
                .iter()
                .all(|issue| !issue.sheet.is_empty() && issue.row > 0)
        );
        for code in [
            "IMPORT_DUPLICATE_ID",
            "IMPORT_REFERENCE_INVALID",
            "IMPORT_CATEGORY_DIRECTION_MISMATCH",
            "IMPORT_FORMULA_CACHE_MISSING",
            "IMPORT_MISSING_FX",
        ] {
            assert!(
                analysis.issues.iter().any(|issue| issue.code == code),
                "missing {code}: {:?}",
                analysis.issues
            );
        }
        assert_eq!(
            manager.commit_import(&analysis.batch_id, true),
            Err(ApplicationError::ImportBlockersPresent)
        );
        assert!(!directory.path().join("ledger.sqlite3").exists());
    }

    #[test]
    fn modified_file_is_a_new_candidate_and_never_merges_into_posted_ledger() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let original = manager
            .analyze_import(&fixture("cash-import-valid.xlsx"))
            .unwrap();
        manager.commit_import(&original.batch_id, true).unwrap();
        let live_hash = manager
            .store
            .as_ref()
            .unwrap()
            .canonical_posting_hash()
            .unwrap();
        let modified = manager
            .analyze_import(&fixture("cash-import-modified.xlsx"))
            .unwrap();
        assert_ne!(original.source_sha256, modified.source_sha256);
        assert_ne!(original.batch_id, modified.batch_id);
        assert_eq!(
            manager.commit_import(&modified.batch_id, true),
            Err(ApplicationError::ImportModifiedMergeForbidden)
        );
        assert_eq!(
            manager
                .store
                .as_ref()
                .unwrap()
                .canonical_posting_hash()
                .unwrap(),
            live_hash
        );
    }

    #[test]
    fn failure_before_switch_preserves_the_absence_of_a_live_ledger() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let analysis = manager
            .analyze_import(&fixture("cash-import-valid.xlsx"))
            .unwrap();
        assert_eq!(
            manager.commit_import_with_failpoint(
                &analysis.batch_id,
                true,
                ImportFailpoint::BeforeSwitch,
            ),
            Err(ApplicationError::ImportCandidateSwitchFailed)
        );
        assert!(!directory.path().join("ledger.sqlite3").exists());
    }

    #[test]
    fn confirmation_and_batch_authorization_fail_closed() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let batch = UuidV7::new().unwrap().to_string();
        assert_eq!(
            manager.commit_import(&batch, false),
            Err(ApplicationError::ImportConfirmationRequired)
        );
        assert_eq!(
            manager.commit_import(&batch, true),
            Err(ApplicationError::ImportBatchNotFound)
        );
    }

    #[test]
    fn full_history_fixture_rebuilds_cash_holdings_and_expense_bridge() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let analysis = manager
            .analyze_import(&m5_fixture("full-import-history.xlsx"))
            .unwrap();
        assert!(
            analysis.can_commit,
            "issues: {:?}; metrics: {:?}; differences: {:?}; balances: {:?}",
            analysis.issues,
            analysis.reconciliation.metrics,
            analysis.reconciliation.difference_items,
            analysis.reconciliation.balances
        );
        assert_eq!(analysis.proposed_events.len(), 5);
        assert!(
            analysis
                .reconciliation
                .metrics
                .iter()
                .all(|item| item.difference == "0")
        );
        assert!(
            analysis
                .reconciliation
                .difference_items
                .iter()
                .all(|item| item.difference == "0")
        );
        manager.commit_import(&analysis.batch_id, true).unwrap();
        let overview = manager
            .get_overview(&LocalDate::parse("2026-03-15").unwrap())
            .unwrap();
        assert_eq!(overview.valued_net_assets, "7140");
        assert_eq!(overview.mtd_expense, "35");
        assert!(overview.unvalued_assets.is_empty());
        let activity = manager
            .get_activity(&ActivityQuery {
                start_date: LocalDate::parse("2026-01-01").unwrap(),
                end_date: LocalDate::parse("2026-03-15").unwrap(),
                context: None,
                event_type: None,
                account_id: None,
                category_id: None,
                search: None,
                cursor: None,
                limit: 10,
            })
            .unwrap();
        assert!(activity.items.iter().any(|item| {
            item.fx_resolutions.iter().any(|resolution| {
                resolution.override_value.as_deref() == Some("7")
                    && resolution.override_reason.as_deref() == Some("Synthetic migration override")
                    && resolution.final_rate == "7"
            })
        }));
    }

    #[test]
    fn cutover_fixture_excludes_pre_and_on_date_rows_and_preserves_zero_history() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let analysis = manager
            .analyze_import(&m5_fixture("full-import-cutover.xlsx"))
            .unwrap();
        assert!(
            analysis.can_commit,
            "issues: {:?}; metrics: {:?}; differences: {:?}; balances: {:?}",
            analysis.issues,
            analysis.reconciliation.metrics,
            analysis.reconciliation.difference_items,
            analysis.reconciliation.balances
        );
        assert_eq!(analysis.proposed_events.len(), 6);
        assert!(
            analysis
                .proposed_events
                .iter()
                .any(|item| item.event_type == "OpeningPosition")
        );
        assert!(
            !analysis
                .proposed_events
                .iter()
                .any(|item| item.effective_date.as_str() < "2026-01-01")
        );
        manager.commit_import(&analysis.batch_id, true).unwrap();
        let workspace = manager
            .get_investment_workspace(&LocalDate::parse("2026-03-15").unwrap())
            .unwrap();
        assert_eq!(workspace.holdings[0].quantity, "2");
        assert_eq!(workspace.holdings[0].realized_trade_pnl, "25");
        assert_eq!(workspace.portfolio_expenses[0].amount, "3");
    }

    #[test]
    fn missing_full_migration_policy_and_unbalanced_checks_block_switch() {
        let directory = tempdir().unwrap();
        let mut manager = SqliteLedgerManager::new(directory.path()).unwrap();
        let analysis = manager
            .analyze_import(&m5_fixture("full-import-invalid.xlsx"))
            .unwrap();
        assert!(!analysis.can_commit);
        assert!(analysis.blocker_count > 0);
        assert_eq!(
            manager.commit_import(&analysis.batch_id, true),
            Err(ApplicationError::ImportBlockersPresent)
        );
        assert!(!directory.path().join("ledger.sqlite3").exists());
    }
}
