#![forbid(unsafe_code)]

use crate::domain::catalog::{BusinessId, CatalogText, CategoryKind, SemanticRole, SortOrder};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::error::DomainError;
use crate::domain::types::{Currency, LocalDate, UuidV7};

use super::error::ApplicationResult;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Institution {
    pub institution_id: UuidV7,
    pub business_id: BusinessId,
    pub name: CatalogText,
    pub region: Option<CatalogText>,
    pub institution_type: CatalogText,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashAccount {
    pub account_id: UuidV7,
    pub business_id: BusinessId,
    pub institution_id: UuidV7,
    pub name: CatalogText,
    pub purpose: CatalogText,
    pub currency: Currency,
    pub opened_on: Option<LocalDate>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Category {
    pub category_id: UuidV7,
    pub name: CatalogText,
    pub kind: CategoryKind,
    pub semantic_role: SemanticRole,
    pub sort_order: SortOrder,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Portfolio {
    pub portfolio_id: UuidV7,
    pub business_id: BusinessId,
    pub institution_id: UuidV7,
    pub settlement_account_id: UuidV7,
    pub name: CatalogText,
    pub portfolio_type: CatalogText,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityInstrument {
    pub instrument_id: UuidV7,
    pub business_id: BusinessId,
    pub code: CatalogText,
    pub name: CatalogText,
    pub trade_currency: Currency,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxRateRevision {
    pub revision_id: UuidV7,
    pub rate_date: LocalDate,
    pub currency: Currency,
    pub rate_to_base: Decimal,
    pub source: CatalogText,
    pub active: bool,
}

impl FxRateRevision {
    /// Creates a positive non-self FX revision.
    ///
    /// # Errors
    ///
    /// Returns a stable Domain error for invalid values or the synthetic self rate.
    pub fn new(
        revision_id: UuidV7,
        rate_date: LocalDate,
        currency: Currency,
        base_currency: Currency,
        rate_to_base: &str,
        source: CatalogText,
        active: bool,
    ) -> Result<Self, DomainError> {
        if currency == base_currency {
            return Err(DomainError::FxSelfRateImmutable);
        }
        let rate_to_base = Decimal::parse(rate_to_base, DecimalUse::FxRate)?;
        if !rate_to_base.is_positive() {
            return Err(DomainError::PositiveValueRequired);
        }
        Ok(Self {
            revision_id,
            rate_date,
            currency,
            rate_to_base,
            source,
            active,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityPriceRevision {
    pub revision_id: UuidV7,
    pub instrument_id: UuidV7,
    pub price_date: LocalDate,
    pub price: Decimal,
    pub price_currency: Currency,
    pub source: CatalogText,
    pub active: bool,
}

impl SecurityPriceRevision {
    /// Creates a positive security-price revision.
    ///
    /// # Errors
    ///
    /// Returns the Decimal contract error or `POSITIVE_VALUE_REQUIRED`.
    pub fn new(
        revision_id: UuidV7,
        instrument_id: UuidV7,
        price_date: LocalDate,
        price: &str,
        price_currency: Currency,
        source: CatalogText,
        active: bool,
    ) -> Result<Self, DomainError> {
        let price = Decimal::parse(price, DecimalUse::UnitPrice)?;
        if !price.is_positive() {
            return Err(DomainError::PositiveValueRequired);
        }
        Ok(Self {
            revision_id,
            instrument_id,
            price_date,
            price,
            price_currency,
            source,
            active,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogRecord {
    pub id: String,
    pub business_id: Option<String>,
    pub name: String,
    pub details: Vec<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketRevisionRecord {
    pub id: String,
    pub owner_id: String,
    pub date: String,
    pub value: String,
    pub currency: String,
    pub source: String,
    pub revision: u32,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityIssue {
    pub code: &'static str,
    pub entity_type: &'static str,
    pub entity_id: String,
    pub fix_operation: &'static str,
    pub fix_field: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarketSelection {
    pub revision_id: Option<String>,
    pub source_date: LocalDate,
    pub value: Decimal,
    pub currency: Currency,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogSnapshot {
    pub as_of_date: LocalDate,
    pub base_currency: Currency,
    pub institutions: Vec<CatalogRecord>,
    pub accounts: Vec<CatalogRecord>,
    pub categories: Vec<CatalogRecord>,
    pub portfolios: Vec<CatalogRecord>,
    pub instruments: Vec<CatalogRecord>,
    pub fx_revisions: Vec<MarketRevisionRecord>,
    pub price_revisions: Vec<MarketRevisionRecord>,
    pub quality_issues: Vec<QualityIssue>,
}

#[allow(clippy::missing_errors_doc)] // Implementations preserve stable ApplicationError values.
pub trait CatalogPort: Send {
    /// Saves one institution. # Errors Returns validation or persistence errors.
    fn save_institution(&mut self, value: &Institution) -> ApplicationResult<()>;
    /// Saves one cash account. # Errors Returns validation or persistence errors.
    fn save_cash_account(&mut self, value: &CashAccount) -> ApplicationResult<()>;
    /// Saves one category. # Errors Returns validation or persistence errors.
    fn save_category(&mut self, value: &Category) -> ApplicationResult<()>;
    /// Saves one portfolio. # Errors Returns validation or persistence errors.
    fn save_portfolio(&mut self, value: &Portfolio) -> ApplicationResult<()>;
    /// Saves one security instrument. # Errors Returns validation or persistence errors.
    fn save_instrument(&mut self, value: &SecurityInstrument) -> ApplicationResult<()>;
    /// Saves or activates an FX revision. # Errors Returns validation or persistence errors.
    fn save_fx_revision(&mut self, value: &FxRateRevision) -> ApplicationResult<()>;
    /// Saves or activates a price revision. # Errors Returns validation or persistence errors.
    fn save_price_revision(&mut self, value: &SecurityPriceRevision) -> ApplicationResult<()>;
    /// Loads catalog data as of a local date. # Errors Returns validation or persistence errors.
    fn catalog_snapshot(&self, as_of_date: &LocalDate) -> ApplicationResult<CatalogSnapshot>;
    /// Resolves the latest non-future active FX revision. # Errors Returns persistence errors.
    fn resolve_fx_rate(
        &self,
        currency: Currency,
        target_date: &LocalDate,
    ) -> ApplicationResult<Option<MarketSelection>>;
    /// Resolves the latest non-future active price revision. # Errors Returns persistence errors.
    fn resolve_price(
        &self,
        instrument_id: UuidV7,
        target_date: &LocalDate,
    ) -> ApplicationResult<Option<MarketSelection>>;
}
