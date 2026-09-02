#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde_json::json;

use crate::application::cash::{CashEventInput, EventInputType};
use crate::application::catalog::{CashAccount, Category, FxRateRevision, Institution};
use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::import::{
    IMPORTER_VERSION, ImportAnalysis, ImportBalance, ImportCommitResult, ImportIssue,
    ImportMapping, ImportPort, ImportPosting, ImportProposedEvent, ImportReconciliation,
};
use crate::application::ledger::MigrationBackupPort;
use crate::domain::catalog::{BusinessId, CatalogText, CategoryKind, SemanticRole, SortOrder};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::settings::UiLocale;
use crate::domain::types::{Currency, LocalDate, Sequence, UuidV7};
use crate::infrastructure::excel::{ParsedRow, ParsedWorkbook, parse_workbook, sha256, value};

use super::SqliteLedgerManager;
use super::cash_store::{insert_prepared_event, rebuild_cash_derived};
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
        let canonical_result_sha256 = reconciliation_hash(&plan.proposed_events, &plan.balances)?;
        let balanced = plan
            .balances
            .iter()
            .all(|balance| balance.difference == "0");
        let row_count =
            u32::try_from(parsed.rows.len()).map_err(|_| ApplicationError::ImportFileTooLarge)?;
        let invalid_rows = plan
            .issues
            .iter()
            .filter(|issue| issue.severity == "blocker" && issue.row > 0)
            .map(|issue| (&issue.sheet, issue.row))
            .collect::<std::collections::BTreeSet<_>>()
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
                difference_bridge: vec![
                    "opening + income - expense + adjustment + transfers + exchanges - fees"
                        .to_owned(),
                    "derived/status/display formulas are evidence-only".to_owned(),
                ],
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
        let events = build_event_inputs(&parsed, &analysis.mappings, &mut Vec::new());
        let prepared = events
            .iter()
            .map(|(_, _, input)| candidate.prepare_write(input))
            .collect::<ApplicationResult<Vec<_>>>()?;
        let transaction = candidate
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let mut watermark = 0;
        for ((_, _, input), prepared_event) in events.iter().zip(&prepared) {
            let event_id = UuidV7::new()?;
            watermark = insert_prepared_event(
                &transaction,
                event_id,
                input,
                prepared_event,
                None,
                1,
                Some("initial-xlsx-import"),
            )?;
            transaction
                .execute(
                    "UPDATE business_events SET import_batch_id=?1 WHERE event_id=?2",
                    params![batch_id, event_id.to_string()],
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
        }
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
    let events = build_event_inputs(parsed, &mappings, &mut issues);
    let mut proposed_events = Vec::new();
    for (sheet, row, input) in &events {
        match candidate.prepare_write(input) {
            Ok(prepared) => {
                if prepared
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
                    event_type: prepared.preview.event_type.to_owned(),
                    effective_date: prepared.preview.effective_date,
                    sequence: prepared.preview.sequence,
                    postings: prepared
                        .preview
                        .postings
                        .into_iter()
                        .map(|posting| ImportPosting {
                            account_id: posting.account_id.unwrap_or_default(),
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
        issues,
    }
}

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
        if amount.is_empty() || amount == "0" {
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
                fx_overrides: Vec::new(),
                currency_precision_confirmed: true,
            })
        })();
        push_event_result(row, result, issues, &mut events);
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "资金调拨") {
        let result = transfer_input(row, &lookup);
        push_event_result(row, result, issues, &mut events);
    }
    for row in parsed.rows.iter().filter(|row| row.sheet == "换汇流水") {
        let result = exchange_input(row, &lookup);
        push_event_result(row, result, issues, &mut events);
    }
    events.sort_by(|left, right| {
        (left.2.effective_date.as_str(), left.2.sequence.get())
            .cmp(&(right.2.effective_date.as_str(), right.2.sequence.get()))
    });
    events
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
        fx_overrides: Vec::new(),
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
        fx_overrides: Vec::new(),
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
    let keys = source
        .keys()
        .chain(proposed.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
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

fn reconciliation_hash(
    events: &[ImportProposedEvent],
    balances: &[ImportBalance],
) -> ApplicationResult<String> {
    let bytes =
        serde_json::to_vec(&(events, balances)).map_err(|_| ApplicationError::TransactionFailed)?;
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

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/sanitized/m3")
            .join(name)
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
}
