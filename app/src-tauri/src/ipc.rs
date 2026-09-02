#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::application::cash::{
    ActivityPage, ActivityQuery, CashEventInput, DrilldownContext, EventInputType, EventPreview,
    ExpenseAnalysis, FxOverrideInput, PostedEvent, ReversalInput, RevisionInput,
};
use crate::application::catalog::{
    CatalogRecord, CatalogSnapshot, MarketRevisionRecord, QualityIssue,
};
use crate::application::error::ApplicationError;
use crate::application::facade::ApplicationFacade;
use crate::application::import::{ImportAnalysis, ImportCommitResult};
use crate::application::ledger::{LedgerState, LedgerStatus};
use crate::application::settings::PRIVILEGED_OPERATION_COUNT;
use crate::domain::catalog::SemanticRole;
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::error::DomainError;
use crate::domain::types::{Currency, LocalDate, Sequence, UuidV7};
use crate::infrastructure::file_settings::FileSettingsRepository;
use crate::infrastructure::sqlite::SqliteLedgerManager;

type DesktopFacade = ApplicationFacade<SqliteLedgerManager, FileSettingsRepository>;

pub struct AppState {
    facade: Arc<Mutex<DesktopFacade>>,
}

impl AppState {
    pub fn new(facade: DesktopFacade) -> Self {
        Self {
            facade: Arc::new(Mutex::new(facade)),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateLedgerRequest {
    base_currency: String,
    ui_locale: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LedgerStatusRequest {
    system_locale: Option<String>,
    as_of_date: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStatusResponse {
    app_version: &'static str,
    ui_locale: &'static str,
    ledger_state: &'static str,
    ledger_id: Option<String>,
    schema_version: Option<u32>,
    base_currency: Option<String>,
    event_watermark: u64,
    projection_watermark: u64,
    calculation_version: &'static str,
    blocked_reason: Option<&'static str>,
    database_location: Option<String>,
    backup_protection_state: String,
    device_loss_protected: bool,
    catalog: Option<CatalogSnapshotResponse>,
    local_only: bool,
    privileged_operation_count: u8,
}

impl LedgerStatusResponse {
    fn new(value: LedgerStatus, catalog: Option<CatalogSnapshot>) -> Self {
        Self {
            app_version: env!("CARGO_PKG_VERSION"),
            ui_locale: value.ui_locale.as_str(),
            ledger_state: value.state.as_str(),
            ledger_id: value.ledger_id,
            schema_version: value.schema_version,
            base_currency: value.base_currency.map(|currency| currency.to_string()),
            event_watermark: value.event_watermark,
            projection_watermark: value.projection_watermark,
            calculation_version: value.calculation_version,
            blocked_reason: value.blocked_reason,
            database_location: value.database_location,
            backup_protection_state: value.backup_protection_state,
            device_loss_protected: value.device_loss_protected,
            catalog: catalog.map(Into::into),
            local_only: true,
            privileged_operation_count: PRIVILEGED_OPERATION_COUNT,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshotResponse {
    as_of_date: String,
    base_currency: String,
    institutions: Vec<CatalogRecordResponse>,
    accounts: Vec<CatalogRecordResponse>,
    categories: Vec<CatalogRecordResponse>,
    portfolios: Vec<CatalogRecordResponse>,
    instruments: Vec<CatalogRecordResponse>,
    fx_revisions: Vec<MarketRevisionResponse>,
    price_revisions: Vec<MarketRevisionResponse>,
    quality_issues: Vec<QualityIssueResponse>,
}

impl From<CatalogSnapshot> for CatalogSnapshotResponse {
    fn from(value: CatalogSnapshot) -> Self {
        Self {
            as_of_date: value.as_of_date.as_str().to_owned(),
            base_currency: value.base_currency.to_string(),
            institutions: value.institutions.into_iter().map(Into::into).collect(),
            accounts: value.accounts.into_iter().map(Into::into).collect(),
            categories: value.categories.into_iter().map(Into::into).collect(),
            portfolios: value.portfolios.into_iter().map(Into::into).collect(),
            instruments: value.instruments.into_iter().map(Into::into).collect(),
            fx_revisions: value.fx_revisions.into_iter().map(Into::into).collect(),
            price_revisions: value.price_revisions.into_iter().map(Into::into).collect(),
            quality_issues: value.quality_issues.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogRecordResponse {
    id: String,
    business_id: Option<String>,
    name: String,
    details: Vec<String>,
    enabled: bool,
}

impl From<CatalogRecord> for CatalogRecordResponse {
    fn from(value: CatalogRecord) -> Self {
        Self {
            id: value.id,
            business_id: value.business_id,
            name: value.name,
            details: value.details,
            enabled: value.enabled,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketRevisionResponse {
    id: String,
    owner_id: String,
    date: String,
    value: String,
    currency: String,
    source: String,
    revision: u32,
    active: bool,
}

impl From<MarketRevisionRecord> for MarketRevisionResponse {
    fn from(value: MarketRevisionRecord) -> Self {
        Self {
            id: value.id,
            owner_id: value.owner_id,
            date: value.date,
            value: value.value,
            currency: value.currency,
            source: value.source,
            revision: value.revision,
            active: value.active,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct QualityIssueResponse {
    code: &'static str,
    entity_type: &'static str,
    entity_id: String,
    fix_operation: &'static str,
    fix_field: &'static str,
}

impl From<QualityIssue> for QualityIssueResponse {
    fn from(value: QualityIssue) -> Self {
        Self {
            code: value.code,
            entity_type: value.entity_type,
            entity_id: value.entity_id,
            fix_operation: value.fix_operation,
            fix_field: value.fix_field,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSettingsRequest {
    ui_locale: String,
    base_currency: Option<String>,
    valuation_defaults: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsResponse {
    ui_locale: &'static str,
    base_currency: Option<String>,
    persisted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveInstitutionRequest {
    institution_id: Option<String>,
    business_id: String,
    name: String,
    region: Option<String>,
    institution_type: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCashAccountRequest {
    account_id: Option<String>,
    business_id: String,
    institution_id: String,
    name: String,
    purpose: String,
    currency: String,
    opened_on: Option<String>,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCategoryRequest {
    category_id: Option<String>,
    name: String,
    kind: String,
    semantic_role: String,
    sort_order: u32,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePortfolioRequest {
    portfolio_id: Option<String>,
    business_id: String,
    institution_id: String,
    settlement_account_id: String,
    name: String,
    portfolio_type: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveInstrumentRequest {
    instrument_id: Option<String>,
    business_id: String,
    code: String,
    name: String,
    trade_currency: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveFxRevisionRequest {
    revision_id: Option<String>,
    rate_date: String,
    currency: String,
    rate_to_base: String,
    source: String,
    active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePriceRevisionRequest {
    revision_id: Option<String>,
    instrument_id: String,
    price_date: String,
    price: String,
    price_currency: String,
    source: String,
    active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FxOverrideRequest {
    currency: String,
    value: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CashEventRequest {
    effective_date: String,
    sequence: u64,
    event_type: String,
    account_id: Option<String>,
    from_account_id: Option<String>,
    to_account_id: Option<String>,
    amount: Option<String>,
    to_amount: Option<String>,
    category_id: Option<String>,
    semantic_role: Option<String>,
    merchant: Option<String>,
    note: Option<String>,
    fee_account_id: Option<String>,
    fee_amount: Option<String>,
    cutover_date: Option<String>,
    migration_policy: Option<String>,
    #[serde(default)]
    fx_overrides: Vec<FxOverrideRequest>,
    #[serde(default)]
    currency_precision_confirmed: bool,
}

impl CashEventRequest {
    fn into_application(self) -> Result<CashEventInput, CommandError> {
        Ok(CashEventInput {
            effective_date: LocalDate::parse(&self.effective_date)?,
            sequence: Sequence::new(self.sequence)?,
            event_type: EventInputType::parse(&self.event_type)?,
            account_id: parse_optional_id(self.account_id.as_deref())?,
            from_account_id: parse_optional_id(self.from_account_id.as_deref())?,
            to_account_id: parse_optional_id(self.to_account_id.as_deref())?,
            amount: parse_optional_decimal(self.amount.as_deref())?,
            to_amount: parse_optional_decimal(self.to_amount.as_deref())?,
            category_id: parse_optional_id(self.category_id.as_deref())?,
            semantic_role: SemanticRole::parse(self.semantic_role.as_deref().unwrap_or("normal"))?,
            merchant: self.merchant,
            note: self.note,
            fee_account_id: parse_optional_id(self.fee_account_id.as_deref())?,
            fee_amount: parse_optional_decimal(self.fee_amount.as_deref())?,
            cutover_date: self
                .cutover_date
                .as_deref()
                .map(LocalDate::parse)
                .transpose()?,
            migration_policy: self.migration_policy,
            fx_overrides: self
                .fx_overrides
                .into_iter()
                .map(|item| {
                    Ok(FxOverrideInput {
                        currency: Currency::parse(&item.currency)?,
                        value: Decimal::parse(&item.value, DecimalUse::FxRate)?,
                        reason: item.reason,
                    })
                })
                .collect::<Result<Vec<_>, CommandError>>()?,
            currency_precision_confirmed: self.currency_precision_confirmed,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviseEventRequest {
    target_event_id: String,
    reason: String,
    replacement: CashEventRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReverseEventRequest {
    target_event_id: String,
    reason: String,
    effective_date: String,
    sequence: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpenseAnalysisRequest {
    start_date: String,
    end_date: String,
    event_watermark: Option<u64>,
}

#[derive(Debug, Deserialize)]
// Drilldown contexts are copied verbatim from the canonical snake_case query result.
#[serde(deny_unknown_fields)]
pub struct DrilldownContextRequest {
    start_date: String,
    end_date: String,
    event_watermark: u64,
    bucket_id: Option<String>,
    semantic_role: Option<String>,
    member_rank_gt: Option<u32>,
    valuation_state: String,
    expense_policy_version: String,
    calculation_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityRequest {
    start_date: String,
    end_date: String,
    context: Option<DrilldownContextRequest>,
    event_type: Option<String>,
    account_id: Option<String>,
    category_id: Option<String>,
    search: Option<String>,
    cursor: Option<u64>,
    limit: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitImportRequest {
    batch_id: String,
    confirmed: bool,
}

#[derive(Debug, Serialize)]
pub struct SaveResult {
    id: String,
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    code: &'static str,
    field: Option<&'static str>,
}

impl From<ApplicationError> for CommandError {
    fn from(value: ApplicationError) -> Self {
        Self {
            code: value.code(),
            field: value.field(),
        }
    }
}

impl From<DomainError> for CommandError {
    fn from(value: DomainError) -> Self {
        ApplicationError::from(value).into()
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn create_ledger(
    request: CreateLedgerRequest,
    state: State<'_, AppState>,
) -> Result<LedgerStatusResponse, CommandError> {
    let mut facade = lock_facade(&state)?;
    let status = facade.create_ledger(&request.base_currency, &request.ui_locale)?;
    Ok(LedgerStatusResponse::new(status, None))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn open_ledger(state: State<'_, AppState>) -> Result<LedgerStatusResponse, CommandError> {
    let mut facade = lock_facade(&state)?;
    let status = facade.open_ledger()?;
    Ok(LedgerStatusResponse::new(status, None))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_ledger_status(
    request: LedgerStatusRequest,
    state: State<'_, AppState>,
) -> Result<LedgerStatusResponse, CommandError> {
    let facade = lock_facade(&state)?;
    let status = facade.get_ledger_status(request.system_locale.as_deref())?;
    let catalog = if status.state == LedgerState::Open {
        request
            .as_of_date
            .as_deref()
            .map(|date| facade.catalog_snapshot(date))
            .transpose()?
    } else {
        None
    };
    Ok(LedgerStatusResponse::new(status, catalog))
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn update_settings(
    request: UpdateSettingsRequest,
    state: State<'_, AppState>,
) -> Result<UpdateSettingsResponse, CommandError> {
    let valuation_defaults = request
        .valuation_defaults
        .map(|value| serde_json::to_string(&value))
        .transpose()
        .map_err(|_| CommandError::from(ApplicationError::StorageUnavailable))?;
    let mut facade = lock_facade(&state)?;
    let status = facade.update_settings(
        &request.ui_locale,
        request.base_currency.as_deref(),
        valuation_defaults,
    )?;
    Ok(UpdateSettingsResponse {
        ui_locale: status.ui_locale.as_str(),
        base_currency: status.base_currency.map(|currency| currency.to_string()),
        persisted: true,
    })
}

macro_rules! save_command {
    ($name:ident, $request:ty, $body:expr) => {
        #[tauri::command]
        #[allow(clippy::needless_pass_by_value)]
        pub fn $name(
            request: $request,
            state: State<'_, AppState>,
        ) -> Result<SaveResult, CommandError> {
            let mut facade = lock_facade(&state)?;
            let action = $body;
            let id = action(&mut facade, request)?;
            Ok(SaveResult { id })
        }
    };
}

save_command!(
    save_institution,
    SaveInstitutionRequest,
    |facade: &mut DesktopFacade, request: SaveInstitutionRequest| {
        facade.save_institution(
            request.institution_id.as_deref(),
            &request.business_id,
            &request.name,
            request.region.as_deref(),
            &request.institution_type,
            request.enabled,
        )
    }
);

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn preview_event(
    request: CashEventRequest,
    state: State<'_, AppState>,
) -> Result<EventPreview, CommandError> {
    let input = request.into_application()?;
    lock_facade(&state)?
        .preview_event(&input)
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn post_event(
    request: CashEventRequest,
    state: State<'_, AppState>,
) -> Result<PostedEvent, CommandError> {
    let input = request.into_application()?;
    lock_facade(&state)?.post_event(&input).map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn revise_event(
    request: ReviseEventRequest,
    state: State<'_, AppState>,
) -> Result<PostedEvent, CommandError> {
    let input = RevisionInput {
        target_event_id: UuidV7::parse(&request.target_event_id)?,
        reason: request.reason,
        replacement: request.replacement.into_application()?,
    };
    lock_facade(&state)?
        .revise_event(&input)
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn reverse_event(
    request: ReverseEventRequest,
    state: State<'_, AppState>,
) -> Result<PostedEvent, CommandError> {
    let input = ReversalInput {
        target_event_id: UuidV7::parse(&request.target_event_id)?,
        reason: request.reason,
        effective_date: LocalDate::parse(&request.effective_date)?,
        sequence: Sequence::new(request.sequence)?,
    };
    lock_facade(&state)?
        .reverse_event(&input)
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_expense_analysis(
    request: ExpenseAnalysisRequest,
    state: State<'_, AppState>,
) -> Result<ExpenseAnalysis, CommandError> {
    lock_facade(&state)?
        .get_expense_analysis(
            &request.start_date,
            &request.end_date,
            request.event_watermark,
        )
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_activity(
    request: ActivityRequest,
    state: State<'_, AppState>,
) -> Result<ActivityPage, CommandError> {
    let start_date = LocalDate::parse(&request.start_date)?;
    let end_date = LocalDate::parse(&request.end_date)?;
    let context = request
        .context
        .map(|context| {
            if context.expense_policy_version != "expense-policy-v1"
                || context.calculation_version != "ledger-calculation-v1"
                || !matches!(
                    context.valuation_state.as_str(),
                    "valued" | "unvalued" | "all"
                )
                || context.start_date != request.start_date
                || context.end_date != request.end_date
            {
                return Err(CommandError::from(ApplicationError::ActivityCursorInvalid));
            }
            Ok(DrilldownContext {
                start_date: context.start_date,
                end_date: context.end_date,
                event_watermark: context.event_watermark,
                calculation_version: "ledger-calculation-v1",
                expense_policy_version: "expense-policy-v1",
                bucket_id: context.bucket_id,
                semantic_role: context.semantic_role,
                member_rank_gt: context.member_rank_gt,
                valuation_state: context.valuation_state,
            })
        })
        .transpose()?;
    let event_type = request
        .event_type
        .map(|value| -> Result<String, CommandError> {
            if value == "Reversal" {
                Ok(value)
            } else {
                Ok(EventInputType::parse(&value)?.as_str().to_owned())
            }
        })
        .transpose()?;
    let query = ActivityQuery {
        start_date,
        end_date,
        context,
        event_type,
        account_id: parse_optional_id(request.account_id.as_deref())?,
        category_id: request.category_id,
        search: request.search,
        cursor: request.cursor,
        limit: request.limit,
    };
    lock_facade(&state)?
        .get_activity(&query)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn analyze_import(state: State<'_, AppState>) -> Result<ImportAnalysis, CommandError> {
    let facade = Arc::clone(&state.facade);
    tauri::async_runtime::spawn_blocking(move || {
        let path = rfd::FileDialog::new()
            .add_filter("Excel workbook", &["xlsx"])
            .pick_file()
            .ok_or(CommandError::from(ApplicationError::ImportCancelled))?;
        facade
            .lock()
            .map_err(|_| CommandError::from(ApplicationError::ApplicationStateUnavailable))?
            .analyze_import(&path)
            .map_err(Into::into)
    })
    .await
    .map_err(|_| CommandError::from(ApplicationError::ImportWorkerFailed))?
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub async fn commit_import(
    request: CommitImportRequest,
    state: State<'_, AppState>,
) -> Result<ImportCommitResult, CommandError> {
    let facade = Arc::clone(&state.facade);
    tauri::async_runtime::spawn_blocking(move || {
        facade
            .lock()
            .map_err(|_| CommandError::from(ApplicationError::ApplicationStateUnavailable))?
            .commit_import(&request.batch_id, request.confirmed)
            .map_err(Into::into)
    })
    .await
    .map_err(|_| CommandError::from(ApplicationError::ImportWorkerFailed))?
}

fn parse_optional_id(value: Option<&str>) -> Result<Option<UuidV7>, CommandError> {
    value.map(UuidV7::parse).transpose().map_err(Into::into)
}

fn parse_optional_decimal(value: Option<&str>) -> Result<Option<Decimal>, CommandError> {
    value
        .map(|item| Decimal::parse(item, DecimalUse::Amount))
        .transpose()
        .map_err(Into::into)
}
save_command!(
    save_cash_account,
    SaveCashAccountRequest,
    |facade: &mut DesktopFacade, request: SaveCashAccountRequest| {
        facade.save_cash_account(
            request.account_id.as_deref(),
            &request.business_id,
            &request.institution_id,
            &request.name,
            &request.purpose,
            &request.currency,
            request.opened_on.as_deref(),
            request.enabled,
        )
    }
);
save_command!(
    save_category,
    SaveCategoryRequest,
    |facade: &mut DesktopFacade, request: SaveCategoryRequest| {
        facade.save_category(
            request.category_id.as_deref(),
            &request.name,
            &request.kind,
            &request.semantic_role,
            request.sort_order,
            request.enabled,
        )
    }
);
save_command!(
    save_portfolio,
    SavePortfolioRequest,
    |facade: &mut DesktopFacade, request: SavePortfolioRequest| {
        facade.save_portfolio(
            request.portfolio_id.as_deref(),
            &request.business_id,
            &request.institution_id,
            &request.settlement_account_id,
            &request.name,
            &request.portfolio_type,
            request.enabled,
        )
    }
);
save_command!(
    save_instrument,
    SaveInstrumentRequest,
    |facade: &mut DesktopFacade, request: SaveInstrumentRequest| {
        facade.save_instrument(
            request.instrument_id.as_deref(),
            &request.business_id,
            &request.code,
            &request.name,
            &request.trade_currency,
            request.enabled,
        )
    }
);
save_command!(
    save_fx_revision,
    SaveFxRevisionRequest,
    |facade: &mut DesktopFacade, request: SaveFxRevisionRequest| {
        facade.save_fx_revision(
            request.revision_id.as_deref(),
            &request.rate_date,
            &request.currency,
            &request.rate_to_base,
            &request.source,
            request.active,
        )
    }
);
save_command!(
    save_price_revision,
    SavePriceRevisionRequest,
    |facade: &mut DesktopFacade, request: SavePriceRevisionRequest| {
        facade.save_price_revision(
            request.revision_id.as_deref(),
            &request.instrument_id,
            &request.price_date,
            &request.price,
            &request.price_currency,
            &request.source,
            request.active,
        )
    }
);

fn lock_facade<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, DesktopFacade>, CommandError> {
    state
        .facade
        .lock()
        .map_err(|_| CommandError::from(ApplicationError::ApplicationStateUnavailable))
}

#[cfg(test)]
mod tests {
    use super::{
        ActivityRequest, CashEventRequest, CommitImportRequest, CreateLedgerRequest,
        SaveFxRevisionRequest,
    };
    use serde_json::json;

    #[test]
    fn command_dtos_reject_arbitrary_paths_and_numeric_financial_values() {
        let forged = json!({ "baseCurrency": "CNY", "uiLocale": "en-US", "databasePath": "D:/synced/ledger.sqlite3" });
        assert!(serde_json::from_value::<CreateLedgerRequest>(forged).is_err());
        let fx_with_float = json!({ "rateDate": "2026-09-02", "currency": "USD", "rateToBase": 7.1, "source": "manual", "active": true });
        assert!(serde_json::from_value::<SaveFxRevisionRequest>(fx_with_float).is_err());
        let canonical_context = json!({
            "startDate": "2026-09-01",
            "endDate": "2026-09-02",
            "context": {
                "start_date": "2026-09-01",
                "end_date": "2026-09-02",
                "event_watermark": 1,
                "calculation_version": "ledger-calculation-v1",
                "expense_policy_version": "expense-policy-v1",
                "bucket_id": "system:ordinary-fee",
                "valuation_state": "valued"
            },
            "limit": 25
        });
        assert!(serde_json::from_value::<ActivityRequest>(canonical_context).is_ok());
        let base_event = json!({
            "effectiveDate": "2026-09-02",
            "sequence": 1,
            "eventType": "Expense",
            "accountId": "019d0000-0000-7000-8000-000000000001",
            "amount": "10"
        });
        assert!(serde_json::from_value::<CashEventRequest>(base_event.clone()).is_ok());
        for (field, value) in [
            ("currency", json!("USD")),
            ("sign", json!("credit")),
            ("posting", json!([])),
            ("status", json!("posted")),
            ("finalFx", json!("7.1")),
        ] {
            let mut forged = base_event.clone();
            forged[field] = value;
            assert!(serde_json::from_value::<CashEventRequest>(forged).is_err());
        }
        let general_activity = json!({
            "startDate": "2026-09-01",
            "endDate": "2026-09-02",
            "eventType": "Reversal",
            "search": "sample",
            "limit": 25
        });
        assert!(serde_json::from_value::<ActivityRequest>(general_activity).is_ok());
        let forged_import = json!({
            "batchId": "019d0000-0000-7000-8000-000000000001",
            "confirmed": true,
            "sourcePath": "D:/private/ledger.xlsx"
        });
        assert!(serde_json::from_value::<CommitImportRequest>(forged_import).is_err());
    }

    #[test]
    fn excel_dialog_parsing_and_commit_run_off_the_ui_thread() {
        let source = include_str!("ipc.rs");
        assert!(source.matches("spawn_blocking(move ||").count() >= 2);
        assert!(source.contains("rfd::FileDialog"));
    }
}
