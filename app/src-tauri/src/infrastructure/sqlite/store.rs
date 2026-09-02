#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::application::canonical::canonical_postings_hash;
use crate::application::error::{ApplicationError, ApplicationResult};
use crate::application::ledger::{
    CreateLedgerCommand, EventTransactionPort, LedgerPort, LedgerState, LedgerStatus,
    PreparedEventCommit, UpdateLedgerSettingsCommand,
};
use crate::domain::decimal::{Decimal, DecimalUse};
use crate::domain::error::DomainError;
use crate::domain::posting::{LedgerPosting, PostingKind};
use crate::domain::settings::UiLocale;
use crate::domain::types::{
    CalculationVersion, Currency, LocalDate, ProjectionWatermark, Sequence, UuidV7,
};

use super::migration::{
    MigrationRunner, VerifiedSqliteMigrationBackup, inspect_read_only, validate_local_data_root,
    validate_schema,
};
use super::projection::{CASH_PROJECTION_NAME, CASH_PROJECTION_VERSION};
use super::schema::{SCHEMA_VERSION, schema_hash};

pub const CALCULATION_VERSION: &str = "ledger-calculation-v1";
const LEDGER_FILENAME: &str = "ledger.sqlite3";

pub struct SqliteLedgerManager {
    pub(super) database_path: PathBuf,
    pub(super) backup: VerifiedSqliteMigrationBackup,
    pub(super) store: Option<LedgerStore>,
}

impl SqliteLedgerManager {
    pub fn new(local_data_root: &std::path::Path) -> ApplicationResult<Self> {
        validate_local_data_root(local_data_root)?;
        Ok(Self {
            database_path: local_data_root.join(LEDGER_FILENAME),
            backup: VerifiedSqliteMigrationBackup::new(local_data_root.join("migration-backups")),
            store: None,
        })
    }
}

impl LedgerPort for SqliteLedgerManager {
    fn create_ledger(&mut self, command: CreateLedgerCommand) -> ApplicationResult<LedgerStatus> {
        if self.store.is_some() {
            return Err(ApplicationError::LedgerAlreadyOpen);
        }
        let connection = MigrationRunner::create_new(&self.database_path)?;
        let store = LedgerStore::initialize(connection, command.base_currency, command.ui_locale)?;
        let status = self.with_database_location(store.status()?);
        self.store = Some(store);
        Ok(status)
    }

    fn open_ledger(&mut self) -> ApplicationResult<LedgerStatus> {
        if self.store.is_some() {
            return Err(ApplicationError::LedgerAlreadyOpen);
        }
        let connection = MigrationRunner::open_existing(&self.database_path, &mut self.backup)?;
        let store = LedgerStore::from_open_connection(connection)?;
        let status = self.with_database_location(store.status()?);
        self.store = Some(store);
        Ok(status)
    }

    fn get_ledger_status(&self, fallback_locale: UiLocale) -> ApplicationResult<LedgerStatus> {
        if let Some(store) = &self.store {
            return store
                .status()
                .map(|status| self.with_database_location(status));
        }
        if !self.database_path.exists() {
            return Ok(self.with_database_location(empty_status(
                LedgerState::NotCreated,
                fallback_locale,
                None,
            )));
        }
        match inspect_read_only(&self.database_path) {
            Ok(identity) if identity.schema_version <= SCHEMA_VERSION => Ok(self
                .with_database_location(empty_status(LedgerState::Closed, fallback_locale, None))),
            Ok(_) => Ok(self.with_database_location(empty_status(
                LedgerState::Blocked,
                fallback_locale,
                Some(ApplicationError::SchemaTooNew.code()),
            ))),
            Err(error) => Ok(self.with_database_location(empty_status(
                LedgerState::Blocked,
                fallback_locale,
                Some(error.code()),
            ))),
        }
    }

    fn update_settings(
        &mut self,
        command: &UpdateLedgerSettingsCommand,
    ) -> ApplicationResult<LedgerStatus> {
        let status = self
            .store
            .as_mut()
            .ok_or(ApplicationError::LedgerNotOpen)?
            .update_settings(command)?;
        Ok(self.with_database_location(status))
    }
}

impl SqliteLedgerManager {
    fn with_database_location(&self, mut status: LedgerStatus) -> LedgerStatus {
        status.database_location = Some(self.database_path.to_string_lossy().into_owned());
        status
    }
}

impl EventTransactionPort for SqliteLedgerManager {
    fn commit_event(
        &mut self,
        prepared: &PreparedEventCommit,
    ) -> ApplicationResult<ProjectionWatermark> {
        self.store
            .as_mut()
            .ok_or(ApplicationError::LedgerNotOpen)?
            .commit_event(prepared, CommitFailpoint::None)
    }

    fn rebuild_cash_projection(&mut self) -> ApplicationResult<ProjectionWatermark> {
        self.store
            .as_mut()
            .ok_or(ApplicationError::LedgerNotOpen)?
            .rebuild_cash_projection()
    }

    fn canonical_posting_hash(&self) -> ApplicationResult<String> {
        self.store
            .as_ref()
            .ok_or(ApplicationError::LedgerNotOpen)?
            .canonical_posting_hash()
    }
}

fn empty_status(
    state: LedgerState,
    locale: UiLocale,
    blocked_reason: Option<&'static str>,
) -> LedgerStatus {
    LedgerStatus {
        state,
        ledger_id: None,
        schema_version: None,
        base_currency: None,
        ui_locale: locale,
        event_watermark: 0,
        projection_watermark: 0,
        calculation_version: CALCULATION_VERSION,
        blocked_reason,
        database_location: None,
        backup_protection_state: "not-configured".to_owned(),
        device_loss_protected: false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitFailpoint {
    None,
    AfterEvent,
    AfterDetail,
    AfterPosting,
    AfterAudit,
    AfterWatermark,
}

pub struct LedgerStore {
    pub(super) connection: Connection,
    pub(super) expense_cache: RefCell<Vec<(String, crate::application::cash::ExpenseAnalysis)>>,
}

impl LedgerStore {
    pub(super) fn initialize(
        mut connection: Connection,
        base_currency: Currency,
        ui_locale: UiLocale,
    ) -> ApplicationResult<Self> {
        let ledger_id = UuidV7::new()?;
        let transaction = connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .execute(
                "INSERT INTO ledger_metadata(singleton_id,ledger_id,created_at_utc,schema_created_by) VALUES(1,?1,CURRENT_TIMESTAMP,?2)",
                params![ledger_id.to_string(), env!("CARGO_PKG_VERSION")],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                "INSERT INTO app_settings(singleton_id,base_currency,ui_locale,valuation_defaults_json,updated_at_utc) VALUES(1,?1,?2,'{}',CURRENT_TIMESTAMP)",
                params![base_currency.as_str(), ui_locale.as_str()],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                "INSERT INTO projection_metadata(projection_name,projection_version,calculation_version,event_watermark,available) VALUES(?1,?2,?3,0,1)",
                params![CASH_PROJECTION_NAME, CASH_PROJECTION_VERSION, CALCULATION_VERSION],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                "INSERT INTO projection_metadata(projection_name,projection_version,calculation_version,event_watermark,available) VALUES('holdings','holding-projection-v1',?1,0,1)",
                [CALCULATION_VERSION],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                "INSERT INTO projection_metadata(projection_name,projection_version,calculation_version,event_watermark,available) VALUES
                 ('monthly-cash-flow','monthly-cash-flow-projection-v1',?1,0,1),
                 ('cash-data-quality','cash-data-quality-projection-v1',?1,0,1),
                 ('expense-daily','expense-daily-projection-v1',?1,0,1)",
                [CALCULATION_VERSION],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                "INSERT INTO backup_status(singleton_id,protection_state,external_target_configured) VALUES(1,'not-configured',0)",
                [],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                "INSERT INTO migration_history(schema_version,applied_at_utc,application_version,schema_hash) VALUES(?1,CURRENT_TIMESTAMP,?2,?3)",
                params![SCHEMA_VERSION, env!("CARGO_PKG_VERSION"), schema_hash()],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        validate_schema(&connection)?;
        Ok(Self {
            connection,
            expense_cache: RefCell::new(Vec::new()),
        })
    }

    pub(super) fn from_open_connection(mut connection: Connection) -> ApplicationResult<Self> {
        validate_schema(&connection)?;
        let metadata_count: u32 = connection
            .query_row("SELECT COUNT(*) FROM ledger_metadata", [], |row| row.get(0))
            .map_err(|_| ApplicationError::SchemaValidationFailed)?;
        if metadata_count != 1 {
            return Err(ApplicationError::SchemaValidationFailed);
        }
        let persisted_schema_hash: String = connection
            .query_row(
                "SELECT schema_hash FROM migration_history WHERE schema_version=?1",
                [SCHEMA_VERSION],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::SchemaValidationFailed)?;
        if persisted_schema_hash != schema_hash() {
            return Err(ApplicationError::SchemaValidationFailed);
        }
        let investment_rebuilt = ensure_investment_projection(&mut connection)?;
        ensure_cash_projection(&mut connection, investment_rebuilt)?;
        Ok(Self {
            connection,
            expense_cache: RefCell::new(Vec::new()),
        })
    }

    pub(super) fn status(&self) -> ApplicationResult<LedgerStatus> {
        let (ledger_id, base_currency, ui_locale): (String, String, String) = self
            .connection
            .query_row(
                "SELECT m.ledger_id,s.base_currency,s.ui_locale FROM ledger_metadata m CROSS JOIN app_settings s WHERE m.singleton_id=1 AND s.singleton_id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|_| ApplicationError::SchemaValidationFailed)?;
        let backup_protection_state: String = self
            .connection
            .query_row(
                "SELECT protection_state FROM backup_status WHERE singleton_id=1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::SchemaValidationFailed)?;
        let event_watermark: i64 = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(event_order),0) FROM business_events",
                [],
                |row| row.get(0),
            )
            .map_err(|_| ApplicationError::SchemaValidationFailed)?;
        let projection_watermark: i64 = self
            .connection
            .query_row(
                "SELECT event_watermark FROM projection_metadata WHERE projection_name=?1 AND projection_version=?2 AND available=1 AND calculation_version=?3",
                params![CASH_PROJECTION_NAME, CASH_PROJECTION_VERSION, CALCULATION_VERSION],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ApplicationError::SchemaValidationFailed)?
            .unwrap_or(0);
        Ok(LedgerStatus {
            state: LedgerState::Open,
            ledger_id: Some(ledger_id),
            schema_version: Some(SCHEMA_VERSION),
            base_currency: Some(Currency::parse(&base_currency)?),
            ui_locale: UiLocale::parse(&ui_locale)
                .ok_or(ApplicationError::SchemaValidationFailed)?,
            event_watermark: u64::try_from(event_watermark)
                .map_err(|_| ApplicationError::SchemaValidationFailed)?,
            projection_watermark: u64::try_from(projection_watermark)
                .map_err(|_| ApplicationError::SchemaValidationFailed)?,
            calculation_version: CALCULATION_VERSION,
            blocked_reason: None,
            database_location: None,
            device_loss_protected: backup_protection_state == "protected",
            backup_protection_state,
        })
    }

    fn update_settings(
        &mut self,
        command: &UpdateLedgerSettingsCommand,
    ) -> ApplicationResult<LedgerStatus> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let base_currency = command.base_currency.map(|value| value.to_string());
        let valuation_defaults = command.valuation_defaults_json.as_deref();
        transaction
            .execute(
                "UPDATE app_settings SET base_currency=COALESCE(?1,base_currency), ui_locale=?2, valuation_defaults_json=COALESCE(?3,valuation_defaults_json), updated_at_utc=CURRENT_TIMESTAMP WHERE singleton_id=1",
                params![base_currency, command.ui_locale.as_str(), valuation_defaults],
            )
            .map_err(map_sqlite_error)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        self.status()
    }

    pub fn commit_event(
        &mut self,
        prepared: &PreparedEventCommit,
        failpoint: CommitFailpoint,
    ) -> ApplicationResult<ProjectionWatermark> {
        prepared.validate()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .execute(
                "INSERT INTO business_events(event_id,event_type,effective_date,sequence,status,revision,created_at_utc,calculation_version) VALUES(?1,?2,?3,?4,'posted',?5,?6,?7)",
                params![prepared.event_id.to_string(), prepared.event_type.as_str(), prepared.effective_date.as_str(), i64::try_from(prepared.sequence.get()).map_err(|_| ApplicationError::TransactionFailed)?, prepared.revision, prepared.created_at_utc, prepared.calculation_version.as_str()],
            )
            .map_err(map_sqlite_error)?;
        let event_watermark: i64 = transaction
            .query_row(
                "SELECT event_order FROM business_events WHERE event_id=?1",
                [prepared.event_id.to_string()],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        maybe_fail(failpoint, CommitFailpoint::AfterEvent)?;
        insert_income_expense_detail(&transaction, prepared)?;
        maybe_fail(failpoint, CommitFailpoint::AfterDetail)?;
        insert_postings(&transaction, &prepared.postings)?;
        maybe_fail(failpoint, CommitFailpoint::AfterPosting)?;
        transaction
            .execute(
                "INSERT INTO audit_events(audit_event_id,business_event_id,actor,action,entity_type,entity_id,entity_revision,occurred_at_utc,reason) VALUES(?1,?2,'local-user','post','business-event',?2,?3,?4,?5)",
                params![prepared.audit_event_id.to_string(), prepared.event_id.to_string(), prepared.revision, prepared.created_at_utc, prepared.audit_reason],
            )
            .map_err(map_sqlite_error)?;
        maybe_fail(failpoint, CommitFailpoint::AfterAudit)?;
        let watermark = ProjectionWatermark::new(
            u64::try_from(event_watermark).map_err(|_| ApplicationError::TransactionFailed)?,
        )?;
        super::cash_store::rebuild_cash_derived(&transaction, watermark.get())?;
        maybe_fail(failpoint, CommitFailpoint::AfterWatermark)?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        Ok(watermark)
    }

    pub fn rebuild_cash_projection(&mut self) -> ApplicationResult<ProjectionWatermark> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        let watermark = ProjectionWatermark::new(
            transaction
                .query_row(
                    "SELECT COALESCE(MAX(event_order),0) FROM business_events",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|_| ApplicationError::TransactionFailed)?
                .try_into()
                .map_err(|_| ApplicationError::TransactionFailed)?,
        )?;
        super::cash_store::rebuild_cash_derived(&transaction, watermark.get())?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        self.expense_cache.borrow_mut().clear();
        Ok(watermark)
    }

    pub fn canonical_posting_hash(&self) -> ApplicationResult<String> {
        canonical_postings_hash(&load_postings(&self.connection)?)
    }
}

fn ensure_cash_projection(
    connection: &mut Connection,
    force_rebuild: bool,
) -> ApplicationResult<()> {
    let event_watermark: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(event_order),0) FROM business_events",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let projection_state: Option<(String, String, i64, bool)> = connection
        .query_row(
            "SELECT projection_version,calculation_version,event_watermark,available FROM projection_metadata WHERE projection_name=?1",
            [CASH_PROJECTION_NAME],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let Some((projection_version, calculation_version, projection_watermark, available)) =
        projection_state
    else {
        return Err(ApplicationError::SchemaValidationFailed);
    };
    let cash_posting_count: i64 = connection
        .query_row(
            "SELECT COUNT(*)
             FROM ledger_postings p JOIN business_events e ON e.event_id=p.event_id
             WHERE p.posting_kind IN ('cash','opening-cash','cash-reversal','settlement-cash') AND e.event_type<>'Reversal'
               AND NOT EXISTS(SELECT 1 FROM business_events n WHERE n.supersedes_event_id=e.event_id)
               AND NOT EXISTS(SELECT 1 FROM business_events r WHERE r.reverses_event_id=e.event_id)",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let cash_projection_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM cash_balance_projection", [], |row| {
            row.get(0)
        })
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let derived_ready: bool = connection
        .query_row(
            "SELECT COUNT(*)=3 FROM projection_metadata WHERE projection_name IN ('monthly-cash-flow','cash-data-quality','expense-daily') AND calculation_version=?1 AND event_watermark=?2 AND available=1",
            params![CALCULATION_VERSION,event_watermark],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let needs_rebuild = force_rebuild
        || projection_version != CASH_PROJECTION_VERSION
        || calculation_version != CALCULATION_VERSION
        || projection_watermark != event_watermark
        || !available
        || cash_posting_count > 0 && cash_projection_count == 0
        || !derived_ready;
    if needs_rebuild {
        let transaction = connection
            .transaction()
            .map_err(|_| ApplicationError::TransactionFailed)?;
        transaction
            .execute(
                "UPDATE projection_metadata SET available=0 WHERE projection_name IN ('cash-balance','monthly-cash-flow','cash-data-quality','expense-daily')",
                [],
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        super::cash_store::rebuild_cash_derived(
            &transaction,
            u64::try_from(event_watermark).map_err(|_| ApplicationError::TransactionFailed)?,
        )?;
        transaction
            .commit()
            .map_err(|_| ApplicationError::TransactionFailed)?;
    }
    Ok(())
}

fn ensure_investment_projection(connection: &mut Connection) -> ApplicationResult<bool> {
    let event_watermark: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(event_order),0) FROM business_events",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let state: Option<(String, String, i64, bool)> = connection
        .query_row(
            "SELECT projection_version,calculation_version,event_watermark,available FROM projection_metadata WHERE projection_name='holdings'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let Some((version, calculation, watermark, available)) = state else {
        return Err(ApplicationError::SchemaValidationFailed);
    };
    let event_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM business_events WHERE status='posted' AND event_type IN ('SecurityTrade','Dividend','InvestmentExpense','OpeningPosition','OpeningPerformance')",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let projection_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM holding_projection", [], |row| {
            row.get(0)
        })
        .map_err(|_| ApplicationError::SchemaValidationFailed)?;
    let needs_rebuild = version != super::investment_store::HOLDING_PROJECTION_VERSION
        || calculation != CALCULATION_VERSION
        || watermark != event_watermark
        || !available
        || event_count > 0 && projection_count == 0;
    if !needs_rebuild {
        return Ok(false);
    }
    let transaction = connection
        .transaction()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    transaction
        .execute(
            "UPDATE projection_metadata SET available=0 WHERE projection_name='holdings'",
            [],
        )
        .map_err(|_| ApplicationError::TransactionFailed)?;
    super::investment_store::rebuild_investment_derived(
        &transaction,
        u64::try_from(event_watermark).map_err(|_| ApplicationError::TransactionFailed)?,
    )?;
    transaction
        .commit()
        .map_err(|_| ApplicationError::TransactionFailed)?;
    Ok(true)
}

fn insert_income_expense_detail(
    transaction: &Transaction<'_>,
    prepared: &PreparedEventCommit,
) -> ApplicationResult<()> {
    transaction
        .execute(
            "INSERT INTO income_expense_details(event_id,account_id,entry_type,category_id,amount,semantic_role) VALUES(?1,?2,?3,?4,?5,?6)",
            params![prepared.event_id.to_string(), prepared.detail.account_id.to_string(), prepared.detail.kind.as_str(), prepared.detail.category_id.map(|value| value.to_string()), prepared.detail.amount.as_str(), prepared.detail.semantic_role],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn insert_postings(
    transaction: &Transaction<'_>,
    postings: &[LedgerPosting],
) -> ApplicationResult<()> {
    for (index, posting) in postings.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO ledger_postings(posting_id,event_id,posting_ordinal,posting_kind,account_id,portfolio_id,instrument_id,quantity_delta,currency,base_value,base_currency,calculation_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![posting.posting_id.to_string(), posting.event_id.to_string(), i64::try_from(index + 1).map_err(|_| ApplicationError::TransactionFailed)?, posting.posting_kind.as_str(), posting.account_id.map(|value| value.to_string()), posting.portfolio_id.map(|value| value.to_string()), posting.instrument_id.map(|value| value.to_string()), posting.quantity_delta.as_str(), posting.currency.as_str(), posting.base_value.as_ref().map(Decimal::as_str), posting.base_currency.as_str(), posting.calculation_version.as_str()],
            )
            .map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn load_postings(connection: &Connection) -> ApplicationResult<Vec<LedgerPosting>> {
    let rows = {
        let mut statement = connection
            .prepare(
                "SELECT p.posting_id,p.event_id,e.effective_date,e.sequence,p.posting_kind,p.account_id,p.portfolio_id,p.instrument_id,p.quantity_delta,p.currency,p.base_value,p.base_currency,p.calculation_version
                 FROM ledger_postings p JOIN business_events e ON e.event_id=p.event_id",
            )
            .map_err(|_| ApplicationError::TransactionFailed)?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                ))
            })
            .map_err(|_| ApplicationError::TransactionFailed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ApplicationError::TransactionFailed)?
    };
    rows.into_iter()
        .map(|row| {
            Ok(LedgerPosting {
                posting_id: UuidV7::parse(&row.0)?,
                event_id: UuidV7::parse(&row.1)?,
                effective_date: LocalDate::parse(&row.2)?,
                sequence: Sequence::new(
                    u64::try_from(row.3).map_err(|_| ApplicationError::TransactionFailed)?,
                )?,
                posting_kind: PostingKind::parse(&row.4)?,
                account_id: row.5.as_deref().map(UuidV7::parse).transpose()?,
                portfolio_id: row.6.as_deref().map(UuidV7::parse).transpose()?,
                instrument_id: row.7.as_deref().map(UuidV7::parse).transpose()?,
                quantity_delta: Decimal::parse(&row.8, DecimalUse::Internal)?,
                currency: Currency::parse(&row.9)?,
                base_value: row
                    .10
                    .as_deref()
                    .map(|value| Decimal::parse(value, DecimalUse::Internal))
                    .transpose()?,
                base_currency: Currency::parse(&row.11)?,
                calculation_version: CalculationVersion::parse(&row.12)?,
            })
        })
        .collect()
}

fn maybe_fail(actual: CommitFailpoint, expected: CommitFailpoint) -> ApplicationResult<()> {
    if actual == expected {
        Err(ApplicationError::TransactionFailed)
    } else {
        Ok(())
    }
}

pub(super) fn map_sqlite_error(error: rusqlite::Error) -> ApplicationError {
    let rusqlite::Error::SqliteFailure(_, Some(message)) = error else {
        return ApplicationError::TransactionFailed;
    };
    if message.contains("BASE_CURRENCY_FROZEN") {
        DomainError::BaseCurrencyFrozen.into()
    } else if message.contains("CASH_ACCOUNT_CURRENCY_FROZEN") {
        DomainError::CashAccountCurrencyFrozen.into()
    } else if message.contains("INSTRUMENT_TRADE_CURRENCY_FROZEN") {
        DomainError::InstrumentCurrencyFrozen.into()
    } else if message.contains("UNIQUE constraint failed") {
        ApplicationError::CatalogDuplicate
    } else if message.contains("FOREIGN KEY constraint failed") {
        ApplicationError::CatalogReferenceInvalid
    } else {
        ApplicationError::TransactionFailed
    }
}

#[cfg(test)]
mod tests {
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::application::ledger::{BusinessEventType, IncomeExpenseDetail, IncomeExpenseKind};

    fn id(seed: u8) -> UuidV7 {
        UuidV7::from_parts(1_777_000_000_000 + u64::from(seed), [seed; 10]).unwrap()
    }

    fn create_store() -> (TempDir, LedgerStore, UuidV7) {
        let directory = tempdir().unwrap();
        let path = directory.path().join(LEDGER_FILENAME);
        let connection = MigrationRunner::create_new(&path).unwrap();
        let store =
            LedgerStore::initialize(connection, Currency::parse("CNY").unwrap(), UiLocale::EnUs)
                .unwrap();
        let account_id = id(1);
        store
            .connection
            .execute(
                "INSERT INTO institutions(institution_id,business_id,name,region,institution_type,enabled,created_at_utc,updated_at_utc) VALUES(?1,'bank','Synthetic Bank','CN','bank',1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
                [id(2).to_string()],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO cash_accounts(account_id,business_id,institution_id,name,purpose,currency,enabled,created_at_utc,updated_at_utc) VALUES(?1,'cash',?2,'Synthetic Cash','daily','CNY',1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
                params![account_id.to_string(), id(2).to_string()],
            )
            .unwrap();
        (directory, store, account_id)
    }

    fn prepared_event(
        seed: u8,
        sequence: u64,
        account_id: UuidV7,
        signed: &str,
    ) -> PreparedEventCommit {
        let event_id = id(seed);
        let event_type = if signed.starts_with('-') {
            BusinessEventType::Expense
        } else {
            BusinessEventType::Income
        };
        let detail_kind = if signed.starts_with('-') {
            IncomeExpenseKind::Expense
        } else {
            IncomeExpenseKind::Income
        };
        let unsigned = signed.strip_prefix('-').unwrap_or(signed);
        PreparedEventCommit {
            event_id,
            event_type,
            effective_date: LocalDate::parse("2026-09-02").unwrap(),
            sequence: Sequence::new(sequence).unwrap(),
            revision: 1,
            created_at_utc: "2026-09-02T00:00:00Z".to_owned(),
            calculation_version: CalculationVersion::parse(CALCULATION_VERSION).unwrap(),
            detail: IncomeExpenseDetail {
                account_id,
                kind: detail_kind,
                category_id: None,
                amount: Decimal::parse(unsigned, DecimalUse::Amount).unwrap(),
                semantic_role: "normal",
            },
            postings: vec![LedgerPosting {
                posting_id: id(seed + 1),
                event_id,
                effective_date: LocalDate::parse("2026-09-02").unwrap(),
                sequence: Sequence::new(sequence).unwrap(),
                posting_kind: PostingKind::Cash,
                account_id: Some(account_id),
                portfolio_id: None,
                instrument_id: None,
                quantity_delta: Decimal::parse(signed, DecimalUse::Amount).unwrap(),
                currency: Currency::parse("CNY").unwrap(),
                base_value: Some(Decimal::parse(signed, DecimalUse::Amount).unwrap()),
                base_currency: Currency::parse("CNY").unwrap(),
                calculation_version: CalculationVersion::parse(CALCULATION_VERSION).unwrap(),
            }],
            audit_event_id: id(seed + 2),
            audit_reason: None,
        }
    }

    #[test]
    fn every_transaction_failpoint_rolls_back_event_detail_posting_audit_and_watermark() {
        for failpoint in [
            CommitFailpoint::AfterEvent,
            CommitFailpoint::AfterDetail,
            CommitFailpoint::AfterPosting,
            CommitFailpoint::AfterAudit,
            CommitFailpoint::AfterWatermark,
        ] {
            let (_directory, mut store, account_id) = create_store();
            let prepared = prepared_event(10, 1, account_id, "10.00");
            assert_eq!(
                store.commit_event(&prepared, failpoint),
                Err(ApplicationError::TransactionFailed)
            );
            let counts: (u32, u32, u32, u32, u32) = store
                .connection
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM business_events),(SELECT COUNT(*) FROM income_expense_details),(SELECT COUNT(*) FROM ledger_postings),(SELECT COUNT(*) FROM audit_events),(SELECT COUNT(*) FROM cash_balance_projection)",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )
                .unwrap();
            let watermark: u32 = store
                .connection
                .query_row(
                    "SELECT event_watermark FROM projection_metadata WHERE projection_name=?1",
                    [CASH_PROJECTION_NAME],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(counts, (0, 0, 0, 0, 0));
            assert_eq!(watermark, 0);
        }
    }

    #[test]
    fn posting_hash_ignores_sqlite_physical_row_order() {
        let (_directory, mut store, account_id) = create_store();
        store
            .commit_event(
                &prepared_event(10, 1, account_id, "10.00"),
                CommitFailpoint::None,
            )
            .unwrap();
        store
            .commit_event(
                &prepared_event(20, 2, account_id, "-3.00"),
                CommitFailpoint::None,
            )
            .unwrap();
        let before = store.canonical_posting_hash().unwrap();
        store
            .connection
            .execute_batch(
                "CREATE TEMP TABLE reordered_postings AS SELECT * FROM ledger_postings;
                 DELETE FROM ledger_postings;
                 INSERT INTO ledger_postings SELECT * FROM reordered_postings ORDER BY posting_id DESC;
                 DROP TABLE reordered_postings;",
            )
            .unwrap();
        assert_eq!(store.canonical_posting_hash().unwrap(), before);
    }

    #[test]
    fn deleted_cash_projection_rebuilds_to_identical_rows_and_watermark() {
        let (_directory, mut store, account_id) = create_store();
        store
            .commit_event(
                &prepared_event(10, 1, account_id, "10.00"),
                CommitFailpoint::None,
            )
            .unwrap();
        store
            .commit_event(
                &prepared_event(20, 2, account_id, "-3.00"),
                CommitFailpoint::None,
            )
            .unwrap();
        let before: (String, String, u64, String) = store
            .connection
            .query_row(
                "SELECT balance,currency,event_watermark,calculation_version FROM cash_balance_projection WHERE account_id=?1",
                [account_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)?.try_into().unwrap(), row.get(3)?)),
            )
            .unwrap();
        store
            .connection
            .execute("DELETE FROM cash_balance_projection", [])
            .unwrap();
        let rebuilt = store.rebuild_cash_projection().unwrap();
        let after: (String, String, u64, String) = store
            .connection
            .query_row(
                "SELECT balance,currency,event_watermark,calculation_version FROM cash_balance_projection WHERE account_id=?1",
                [account_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)?.try_into().unwrap(), row.get(3)?)),
            )
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(rebuilt.get(), 2);
        assert_eq!(after.0, "7.00");
    }

    #[test]
    fn opening_rebuilds_an_unavailable_projection_before_normal_use() {
        let (directory, mut store, account_id) = create_store();
        store
            .commit_event(
                &prepared_event(10, 1, account_id, "10.00"),
                CommitFailpoint::None,
            )
            .unwrap();
        drop(store);
        let path = directory.path().join(LEDGER_FILENAME);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "DELETE FROM cash_balance_projection;
                 UPDATE projection_metadata SET available=0,event_watermark=0 WHERE projection_name='cash-balance';",
            )
            .unwrap();
        drop(connection);

        let mut backup = VerifiedSqliteMigrationBackup::new(directory.path().join("backups"));
        let connection = MigrationRunner::open_existing(&path, &mut backup).unwrap();
        let reopened = LedgerStore::from_open_connection(connection).unwrap();
        let rebuilt: (String, i64, bool) = reopened
            .connection
            .query_row(
                "SELECT p.balance,m.event_watermark,m.available FROM cash_balance_projection p CROSS JOIN projection_metadata m WHERE p.account_id=?1 AND m.projection_name='cash-balance'",
                [account_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(rebuilt, ("10.00".to_owned(), 1, true));
    }

    #[test]
    fn dependent_base_and_transaction_currencies_are_frozen() {
        let (_directory, mut store, account_id) = create_store();
        let settings = UpdateLedgerSettingsCommand {
            base_currency: Some(Currency::parse("USD").unwrap()),
            ui_locale: UiLocale::EnUs,
            valuation_defaults_json: None,
        };
        assert_eq!(
            store.update_settings(&settings),
            Err(ApplicationError::Domain(DomainError::BaseCurrencyFrozen))
        );

        store
            .connection
            .execute(
                "UPDATE cash_accounts SET currency='USD' WHERE account_id=?1",
                [account_id.to_string()],
            )
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE cash_accounts SET currency='CNY' WHERE account_id=?1",
                [account_id.to_string()],
            )
            .unwrap();
        store
            .commit_event(
                &prepared_event(10, 1, account_id, "10.00"),
                CommitFailpoint::None,
            )
            .unwrap();
        let error = store
            .connection
            .execute(
                "UPDATE cash_accounts SET currency='USD' WHERE account_id=?1",
                [account_id.to_string()],
            )
            .unwrap_err();
        assert_eq!(
            map_sqlite_error(error),
            ApplicationError::Domain(DomainError::CashAccountCurrencyFrozen)
        );

        let portfolio_id = id(30);
        let instrument_id = id(31);
        let trade_event_id = id(32);
        store
            .connection
            .execute(
                "INSERT INTO portfolios(portfolio_id,business_id,settlement_account_id,name,portfolio_type,enabled,created_at_utc,updated_at_utc) VALUES(?1,'portfolio',?2,'Synthetic Portfolio','brokerage',1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
                params![portfolio_id.to_string(), account_id.to_string()],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO security_instruments(instrument_id,business_id,code,name,trade_currency,enabled,created_at_utc,updated_at_utc) VALUES(?1,'instrument','SYN','Synthetic Instrument','CNY',1,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
                [instrument_id.to_string()],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO business_events(event_id,event_type,effective_date,sequence,status,revision,created_at_utc,calculation_version) VALUES(?1,'SecurityTrade','2026-09-03',1,'posted',1,CURRENT_TIMESTAMP,?2)",
                params![trade_event_id.to_string(), CALCULATION_VERSION],
            )
            .unwrap();
        store
            .connection
            .execute(
                "INSERT INTO security_trade_details(event_id,trade_type,portfolio_id,instrument_id,settlement_account_id,quantity,unit_price,trade_fee) VALUES(?1,'BUY',?2,?3,?4,'1','10','0')",
                params![trade_event_id.to_string(), portfolio_id.to_string(), instrument_id.to_string(), account_id.to_string()],
            )
            .unwrap();
        let error = store
            .connection
            .execute(
                "UPDATE security_instruments SET trade_currency='USD' WHERE instrument_id=?1",
                [instrument_id.to_string()],
            )
            .unwrap_err();
        assert_eq!(
            map_sqlite_error(error),
            ApplicationError::Domain(DomainError::InstrumentCurrencyFrozen)
        );
    }

    #[test]
    fn manager_uses_only_fixed_local_data_path_for_foundation_operations() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("LedgerKit");
        let mut manager = SqliteLedgerManager::new(&root).unwrap();
        assert_eq!(
            manager.get_ledger_status(UiLocale::EnUs).unwrap().state,
            LedgerState::NotCreated
        );
        let created = manager
            .create_ledger(CreateLedgerCommand {
                base_currency: Currency::parse("CNY").unwrap(),
                ui_locale: UiLocale::EnUs,
            })
            .unwrap();
        assert_eq!(created.state, LedgerState::Open);
        assert!(root.join(LEDGER_FILENAME).is_file());
        drop(manager);

        let mut reopened = SqliteLedgerManager::new(&root).unwrap();
        assert_eq!(
            reopened.get_ledger_status(UiLocale::ZhCn).unwrap().state,
            LedgerState::Closed
        );
        assert_eq!(reopened.open_ledger().unwrap().state, LedgerState::Open);
        let updated = reopened
            .update_settings(&UpdateLedgerSettingsCommand {
                base_currency: Some(Currency::parse("USD").unwrap()),
                ui_locale: UiLocale::ZhCn,
                valuation_defaults_json: Some("{\"stalePriceDays\":7}".to_owned()),
            })
            .unwrap();
        assert_eq!(updated.base_currency.unwrap().as_str(), "USD");
        assert_eq!(updated.ui_locale, UiLocale::ZhCn);
    }
}
