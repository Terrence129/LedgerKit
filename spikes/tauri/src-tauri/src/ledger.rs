use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::canonical::canonical_hash;
use crate::decimal::{parse_stored_decimal, validate_positive_amount};
use crate::error::{SpikeError, SpikeResult};

pub const SCHEMA_VERSION: i64 = 2;
pub const CALCULATION_VERSION: &str = "ledger-calculation-v1";
pub const PROJECTION_VERSION: &str = "projection-v1";

struct DecimalSum;

impl Aggregate<Decimal, String> for DecimalSum {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<Decimal> {
        Ok(Decimal::ZERO)
    }

    fn step(&self, context: &mut Context<'_>, sum: &mut Decimal) -> rusqlite::Result<()> {
        let amount_text = context.get::<String>(0)?;
        let amount = parse_stored_decimal(&amount_text)
            .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
        *sum += amount;
        Ok(())
    }

    fn finalize(&self, _: &mut Context<'_>, sum: Option<Decimal>) -> rusqlite::Result<String> {
        Ok(sum.unwrap_or(Decimal::ZERO).to_string())
    }
}

fn register_decimal_functions(connection: &Connection) -> SpikeResult<()> {
    connection.create_aggregate_function(
        "ledgerkit_decimal_sum",
        1,
        FunctionFlags::SQLITE_UTF8
            | FunctionFlags::SQLITE_DETERMINISTIC
            | FunctionFlags::SQLITE_INNOCUOUS,
        DecimalSum,
    )?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostEventRequest {
    pub event_type: String,
    pub effective_date: String,
    pub account_id: String,
    pub amount: String,
    pub currency: String,
    pub category_id: Option<String>,
    #[serde(default)]
    pub currency_precision_confirmed: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Posting {
    pub posting_id: String,
    pub event_id: String,
    pub posting_kind: String,
    pub calculation_version: String,
    pub account_id: String,
    pub quantity_delta: String,
    pub currency: String,
    pub base_value: String,
    pub base_currency: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    pub event_id: String,
    pub event_type: String,
    pub effective_date: String,
    pub sequence: u32,
    pub account_id: String,
    pub amount: String,
    pub signed_amount: String,
    pub currency: String,
    pub category_id: Option<String>,
    pub category_label: Option<String>,
    pub note: Option<String>,
    pub event_watermark: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PostEventResponse {
    pub event: EventRecord,
    pub posting: Posting,
    pub account_balance: String,
    pub projection_watermark: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPage {
    pub items: Vec<EventRecord>,
    pub page: u32,
    pub page_size: u32,
    pub total_count: u64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub base_currency: String,
    pub net_worth: String,
    pub cash_value: String,
    pub security_value: String,
    pub valued_ratio_percent: u8,
    pub event_watermark: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStatus {
    pub schema_version: i64,
    pub sqlite_version: String,
    pub event_watermark: u64,
    pub projection_watermark: u64,
    pub database_bytes: u64,
    pub calculation_version: String,
    pub default_network_enabled: bool,
}

pub struct LedgerStore {
    path: PathBuf,
    connection: Mutex<Connection>,
}

impl LedgerStore {
    pub fn open(path: impl AsRef<Path>) -> SpikeResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let existed = path.exists();
        let mut connection = Connection::open(&path)?;
        register_decimal_functions(&connection)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )?;
        let schema_version: i64 =
            connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if schema_version == 0 {
            if existed {
                let backup_path = path.with_extension("pre-migration.sqlite");
                connection.backup(MAIN_DB, &backup_path, None)?;
                verify_database_file(&backup_path)?;
            }
            apply_schema_v1(&mut connection)?;
        } else if schema_version == 1 {
            let backup_path = path.with_extension("pre-migration.sqlite");
            connection.backup(MAIN_DB, &backup_path, None)?;
            verify_database_file(&backup_path)?;
            apply_schema_v2(&mut connection)?;
        } else if schema_version != SCHEMA_VERSION {
            return Err(SpikeError::Database(rusqlite::Error::InvalidQuery));
        }
        verify_connection(&connection)?;

        Ok(Self {
            path,
            connection: Mutex::new(connection),
        })
    }

    pub fn initialize_demo(&self) -> SpikeResult<()> {
        let mut connection = self.lock()?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM business_events", [], |row| row.get(0))?;
        if count != 0 {
            return Ok(());
        }
        seed_expense_fixture_on_connection(&mut connection)?;
        let request = PostEventRequest {
            event_type: "Income".to_owned(),
            effective_date: "2026-02-01".to_owned(),
            account_id: "cash-cny-1".to_owned(),
            amount: "2000".to_owned(),
            currency: "CNY".to_owned(),
            category_id: Some("cat-income".to_owned()),
            currency_precision_confirmed: false,
            note: Some("Synthetic opening income for the disposable spike".to_owned()),
        };
        post_event_on_connection(&mut connection, &request, None, false)?;
        Ok(())
    }

    pub fn status(&self) -> SpikeResult<LedgerStatus> {
        let connection = self.lock()?;
        let schema_version = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let sqlite_version =
            connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
        let event_watermark = event_watermark(&connection)?;
        let projection_watermark = projection_watermark(&connection)?;
        let database_bytes = std::fs::metadata(&self.path).map_or(0, |metadata| metadata.len());
        Ok(LedgerStatus {
            schema_version,
            sqlite_version,
            event_watermark,
            projection_watermark,
            database_bytes,
            calculation_version: CALCULATION_VERSION.to_owned(),
            default_network_enabled: false,
        })
    }

    pub fn post_event(&self, request: &PostEventRequest) -> SpikeResult<PostEventResponse> {
        let mut connection = self.lock()?;
        post_event_on_connection(&mut connection, request, None, false)
    }

    pub fn activity(&self, page: u32, page_size: u32) -> SpikeResult<ActivityPage> {
        if page == 0 || page_size == 0 || page_size > 50 {
            return Err(SpikeError::PageInvalid);
        }
        let connection = self.lock()?;
        activity_on_connection(&connection, page, page_size)
    }

    pub fn all_activity(&self) -> SpikeResult<Vec<EventRecord>> {
        let connection = self.lock()?;
        let total_count: i64 =
            connection.query_row("SELECT COUNT(*) FROM business_events", [], |row| row.get(0))?;
        let mut events = Vec::with_capacity(total_count as usize);
        let mut page = 1;
        loop {
            let result = activity_on_connection(&connection, page, 50)?;
            events.extend(result.items);
            if !result.has_more {
                break;
            }
            page += 1;
        }
        Ok(events)
    }

    pub fn overview(&self) -> SpikeResult<Overview> {
        let connection = self.lock()?;
        let cash_value: String = connection
            .query_row(
                "SELECT balance FROM cash_balance_projection WHERE account_id = 'cash-cny-1'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "0".to_owned());
        Ok(Overview {
            base_currency: "CNY".to_owned(),
            net_worth: cash_value.clone(),
            cash_value,
            security_value: "0".to_owned(),
            valued_ratio_percent: 100,
            event_watermark: event_watermark(&connection)?,
        })
    }

    pub fn expense_analysis(&self, start_date: &str, end_date: &str) -> SpikeResult<Value> {
        validate_date(start_date)?;
        validate_date(end_date)?;
        if start_date > end_date {
            return Err(SpikeError::DateInvalid);
        }
        let connection = self.lock()?;
        expense_analysis_on_connection(&connection, start_date, end_date)
    }

    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> SpikeResult<T>,
    ) -> SpikeResult<T> {
        let mut connection = self.lock()?;
        operation(&mut connection)
    }

    pub fn database_path(&self) -> &Path {
        &self.path
    }

    pub fn rebuild_expense_projection(&self) -> SpikeResult<()> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        rebuild_expense_projection_on_connection(&transaction)?;
        transaction.commit()?;
        Ok(())
    }

    fn lock(&self) -> SpikeResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| SpikeError::InternalState)
    }
}

fn apply_schema_v1(connection: &mut Connection) -> SpikeResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        CREATE TABLE business_events (
            event_order INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL UNIQUE,
            event_type TEXT NOT NULL CHECK (event_type IN ('Income', 'Expense')),
            effective_date TEXT NOT NULL,
            sequence INTEGER NOT NULL CHECK (sequence > 0),
            account_id TEXT NOT NULL,
            amount TEXT NOT NULL,
            signed_amount TEXT NOT NULL,
            currency TEXT NOT NULL,
            category_id TEXT,
            note TEXT,
            calculation_version TEXT NOT NULL,
            created_at_utc TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE (effective_date, sequence, event_id)
        );
        CREATE TABLE ledger_postings (
            posting_id TEXT PRIMARY KEY,
            event_id TEXT NOT NULL UNIQUE REFERENCES business_events(event_id),
            posting_kind TEXT NOT NULL,
            account_id TEXT NOT NULL,
            quantity_delta TEXT NOT NULL,
            currency TEXT NOT NULL,
            base_value TEXT NOT NULL,
            base_currency TEXT NOT NULL,
            calculation_version TEXT NOT NULL
        );
        CREATE TABLE cash_balance_projection (
            account_id TEXT PRIMARY KEY,
            balance TEXT NOT NULL,
            currency TEXT NOT NULL,
            event_watermark INTEGER NOT NULL,
            calculation_version TEXT NOT NULL
        );
        CREATE TABLE expense_daily_projection (
            effective_date TEXT NOT NULL,
            category_id TEXT NOT NULL,
            amount TEXT NOT NULL,
            distinct_event_count INTEGER NOT NULL CHECK (distinct_event_count >= 0),
            event_watermark INTEGER NOT NULL,
            calculation_version TEXT NOT NULL,
            PRIMARY KEY (effective_date, category_id)
        );
        CREATE TABLE categories (
            category_id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1))
        );
        CREATE TABLE projection_state (
            projection_name TEXT PRIMARY KEY,
            event_watermark INTEGER NOT NULL,
            calculation_version TEXT NOT NULL
        );
        CREATE TABLE app_metadata (
            metadata_key TEXT PRIMARY KEY,
            metadata_value TEXT NOT NULL
        );
        CREATE INDEX idx_business_events_activity
            ON business_events(effective_date DESC, sequence DESC, event_id DESC);
        CREATE INDEX idx_business_events_expense
            ON business_events(event_type, effective_date, category_id, amount);
        CREATE INDEX idx_expense_daily_projection_range
            ON expense_daily_projection(effective_date, category_id, amount, distinct_event_count);
        INSERT INTO projection_state(projection_name, event_watermark, calculation_version)
            VALUES ('cash-balances', 0, 'ledger-calculation-v1');
        INSERT INTO projection_state(projection_name, event_watermark, calculation_version)
            VALUES ('expense-daily', 0, 'ledger-calculation-v1');
        INSERT INTO app_metadata(metadata_key, metadata_value)
            VALUES ('master_data_watermark', '0');
        PRAGMA user_version = 2;
        "#,
    )?;
    transaction.commit()?;
    Ok(())
}

fn apply_schema_v2(connection: &mut Connection) -> SpikeResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        r#"
        CREATE TABLE expense_daily_projection (
            effective_date TEXT NOT NULL,
            category_id TEXT NOT NULL,
            amount TEXT NOT NULL,
            distinct_event_count INTEGER NOT NULL CHECK (distinct_event_count >= 0),
            event_watermark INTEGER NOT NULL,
            calculation_version TEXT NOT NULL,
            PRIMARY KEY (effective_date, category_id)
        );
        CREATE INDEX idx_expense_daily_projection_range
            ON expense_daily_projection(effective_date, category_id, amount, distinct_event_count);
        INSERT INTO projection_state(projection_name, event_watermark, calculation_version)
            VALUES ('expense-daily', 0, 'ledger-calculation-v1');
        "#,
    )?;
    rebuild_expense_projection_on_connection(&transaction)?;
    transaction.execute_batch("PRAGMA user_version = 2;")?;
    transaction.commit()?;
    Ok(())
}

fn rebuild_expense_projection_on_connection(connection: &Connection) -> SpikeResult<()> {
    connection.execute_batch(
        r#"
        DELETE FROM expense_daily_projection;
        INSERT INTO expense_daily_projection(
            effective_date, category_id, amount, distinct_event_count,
            event_watermark, calculation_version
        )
        SELECT effective_date, category_id, ledgerkit_decimal_sum(amount), COUNT(*),
               MAX(event_order), 'ledger-calculation-v1'
        FROM business_events
        WHERE event_type = 'Expense'
        GROUP BY effective_date, category_id;
        UPDATE projection_state
        SET event_watermark = COALESCE(
                (SELECT MAX(event_order) FROM business_events WHERE event_type = 'Expense'),
                0
            ),
            calculation_version = 'ledger-calculation-v1'
        WHERE projection_name = 'expense-daily';
        "#,
    )?;
    Ok(())
}

fn post_event_on_connection(
    connection: &mut Connection,
    request: &PostEventRequest,
    explicit_event_id: Option<&str>,
    fail_after_posting: bool,
) -> SpikeResult<PostEventResponse> {
    validate_date(&request.effective_date)?;
    if request.currency != "CNY" || request.account_id != "cash-cny-1" {
        return Err(SpikeError::EventTypeUnsupported);
    }
    if request
        .note
        .as_ref()
        .is_some_and(|note| note.chars().count() > 200)
    {
        return Err(SpikeError::EventTypeUnsupported);
    }
    let amount = validate_positive_amount(&request.amount, request.currency_precision_confirmed)?;
    let signed = match request.event_type.as_str() {
        "Income" => amount.value,
        "Expense" => {
            if request.category_id.is_none() {
                return Err(SpikeError::EventTypeUnsupported);
            }
            -amount.value
        }
        _ => return Err(SpikeError::EventTypeUnsupported),
    };
    let signed_text = signed.to_string();

    let transaction = connection.transaction()?;
    let next_order: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(event_order), 0) + 1 FROM business_events",
        [],
        |row| row.get(0),
    )?;
    let event_id = explicit_event_id
        .map(str::to_owned)
        .unwrap_or_else(|| format!("evt-spike-{next_order:06}"));
    let sequence: u32 = transaction.query_row(
        "SELECT COALESCE(MAX(sequence), 0) + 1 FROM business_events WHERE effective_date = ?1",
        [&request.effective_date],
        |row| row.get(0),
    )?;
    transaction.execute(
        "INSERT INTO business_events(
            event_id, event_type, effective_date, sequence, account_id, amount,
            signed_amount, currency, category_id, note, calculation_version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            event_id,
            request.event_type,
            request.effective_date,
            sequence,
            request.account_id,
            amount.text,
            signed_text,
            request.currency,
            request.category_id,
            request.note,
            CALCULATION_VERSION,
        ],
    )?;

    let posting = Posting {
        posting_id: format!("post-{event_id}-01"),
        event_id: event_id.clone(),
        posting_kind: "cash".to_owned(),
        calculation_version: CALCULATION_VERSION.to_owned(),
        account_id: request.account_id.clone(),
        quantity_delta: signed_text.clone(),
        currency: request.currency.clone(),
        base_value: signed_text.clone(),
        base_currency: "CNY".to_owned(),
    };
    transaction.execute(
        "INSERT INTO ledger_postings(
            posting_id, event_id, posting_kind, account_id, quantity_delta,
            currency, base_value, base_currency, calculation_version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            posting.posting_id,
            posting.event_id,
            posting.posting_kind,
            posting.account_id,
            posting.quantity_delta,
            posting.currency,
            posting.base_value,
            posting.base_currency,
            posting.calculation_version,
        ],
    )?;
    if fail_after_posting {
        return Err(SpikeError::SyntheticFailpoint);
    }

    if request.event_type == "Expense" {
        let category_id = request
            .category_id
            .as_deref()
            .ok_or(SpikeError::EventTypeUnsupported)?;
        let existing: Option<(String, i64)> = transaction
            .query_row(
                "SELECT amount, distinct_event_count
                 FROM expense_daily_projection
                 WHERE effective_date = ?1 AND category_id = ?2",
                params![request.effective_date, category_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (existing_amount, existing_count) = existing.unwrap_or_else(|| ("0".to_owned(), 0));
        let projected_amount = parse_stored_decimal(&existing_amount)? + amount.value;
        transaction.execute(
            "INSERT INTO expense_daily_projection(
                effective_date, category_id, amount, distinct_event_count,
                event_watermark, calculation_version
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(effective_date, category_id) DO UPDATE SET
                amount = excluded.amount,
                distinct_event_count = excluded.distinct_event_count,
                event_watermark = excluded.event_watermark,
                calculation_version = excluded.calculation_version",
            params![
                request.effective_date,
                category_id,
                projected_amount.to_string(),
                existing_count + 1,
                next_order,
                CALCULATION_VERSION,
            ],
        )?;
        transaction.execute(
            "UPDATE projection_state
             SET event_watermark = ?1, calculation_version = ?2
             WHERE projection_name = 'expense-daily'",
            params![next_order, CALCULATION_VERSION],
        )?;
    }

    let existing_balance: String = transaction
        .query_row(
            "SELECT balance FROM cash_balance_projection WHERE account_id = ?1",
            [&request.account_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "0".to_owned());
    let new_balance = parse_stored_decimal(&existing_balance)? + signed;
    transaction.execute(
        "INSERT INTO cash_balance_projection(
            account_id, balance, currency, event_watermark, calculation_version
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(account_id) DO UPDATE SET
            balance = excluded.balance,
            event_watermark = excluded.event_watermark,
            calculation_version = excluded.calculation_version",
        params![
            request.account_id,
            new_balance.to_string(),
            request.currency,
            next_order,
            CALCULATION_VERSION,
        ],
    )?;
    transaction.execute(
        "UPDATE projection_state SET event_watermark = ?1, calculation_version = ?2
         WHERE projection_name = 'cash-balances'",
        params![next_order, CALCULATION_VERSION],
    )?;
    transaction.commit()?;

    let category_label = request.category_id.as_ref().and_then(|category_id| {
        connection
            .query_row(
                "SELECT label FROM categories WHERE category_id = ?1",
                [category_id],
                |row| row.get(0),
            )
            .optional()
            .ok()
            .flatten()
    });
    Ok(PostEventResponse {
        event: EventRecord {
            event_id,
            event_type: request.event_type.clone(),
            effective_date: request.effective_date.clone(),
            sequence,
            account_id: request.account_id.clone(),
            amount: request.amount.clone(),
            signed_amount: signed_text,
            currency: request.currency.clone(),
            category_id: request.category_id.clone(),
            category_label,
            note: request.note.clone(),
            event_watermark: next_order as u64,
        },
        posting,
        account_balance: new_balance.to_string(),
        projection_watermark: next_order as u64,
    })
}

fn activity_on_connection(
    connection: &Connection,
    page: u32,
    page_size: u32,
) -> SpikeResult<ActivityPage> {
    let total_count_i64: i64 =
        connection.query_row("SELECT COUNT(*) FROM business_events", [], |row| row.get(0))?;
    let total_count = total_count_i64 as u64;
    let offset = i64::from(page - 1) * i64::from(page_size);
    let mut statement = connection.prepare(
        "SELECT e.event_id, e.event_type, e.effective_date, e.sequence, e.account_id,
                e.amount, e.signed_amount, e.currency, e.category_id, c.label, e.note,
                e.event_order
         FROM business_events e
         LEFT JOIN categories c ON c.category_id = e.category_id
         ORDER BY e.effective_date DESC, e.sequence DESC, e.event_id DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let items = statement
        .query_map(params![page_size, offset], |row| {
            Ok(EventRecord {
                event_id: row.get(0)?,
                event_type: row.get(1)?,
                effective_date: row.get(2)?,
                sequence: row.get(3)?,
                account_id: row.get(4)?,
                amount: row.get(5)?,
                signed_amount: row.get(6)?,
                currency: row.get(7)?,
                category_id: row.get(8)?,
                category_label: row.get(9)?,
                note: row.get(10)?,
                event_watermark: row.get::<_, i64>(11)? as u64,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ActivityPage {
        items,
        page,
        page_size,
        total_count,
        has_more: (offset as u64) + u64::from(page_size) < total_count,
    })
}

fn expense_analysis_on_connection(
    connection: &Connection,
    start_date: &str,
    end_date: &str,
) -> SpikeResult<Value> {
    let category_lookup: BTreeMap<String, (String, bool)> = {
        let mut statement = connection
            .prepare("SELECT category_id, label, archived FROM categories ORDER BY category_id")?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (row.get::<_, String>(1)?, row.get::<_, i64>(2)? != 0),
                ))
            })?
            .collect::<Result<_, _>>()?
    };
    let mut statement = connection.prepare(
        "SELECT category_id, ledgerkit_decimal_sum(amount), SUM(distinct_event_count)
         FROM expense_daily_projection
         WHERE effective_date BETWEEN ?1 AND ?2
         GROUP BY category_id",
    )?;
    let mut rows = statement.query(params![start_date, end_date])?;
    let mut bucket_values: BTreeMap<String, (String, bool, Decimal, u64)> = BTreeMap::new();
    let mut global_distinct_event_count = 0_u64;
    while let Some(row) = rows.next()? {
        let bucket_id = row.get::<_, String>(0)?;
        let amount_text = row.get::<_, String>(1)?;
        let distinct_event_count = u64::try_from(row.get::<_, i64>(2)?)
            .map_err(|_| SpikeError::Database(rusqlite::Error::InvalidQuery))?;
        let (label, archived) = category_lookup
            .get(&bucket_id)
            .cloned()
            .unwrap_or_else(|| (bucket_id.clone(), false));
        let amount = parse_stored_decimal(&amount_text)?;
        let entry = bucket_values
            .entry(bucket_id)
            .or_insert_with(|| (label, archived, Decimal::ZERO, 0));
        entry.2 += amount;
        entry.3 += distinct_event_count;
        global_distinct_event_count += distinct_event_count;
    }

    let watermark = event_watermark(connection)?;
    let mut buckets: Vec<Value> = bucket_values
        .into_iter()
        .map(
            |(bucket_id, (label, archived, amount, distinct_event_count))| {
                json!({
                    "bucket_id": bucket_id,
                    "bucket_kind": "category",
                    "label": label,
                    "archived": archived,
                    "amount": amount.to_string(),
                    "distinct_event_count": distinct_event_count,
                    "drilldown_context": bucket_context(
                        start_date,
                        end_date,
                        watermark,
                        &bucket_id,
                    )
                })
            },
        )
        .collect();
    buckets.sort_by(|left, right| {
        let left_amount = parse_stored_decimal(left["amount"].as_str().unwrap()).unwrap();
        let right_amount = parse_stored_decimal(right["amount"].as_str().unwrap()).unwrap();
        right_amount.cmp(&left_amount).then_with(|| {
            left["bucket_id"]
                .as_str()
                .unwrap()
                .cmp(right["bucket_id"].as_str().unwrap())
        })
    });

    let total = buckets.iter().try_fold(Decimal::ZERO, |sum, bucket| {
        Ok::<_, SpikeError>(sum + parse_stored_decimal(bucket["amount"].as_str().unwrap())?)
    })?;
    let top_items: Vec<Value> = buckets
        .iter()
        .take(10)
        .map(|bucket| {
            json!({
                "bucket_id": bucket["bucket_id"],
                "label": bucket["label"],
                "amount": bucket["amount"],
                "distinct_event_count": bucket["distinct_event_count"],
                "drilldown_context": bucket["drilldown_context"],
            })
        })
        .collect();
    let other = if buckets.len() > 10 {
        let other_amount = buckets
            .iter()
            .skip(10)
            .try_fold(Decimal::ZERO, |sum, bucket| {
                Ok::<_, SpikeError>(sum + parse_stored_decimal(bucket["amount"].as_str().unwrap())?)
            })?;
        let other_count: u64 = buckets
            .iter()
            .skip(10)
            .map(|bucket| bucket["distinct_event_count"].as_u64().unwrap_or(0))
            .sum();
        Some(json!({
            "bucket_id": "system:top10-other",
            "label": "Other categories",
            "amount": other_amount.to_string(),
            "distinct_event_count": other_count,
            "drilldown_context": {
                "start_date": start_date,
                "end_date": end_date,
                "event_watermark": watermark,
                "calculation_version": CALCULATION_VERSION,
                "expense_policy_version": "expense-policy-v1",
                "bucket_id": "system:top10-other",
                "member_rank_gt": 10,
                "valuation_state": "valued"
            }
        }))
    } else {
        None
    };
    let largest_category = buckets.first().map(|bucket| {
        json!({
            "bucket_id": bucket["bucket_id"],
            "amount": bucket["amount"]
        })
    });
    let master_data_watermark: u64 = connection
        .query_row(
            "SELECT metadata_value FROM app_metadata WHERE metadata_key = 'master_data_watermark'",
            [],
            |row| row.get::<_, String>(0),
        )?
        .parse()
        .map_err(|_| SpikeError::Database(rusqlite::Error::InvalidQuery))?;
    let refund_context = |semantic_role: &str| {
        json!({
            "start_date": start_date,
            "end_date": end_date,
            "event_watermark": watermark,
            "calculation_version": CALCULATION_VERSION,
            "expense_policy_version": "expense-policy-v1",
            "semantic_role": semantic_role,
            "valuation_state": "all"
        })
    };
    let mut result = json!({
        "contract": "expense-analysis-query-result/v1",
        "query": {
            "start_date": start_date,
            "end_date": end_date,
            "base_currency": "CNY"
        },
        "summary": {
            "label": "Total expense",
            "total_expense": total.to_string(),
            "valued_subtotal": total.to_string(),
            "global_distinct_event_count": global_distinct_event_count,
            "largest_category": largest_category
        },
        "buckets": buckets,
        "top10": {
            "items": top_items,
            "other": other
        },
        "refunds": {
            "refund": {
                "amount": "0",
                "distinct_event_count": 0,
                "unvalued_count": 0,
                "drilldown_context": refund_context("refund")
            },
            "reimbursement": {
                "amount": "0",
                "distinct_event_count": 0,
                "unvalued_count": 0,
                "drilldown_context": refund_context("reimbursement")
            }
        },
        "unvalued": {
            "expense_count": 0,
            "drilldown_context": {
                "start_date": start_date,
                "end_date": end_date,
                "event_watermark": watermark,
                "calculation_version": CALCULATION_VERSION,
                "expense_policy_version": "expense-policy-v1",
                "semantic_role": "expense",
                "valuation_state": "unvalued"
            }
        },
        "watermarks": {
            "event": watermark,
            "master_data": master_data_watermark
        },
        "versions": {
            "calculation": CALCULATION_VERSION,
            "expense_policy": "expense-policy-v1",
            "bucket_policy": "expense-bucket-policy-v1",
            "refund_policy": "refund-policy-v1"
        },
        "canonicalization": "ledgerkit-canonical-json-v1"
    });
    let hash = canonical_hash(&result)?;
    result["canonical_hash"] = Value::String(hash);
    Ok(result)
}

fn bucket_context(start_date: &str, end_date: &str, watermark: u64, bucket_id: &str) -> Value {
    json!({
        "start_date": start_date,
        "end_date": end_date,
        "event_watermark": watermark,
        "calculation_version": CALCULATION_VERSION,
        "expense_policy_version": "expense-policy-v1",
        "bucket_id": bucket_id,
        "valuation_state": "valued"
    })
}

fn seed_expense_fixture_on_connection(connection: &mut Connection) -> SpikeResult<()> {
    let transaction = connection.transaction()?;
    for index in 1..=12 {
        transaction.execute(
            "INSERT OR REPLACE INTO categories(category_id, label, archived) VALUES (?1, ?2, 0)",
            params![format!("cat-{index:02}"), format!("Category {index:02}")],
        )?;
    }
    transaction.execute(
        "UPDATE app_metadata SET metadata_value = '1' WHERE metadata_key = 'master_data_watermark'",
        [],
    )?;
    transaction.commit()?;

    for index in 1..=12 {
        let request = PostEventRequest {
            event_type: "Expense".to_owned(),
            effective_date: format!("2026-02-{:02}", index),
            account_id: "cash-cny-1".to_owned(),
            amount: (130 - index * 10).to_string(),
            currency: "CNY".to_owned(),
            category_id: Some(format!("cat-{index:02}")),
            currency_precision_confirmed: false,
            note: Some(format!("Synthetic category {index:02}")),
        };
        post_event_on_connection(
            connection,
            &request,
            Some(&format!("evt-expense-top10-{index:02}")),
            false,
        )?;
    }
    Ok(())
}

fn event_watermark(connection: &Connection) -> SpikeResult<u64> {
    let value: i64 = connection.query_row(
        "SELECT COALESCE(MAX(event_order), 0) FROM business_events",
        [],
        |row| row.get(0),
    )?;
    Ok(value as u64)
}

fn projection_watermark(connection: &Connection) -> SpikeResult<u64> {
    let value: i64 = connection.query_row(
        "SELECT event_watermark FROM projection_state WHERE projection_name = 'cash-balances'",
        [],
        |row| row.get(0),
    )?;
    Ok(value as u64)
}

fn validate_date(value: &str) -> SpikeResult<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return Err(SpikeError::DateInvalid);
    }
    let month: u8 = value[5..7].parse().map_err(|_| SpikeError::DateInvalid)?;
    let day: u8 = value[8..10].parse().map_err(|_| SpikeError::DateInvalid)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(SpikeError::DateInvalid);
    }
    Ok(())
}

pub fn verify_connection(connection: &Connection) -> SpikeResult<()> {
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(SpikeError::BackupIntegrityFailed);
    }
    let foreign_key_errors: i64 =
        connection.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if foreign_key_errors != 0 {
        return Err(SpikeError::BackupIntegrityFailed);
    }
    Ok(())
}

pub fn verify_database_file(path: &Path) -> SpikeResult<()> {
    let connection = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_connection(&connection)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;

    fn empty_store() -> (tempfile::TempDir, LedgerStore) {
        let directory = tempdir().unwrap();
        let store = LedgerStore::open(directory.path().join("ledger.sqlite")).unwrap();
        (directory, store)
    }

    #[test]
    fn migration_and_atomic_posting_update_projection_together() {
        let (_directory, store) = empty_store();
        let request = PostEventRequest {
            event_type: "Income".to_owned(),
            effective_date: "2026-01-05".to_owned(),
            account_id: "cash-cny-1".to_owned(),
            amount: "100.00".to_owned(),
            currency: "CNY".to_owned(),
            category_id: Some("cat-salary".to_owned()),
            currency_precision_confirmed: false,
            note: None,
        };
        let response = store.post_event(&request).unwrap();
        assert_eq!(response.account_balance, "100.00");
        assert_eq!(response.posting.quantity_delta, "100.00");
        assert_eq!(store.status().unwrap().schema_version, SCHEMA_VERSION);
        assert_eq!(store.status().unwrap().event_watermark, 1);
    }

    #[test]
    fn failpoint_rolls_back_event_posting_and_projection() {
        let (_directory, store) = empty_store();
        let request = PostEventRequest {
            event_type: "Expense".to_owned(),
            effective_date: "2026-01-06".to_owned(),
            account_id: "cash-cny-1".to_owned(),
            amount: "25.50".to_owned(),
            currency: "CNY".to_owned(),
            category_id: Some("cat-food".to_owned()),
            currency_precision_confirmed: false,
            note: None,
        };
        let error =
            post_event_on_connection(&mut store.lock().unwrap(), &request, Some("evt-fail"), true)
                .unwrap_err();
        assert_eq!(error.code(), "SYNTHETIC_FAILPOINT");
        assert_eq!(store.status().unwrap().event_watermark, 0);
        assert_eq!(store.status().unwrap().projection_watermark, 0);
        let projected_rows: i64 = store
            .with_connection(|connection| {
                Ok(connection.query_row(
                    "SELECT COUNT(*) FROM expense_daily_projection",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(projected_rows, 0);
    }

    #[test]
    fn expense_query_has_ten_items_other_and_one_canonical_result() {
        let (_directory, store) = empty_store();
        seed_expense_fixture_on_connection(&mut store.lock().unwrap()).unwrap();
        let result = store.expense_analysis("2026-02-01", "2026-02-28").unwrap();
        assert_eq!(result["top10"]["items"].as_array().unwrap().len(), 10);
        assert_eq!(result["top10"]["other"]["amount"], "30");
        assert_eq!(result["buckets"].as_array().unwrap().len(), 12);
        assert_eq!(
            result["canonical_hash"],
            "sha256:7cd365ef12db020eb178975704fd2388cad37b5a4f378c6debf5e3aef27a8beb"
        );
        store
            .with_connection(|connection| {
                connection.execute("DELETE FROM expense_daily_projection", [])?;
                Ok(())
            })
            .unwrap();
        store.rebuild_expense_projection().unwrap();
        let rebuilt = store.expense_analysis("2026-02-01", "2026-02-28").unwrap();
        assert_eq!(rebuilt, result);
    }

    #[test]
    fn m0_fixture_01_postings_and_sequence_hash_match_exactly() {
        let (_directory, store) = empty_store();
        let requests = [
            (
                "evt-01-normal-01",
                PostEventRequest {
                    event_type: "Income".to_owned(),
                    effective_date: "2026-01-05".to_owned(),
                    account_id: "cash-cny-1".to_owned(),
                    amount: "100.00".to_owned(),
                    currency: "CNY".to_owned(),
                    category_id: Some("cat-salary".to_owned()),
                    currency_precision_confirmed: false,
                    note: None,
                },
            ),
            (
                "evt-01-normal-02",
                PostEventRequest {
                    event_type: "Expense".to_owned(),
                    effective_date: "2026-01-06".to_owned(),
                    account_id: "cash-cny-1".to_owned(),
                    amount: "25.50".to_owned(),
                    currency: "CNY".to_owned(),
                    category_id: Some("cat-food".to_owned()),
                    currency_precision_confirmed: false,
                    note: None,
                },
            ),
        ];
        let mut postings = Vec::new();
        for (event_id, request) in requests {
            let response = post_event_on_connection(
                &mut store.lock().unwrap(),
                &request,
                Some(event_id),
                false,
            )
            .unwrap();
            postings.push(response.posting);
        }
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/sanitized/01-cny-income-expense/expected-postings.json");
        let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path).unwrap()).unwrap();
        let expected = &fixture["scenarios"][0];
        let actual = serde_json::to_value(&postings).unwrap();
        assert_eq!(&actual, &expected["postings"]);
        assert_eq!(
            canonical_hash(&actual).unwrap(),
            expected["sequence_hash"].as_str().unwrap()
        );
        assert_eq!(store.overview().unwrap().cash_value, "74.50");
    }

    #[test]
    fn forged_posting_field_is_rejected_at_the_ipc_dto_boundary() {
        let forged = json!({
            "eventType": "Income",
            "effectiveDate": "2026-01-05",
            "accountId": "cash-cny-1",
            "amount": "100.00",
            "currency": "CNY",
            "categoryId": "cat-salary",
            "currencyPrecisionConfirmed": false,
            "note": null,
            "posting": { "quantityDelta": "999999" }
        });
        assert!(serde_json::from_value::<PostEventRequest>(forged).is_err());
    }
}
