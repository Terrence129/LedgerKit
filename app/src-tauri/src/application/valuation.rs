#![forbid(unsafe_code)]

use serde::Serialize;

use crate::domain::types::LocalDate;

use super::error::ApplicationResult;

pub const OVERVIEW_CONTRACT: &str = "ledgerkit-overview-v1";
pub const DATA_QUALITY_CONTRACT: &str = "ledgerkit-data-quality-v1";
pub const VALUATION_SNAPSHOT_VERSION: &str = "valuation-snapshot-v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionItem {
    pub id: String,
    pub label: String,
    pub base_value: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewComposition {
    pub institutions: Vec<CompositionItem>,
    pub currencies: Vec<CompositionItem>,
    pub cash_accounts: Vec<CompositionItem>,
    pub holdings: Vec<CompositionItem>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnvaluedAsset {
    pub asset_type: String,
    pub entity_id: String,
    pub native_value: String,
    pub native_currency: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverviewWatermarks {
    pub event: u64,
    pub market_data: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub contract: &'static str,
    pub valuation_date: String,
    pub mtd_start_date: String,
    pub mtd_end_date: String,
    pub base_currency: String,
    pub valued_net_assets: String,
    pub valued_cash: String,
    pub valued_holdings: String,
    pub mtd_expense: String,
    pub mtd_unvalued_expense_count: u64,
    pub composition: OverviewComposition,
    pub unvalued_assets: Vec<UnvaluedAsset>,
    pub anomaly_codes: Vec<String>,
    pub watermarks: OverviewWatermarks,
    pub calculation_version: &'static str,
    pub snapshot_version: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixContext {
    pub operation: String,
    pub field: String,
    pub entity_type: String,
    pub entity_id: String,
    pub as_of_date: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataQualityIssue {
    pub issue_id: String,
    pub code: String,
    pub severity: String,
    pub status: String,
    pub context: FixContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataQualityReport {
    pub contract: &'static str,
    pub as_of_date: String,
    pub blocker_count: u64,
    pub warning_count: u64,
    pub issues: Vec<DataQualityIssue>,
    pub event_watermark: u64,
    pub calculation_version: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValuationSnapshot {
    pub snapshot_id: String,
    pub supersedes_snapshot_id: Option<String>,
    pub valuation_date: String,
    pub base_currency: String,
    pub line_count: u64,
    pub valued_net_assets: String,
    pub event_watermark: u64,
    pub market_data_watermark: u64,
    pub calculation_version: &'static str,
}

#[allow(clippy::missing_errors_doc)]
pub trait ValuationPort: Send {
    fn get_overview(&self, as_of_date: &LocalDate) -> ApplicationResult<Overview>;
    fn get_data_quality(&self, as_of_date: &LocalDate) -> ApplicationResult<DataQualityReport>;
    fn confirm_valuation_snapshot(
        &mut self,
        as_of_date: &LocalDate,
    ) -> ApplicationResult<ValuationSnapshot>;
}
