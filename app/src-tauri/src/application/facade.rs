#![forbid(unsafe_code)]

use crate::domain::catalog::{BusinessId, CatalogText, CategoryKind, SemanticRole, SortOrder};
use crate::domain::settings::UiLocale;
use crate::domain::types::{Currency, LocalDate, UuidV7};

use super::cash::{
    ActivityPage, ActivityQuery, CashEventInput, CashPort, EventPreview, ExpenseAnalysis,
    PostedEvent, ReversalInput, RevisionInput,
};
use super::catalog::{
    CashAccount, CatalogPort, CatalogSnapshot, Category, FxRateRevision, Institution, Portfolio,
    SecurityInstrument, SecurityPriceRevision,
};
use super::error::{ApplicationError, ApplicationResult};
use super::ledger::{
    CreateLedgerCommand, LedgerPort, LedgerState, LedgerStatus, UpdateLedgerSettingsCommand,
};
use super::settings::{SettingsError, SettingsRepository};

pub struct ApplicationFacade<L: LedgerPort + CatalogPort + CashPort, S: SettingsRepository> {
    ledger: L,
    shell_settings: S,
}

impl<L: LedgerPort + CatalogPort + CashPort, S: SettingsRepository> ApplicationFacade<L, S> {
    pub const fn new(ledger: L, shell_settings: S) -> Self {
        Self {
            ledger,
            shell_settings,
        }
    }

    /// Creates the single ledger through validated application commands.
    ///
    /// # Errors
    ///
    /// Returns a stable application error when input validation, persistence,
    /// or shell-locale persistence fails.
    pub fn create_ledger(
        &mut self,
        base_currency: &str,
        ui_locale: &str,
    ) -> ApplicationResult<LedgerStatus> {
        let command = CreateLedgerCommand {
            base_currency: Currency::parse(base_currency)?,
            ui_locale: parse_locale(ui_locale)?,
        };
        let status = self.ledger.create_ledger(command)?;
        self.save_shell_locale(command.ui_locale)?;
        Ok(status)
    }

    /// Opens the fixed ledger owned by the local application-data directory.
    ///
    /// # Errors
    ///
    /// Returns a stable application error when identification, migration,
    /// verification, or opening fails.
    pub fn open_ledger(&mut self) -> ApplicationResult<LedgerStatus> {
        let status = self.ledger.open_ledger()?;
        self.save_shell_locale(status.ui_locale)?;
        Ok(status)
    }

    /// Returns status without opening or mutating a closed ledger.
    ///
    /// # Errors
    ///
    /// Returns a stable application error when shell settings cannot be read.
    pub fn get_ledger_status(
        &self,
        system_locale_hint: Option<&str>,
    ) -> ApplicationResult<LedgerStatus> {
        let fallback = self
            .shell_settings
            .load_ui_locale()
            .map_err(map_settings_error)?
            .unwrap_or_else(|| UiLocale::from_system_hint(system_locale_hint));
        self.ledger.get_ledger_status(fallback)
    }

    /// Updates mutable settings while enforcing ledger currency freezes.
    ///
    /// # Errors
    ///
    /// Returns a stable application error for invalid input, a closed ledger
    /// when ledger-owned settings are requested, or persistence failure.
    pub fn update_settings(
        &mut self,
        ui_locale: &str,
        base_currency: Option<&str>,
        valuation_defaults_json: Option<String>,
    ) -> ApplicationResult<LedgerStatus> {
        let locale = parse_locale(ui_locale)?;
        let command = UpdateLedgerSettingsCommand {
            base_currency: base_currency.map(Currency::parse).transpose()?,
            ui_locale: locale,
            valuation_defaults_json,
        };
        let mut status = self.ledger.get_ledger_status(locale)?;
        if status.state == LedgerState::Open {
            status = self.ledger.update_settings(&command)?;
        } else if command.base_currency.is_some() || command.valuation_defaults_json.is_some() {
            return Err(ApplicationError::LedgerNotOpen);
        } else {
            status.ui_locale = locale;
        }
        self.save_shell_locale(locale)?;
        Ok(status)
    }

    /// Returns persisted setup/reference data and deterministic quality issues.
    ///
    /// # Errors
    ///
    /// Returns a stable validation or persistence error.
    pub fn catalog_snapshot(&self, as_of_date: &str) -> ApplicationResult<CatalogSnapshot> {
        self.ledger.catalog_snapshot(&LocalDate::parse(as_of_date)?)
    }

    /// Validates and saves an institution while preserving its stable ID.
    ///
    /// # Errors
    ///
    /// Returns a stable validation, uniqueness, reference, or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn save_institution(
        &mut self,
        institution_id: Option<&str>,
        business_id: &str,
        name: &str,
        region: Option<&str>,
        institution_type: &str,
        enabled: bool,
    ) -> ApplicationResult<String> {
        let value = Institution {
            institution_id: parse_or_create_id(institution_id)?,
            business_id: BusinessId::parse(business_id)?,
            name: CatalogText::parse(name)?,
            region: region.map(CatalogText::parse).transpose()?,
            institution_type: CatalogText::parse(institution_type)?,
            enabled,
        };
        let id = value.institution_id.to_string();
        self.ledger.save_institution(&value)?;
        Ok(id)
    }

    /// Validates and saves a cash account while enforcing stable references.
    ///
    /// # Errors
    ///
    /// Returns a stable validation, balance, reference, or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn save_cash_account(
        &mut self,
        account_id: Option<&str>,
        business_id: &str,
        institution_id: &str,
        name: &str,
        purpose: &str,
        currency: &str,
        opened_on: Option<&str>,
        enabled: bool,
    ) -> ApplicationResult<String> {
        let value = CashAccount {
            account_id: parse_or_create_id(account_id)?,
            business_id: BusinessId::parse(business_id)?,
            institution_id: UuidV7::parse(institution_id)?,
            name: CatalogText::parse(name)?,
            purpose: CatalogText::parse(purpose)?,
            currency: Currency::parse(currency)?,
            opened_on: opened_on.map(LocalDate::parse).transpose()?,
            enabled,
        };
        let id = value.account_id.to_string();
        self.ledger.save_cash_account(&value)?;
        Ok(id)
    }

    /// Validates and saves a stable category definition.
    ///
    /// # Errors
    ///
    /// Returns a stable validation, uniqueness, or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn save_category(
        &mut self,
        category_id: Option<&str>,
        name: &str,
        kind: &str,
        semantic_role: &str,
        sort_order: u32,
        enabled: bool,
    ) -> ApplicationResult<String> {
        let value = Category {
            category_id: parse_or_create_id(category_id)?,
            name: CatalogText::parse(name)?,
            kind: CategoryKind::parse(kind)?,
            semantic_role: SemanticRole::parse(semantic_role)?,
            sort_order: SortOrder::new(sort_order)?,
            enabled,
        };
        let id = value.category_id.to_string();
        self.ledger.save_category(&value)?;
        Ok(id)
    }

    /// Validates and saves a portfolio and settlement-account relationship.
    ///
    /// # Errors
    ///
    /// Returns a stable validation, institution, reference, or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn save_portfolio(
        &mut self,
        portfolio_id: Option<&str>,
        business_id: &str,
        institution_id: &str,
        settlement_account_id: &str,
        name: &str,
        portfolio_type: &str,
        enabled: bool,
    ) -> ApplicationResult<String> {
        let value = Portfolio {
            portfolio_id: parse_or_create_id(portfolio_id)?,
            business_id: BusinessId::parse(business_id)?,
            institution_id: UuidV7::parse(institution_id)?,
            settlement_account_id: UuidV7::parse(settlement_account_id)?,
            name: CatalogText::parse(name)?,
            portfolio_type: CatalogText::parse(portfolio_type)?,
            enabled,
        };
        let id = value.portfolio_id.to_string();
        self.ledger.save_portfolio(&value)?;
        Ok(id)
    }

    /// Validates and saves a security instrument.
    ///
    /// # Errors
    ///
    /// Returns a stable validation, uniqueness, freeze, or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn save_instrument(
        &mut self,
        instrument_id: Option<&str>,
        business_id: &str,
        code: &str,
        name: &str,
        trade_currency: &str,
        enabled: bool,
    ) -> ApplicationResult<String> {
        let value = SecurityInstrument {
            instrument_id: parse_or_create_id(instrument_id)?,
            business_id: BusinessId::parse(business_id)?,
            code: CatalogText::parse(code)?,
            name: CatalogText::parse(name)?,
            trade_currency: Currency::parse(trade_currency)?,
            enabled,
        };
        let id = value.instrument_id.to_string();
        self.ledger.save_instrument(&value)?;
        Ok(id)
    }

    /// Creates or activates an immutable FX-rate revision.
    ///
    /// # Errors
    ///
    /// Returns a stable Decimal, currency, immutability, or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn save_fx_revision(
        &mut self,
        revision_id: Option<&str>,
        rate_date: &str,
        currency: &str,
        rate_to_base: &str,
        source: &str,
        active: bool,
    ) -> ApplicationResult<String> {
        let base_currency = self
            .ledger
            .get_ledger_status(UiLocale::EnUs)?
            .base_currency
            .ok_or(ApplicationError::LedgerNotOpen)?;
        let value = FxRateRevision::new(
            parse_or_create_id(revision_id)?,
            LocalDate::parse(rate_date)?,
            Currency::parse(currency)?,
            base_currency,
            rate_to_base,
            CatalogText::parse(source)?,
            active,
        )?;
        let id = value.revision_id.to_string();
        self.ledger.save_fx_revision(&value)?;
        Ok(id)
    }

    /// Creates or activates an immutable security-price revision.
    ///
    /// # Errors
    ///
    /// Returns a stable Decimal, reference, immutability, or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn save_price_revision(
        &mut self,
        revision_id: Option<&str>,
        instrument_id: &str,
        price_date: &str,
        price: &str,
        price_currency: &str,
        source: &str,
        active: bool,
    ) -> ApplicationResult<String> {
        let value = SecurityPriceRevision::new(
            parse_or_create_id(revision_id)?,
            UuidV7::parse(instrument_id)?,
            LocalDate::parse(price_date)?,
            price,
            Currency::parse(price_currency)?,
            CatalogText::parse(source)?,
            active,
        )?;
        let id = value.revision_id.to_string();
        self.ledger.save_price_revision(&value)?;
        Ok(id)
    }

    /// Previews authoritative postings and frozen FX choices without mutation.
    ///
    /// # Errors
    ///
    /// Returns stable validation, catalog, or storage errors.
    pub fn preview_event(&self, input: &CashEventInput) -> ApplicationResult<EventPreview> {
        self.ledger.preview_event(input)
    }

    /// Posts a validated cash event atomically.
    ///
    /// # Errors
    ///
    /// Returns stable validation, catalog, or transaction errors.
    pub fn post_event(&mut self, input: &CashEventInput) -> ApplicationResult<PostedEvent> {
        self.ledger.post_event(input)
    }

    /// Appends a complete replacement for the current effective event leaf.
    ///
    /// # Errors
    ///
    /// Returns stable target, validation, or transaction errors.
    pub fn revise_event(&mut self, input: &RevisionInput) -> ApplicationResult<PostedEvent> {
        self.ledger.revise_event(input)
    }

    /// Appends an exact posting reversal for the current effective event leaf.
    ///
    /// # Errors
    ///
    /// Returns stable target, validation, or transaction errors.
    pub fn reverse_event(&mut self, input: &ReversalInput) -> ApplicationResult<PostedEvent> {
        self.ledger.reverse_event(input)
    }

    /// Returns the versioned P0 expense query result from one SQLite snapshot.
    ///
    /// # Errors
    ///
    /// Returns stable date, storage, or response-bound errors.
    pub fn get_expense_analysis(
        &self,
        start_date: &str,
        end_date: &str,
        event_watermark: Option<u64>,
    ) -> ApplicationResult<ExpenseAnalysis> {
        let start_date = LocalDate::parse(start_date)?;
        let end_date = LocalDate::parse(end_date)?;
        if start_date > end_date {
            return Err(ApplicationError::ExpenseDateRangeInvalid);
        }
        self.ledger
            .get_expense_analysis(&start_date, &end_date, event_watermark)
    }

    /// Returns one bounded cursor page using the same effective-event policy.
    ///
    /// # Errors
    ///
    /// Returns stable cursor, limit, date, or storage errors.
    pub fn get_activity(&self, query: &ActivityQuery) -> ApplicationResult<ActivityPage> {
        self.ledger.get_activity(query)
    }

    fn save_shell_locale(&self, locale: UiLocale) -> ApplicationResult<()> {
        self.shell_settings
            .save_ui_locale(locale)
            .map_err(map_settings_error)
    }
}

fn parse_or_create_id(value: Option<&str>) -> ApplicationResult<UuidV7> {
    value
        .map(UuidV7::parse)
        .transpose()?
        .map_or_else(|| UuidV7::new().map_err(ApplicationError::from), Ok)
}

fn parse_locale(value: &str) -> ApplicationResult<UiLocale> {
    UiLocale::parse(value).ok_or(ApplicationError::InvalidLocale)
}

const fn map_settings_error(error: SettingsError) -> ApplicationError {
    match error {
        SettingsError::InvalidLocale => ApplicationError::InvalidLocale,
        SettingsError::StorageUnavailable => ApplicationError::StorageUnavailable,
    }
}
