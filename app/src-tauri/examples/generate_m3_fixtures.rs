#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use rust_xlsxwriter::{DocProperties, ExcelDateTime, Formula, Workbook, Worksheet, XlsxError};

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
];

fn main() -> Result<(), XlsxError> {
    let root = std::env::args_os().nth(1).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sanitized/m3"),
        PathBuf::from,
    );
    std::fs::create_dir_all(&root).map_err(XlsxError::IoError)?;
    write_fixture(&root.join("cash-import-valid.xlsx"), FixtureKind::Valid)?;
    write_fixture(&root.join("cash-import-invalid.xlsx"), FixtureKind::Invalid)?;
    write_fixture(
        &root.join("cash-import-modified.xlsx"),
        FixtureKind::Modified,
    )
}

#[derive(Clone, Copy)]
enum FixtureKind {
    Valid,
    Invalid,
    Modified,
}

fn write_fixture(path: &Path, kind: FixtureKind) -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();
    let created = ExcelDateTime::from_ymd(2026, 1, 1)?;
    workbook.set_properties(
        &DocProperties::new()
            .set_title("LedgerKit deterministic synthetic migration fixture")
            .set_creation_datetime(&created),
    );
    for (name, headers) in HEADERS {
        let sheet = workbook.add_worksheet();
        sheet.set_name(*name)?;
        write_row(sheet, 0, headers)?;
    }
    write_common(&mut workbook, kind)?;
    workbook.save(path)
}

#[allow(clippy::too_many_lines)] // The generator mirrors the eight-sheet fixture in one obvious sequence.
fn write_common(workbook: &mut Workbook, kind: FixtureKind) -> Result<(), XlsxError> {
    write_row(
        workbook.worksheet_from_name("设置")?,
        1,
        &["ledgerkit-workbook-v1.4", "CNY", "zh-CN"],
    )?;
    write_row(
        workbook.worksheet_from_name("机构")?,
        1,
        &["inst-bank", "Synthetic Bank", "SG", "bank", "true"],
    )?;
    if matches!(kind, FixtureKind::Invalid) {
        write_row(
            workbook.worksheet_from_name("机构")?,
            2,
            &["inst-bank", "Duplicate Bank", "SG", "bank", "true"],
        )?;
    }
    let account_sheet = workbook.worksheet_from_name("资金子账户")?;
    write_row(
        account_sheet,
        1,
        &[
            "acct-cny-main",
            "inst-bank",
            "Main CNY",
            "daily",
            "CNY",
            "2025-01-01",
            "1000",
            "2026-01-01",
            "explicit_cutover",
            "true",
        ],
    )?;
    write_row(
        account_sheet,
        2,
        &[
            "acct-cny-save",
            "inst-bank",
            "Savings CNY",
            "reserve",
            "CNY",
            "2025-01-01",
            "500",
            "2026-01-01",
            "explicit_cutover",
            "true",
        ],
    )?;
    write_row(
        account_sheet,
        3,
        &[
            "acct-usd",
            "inst-bank",
            "USD Wallet",
            "travel",
            "USD",
            "2025-01-01",
            "100",
            "2026-01-01",
            "explicit_cutover",
            "true",
        ],
    )?;
    if matches!(kind, FixtureKind::Invalid) {
        write_row(
            account_sheet,
            4,
            &[
                "acct-bad",
                "missing-inst",
                "Bad Ref",
                "test",
                "JPY",
                "bad-date",
                "1",
                "2026-01-01",
                "",
                "true",
            ],
        )?;
    }
    let categories = workbook.worksheet_from_name("分类")?;
    write_row(
        categories,
        1,
        &["cat-income", "Salary", "income", "normal", "1", "true"],
    )?;
    write_row(
        categories,
        2,
        &["cat-expense", "Food", "expense", "normal", "2", "true"],
    )?;
    let fx = workbook.worksheet_from_name("汇率")?;
    if !matches!(kind, FixtureKind::Invalid) {
        write_row(fx, 1, &["2026-01-01", "USD", "7", "synthetic", "true"])?;
    }
    let activity = workbook.worksheet_from_name("收支流水")?;
    activity.write_string(1, 0, "2026-02-01")?;
    activity.write_string(1, 1, "1")?;
    activity.write_string(1, 2, "Income")?;
    activity.write_string(1, 3, "acct-cny-main")?;
    activity.write_string(
        1,
        4,
        if matches!(kind, FixtureKind::Invalid) {
            "cat-expense"
        } else {
            "cat-income"
        },
    )?;
    let amount = if matches!(kind, FixtureKind::Modified) {
        "26"
    } else {
        "25"
    };
    activity.write_formula(1, 5, Formula::new("20+5").set_result(amount))?;
    activity.write_string(1, 6, "Synthetic Employer")?;
    activity.write_string(1, 7, "Synthetic income")?;
    activity.write_string(1, 8, "normal")?;
    activity.write_formula(1, 11, Formula::new("F2").set_result(amount))?;
    activity.write_formula(
        1,
        12,
        Formula::new("IF(F2>0,\"ok\",\"check\")").set_result("ok"),
    )?;
    activity.write_formula(
        1,
        13,
        Formula::new("C2&\" item\"").set_result("Income item"),
    )?;
    if matches!(kind, FixtureKind::Invalid) {
        activity.write_string(2, 0, "2026-02-30")?;
        activity.write_string(2, 1, "1")?;
        activity.write_string(2, 2, "Expense")?;
        activity.write_string(2, 3, "missing-account")?;
        activity.write_string(2, 4, "cat-income")?;
        activity.write_formula(2, 5, Formula::new("1/0").set_result("#DIV/0!"))?;
        activity.write_string(2, 8, "normal")?;
    } else {
        write_row(
            activity,
            2,
            &[
                "2026-02-02",
                "1",
                "Expense",
                "acct-cny-main",
                "cat-expense",
                "10",
                "Synthetic Shop",
                "Synthetic expense",
                "normal",
                "acct-cny-main",
                "1",
                "",
                "",
                "",
            ],
        )?;
    }
    write_row(
        workbook.worksheet_from_name("资金调拨")?,
        1,
        &[
            "2026-02-03",
            "1",
            "acct-cny-main",
            "acct-cny-save",
            "100",
            "Synthetic transfer",
        ],
    )?;
    write_row(
        workbook.worksheet_from_name("换汇流水")?,
        1,
        &[
            "2026-02-04",
            "1",
            "acct-usd",
            "acct-cny-main",
            "10",
            "70",
            "acct-usd",
            "1",
            "Synthetic exchange",
        ],
    )?;
    Ok(())
}

fn write_row(worksheet: &mut Worksheet, row: u32, values: &[&str]) -> Result<(), XlsxError> {
    for (column, value) in values.iter().enumerate() {
        worksheet.write_string(
            row,
            u16::try_from(column).expect("fixture columns fit in u16"),
            *value,
        )?;
    }
    Ok(())
}
