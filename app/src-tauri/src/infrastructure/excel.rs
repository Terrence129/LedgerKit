#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::import::ImportIssue;

pub(super) const TEMPLATE_VERSION: &str = "ledgerkit-workbook-v1.4";
const MAX_SOURCE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ROWS: usize = 20_000;
const MAX_COLUMNS: usize = 32;
const MAX_CELL_CHARS: usize = 4_096;

const CASH_SHEETS: &[SheetContract] = &[
    SheetContract::new("设置", &["template_version", "base_currency", "ui_locale"]),
    SheetContract::new(
        "机构",
        &["legacy_id", "name", "region", "institution_type", "enabled"],
    ),
    SheetContract::new(
        "资金子账户",
        &[
            "legacy_id",
            "institution_legacy_id",
            "name",
            "purpose",
            "currency",
            "opened_on",
            "opening_balance",
            "cutover_date",
            "migration_policy",
            "enabled",
        ],
    ),
    SheetContract::new(
        "分类",
        &[
            "legacy_id",
            "name",
            "kind",
            "semantic_role",
            "sort_order",
            "enabled",
        ],
    ),
    SheetContract::new(
        "汇率",
        &["rate_date", "currency", "rate_to_base", "source", "active"],
    ),
    SheetContract::new(
        "收支流水",
        &[
            "date",
            "sequence",
            "type",
            "account_legacy_id",
            "category_legacy_id",
            "amount",
            "merchant",
            "note",
            "semantic_role",
            "fee_account_legacy_id",
            "fee_amount",
            "derived_base_value",
            "status",
            "display_label",
            "fx_override_currency",
            "fx_override_value",
            "fx_override_reason",
            "fee_fx_override_currency",
            "fee_fx_override_value",
            "fee_fx_override_reason",
        ],
    ),
    SheetContract::new(
        "资金调拨",
        &[
            "date",
            "sequence",
            "from_account_legacy_id",
            "to_account_legacy_id",
            "amount",
            "note",
            "fx_override_currency",
            "fx_override_value",
            "fx_override_reason",
        ],
    ),
    SheetContract::new(
        "换汇流水",
        &[
            "date",
            "sequence",
            "from_account_legacy_id",
            "to_account_legacy_id",
            "from_amount",
            "to_amount",
            "fee_account_legacy_id",
            "fee_amount",
            "note",
            "from_fx_override_currency",
            "from_fx_override_value",
            "from_fx_override_reason",
            "to_fx_override_currency",
            "to_fx_override_value",
            "to_fx_override_reason",
            "fee_fx_override_currency",
            "fee_fx_override_value",
            "fee_fx_override_reason",
        ],
    ),
];

const INVESTMENT_SHEETS: &[SheetContract] = &[
    SheetContract::new(
        "投资组合",
        &[
            "legacy_id",
            "institution_legacy_id",
            "settlement_account_legacy_id",
            "name",
            "portfolio_type",
            "enabled",
            "migration_policy",
            "cutover_date",
        ],
    ),
    SheetContract::new(
        "证券",
        &["legacy_id", "code", "name", "trade_currency", "enabled"],
    ),
    SheetContract::new(
        "证券价格",
        &[
            "instrument_legacy_id",
            "price_date",
            "price",
            "price_currency",
            "source",
            "active",
        ],
    ),
    SheetContract::new(
        "投资流水",
        &[
            "date",
            "sequence",
            "type",
            "portfolio_legacy_id",
            "instrument_legacy_id",
            "settlement_account_legacy_id",
            "quantity",
            "unit_price",
            "trade_fee",
            "gross_cash_amount",
            "withholding_tax",
            "fee_amount",
            "amount",
            "fee_scope",
            "settlement_override_reason",
            "fx_override_currency",
            "fx_override_value",
            "fx_override_reason",
        ],
    ),
    SheetContract::new(
        "持仓基线",
        &[
            "portfolio_legacy_id",
            "instrument_legacy_id",
            "quantity",
            "carrying_cost",
            "realized_trade_pnl",
            "net_dividend",
            "independent_expense",
            "currency",
            "as_of_date",
        ],
    ),
    SheetContract::new(
        "检查",
        &["scope", "legacy_id", "metric", "source_value", "as_of_date"],
    ),
    SheetContract::new(
        "支出分析",
        &[
            "start_date",
            "end_date",
            "bucket_id",
            "source_amount",
            "source_count",
            "explanation",
        ],
    ),
];

struct SheetContract {
    name: &'static str,
    headers: &'static [&'static str],
}

impl SheetContract {
    const fn new(name: &'static str, headers: &'static [&'static str]) -> Self {
        Self { name, headers }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct FormulaEvidence {
    pub column: String,
    pub role: String,
    pub formula: String,
    pub cached_value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct ParsedRow {
    pub sheet: String,
    pub row: u32,
    pub raw: BTreeMap<String, String>,
    pub formulas: Vec<FormulaEvidence>,
    pub content_sha256: String,
    pub issues: Vec<ImportIssue>,
}

#[derive(Debug)]
pub(super) struct ParsedWorkbook {
    pub source_sha256: String,
    pub template_version: String,
    pub base_currency: String,
    pub ui_locale: String,
    pub rows: Vec<ParsedRow>,
    pub issues: Vec<ImportIssue>,
}

#[allow(clippy::too_many_lines)] // One pass keeps cached values aligned with formula coordinates.
pub(super) fn parse_workbook(path: &Path) -> ApplicationResult<ParsedWorkbook> {
    if path.extension().and_then(|value| value.to_str()) != Some("xlsx") {
        return Err(ApplicationError::ImportTemplateUnsupported);
    }
    let metadata = fs::metadata(path).map_err(|_| ApplicationError::ImportFileInvalid)?;
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err(ApplicationError::ImportFileTooLarge);
    }
    let bytes = fs::read(path).map_err(|_| ApplicationError::ImportFileInvalid)?;
    if contains_ascii(&bytes, b"vbaProject.bin") || contains_ascii(&bytes, b"externalLinks/") {
        return Err(ApplicationError::ImportTemplateUnsupported);
    }
    let source_sha256 = sha256(&bytes);
    let mut workbook = open_workbook_auto(path).map_err(|_| ApplicationError::ImportFileInvalid)?;
    let names = workbook.sheet_names();
    let full_contract = names.len() == CASH_SHEETS.len() + INVESTMENT_SHEETS.len()
        && INVESTMENT_SHEETS
            .iter()
            .all(|contract| names.iter().any(|name| name == contract.name));
    let cash_contract = names.len() == CASH_SHEETS.len();
    if (!cash_contract && !full_contract)
        || CASH_SHEETS
            .iter()
            .any(|contract| !names.iter().any(|name| name == contract.name))
    {
        return Err(ApplicationError::ImportTemplateUnsupported);
    }

    let mut rows = Vec::new();
    let mut issues = Vec::new();
    for contract in CASH_SHEETS.iter().chain(
        full_contract
            .then_some(INVESTMENT_SHEETS)
            .into_iter()
            .flatten(),
    ) {
        let range = workbook
            .worksheet_range(contract.name)
            .map_err(|_| ApplicationError::ImportFileInvalid)?;
        let formulas = workbook
            .worksheet_formula(contract.name)
            .map_err(|_| ApplicationError::ImportFileInvalid)?;
        if range.width() > MAX_COLUMNS || rows.len().saturating_add(range.height()) > MAX_ROWS {
            return Err(ApplicationError::ImportFileTooLarge);
        }
        let mut data_rows = range.rows();
        let Some(header_cells) = data_rows.next() else {
            return Err(ApplicationError::ImportTemplateUnsupported);
        };
        let headers = header_cells
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if headers != contract.headers {
            return Err(ApplicationError::ImportTemplateUnsupported);
        }
        let start = range.start().unwrap_or((0, 0));
        for (offset, cells) in data_rows.enumerate() {
            if cells.iter().all(|cell| matches!(cell, Data::Empty)) {
                continue;
            }
            let excel_row =
                u32::try_from(offset + 2).map_err(|_| ApplicationError::ImportFileTooLarge)?;
            let absolute_row = start.0.saturating_add(excel_row - 1);
            let mut raw = BTreeMap::new();
            let mut row_issues = Vec::new();
            let mut formula_evidence = Vec::new();
            for (column, header) in contract.headers.iter().enumerate() {
                let value = cells.get(column).unwrap_or(&Data::Empty);
                let text = cell_text(value);
                if text.chars().count() > MAX_CELL_CHARS {
                    return Err(ApplicationError::ImportFileTooLarge);
                }
                raw.insert((*header).to_owned(), text.clone());
                if matches!(value, Data::Error(_)) {
                    row_issues.push(issue(
                        "IMPORT_CELL_ERROR",
                        "blocker",
                        contract.name,
                        excel_row,
                        header,
                    ));
                }
                let absolute_column = start.1.saturating_add(
                    u32::try_from(column).map_err(|_| ApplicationError::ImportFileTooLarge)?,
                );
                if let Some(formula) = formulas.get_value((absolute_row, absolute_column))
                    && !formula.is_empty()
                {
                    let role = column_role(header);
                    formula_evidence.push(FormulaEvidence {
                        column: (*header).to_owned(),
                        role: role.to_owned(),
                        formula: formula.clone(),
                        cached_value: text.clone(),
                    });
                    if role == "formula-input"
                        && (text.is_empty() || matches!(value, Data::Error(_)))
                    {
                        row_issues.push(issue(
                            "IMPORT_FORMULA_CACHE_MISSING",
                            "blocker",
                            contract.name,
                            excel_row,
                            header,
                        ));
                    }
                }
            }
            let canonical =
                serde_json::to_vec(&raw).map_err(|_| ApplicationError::ImportFileInvalid)?;
            rows.push(ParsedRow {
                sheet: contract.name.to_owned(),
                row: excel_row,
                raw,
                formulas: formula_evidence,
                content_sha256: sha256(&canonical),
                issues: row_issues,
            });
        }
    }

    let settings = rows
        .iter()
        .find(|row| row.sheet == "设置" && row.row == 2)
        .ok_or(ApplicationError::ImportTemplateUnsupported)?;
    let template_version = value(settings, "template_version").to_owned();
    if template_version != TEMPLATE_VERSION {
        return Err(ApplicationError::ImportTemplateUnsupported);
    }
    let base_currency = value(settings, "base_currency").to_owned();
    let ui_locale = value(settings, "ui_locale").to_owned();
    validate_duplicates_and_references(&rows, &mut issues);
    issues.extend(rows.iter().flat_map(|row| row.issues.iter().cloned()));
    issues.sort_by(|left, right| {
        (&left.sheet, left.row, &left.field, &left.code).cmp(&(
            &right.sheet,
            right.row,
            &right.field,
            &right.code,
        ))
    });
    Ok(ParsedWorkbook {
        source_sha256,
        template_version,
        base_currency,
        ui_locale,
        rows,
        issues,
    })
}

fn validate_duplicates_and_references(rows: &[ParsedRow], issues: &mut Vec<ImportIssue>) {
    let institutions = ids(rows, "机构");
    let accounts = ids(rows, "资金子账户");
    let categories = ids(rows, "分类");
    let portfolios = ids(rows, "投资组合");
    let instruments = ids(rows, "证券");
    flag_duplicate_ids(rows, "机构", issues);
    flag_duplicate_ids(rows, "资金子账户", issues);
    flag_duplicate_ids(rows, "分类", issues);
    flag_duplicate_ids(rows, "投资组合", issues);
    flag_duplicate_ids(rows, "证券", issues);
    let mut events = BTreeSet::new();
    for row in rows {
        match row.sheet.as_str() {
            "资金子账户" => {
                require_reference(row, "institution_legacy_id", &institutions, issues);
            }
            "收支流水" => {
                require_reference(row, "account_legacy_id", &accounts, issues);
                let category = value(row, "category_legacy_id");
                if !category.is_empty() {
                    require_reference(row, "category_legacy_id", &categories, issues);
                    if let Some(category_row) = rows.iter().find(|candidate| {
                        candidate.sheet == "分类" && value(candidate, "legacy_id") == category
                    }) {
                        let event_type = value(row, "type");
                        let kind = value(category_row, "kind");
                        if (event_type == "Income" && kind != "income")
                            || (event_type == "Expense" && kind != "expense")
                        {
                            issues.push(issue(
                                "IMPORT_CATEGORY_DIRECTION_MISMATCH",
                                "blocker",
                                &row.sheet,
                                row.row,
                                "category_legacy_id",
                            ));
                        }
                    }
                }
                optional_reference(row, "fee_account_legacy_id", &accounts, issues);
                flag_event_duplicate(row, &mut events, issues);
            }
            "资金调拨" | "换汇流水" => {
                require_reference(row, "from_account_legacy_id", &accounts, issues);
                require_reference(row, "to_account_legacy_id", &accounts, issues);
                optional_reference(row, "fee_account_legacy_id", &accounts, issues);
                flag_event_duplicate(row, &mut events, issues);
            }
            "投资组合" => {
                require_reference(row, "institution_legacy_id", &institutions, issues);
                require_reference(row, "settlement_account_legacy_id", &accounts, issues);
            }
            "证券价格" => {
                require_reference(row, "instrument_legacy_id", &instruments, issues);
            }
            "投资流水" => {
                require_reference(row, "portfolio_legacy_id", &portfolios, issues);
                optional_reference(row, "instrument_legacy_id", &instruments, issues);
                require_reference(row, "settlement_account_legacy_id", &accounts, issues);
                flag_event_duplicate(row, &mut events, issues);
            }
            "持仓基线" => {
                require_reference(row, "portfolio_legacy_id", &portfolios, issues);
                optional_reference(row, "instrument_legacy_id", &instruments, issues);
            }
            _ => {}
        }
    }
}

fn ids(rows: &[ParsedRow], sheet: &str) -> BTreeSet<String> {
    rows.iter()
        .filter(|row| row.sheet == sheet)
        .map(|row| value(row, "legacy_id").to_owned())
        .filter(|value| !value.is_empty())
        .collect()
}

fn flag_duplicate_ids(rows: &[ParsedRow], sheet: &str, issues: &mut Vec<ImportIssue>) {
    let mut seen = BTreeSet::new();
    for row in rows.iter().filter(|row| row.sheet == sheet) {
        let id = value(row, "legacy_id");
        if id.is_empty() || !seen.insert(id.to_owned()) {
            issues.push(issue(
                "IMPORT_DUPLICATE_ID",
                "blocker",
                sheet,
                row.row,
                "legacy_id",
            ));
        }
    }
}

fn flag_event_duplicate(
    row: &ParsedRow,
    seen: &mut BTreeSet<(String, String)>,
    issues: &mut Vec<ImportIssue>,
) {
    let key = (
        value(row, "date").to_owned(),
        value(row, "sequence").to_owned(),
    );
    if !seen.insert(key) {
        issues.push(issue(
            "IMPORT_DUPLICATE_EVENT",
            "blocker",
            &row.sheet,
            row.row,
            "sequence",
        ));
    }
}

fn require_reference(
    row: &ParsedRow,
    field: &str,
    known: &BTreeSet<String>,
    issues: &mut Vec<ImportIssue>,
) {
    let id = value(row, field);
    if id.is_empty() || !known.contains(id) {
        issues.push(issue(
            "IMPORT_REFERENCE_INVALID",
            "blocker",
            &row.sheet,
            row.row,
            field,
        ));
    }
}

fn optional_reference(
    row: &ParsedRow,
    field: &str,
    known: &BTreeSet<String>,
    issues: &mut Vec<ImportIssue>,
) {
    let id = value(row, field);
    if !id.is_empty() && !known.contains(id) {
        issues.push(issue(
            "IMPORT_REFERENCE_INVALID",
            "blocker",
            &row.sheet,
            row.row,
            field,
        ));
    }
}

pub(super) fn value<'a>(row: &'a ParsedRow, field: &str) -> &'a str {
    row.raw.get(field).map_or("", String::as_str)
}

fn column_role(header: &str) -> &'static str {
    match header {
        "derived_base_value" => "derived-formula",
        "status" => "status",
        "display_label" => "display",
        _ => "formula-input",
    }
}

fn cell_text(value: &Data) -> String {
    match value {
        Data::Empty => String::new(),
        _ => value.to_string(),
    }
}

fn issue(code: &str, severity: &str, sheet: &str, row: u32, field: &str) -> ImportIssue {
    ImportIssue {
        code: code.to_owned(),
        severity: severity.to_owned(),
        sheet: sheet.to_owned(),
        row,
        field: field.to_owned(),
    }
}

fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(71);
    output.push_str("sha256:");
    for byte in digest {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn financial_formula_roles_never_treat_derived_or_display_cells_as_inputs() {
        assert_eq!(column_role("amount"), "formula-input");
        assert_eq!(column_role("derived_base_value"), "derived-formula");
        assert_eq!(column_role("status"), "status");
        assert_eq!(column_role("display_label"), "display");
    }
}
