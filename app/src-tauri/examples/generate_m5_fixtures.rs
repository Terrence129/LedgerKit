#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use rust_xlsxwriter::{DocProperties, ExcelDateTime, Workbook, Worksheet, XlsxError};

const HEADERS: &[(&str, &[&str])] = &[
    ("设置", &["template_version", "base_currency", "ui_locale"]),
    (
        "机构",
        &["legacy_id", "name", "region", "institution_type", "enabled"],
    ),
    (
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
    (
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
    (
        "汇率",
        &["rate_date", "currency", "rate_to_base", "source", "active"],
    ),
    (
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
    (
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
    (
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
    (
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
    (
        "证券",
        &["legacy_id", "code", "name", "trade_currency", "enabled"],
    ),
    (
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
    (
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
    (
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
    (
        "检查",
        &["scope", "legacy_id", "metric", "source_value", "as_of_date"],
    ),
    (
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

#[derive(Clone, Copy)]
enum FixtureKind {
    FullHistory,
    Cutover,
    Invalid,
}

fn main() -> Result<(), XlsxError> {
    let root = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sanitized/m5"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&root).map_err(XlsxError::IoError)?;
    write_fixture(
        &root.join("full-import-history.xlsx"),
        FixtureKind::FullHistory,
    )?;
    write_fixture(&root.join("full-import-cutover.xlsx"), FixtureKind::Cutover)?;
    write_fixture(&root.join("full-import-invalid.xlsx"), FixtureKind::Invalid)
}

fn write_fixture(path: &Path, kind: FixtureKind) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();
    workbook.set_properties(
        &DocProperties::new()
            .set_title("LedgerKit deterministic synthetic full migration fixture")
            .set_creation_datetime(&ExcelDateTime::from_ymd(2026, 1, 1)?),
    );
    for (name, headers) in HEADERS {
        let sheet = workbook.add_worksheet();
        sheet.set_name(*name)?;
        write_row(sheet, 0, headers)?;
    }
    write_common(&mut workbook, kind)?;
    match kind {
        FixtureKind::FullHistory => write_full_history(&mut workbook)?,
        FixtureKind::Cutover => write_cutover(&mut workbook)?,
        FixtureKind::Invalid => write_invalid(&mut workbook)?,
    }
    workbook.save(path)
}

#[allow(clippy::too_many_lines)] // Shared fixture rows stay adjacent for auditability.
fn write_common(workbook: &mut Workbook, kind: FixtureKind) -> Result<(), XlsxError> {
    write_row(
        workbook.worksheet_from_name("设置")?,
        1,
        &["ledgerkit-workbook-v1.4", "CNY", "zh-CN"],
    )?;
    write_row(
        workbook.worksheet_from_name("机构")?,
        1,
        &[
            "inst-synthetic",
            "Synthetic Institution",
            "SG",
            "broker",
            "true",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("分类")?,
        1,
        &[
            "cat-income",
            "Synthetic Income",
            "income",
            "normal",
            "1",
            "true",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("分类")?,
        2,
        &[
            "cat-expense",
            "Synthetic Expense",
            "expense",
            "normal",
            "2",
            "true",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("汇率")?,
        1,
        &["2025-01-01", "USD", "7", "synthetic", "true"],
    )?;
    write_row(
        workbook.worksheet_from_name("证券")?,
        1,
        &[
            "instrument-alpha",
            "ALPHA",
            "Synthetic Alpha",
            "USD",
            "true",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("证券价格")?,
        1,
        &[
            "instrument-alpha",
            "2026-03-15",
            "12",
            "USD",
            "synthetic",
            "true",
        ],
    )?;
    let policy = match kind {
        FixtureKind::FullHistory => "full_history",
        FixtureKind::Cutover => "explicit_cutover",
        FixtureKind::Invalid => "",
    };
    let opening = if matches!(kind, FixtureKind::Cutover) {
        "900"
    } else {
        "0"
    };
    let cutover = if matches!(kind, FixtureKind::FullHistory) {
        ""
    } else {
        "2026-01-01"
    };
    write_row(
        workbook.worksheet_from_name("资金子账户")?,
        1,
        &[
            "acct-usd",
            "inst-synthetic",
            "Synthetic USD",
            "investment",
            "USD",
            "2025-01-01",
            opening,
            cutover,
            policy,
            "false",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("投资组合")?,
        1,
        &[
            "portfolio-alpha",
            "inst-synthetic",
            "acct-usd",
            "Synthetic Portfolio",
            "brokerage",
            "true",
            policy,
            cutover,
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_lines)] // One contiguous scenario makes the fixture contract reviewable.
fn write_full_history(workbook: &mut Workbook) -> Result<(), XlsxError> {
    write_cash(
        workbook,
        1,
        "2026-01-01",
        "1",
        "Income",
        "cat-income",
        "1000",
    )?;
    write_cash(
        workbook,
        2,
        "2026-03-05",
        "1",
        "Expense",
        "cat-expense",
        "5",
    )?;
    write_investment(
        workbook,
        1,
        &[
            "2026-01-02",
            "1",
            "SecurityBuy",
            "portfolio-alpha",
            "instrument-alpha",
            "acct-usd",
            "10",
            "10",
            "1",
            "",
            "",
            "",
            "",
            "",
            "",
            "USD",
            "7",
            "Synthetic migration override",
        ],
    )?;
    write_investment(
        workbook,
        2,
        &[
            "2026-02-01",
            "1",
            "Dividend",
            "portfolio-alpha",
            "instrument-alpha",
            "acct-usd",
            "",
            "",
            "",
            "10",
            "1",
            "1",
            "",
            "",
            "",
        ],
    )?;
    write_investment(
        workbook,
        3,
        &[
            "2026-02-02",
            "1",
            "InvestmentExpense",
            "portfolio-alpha",
            "instrument-alpha",
            "acct-usd",
            "",
            "",
            "",
            "",
            "",
            "",
            "2",
            "instrument",
            "",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("持仓基线")?,
        1,
        &[
            "portfolio-alpha",
            "instrument-alpha",
            "10",
            "101",
            "0",
            "8",
            "2",
            "USD",
            "2026-03-15",
        ],
    )?;
    write_checks(workbook, 5, "7140")?;
    write_row(
        workbook.worksheet_from_name("支出分析")?,
        1,
        &["2026-03-01", "2026-03-15", "all", "35", "1", ""],
    )
}

#[allow(clippy::too_many_lines)] // One contiguous scenario makes the cut-over boundary reviewable.
fn write_cutover(workbook: &mut Workbook) -> Result<(), XlsxError> {
    write_cash(
        workbook,
        1,
        "2026-01-01",
        "1",
        "Income",
        "cat-income",
        "100",
    )?;
    write_cash(
        workbook,
        2,
        "2026-03-05",
        "1",
        "Expense",
        "cat-expense",
        "5",
    )?;
    write_investment(
        workbook,
        1,
        &[
            "2025-12-30",
            "1",
            "SecurityBuy",
            "portfolio-alpha",
            "instrument-alpha",
            "acct-usd",
            "10",
            "10",
            "0",
            "",
            "",
            "",
            "",
            "",
            "",
        ],
    )?;
    write_investment(
        workbook,
        2,
        &[
            "2026-01-01",
            "2",
            "Dividend",
            "portfolio-alpha",
            "instrument-alpha",
            "acct-usd",
            "",
            "",
            "",
            "10",
            "0",
            "0",
            "",
            "",
            "",
        ],
    )?;
    write_investment(
        workbook,
        3,
        &[
            "2026-01-02",
            "1",
            "SecurityBuy",
            "portfolio-alpha",
            "instrument-alpha",
            "acct-usd",
            "2",
            "10",
            "0",
            "",
            "",
            "",
            "",
            "",
            "",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("持仓基线")?,
        1,
        &[
            "portfolio-alpha",
            "instrument-alpha",
            "0",
            "0",
            "25",
            "5",
            "2",
            "USD",
            "2026-01-01",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("持仓基线")?,
        2,
        &[
            "portfolio-alpha",
            "",
            "0",
            "0",
            "0",
            "0",
            "3",
            "USD",
            "2026-01-01",
        ],
    )?;
    write_checks(workbook, 6, "6293")?;
    write_row(
        workbook.worksheet_from_name("支出分析")?,
        1,
        &["2026-03-01", "2026-03-15", "all", "35", "1", ""],
    )
}

fn write_invalid(workbook: &mut Workbook) -> Result<(), XlsxError> {
    write_cash(workbook, 1, "2026-02-01", "1", "Income", "cat-income", "1")?;
    write_investment(
        workbook,
        1,
        &[
            "2026-02-02",
            "1",
            "SecurityBuy",
            "portfolio-alpha",
            "instrument-alpha",
            "acct-usd",
            "1",
            "10",
            "0",
            "",
            "",
            "",
            "",
            "",
            "",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("检查")?,
        1,
        &["mapping", "portfolio", "count", "2"],
    )
}

fn write_cash(
    workbook: &mut Workbook,
    row: u32,
    date: &str,
    sequence: &str,
    kind: &str,
    category: &str,
    amount: &str,
) -> Result<(), XlsxError> {
    write_row(
        workbook.worksheet_from_name("收支流水")?,
        row,
        &[
            date,
            sequence,
            kind,
            "acct-usd",
            category,
            amount,
            "Synthetic Merchant",
            "Synthetic row",
            "normal",
            "",
            "",
            "",
            "ok",
            "Synthetic",
        ],
    )
}

fn write_investment(workbook: &mut Workbook, row: u32, values: &[&str]) -> Result<(), XlsxError> {
    write_row(workbook.worksheet_from_name("投资流水")?, row, values)
}

fn write_checks(
    workbook: &mut Workbook,
    event_count: u64,
    valued_net_assets: &str,
) -> Result<(), XlsxError> {
    let sheet = workbook.worksheet_from_name("检查")?;
    for (offset, entity) in ["institution", "account", "portfolio", "instrument"]
        .iter()
        .enumerate()
    {
        write_row(
            sheet,
            u32::try_from(offset + 1).unwrap(),
            &["mapping", entity, "count", "1"],
        )?;
    }
    write_row(sheet, 5, &["mapping", "category", "count", "2"])?;
    write_row(sheet, 6, &["currency", "all", "count", "1"])?;
    write_row(
        sheet,
        7,
        &["events", "all", "count", &event_count.to_string()],
    )?;
    write_row(
        sheet,
        8,
        &[
            "valuation",
            "ledger",
            "valued_net_assets",
            valued_net_assets,
            "2026-03-15",
        ],
    )
}

fn write_row(sheet: &mut Worksheet, row: u32, values: &[&str]) -> Result<(), XlsxError> {
    for (column, value) in values.iter().enumerate() {
        sheet.write_string(row, u16::try_from(column).unwrap(), *value)?;
    }
    Ok(())
}
