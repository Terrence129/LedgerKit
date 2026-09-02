#![forbid(unsafe_code)]

use rusqlite::{OptionalExtension, Transaction, params};

use crate::application::catalog::{
    CashAccount, CatalogPort, CatalogRecord, CatalogSnapshot, Category, FxRateRevision,
    Institution, MarketRevisionRecord, MarketSelection, Portfolio, QualityIssue,
    SecurityInstrument, SecurityPriceRevision,
};
use crate::application::error::{ApplicationError, ApplicationResult};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::error::DomainError;
use crate::domain::types::{Currency, LocalDate, UuidV7};

use super::store::{LedgerStore, SqliteLedgerManager, map_sqlite_error};

impl CatalogPort for SqliteLedgerManager {
    fn save_institution(&mut self, value: &Institution) -> ApplicationResult<()> {
        self.open_store_mut()?.save_institution(value)
    }

    fn save_cash_account(&mut self, value: &CashAccount) -> ApplicationResult<()> {
        self.open_store_mut()?.save_cash_account(value)
    }

    fn save_category(&mut self, value: &Category) -> ApplicationResult<()> {
        self.open_store_mut()?.save_category(value)
    }

    fn save_portfolio(&mut self, value: &Portfolio) -> ApplicationResult<()> {
        self.open_store_mut()?.save_portfolio(value)
    }

    fn save_instrument(&mut self, value: &SecurityInstrument) -> ApplicationResult<()> {
        self.open_store_mut()?.save_instrument(value)
    }

    fn save_fx_revision(&mut self, value: &FxRateRevision) -> ApplicationResult<()> {
        self.open_store_mut()?.save_fx_revision(value)
    }

    fn save_price_revision(&mut self, value: &SecurityPriceRevision) -> ApplicationResult<()> {
        self.open_store_mut()?.save_price_revision(value)
    }

    fn catalog_snapshot(&self, as_of_date: &LocalDate) -> ApplicationResult<CatalogSnapshot> {
        self.open_store()?.catalog_snapshot(as_of_date)
    }

    fn resolve_fx_rate(
        &self,
        currency: Currency,
        target_date: &LocalDate,
    ) -> ApplicationResult<Option<MarketSelection>> {
        self.open_store()?.resolve_fx_rate(currency, target_date)
    }

    fn resolve_price(
        &self,
        instrument_id: UuidV7,
        target_date: &LocalDate,
    ) -> ApplicationResult<Option<MarketSelection>> {
        self.open_store()?.resolve_price(instrument_id, target_date)
    }
}

impl SqliteLedgerManager {
    pub(super) fn open_store(&self) -> ApplicationResult<&LedgerStore> {
        self.store.as_ref().ok_or(ApplicationError::LedgerNotOpen)
    }

    pub(super) fn open_store_mut(&mut self) -> ApplicationResult<&mut LedgerStore> {
        self.store.as_mut().ok_or(ApplicationError::LedgerNotOpen)
    }
}

impl LedgerStore {
    pub(super) fn save_institution(&mut self, value: &Institution) -> ApplicationResult<()> {
        let transaction = begin(&mut self.connection)?;
        ensure_unique_business_id(
            &transaction,
            "institutions",
            "institution_id",
            value.business_id.as_str(),
            value.institution_id,
        )?;
        transaction
            .execute(
                "INSERT INTO institutions(institution_id,business_id,name,region,institution_type,enabled,created_at_utc,updated_at_utc) VALUES(?1,?2,?3,?4,?5,?6,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP) ON CONFLICT(institution_id) DO UPDATE SET business_id=excluded.business_id,name=excluded.name,region=excluded.region,institution_type=excluded.institution_type,enabled=excluded.enabled,updated_at_utc=CURRENT_TIMESTAMP",
                params![value.institution_id.to_string(), value.business_id.as_str(), value.name.as_str(), value.region.as_ref().map(crate::domain::catalog::CatalogText::as_str), value.institution_type.as_str(), value.enabled],
            )
            .map_err(map_sqlite_error)?;
        audit_catalog(&transaction, "save", "institution", value.institution_id)?;
        commit(transaction)
    }

    pub(super) fn save_cash_account(&mut self, value: &CashAccount) -> ApplicationResult<()> {
        let transaction = begin(&mut self.connection)?;
        ensure_entity(
            &transaction,
            "institutions",
            "institution_id",
            value.institution_id,
        )?;
        ensure_unique_business_id(
            &transaction,
            "cash_accounts",
            "account_id",
            value.business_id.as_str(),
            value.account_id,
        )?;
        if !value.enabled {
            let balance: Option<String> = transaction
                .query_row(
                    "SELECT balance FROM cash_balance_projection WHERE account_id=?1",
                    [value.account_id.to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ApplicationError::TransactionFailed)?;
            if balance.is_some_and(|item| {
                Decimal::parse(&item, DecimalUse::Amount).is_ok_and(|decimal| !decimal.is_zero())
            }) {
                return Err(DomainError::AccountBalanceNonzero.into());
            }
        }
        transaction
            .execute(
                "INSERT INTO cash_accounts(account_id,business_id,institution_id,name,purpose,currency,opened_on,enabled,created_at_utc,updated_at_utc) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP) ON CONFLICT(account_id) DO UPDATE SET business_id=excluded.business_id,institution_id=excluded.institution_id,name=excluded.name,purpose=excluded.purpose,currency=excluded.currency,opened_on=excluded.opened_on,enabled=excluded.enabled,updated_at_utc=CURRENT_TIMESTAMP",
                params![value.account_id.to_string(), value.business_id.as_str(), value.institution_id.to_string(), value.name.as_str(), value.purpose.as_str(), value.currency.as_str(), value.opened_on.as_ref().map(LocalDate::as_str), value.enabled],
            )
            .map_err(map_sqlite_error)?;
        audit_catalog(&transaction, "save", "cash-account", value.account_id)?;
        commit(transaction)
    }

    pub(super) fn save_category(&mut self, value: &Category) -> ApplicationResult<()> {
        let transaction = begin(&mut self.connection)?;
        let duplicate: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE name=?1 AND category_kind=?2 AND category_id<>?3)",
                params![value.name.as_str(), value.kind.as_str(), value.category_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        if duplicate {
            return Err(ApplicationError::CatalogDuplicate);
        }
        transaction
            .execute(
                "INSERT INTO categories(category_id,name,category_kind,semantic_role,sort_order,enabled,created_at_utc,updated_at_utc) VALUES(?1,?2,?3,?4,?5,?6,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP) ON CONFLICT(category_id) DO UPDATE SET name=excluded.name,category_kind=excluded.category_kind,semantic_role=excluded.semantic_role,sort_order=excluded.sort_order,enabled=excluded.enabled,updated_at_utc=CURRENT_TIMESTAMP",
                params![value.category_id.to_string(), value.name.as_str(), value.kind.as_str(), value.semantic_role.as_str(), value.sort_order.get(), value.enabled],
            )
            .map_err(map_sqlite_error)?;
        audit_catalog(&transaction, "save", "category", value.category_id)?;
        commit(transaction)
    }

    fn save_portfolio(&mut self, value: &Portfolio) -> ApplicationResult<()> {
        let transaction = begin(&mut self.connection)?;
        ensure_entity(
            &transaction,
            "institutions",
            "institution_id",
            value.institution_id,
        )?;
        let account_institution: Option<String> = transaction
            .query_row(
                "SELECT institution_id FROM cash_accounts WHERE account_id=?1",
                [value.settlement_account_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        if account_institution.as_deref() != Some(&value.institution_id.to_string()) {
            return Err(DomainError::PortfolioInstitutionMismatch.into());
        }
        ensure_unique_business_id(
            &transaction,
            "portfolios",
            "portfolio_id",
            value.business_id.as_str(),
            value.portfolio_id,
        )?;
        transaction
            .execute(
                "INSERT INTO portfolios(portfolio_id,business_id,institution_id,settlement_account_id,name,portfolio_type,enabled,created_at_utc,updated_at_utc) VALUES(?1,?2,?3,?4,?5,?6,?7,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP) ON CONFLICT(portfolio_id) DO UPDATE SET business_id=excluded.business_id,institution_id=excluded.institution_id,settlement_account_id=excluded.settlement_account_id,name=excluded.name,portfolio_type=excluded.portfolio_type,enabled=excluded.enabled,updated_at_utc=CURRENT_TIMESTAMP",
                params![value.portfolio_id.to_string(), value.business_id.as_str(), value.institution_id.to_string(), value.settlement_account_id.to_string(), value.name.as_str(), value.portfolio_type.as_str(), value.enabled],
            )
            .map_err(map_sqlite_error)?;
        audit_catalog(&transaction, "save", "portfolio", value.portfolio_id)?;
        commit(transaction)
    }

    fn save_instrument(&mut self, value: &SecurityInstrument) -> ApplicationResult<()> {
        let transaction = begin(&mut self.connection)?;
        ensure_unique_business_id(
            &transaction,
            "security_instruments",
            "instrument_id",
            value.business_id.as_str(),
            value.instrument_id,
        )?;
        let duplicate: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM security_instruments WHERE code=?1 AND trade_currency=?2 AND instrument_id<>?3)",
                params![value.code.as_str(), value.trade_currency.as_str(), value.instrument_id.to_string()],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        if duplicate {
            return Err(ApplicationError::CatalogDuplicate);
        }
        transaction
            .execute(
                "INSERT INTO security_instruments(instrument_id,business_id,code,name,trade_currency,enabled,created_at_utc,updated_at_utc) VALUES(?1,?2,?3,?4,?5,?6,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP) ON CONFLICT(instrument_id) DO UPDATE SET business_id=excluded.business_id,code=excluded.code,name=excluded.name,trade_currency=excluded.trade_currency,enabled=excluded.enabled,updated_at_utc=CURRENT_TIMESTAMP",
                params![value.instrument_id.to_string(), value.business_id.as_str(), value.code.as_str(), value.name.as_str(), value.trade_currency.as_str(), value.enabled],
            )
            .map_err(map_sqlite_error)?;
        audit_catalog(
            &transaction,
            "save",
            "security-instrument",
            value.instrument_id,
        )?;
        commit(transaction)
    }

    pub(super) fn save_fx_revision(&mut self, value: &FxRateRevision) -> ApplicationResult<()> {
        let transaction = begin(&mut self.connection)?;
        let base_currency: String = transaction
            .query_row(
                "SELECT base_currency FROM app_settings WHERE singleton_id=1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let existing: Option<(String, String, String, String, String)> = transaction
            .query_row(
                "SELECT rate_date,currency,base_currency,rate_to_base,source FROM fx_rate_revisions WHERE fx_rate_revision_id=?1",
                [value.revision_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        if let Some(stored) = existing {
            if stored
                != (
                    value.rate_date.as_str().to_owned(),
                    value.currency.to_string(),
                    base_currency.clone(),
                    value.rate_to_base.as_str().to_owned(),
                    value.source.as_str().to_owned(),
                )
            {
                return Err(DomainError::RevisionImmutable.into());
            }
            switch_fx_active(&transaction, value, &base_currency)?;
        } else {
            let revision: u32 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(revision),0)+1 FROM fx_rate_revisions WHERE currency=?1 AND base_currency=?2 AND rate_date=?3",
                    params![value.currency.as_str(), base_currency, value.rate_date.as_str()],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
            switch_fx_active(&transaction, value, &base_currency)?;
            transaction
                .execute(
                    "INSERT INTO fx_rate_revisions(fx_rate_revision_id,rate_date,currency,base_currency,rate_to_base,source,revision,active,created_at_utc) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,CURRENT_TIMESTAMP)",
                    params![value.revision_id.to_string(), value.rate_date.as_str(), value.currency.as_str(), base_currency, value.rate_to_base.as_str(), value.source.as_str(), revision, value.active],
                )
                .map_err(map_sqlite_error)?;
        }
        audit_catalog(&transaction, "save", "fx-rate-revision", value.revision_id)?;
        commit(transaction)
    }

    fn save_price_revision(&mut self, value: &SecurityPriceRevision) -> ApplicationResult<()> {
        let transaction = begin(&mut self.connection)?;
        let trade_currency: Option<String> = transaction
            .query_row(
                "SELECT trade_currency FROM security_instruments WHERE instrument_id=?1",
                [value.instrument_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let trade_currency = trade_currency.ok_or(ApplicationError::CatalogEntityNotFound)?;
        if trade_currency != value.price_currency.as_str() {
            return Err(ApplicationError::CatalogReferenceInvalid);
        }
        let existing: Option<(String, String, String, String, String)> = transaction
            .query_row(
                "SELECT instrument_id,price_date,price,price_currency,source FROM security_price_revisions WHERE security_price_revision_id=?1",
                [value.revision_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        if let Some(stored) = existing {
            if stored
                != (
                    value.instrument_id.to_string(),
                    value.price_date.as_str().to_owned(),
                    value.price.as_str().to_owned(),
                    value.price_currency.to_string(),
                    value.source.as_str().to_owned(),
                )
            {
                return Err(DomainError::RevisionImmutable.into());
            }
            switch_price_active(&transaction, value)?;
        } else {
            let revision: u32 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(revision),0)+1 FROM security_price_revisions WHERE instrument_id=?1 AND price_date=?2",
                    params![value.instrument_id.to_string(), value.price_date.as_str()],
                    |row| row.get(0),
                )
                .map_err(|_| ApplicationError::TransactionFailed)?;
            switch_price_active(&transaction, value)?;
            transaction
                .execute(
                    "INSERT INTO security_price_revisions(security_price_revision_id,instrument_id,price_date,price,price_currency,source,revision,active,created_at_utc) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,CURRENT_TIMESTAMP)",
                    params![value.revision_id.to_string(), value.instrument_id.to_string(), value.price_date.as_str(), value.price.as_str(), value.price_currency.as_str(), value.source.as_str(), revision, value.active],
                )
                .map_err(map_sqlite_error)?;
        }
        audit_catalog(
            &transaction,
            "save",
            "security-price-revision",
            value.revision_id,
        )?;
        commit(transaction)
    }

    pub(super) fn resolve_fx_rate(
        &self,
        currency: Currency,
        target_date: &LocalDate,
    ) -> ApplicationResult<Option<MarketSelection>> {
        let base = self.base_currency()?;
        if currency == base {
            return Ok(Some(MarketSelection {
                revision_id: None,
                source_date: target_date.clone(),
                value: Decimal::parse("1", DecimalUse::FxRate)?,
                currency: base,
            }));
        }
        self.connection
            .query_row(
                "SELECT fx_rate_revision_id,rate_date,rate_to_base FROM fx_rate_revisions WHERE currency=?1 AND base_currency=?2 AND active=1 AND rate_date<=?3 ORDER BY rate_date DESC LIMIT 1",
                params![currency.as_str(), base.as_str(), target_date.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?
            .map(|(id, date, value)| {
                Ok(MarketSelection {
                    revision_id: Some(id),
                    source_date: LocalDate::parse(&date)?,
                    value: Decimal::parse(&value, DecimalUse::FxRate)?,
                    currency: base,
                })
            })
            .transpose()
    }

    pub(super) fn resolve_price(
        &self,
        instrument_id: UuidV7,
        target_date: &LocalDate,
    ) -> ApplicationResult<Option<MarketSelection>> {
        self.connection
            .query_row(
                "SELECT security_price_revision_id,price_date,price,price_currency FROM security_price_revisions WHERE instrument_id=?1 AND active=1 AND price_date<=?2 ORDER BY price_date DESC LIMIT 1",
                params![instrument_id.to_string(), target_date.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
            )
            .optional()
            .map_err(|_| ApplicationError::TransactionFailed)?
            .map(|(id, date, value, currency)| {
                Ok(MarketSelection {
                    revision_id: Some(id),
                    source_date: LocalDate::parse(&date)?,
                    value: Decimal::parse(&value, DecimalUse::UnitPrice)?,
                    currency: Currency::parse(&currency)?,
                })
            })
            .transpose()
    }

    fn catalog_snapshot(&self, as_of_date: &LocalDate) -> ApplicationResult<CatalogSnapshot> {
        let base_currency = self.base_currency()?;
        let institutions = load_records(
            &self.connection,
            "SELECT institution_id,business_id,name,json_array(COALESCE(region,''),institution_type),enabled FROM institutions ORDER BY business_id,institution_id",
        )?;
        let accounts = load_records(
            &self.connection,
            "SELECT account_id,business_id,name,json_array(institution_id,purpose,currency,COALESCE(opened_on,'')),enabled FROM cash_accounts ORDER BY business_id,account_id",
        )?;
        let categories = load_records(
            &self.connection,
            "SELECT category_id,NULL,name,json_array(category_kind,semantic_role,CAST(sort_order AS TEXT)),enabled FROM categories ORDER BY sort_order,category_id",
        )?;
        let portfolios = load_records(
            &self.connection,
            "SELECT portfolio_id,business_id,name,json_array(institution_id,settlement_account_id,portfolio_type),enabled FROM portfolios ORDER BY business_id,portfolio_id",
        )?;
        let instruments = load_records(
            &self.connection,
            "SELECT instrument_id,business_id,name,json_array(code,trade_currency),enabled FROM security_instruments ORDER BY business_id,instrument_id",
        )?;
        let fx_revisions = load_market_records(
            &self.connection,
            "SELECT fx_rate_revision_id,currency,rate_date,rate_to_base,base_currency,source,revision,active FROM fx_rate_revisions ORDER BY currency,rate_date DESC,revision DESC,fx_rate_revision_id",
        )?;
        let price_revisions = load_market_records(
            &self.connection,
            "SELECT security_price_revision_id,instrument_id,price_date,price,price_currency,source,revision,active FROM security_price_revisions ORDER BY instrument_id,price_date DESC,revision DESC,security_price_revision_id",
        )?;
        let mut quality_issues = self.quality_issues(as_of_date, base_currency)?;
        quality_issues.sort_by(|left, right| {
            (left.code, left.entity_type, &left.entity_id).cmp(&(
                right.code,
                right.entity_type,
                &right.entity_id,
            ))
        });
        Ok(CatalogSnapshot {
            as_of_date: as_of_date.clone(),
            base_currency,
            institutions,
            accounts,
            categories,
            portfolios,
            instruments,
            fx_revisions,
            price_revisions,
            quality_issues,
        })
    }

    pub(super) fn base_currency(&self) -> ApplicationResult<Currency> {
        let value: String = self
            .connection
            .query_row(
                "SELECT base_currency FROM app_settings WHERE singleton_id=1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::SchemaValidationFailed)?;
        Currency::parse(&value).map_err(Into::into)
    }

    fn quality_issues(
        &self,
        as_of_date: &LocalDate,
        base_currency: Currency,
    ) -> ApplicationResult<Vec<QualityIssue>> {
        let mut issues = Vec::new();
        let mut statement = self
            .connection
            .prepare(
                "SELECT account_id,currency FROM cash_accounts WHERE enabled=1 ORDER BY account_id",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let accounts = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for (account_id, currency_text) in accounts {
            let currency = Currency::parse(&currency_text)?;
            if currency != base_currency && self.resolve_fx_rate(currency, as_of_date)?.is_none() {
                issues.push(QualityIssue {
                    code: "FX_MISSING_AS_OF",
                    entity_type: "cash-account",
                    entity_id: account_id,
                    fix_operation: "save_fx_revision",
                    fix_field: "currency",
                });
            }
        }
        let mut statement = self
            .connection
            .prepare("SELECT instrument_id FROM security_instruments WHERE enabled=1 ORDER BY instrument_id")
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let instruments = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for instrument_id in instruments {
            let parsed = UuidV7::parse(&instrument_id)?;
            match self.resolve_price(parsed, as_of_date)? {
                None => issues.push(QualityIssue {
                    code: "PRICE_MISSING_AS_OF",
                    entity_type: "security-instrument",
                    entity_id: instrument_id,
                    fix_operation: "save_price_revision",
                    fix_field: "instrumentId",
                }),
                Some(selection) => {
                    let age: i64 = self
                        .connection
                        .query_row(
                            "SELECT CAST(julianday(?1)-julianday(?2) AS INTEGER)",
                            params![as_of_date.as_str(), selection.source_date.as_str()],
                            |row| row.get(0),
                        )
                        .map_err(|_| ApplicationError::TransactionFailed)?;
                    if age > 7 {
                        issues.push(QualityIssue {
                            code: "STALE_PRICE",
                            entity_type: "security-instrument",
                            entity_id: instrument_id,
                            fix_operation: "save_price_revision",
                            fix_field: "priceDate",
                        });
                    }
                }
            }
        }
        append_integrity_issues(&self.connection, &mut issues)?;
        Ok(issues)
    }
}

fn append_integrity_issues(
    connection: &rusqlite::Connection,
    issues: &mut Vec<QualityIssue>,
) -> ApplicationResult<()> {
    let mut statement = connection
        .prepare("SELECT p.portfolio_id FROM portfolios p JOIN cash_accounts a ON a.account_id=p.settlement_account_id WHERE p.institution_id<>a.institution_id ORDER BY p.portfolio_id")
        .map_err(|_| ApplicationError::TransactionFailed)?;
    for id in statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| ApplicationError::TransactionFailed)?
    {
        issues.push(QualityIssue {
            code: "PORTFOLIO_INSTITUTION_MISMATCH",
            entity_type: "portfolio",
            entity_id: id.map_err(|_| ApplicationError::TransactionFailed)?,
            fix_operation: "save_portfolio",
            fix_field: "settlementAccountId",
        });
    }
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| ApplicationError::TransactionFailed)?;
    let dangling = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .map_err(|_| ApplicationError::TransactionFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    for (table, row_id) in dangling {
        issues.push(QualityIssue {
            code: "DANGLING_REFERENCE",
            entity_type: "database-row",
            entity_id: format!("{table}:{}", row_id.unwrap_or_default()),
            fix_operation: "get_data_quality",
            fix_field: "reference",
        });
    }
    for (sql, entity_type) in [
        (
            "SELECT currency||':'||base_currency||':'||rate_date FROM fx_rate_revisions WHERE active=1 GROUP BY currency,base_currency,rate_date HAVING COUNT(*)>1",
            "fx-rate-revision",
        ),
        (
            "SELECT instrument_id||':'||price_date FROM security_price_revisions WHERE active=1 GROUP BY instrument_id,price_date HAVING COUNT(*)>1",
            "security-price-revision",
        ),
    ] {
        let mut statement = connection
            .prepare(sql)
            .map_err(|_| ApplicationError::TransactionFailed)?;
        for id in statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|_| ApplicationError::TransactionFailed)?
        {
            issues.push(QualityIssue {
                code: "ACTIVE_REVISION_CONFLICT",
                entity_type,
                entity_id: id.map_err(|_| ApplicationError::TransactionFailed)?,
                fix_operation: "get_data_quality",
                fix_field: "active",
            });
        }
    }
    Ok(())
}

fn begin(connection: &mut rusqlite::Connection) -> ApplicationResult<Transaction<'_>> {
    connection
        .transaction()
        .map_err(|_| ApplicationError::TransactionFailed)
}

fn commit(transaction: Transaction<'_>) -> ApplicationResult<()> {
    transaction
        .commit()
        .map_err(|_| ApplicationError::TransactionFailed)
}

fn ensure_entity(
    transaction: &Transaction<'_>,
    table: &str,
    id_column: &str,
    id: UuidV7,
) -> ApplicationResult<()> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {id_column}=?1)");
    let exists: bool = transaction
        .query_row(&sql, [id.to_string()], |row| row.get(0))
        .map_err(|_| ApplicationError::TransactionFailed)?;
    if exists {
        Ok(())
    } else {
        Err(ApplicationError::CatalogEntityNotFound)
    }
}

fn ensure_unique_business_id(
    transaction: &Transaction<'_>,
    table: &str,
    id_column: &str,
    business_id: &str,
    id: UuidV7,
) -> ApplicationResult<()> {
    let sql =
        format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE business_id=?1 AND {id_column}<>?2)");
    let duplicate: bool = transaction
        .query_row(&sql, params![business_id, id.to_string()], |row| row.get(0))
        .map_err(|_| ApplicationError::TransactionFailed)?;
    if duplicate {
        Err(ApplicationError::CatalogDuplicate)
    } else {
        Ok(())
    }
}

fn audit_catalog(
    transaction: &Transaction<'_>,
    action: &str,
    entity_type: &str,
    entity_id: UuidV7,
) -> ApplicationResult<()> {
    let audit_id = UuidV7::new()?;
    transaction
        .execute(
            "INSERT INTO audit_events(audit_event_id,business_event_id,actor,action,entity_type,entity_id,entity_revision,occurred_at_utc) VALUES(?1,NULL,'local-user',?2,?3,?4,1,CURRENT_TIMESTAMP)",
            params![audit_id.to_string(), action, entity_type, entity_id.to_string()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn switch_fx_active(
    transaction: &Transaction<'_>,
    value: &FxRateRevision,
    base_currency: &str,
) -> ApplicationResult<()> {
    if value.active {
        transaction
            .execute(
                "UPDATE fx_rate_revisions SET active=0 WHERE currency=?1 AND base_currency=?2 AND rate_date=?3 AND fx_rate_revision_id<>?4",
                params![value.currency.as_str(), base_currency, value.rate_date.as_str(), value.revision_id.to_string()],
            )
            .map_err(map_sqlite_error)?;
    }
    transaction
        .execute(
            "UPDATE fx_rate_revisions SET active=?1 WHERE fx_rate_revision_id=?2",
            params![value.active, value.revision_id.to_string()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn switch_price_active(
    transaction: &Transaction<'_>,
    value: &SecurityPriceRevision,
) -> ApplicationResult<()> {
    if value.active {
        transaction
            .execute(
                "UPDATE security_price_revisions SET active=0 WHERE instrument_id=?1 AND price_date=?2 AND security_price_revision_id<>?3",
                params![value.instrument_id.to_string(), value.price_date.as_str(), value.revision_id.to_string()],
            )
            .map_err(map_sqlite_error)?;
    }
    transaction
        .execute(
            "UPDATE security_price_revisions SET active=?1 WHERE security_price_revision_id=?2",
            params![value.active, value.revision_id.to_string()],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn load_records(
    connection: &rusqlite::Connection,
    sql: &str,
) -> ApplicationResult<Vec<CatalogRecord>> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| ApplicationError::TransactionFailed)?;
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, bool>(4)?,
            ))
        })
        .map_err(|_| ApplicationError::TransactionFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::TransactionFailed)?
        .into_iter()
        .map(|(id, business_id, name, details, enabled)| {
            Ok(CatalogRecord {
                id,
                business_id,
                name,
                details: serde_json::from_str(&details)
                    .map_err(|_| ApplicationError::SchemaValidationFailed)?,
                enabled,
            })
        })
        .collect()
}

fn load_market_records(
    connection: &rusqlite::Connection,
    sql: &str,
) -> ApplicationResult<Vec<MarketRevisionRecord>> {
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| ApplicationError::TransactionFailed)?;
    statement
        .query_map([], |row| {
            Ok(MarketRevisionRecord {
                id: row.get(0)?,
                owner_id: row.get(1)?,
                date: row.get(2)?,
                value: row.get(3)?,
                currency: row.get(4)?,
                source: row.get(5)?,
                revision: row.get(6)?,
                active: row.get(7)?,
            })
        })
        .map_err(|_| ApplicationError::TransactionFailed)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApplicationError::TransactionFailed)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::application::ledger::{CreateLedgerCommand, LedgerPort};
    use crate::domain::catalog::{BusinessId, CatalogText, CategoryKind, SemanticRole, SortOrder};
    use crate::domain::settings::UiLocale;

    fn id(seed: u8) -> UuidV7 {
        UuidV7::from_parts(1_777_000_100_000 + u64::from(seed), [seed; 10]).unwrap()
    }

    fn manager() -> SqliteLedgerManager {
        let directory = tempdir().unwrap().keep();
        let mut manager = SqliteLedgerManager::new(&directory).unwrap();
        manager
            .create_ledger(CreateLedgerCommand {
                base_currency: Currency::parse("CNY").unwrap(),
                ui_locale: UiLocale::EnUs,
            })
            .unwrap();
        manager
    }

    fn institution(seed: u8) -> Institution {
        Institution {
            institution_id: id(seed),
            business_id: BusinessId::parse(&format!("institution-{seed}")).unwrap(),
            name: CatalogText::parse(&format!("Institution {seed}")).unwrap(),
            region: None,
            institution_type: CatalogText::parse("bank").unwrap(),
            enabled: true,
        }
    }

    fn account(seed: u8, institution_id: UuidV7, currency: &str) -> CashAccount {
        CashAccount {
            account_id: id(seed),
            business_id: BusinessId::parse(&format!("account-{seed}")).unwrap(),
            institution_id,
            name: CatalogText::parse(&format!("Account {seed}")).unwrap(),
            purpose: CatalogText::parse("daily").unwrap(),
            currency: Currency::parse(currency).unwrap(),
            opened_on: None,
            enabled: true,
        }
    }

    #[test]
    fn stable_ids_survive_catalog_edits_and_portfolios_enforce_institution() {
        let mut manager = manager();
        let first = institution(1);
        let second = institution(2);
        manager.save_institution(&first).unwrap();
        manager.save_institution(&second).unwrap();
        let cash = account(3, first.institution_id, "CNY");
        manager.save_cash_account(&cash).unwrap();
        let mut renamed = first.clone();
        renamed.name = CatalogText::parse("Renamed Institution").unwrap();
        renamed.enabled = false;
        manager.save_institution(&renamed).unwrap();
        let portfolio = Portfolio {
            portfolio_id: id(4),
            business_id: BusinessId::parse("portfolio-4").unwrap(),
            institution_id: second.institution_id,
            settlement_account_id: cash.account_id,
            name: CatalogText::parse("Portfolio").unwrap(),
            portfolio_type: CatalogText::parse("brokerage").unwrap(),
            enabled: true,
        };
        assert_eq!(
            manager.save_portfolio(&portfolio),
            Err(ApplicationError::Domain(
                DomainError::PortfolioInstitutionMismatch
            ))
        );
        let snapshot = manager
            .catalog_snapshot(&LocalDate::parse("2026-09-02").unwrap())
            .unwrap();
        assert_eq!(
            snapshot.institutions[0].id,
            first.institution_id.to_string()
        );
        assert!(!snapshot.institutions[0].enabled);
    }

    #[test]
    fn fx_and_price_as_of_exclude_future_and_ignore_insert_order() {
        let mut manager = manager();
        let bank = institution(10);
        manager.save_institution(&bank).unwrap();
        manager
            .save_cash_account(&account(11, bank.institution_id, "USD"))
            .unwrap();
        let instrument = SecurityInstrument {
            instrument_id: id(12),
            business_id: BusinessId::parse("instrument-12").unwrap(),
            code: CatalogText::parse("SYN").unwrap(),
            name: CatalogText::parse("Synthetic").unwrap(),
            trade_currency: Currency::parse("USD").unwrap(),
            enabled: true,
        };
        manager.save_instrument(&instrument).unwrap();
        for (seed, date, value) in [
            (20, "2026-09-10", "7.20"),
            (21, "2026-09-01", "7.00"),
            (22, "2026-09-02", "7.10"),
        ] {
            manager
                .save_fx_revision(
                    &FxRateRevision::new(
                        id(seed),
                        LocalDate::parse(date).unwrap(),
                        Currency::parse("USD").unwrap(),
                        Currency::parse("CNY").unwrap(),
                        value,
                        CatalogText::parse("manual").unwrap(),
                        true,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let selected = manager
            .resolve_fx_rate(
                Currency::parse("USD").unwrap(),
                &LocalDate::parse("2026-09-02").unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(selected.source_date.as_str(), "2026-09-02");
        assert_eq!(selected.value.as_str(), "7.10");
        let self_rate = manager
            .resolve_fx_rate(
                Currency::parse("CNY").unwrap(),
                &LocalDate::parse("2020-01-01").unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(self_rate.value.as_str(), "1");

        for (seed, date, value) in [
            (30, "2026-09-03", "11"),
            (31, "2026-08-20", "9"),
            (32, "2026-09-02", "10"),
        ] {
            manager
                .save_price_revision(
                    &SecurityPriceRevision::new(
                        id(seed),
                        instrument.instrument_id,
                        LocalDate::parse(date).unwrap(),
                        value,
                        Currency::parse("USD").unwrap(),
                        CatalogText::parse("manual").unwrap(),
                        true,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        let price = manager
            .resolve_price(
                instrument.instrument_id,
                &LocalDate::parse("2026-09-02").unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(price.source_date.as_str(), "2026-09-02");
        assert_eq!(price.value.as_str(), "10");
    }

    #[test]
    fn revisions_are_immutable_and_active_switch_is_atomic() {
        let mut manager = manager();
        let original = FxRateRevision::new(
            id(40),
            LocalDate::parse("2026-09-02").unwrap(),
            Currency::parse("USD").unwrap(),
            Currency::parse("CNY").unwrap(),
            "7.00",
            CatalogText::parse("manual").unwrap(),
            true,
        )
        .unwrap();
        manager.save_fx_revision(&original).unwrap();
        let replacement = FxRateRevision::new(
            id(41),
            original.rate_date.clone(),
            original.currency,
            Currency::parse("CNY").unwrap(),
            "7.10",
            CatalogText::parse("corrected").unwrap(),
            true,
        )
        .unwrap();
        manager.save_fx_revision(&replacement).unwrap();
        let snapshot = manager.catalog_snapshot(&original.rate_date).unwrap();
        assert_eq!(
            snapshot
                .fx_revisions
                .iter()
                .filter(|item| item.active)
                .count(),
            1
        );
        assert_eq!(
            snapshot
                .fx_revisions
                .iter()
                .find(|item| item.active)
                .unwrap()
                .id,
            replacement.revision_id.to_string()
        );

        let changed = FxRateRevision::new(
            original.revision_id,
            original.rate_date,
            original.currency,
            Currency::parse("CNY").unwrap(),
            "8.00",
            CatalogText::parse("manual").unwrap(),
            true,
        )
        .unwrap();
        assert_eq!(
            manager.save_fx_revision(&changed),
            Err(ApplicationError::Domain(DomainError::RevisionImmutable))
        );
    }

    #[test]
    fn quality_results_have_stable_repair_context_and_never_guess_values() {
        let mut manager = manager();
        let bank = institution(50);
        manager.save_institution(&bank).unwrap();
        manager
            .save_cash_account(&account(51, bank.institution_id, "USD"))
            .unwrap();
        let instrument = SecurityInstrument {
            instrument_id: id(52),
            business_id: BusinessId::parse("instrument-52").unwrap(),
            code: CatalogText::parse("MISS").unwrap(),
            name: CatalogText::parse("Missing Price").unwrap(),
            trade_currency: Currency::parse("USD").unwrap(),
            enabled: true,
        };
        manager.save_instrument(&instrument).unwrap();
        let snapshot = manager
            .catalog_snapshot(&LocalDate::parse("2026-09-02").unwrap())
            .unwrap();
        assert!(snapshot.quality_issues.iter().any(
            |item| item.code == "FX_MISSING_AS_OF" && item.fix_operation == "save_fx_revision"
        ));
        assert!(
            snapshot
                .quality_issues
                .iter()
                .any(|item| item.code == "PRICE_MISSING_AS_OF"
                    && item.fix_operation == "save_price_revision")
        );
        assert!(
            manager
                .resolve_fx_rate(Currency::parse("USD").unwrap(), &snapshot.as_of_date)
                .unwrap()
                .is_none()
        );
        assert!(
            manager
                .resolve_price(instrument.instrument_id, &snapshot.as_of_date)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn category_edits_keep_the_stable_id_and_account_close_requires_zero_balance() {
        let mut manager = manager();
        let bank = institution(60);
        manager.save_institution(&bank).unwrap();
        let mut cash = account(61, bank.institution_id, "CNY");
        manager.save_cash_account(&cash).unwrap();
        let category_id = id(62);
        let mut category = Category {
            category_id,
            name: CatalogText::parse("Original").unwrap(),
            kind: CategoryKind::Expense,
            semantic_role: SemanticRole::Normal,
            sort_order: SortOrder::new(10).unwrap(),
            enabled: true,
        };
        manager.save_category(&category).unwrap();
        category.name = CatalogText::parse("Renamed").unwrap();
        category.sort_order = SortOrder::new(2).unwrap();
        category.enabled = false;
        manager.save_category(&category).unwrap();
        let snapshot = manager
            .catalog_snapshot(&LocalDate::parse("2026-09-02").unwrap())
            .unwrap();
        assert_eq!(snapshot.categories[0].id, category_id.to_string());
        assert_eq!(snapshot.categories[0].name, "Renamed");
        assert!(!snapshot.categories[0].enabled);

        manager
            .store
            .as_ref()
            .unwrap()
            .connection
            .execute(
                "INSERT INTO cash_balance_projection(account_id,balance,currency,event_watermark,calculation_version) VALUES(?1,'1.00','CNY',0,'ledger-calculation-v1')",
                [cash.account_id.to_string()],
            )
            .unwrap();
        cash.enabled = false;
        assert_eq!(
            manager.save_cash_account(&cash),
            Err(ApplicationError::Domain(DomainError::AccountBalanceNonzero))
        );
        assert_eq!(
            ApplicationError::Domain(DomainError::AccountBalanceNonzero).code(),
            "ACCOUNT_BALANCE_NONZERO"
        );
    }
}
