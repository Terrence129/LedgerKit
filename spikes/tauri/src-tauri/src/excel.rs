use std::path::Path;
use std::time::Instant;

use calamine::{Data, Reader, open_workbook_auto};
use rust_xlsxwriter::{Format, Workbook};
use serde::Serialize;

use crate::canonical::sha256_prefixed;
use crate::error::{SpikeError, SpikeResult};
use crate::ledger::EventRecord;

pub const KNOWN_HEADERS: [&str; 8] = [
    "event_id",
    "effective_date",
    "event_type",
    "account_id",
    "category_id",
    "amount",
    "currency",
    "note",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub worksheet: String,
    pub row_count: usize,
    pub file_sha256: String,
    pub elapsed_ms: u128,
    pub financial_values_remained_strings: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub file_name: String,
    pub row_count: usize,
    pub file_sha256: String,
}

pub fn analyze_known_template(path: &Path) -> SpikeResult<ImportSummary> {
    let started = Instant::now();
    let bytes = std::fs::read(path)?;
    let file_sha256 = sha256_prefixed(&bytes);
    let mut workbook =
        open_workbook_auto(path).map_err(|error| SpikeError::Workbook(error.to_string()))?;
    let range = workbook
        .worksheet_range("Transactions")
        .map_err(|error| SpikeError::Workbook(error.to_string()))?;
    let mut rows = range.rows();
    let headers = rows.next().ok_or(SpikeError::WorkbookContractMismatch)?;
    if headers.len() != KNOWN_HEADERS.len()
        || headers
            .iter()
            .zip(KNOWN_HEADERS)
            .any(|(actual, expected)| cell_text(actual) != expected)
    {
        return Err(SpikeError::WorkbookContractMismatch);
    }

    let mut row_count = 0usize;
    for row in rows {
        if row.len() < KNOWN_HEADERS.len() {
            return Err(SpikeError::WorkbookContractMismatch);
        }
        if !matches!(row[5], Data::String(_)) {
            return Err(SpikeError::WorkbookContractMismatch);
        }
        let event_id = cell_text(&row[0]);
        let effective_date = cell_text(&row[1]);
        let event_type = cell_text(&row[2]);
        let currency = cell_text(&row[6]);
        if !event_id.starts_with("syn-event-")
            || effective_date.len() != 10
            || !matches!(event_type.as_str(), "Income" | "Expense")
            || currency != "CNY"
        {
            return Err(SpikeError::WorkbookContractMismatch);
        }
        row_count += 1;
    }
    if row_count != 10_000 {
        return Err(SpikeError::WorkbookContractMismatch);
    }

    Ok(ImportSummary {
        worksheet: "Transactions".to_owned(),
        row_count,
        file_sha256,
        elapsed_ms: started.elapsed().as_millis(),
        financial_values_remained_strings: true,
    })
}

pub fn export_standardized(events: &[EventRecord], path: &Path) -> SpikeResult<ExportSummary> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut workbook = Workbook::new();
    let header = Format::new().set_bold();
    let worksheet = workbook.add_worksheet().set_name("Transactions")?;
    for (column, name) in KNOWN_HEADERS.iter().enumerate() {
        worksheet.write_string_with_format(0, column as u16, *name, &header)?;
    }
    for (index, event) in events.iter().enumerate() {
        let row = (index + 1) as u32;
        worksheet.write_string(row, 0, &event.event_id)?;
        worksheet.write_string(row, 1, &event.effective_date)?;
        worksheet.write_string(row, 2, &event.event_type)?;
        worksheet.write_string(row, 3, &event.account_id)?;
        worksheet.write_string(row, 4, event.category_id.as_deref().unwrap_or(""))?;
        worksheet.write_string(row, 5, &event.amount)?;
        worksheet.write_string(row, 6, &event.currency)?;
        worksheet.write_string(row, 7, event.note.as_deref().unwrap_or(""))?;
    }
    worksheet.set_freeze_panes(1, 0)?;
    worksheet.set_column_width(0, 24)?;
    worksheet.set_column_width(1, 14)?;
    worksheet.set_column_width(2, 12)?;
    worksheet.set_column_width(3, 16)?;
    worksheet.set_column_width(4, 16)?;
    worksheet.set_column_width(5, 14)?;
    worksheet.set_column_width(6, 10)?;
    worksheet.set_column_width(7, 42)?;
    workbook
        .save(path)
        .map_err(|error| SpikeError::Workbook(error.to_string()))?;
    let bytes = std::fs::read(path)?;
    Ok(ExportSummary {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ledgerkit-standardized.xlsx")
            .to_owned(),
        row_count: events.len(),
        file_sha256: sha256_prefixed(&bytes),
    })
}

fn cell_text(cell: &Data) -> String {
    match cell {
        Data::String(value) => value.clone(),
        _ => cell.to_string(),
    }
}

impl From<rust_xlsxwriter::XlsxError> for SpikeError {
    fn from(value: rust_xlsxwriter::XlsxError) -> Self {
        Self::Workbook(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_adapter_reads_shared_10k_fixture_as_decimal_strings() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/sanitized/m1/ledgerkit-known-template-10000.xlsx");
        let summary = analyze_known_template(&path).expect("shared fixture must match");
        assert_eq!(summary.row_count, 10_000);
        assert!(summary.financial_values_remained_strings);
    }
}
