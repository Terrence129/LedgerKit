#![forbid(unsafe_code)]

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::ApplicationResult;

pub const IMPORTER_VERSION: &str = "ledgerkit-xlsx-full-v3";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportIssue {
    pub code: String,
    pub severity: String,
    pub sheet: String,
    pub row: u32,
    pub field: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMapping {
    pub entity_type: String,
    pub legacy_id: String,
    pub target_id: String,
    pub migration_policy: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPosting {
    pub account_id: String,
    #[serde(default)]
    pub portfolio_id: Option<String>,
    #[serde(default)]
    pub instrument_id: Option<String>,
    pub quantity_delta: String,
    pub currency: String,
    pub base_value: Option<String>,
    pub role: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProposedEvent {
    pub source_sheet: String,
    pub source_row: u32,
    pub event_type: String,
    pub effective_date: String,
    pub sequence: u64,
    pub postings: Vec<ImportPosting>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBalance {
    pub account_id: String,
    pub currency: String,
    pub source_balance: String,
    pub proposed_balance: String,
    pub difference: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportMetric {
    pub scope: String,
    pub entity_id: String,
    pub metric: String,
    pub source_value: String,
    pub proposed_value: String,
    pub difference: String,
    #[serde(default)]
    pub as_of_date: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDifference {
    pub scope: String,
    pub key: String,
    pub excel_value: String,
    pub application_value: String,
    pub difference: String,
    pub explanation: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReconciliation {
    pub balances: Vec<ImportBalance>,
    #[serde(default)]
    pub metrics: Vec<ImportMetric>,
    pub difference_bridge: Vec<String>,
    #[serde(default)]
    pub difference_items: Vec<ImportDifference>,
    pub canonical_result_sha256: String,
    pub balanced: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAnalysis {
    pub batch_id: String,
    pub source_sha256: String,
    pub template_version: String,
    pub importer_version: String,
    pub target_schema_version: u32,
    pub status: String,
    pub row_count: u32,
    pub valid_row_count: u32,
    pub blocker_count: u32,
    pub warning_count: u32,
    pub issues: Vec<ImportIssue>,
    pub mappings: Vec<ImportMapping>,
    pub proposed_events: Vec<ImportProposedEvent>,
    pub reconciliation: ImportReconciliation,
    pub can_commit: bool,
    pub reused_staging: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCommitResult {
    pub batch_id: String,
    pub source_sha256: String,
    pub status: String,
    pub ledger_id: String,
    pub event_watermark: u64,
    pub canonical_posting_sha256: String,
    pub already_committed: bool,
}

#[allow(clippy::missing_errors_doc)]
pub trait ImportPort: Send {
    fn analyze_import(&mut self, path: &Path) -> ApplicationResult<ImportAnalysis>;
    fn commit_import(
        &mut self,
        batch_id: &str,
        confirmed: bool,
    ) -> ApplicationResult<ImportCommitResult>;
}
