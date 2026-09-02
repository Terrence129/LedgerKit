#![forbid(unsafe_code)]

use sha2::{Digest, Sha256};

pub const APPLICATION_ID: u32 = 1_280_002_388;
pub const SCHEMA_VERSION: u32 = 2;

pub const REQUIRED_TABLES: &[&str] = &[
    "app_settings",
    "audit_events",
    "backup_status",
    "business_events",
    "cash_accounts",
    "cash_balance_projection",
    "cash_data_quality_projection",
    "cash_event_fees",
    "categories",
    "currency_exchange_details",
    "dividend_details",
    "fx_rate_revisions",
    "fx_resolutions",
    "holding_projection",
    "import_batches",
    "import_rows",
    "income_expense_details",
    "institutions",
    "investment_expense_details",
    "expense_daily_event_bucket_projection",
    "expense_daily_projection",
    "expense_daily_summary_projection",
    "ledger_metadata",
    "ledger_postings",
    "migration_history",
    "monthly_cash_flow_projection",
    "opening_balance_details",
    "opening_performance_details",
    "opening_position_details",
    "portfolios",
    "projection_metadata",
    "security_instruments",
    "security_price_revisions",
    "security_trade_details",
    "transfer_details",
    "valuation_snapshot_lines",
    "valuation_snapshots",
];

pub const REQUIRED_INDEXES: &[&str] = &[
    "idx_business_events_activity",
    "idx_business_events_revision_target",
    "idx_business_events_reversal_target",
    "idx_cash_event_fees_event",
    "idx_expense_daily_bucket",
    "idx_expense_daily_event_bucket",
    "idx_fx_rate_as_of",
    "idx_income_expense_category",
    "idx_ledger_postings_event_kind",
    "idx_price_as_of",
    "idx_security_trade_holding",
    "uq_active_fx_rate_revision",
    "uq_active_security_price_revision",
];

pub const REQUIRED_TRIGGERS: &[&str] = &[
    "trg_freeze_base_currency",
    "trg_freeze_cash_account_currency",
    "trg_freeze_instrument_trade_currency",
];

pub const SCHEMA_CURRENT: &str = r"
CREATE TABLE ledger_metadata (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    ledger_id TEXT NOT NULL UNIQUE CHECK (
        length(ledger_id) = 36 AND substr(ledger_id, 15, 1) = '7'
    ),
    created_at_utc TEXT NOT NULL,
    schema_created_by TEXT NOT NULL
) STRICT;

CREATE TABLE app_settings (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    base_currency TEXT NOT NULL CHECK (
        length(base_currency) = 3 AND base_currency = upper(base_currency)
    ),
    ui_locale TEXT NOT NULL CHECK (ui_locale IN ('zh-CN', 'en-US')),
    valuation_defaults_json TEXT NOT NULL DEFAULT '{}' CHECK (
        json_valid(valuation_defaults_json) AND json_type(valuation_defaults_json) = 'object'
    ),
    updated_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE institutions (
    institution_id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    region TEXT,
    institution_type TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE cash_accounts (
    account_id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL UNIQUE,
    institution_id TEXT REFERENCES institutions(institution_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    purpose TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    opened_on TEXT CHECK (opened_on IS NULL OR opened_on = date(opened_on, '+0 days')),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE categories (
    category_id TEXT PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    category_kind TEXT NOT NULL CHECK (category_kind IN ('income', 'expense')),
    semantic_role TEXT NOT NULL CHECK (
        semantic_role IN ('normal', 'refund', 'reimbursement')
    ),
    sort_order INTEGER NOT NULL CHECK (sort_order >= 0),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (name, category_kind)
) STRICT;

CREATE TABLE portfolios (
    portfolio_id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL UNIQUE,
    institution_id TEXT REFERENCES institutions(institution_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    settlement_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    portfolio_type TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE security_instruments (
    instrument_id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL UNIQUE,
    code TEXT NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    trade_currency TEXT NOT NULL CHECK (
        length(trade_currency) = 3 AND trade_currency = upper(trade_currency)
    ),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (code, trade_currency)
) STRICT;

CREATE TABLE import_batches (
    import_batch_id TEXT PRIMARY KEY,
    source_sha256 TEXT NOT NULL CHECK (source_sha256 GLOB 'sha256:[0-9a-f]*' AND length(source_sha256) = 71),
    importer_version TEXT NOT NULL,
    source_schema_version TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN ('staging', 'needs-review', 'ready', 'committed', 'rejected', 'failed')
    ),
    created_at_utc TEXT NOT NULL,
    committed_at_utc TEXT,
    UNIQUE (source_sha256, importer_version, source_schema_version)
) STRICT;

CREATE TABLE import_rows (
    import_row_id TEXT PRIMARY KEY,
    import_batch_id TEXT NOT NULL REFERENCES import_batches(import_batch_id) ON UPDATE RESTRICT ON DELETE CASCADE,
    sheet_name TEXT NOT NULL,
    source_row_number INTEGER NOT NULL CHECK (source_row_number > 0),
    raw_values_json TEXT NOT NULL CHECK (json_valid(raw_values_json)),
    normalized_values_json TEXT CHECK (
        normalized_values_json IS NULL OR json_valid(normalized_values_json)
    ),
    formula_evidence_json TEXT CHECK (
        formula_evidence_json IS NULL OR json_valid(formula_evidence_json)
    ),
    content_sha256 TEXT NOT NULL CHECK (content_sha256 GLOB 'sha256:[0-9a-f]*' AND length(content_sha256) = 71),
    status TEXT NOT NULL CHECK (status IN ('valid', 'warning', 'error', 'evidence-only')),
    errors_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(errors_json)),
    UNIQUE (import_batch_id, sheet_name, source_row_number)
) STRICT;

CREATE TABLE business_events (
    event_order INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL CHECK (event_type IN (
        'Income', 'Expense', 'BalanceAdjustment', 'Transfer', 'CurrencyExchange',
        'SecurityTrade', 'Dividend', 'InvestmentExpense', 'OpeningBalance',
        'OpeningPosition', 'OpeningPerformance', 'Reversal'
    )),
    effective_date TEXT NOT NULL CHECK (effective_date = date(effective_date, '+0 days')),
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    status TEXT NOT NULL CHECK (status IN ('posted', 'evidence-only')),
    revision INTEGER NOT NULL CHECK (revision > 0),
    supersedes_event_id TEXT REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    reverses_event_id TEXT REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    revision_reason TEXT,
    created_at_utc TEXT NOT NULL,
    import_batch_id TEXT REFERENCES import_batches(import_batch_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    calculation_version TEXT NOT NULL,
    CHECK (supersedes_event_id IS NULL OR supersedes_event_id <> event_id),
    CHECK (reverses_event_id IS NULL OR reverses_event_id <> event_id),
    CHECK (NOT (supersedes_event_id IS NOT NULL AND reverses_event_id IS NOT NULL)),
    CHECK ((supersedes_event_id IS NULL AND reverses_event_id IS NULL) OR length(trim(revision_reason)) > 0),
    UNIQUE (effective_date, sequence)
) STRICT;

CREATE TABLE opening_balance_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    balance_amount TEXT NOT NULL,
    cutover_date TEXT NOT NULL CHECK (cutover_date = date(cutover_date, '+0 days')),
    migration_policy TEXT NOT NULL CHECK (migration_policy IN ('full_history', 'explicit_cutover'))
) STRICT;

CREATE TABLE income_expense_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    entry_type TEXT NOT NULL CHECK (entry_type IN ('income', 'expense', 'balance_adjustment')),
    category_id TEXT REFERENCES categories(category_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    amount TEXT NOT NULL,
    merchant TEXT,
    note TEXT,
    semantic_role TEXT NOT NULL DEFAULT 'normal' CHECK (
        semantic_role IN ('normal', 'refund', 'reimbursement')
    )
) STRICT;

CREATE TABLE cash_event_fees (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    fee_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    fee_amount TEXT NOT NULL
) STRICT;

CREATE INDEX idx_cash_event_fees_event ON cash_event_fees(event_id, fee_account_id);

CREATE TABLE transfer_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    from_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    to_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    amount TEXT NOT NULL,
    CHECK (from_account_id <> to_account_id)
) STRICT;

CREATE TABLE currency_exchange_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    from_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    to_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    from_amount TEXT NOT NULL,
    to_amount TEXT NOT NULL,
    fee_account_id TEXT REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    fee_amount TEXT,
    CHECK (from_account_id <> to_account_id),
    CHECK ((fee_account_id IS NULL) = (fee_amount IS NULL))
) STRICT;

CREATE TABLE security_trade_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    trade_type TEXT NOT NULL CHECK (trade_type IN ('BUY', 'SELL')),
    portfolio_id TEXT NOT NULL REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    instrument_id TEXT NOT NULL REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    settlement_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    quantity TEXT NOT NULL,
    unit_price TEXT NOT NULL,
    trade_fee TEXT NOT NULL,
    settlement_override_reason TEXT
) STRICT;

CREATE TABLE dividend_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    portfolio_id TEXT NOT NULL REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    instrument_id TEXT NOT NULL REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    settlement_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    gross_cash_amount TEXT NOT NULL,
    withholding_tax TEXT NOT NULL,
    fee_amount TEXT NOT NULL
) STRICT;

CREATE TABLE investment_expense_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    portfolio_id TEXT NOT NULL REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    instrument_id TEXT REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    settlement_account_id TEXT NOT NULL REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    amount TEXT NOT NULL,
    fee_scope TEXT NOT NULL CHECK (fee_scope IN ('instrument', 'portfolio')),
    CHECK ((fee_scope = 'instrument' AND instrument_id IS NOT NULL) OR (fee_scope = 'portfolio' AND instrument_id IS NULL))
) STRICT;

CREATE TABLE opening_position_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    portfolio_id TEXT NOT NULL REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    instrument_id TEXT NOT NULL REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    quantity TEXT NOT NULL,
    carrying_cost TEXT NOT NULL,
    cost_currency TEXT NOT NULL CHECK (length(cost_currency) = 3 AND cost_currency = upper(cost_currency)),
    cutover_date TEXT NOT NULL CHECK (cutover_date = date(cutover_date, '+0 days')),
    migration_policy TEXT NOT NULL CHECK (migration_policy IN ('full_history', 'explicit_cutover'))
) STRICT;

CREATE TABLE opening_performance_details (
    event_id TEXT PRIMARY KEY REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    portfolio_id TEXT NOT NULL REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    instrument_id TEXT REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    realized_trade_pnl TEXT NOT NULL,
    net_dividend TEXT NOT NULL,
    independent_expense TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    cutover_date TEXT NOT NULL CHECK (cutover_date = date(cutover_date, '+0 days'))
) STRICT;

CREATE TABLE fx_rate_revisions (
    fx_rate_revision_id TEXT PRIMARY KEY,
    rate_date TEXT NOT NULL CHECK (rate_date = date(rate_date, '+0 days')),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    base_currency TEXT NOT NULL CHECK (length(base_currency) = 3 AND base_currency = upper(base_currency)),
    rate_to_base TEXT NOT NULL,
    source TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    CHECK (currency <> base_currency),
    UNIQUE (currency, base_currency, rate_date, revision)
) STRICT;

CREATE UNIQUE INDEX uq_active_fx_rate_revision
    ON fx_rate_revisions(currency, base_currency, rate_date) WHERE active = 1;
CREATE INDEX idx_fx_rate_as_of
    ON fx_rate_revisions(currency, base_currency, rate_date DESC) WHERE active = 1;

CREATE TABLE security_price_revisions (
    security_price_revision_id TEXT PRIMARY KEY,
    instrument_id TEXT NOT NULL REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    price_date TEXT NOT NULL CHECK (price_date = date(price_date, '+0 days')),
    price TEXT NOT NULL,
    price_currency TEXT NOT NULL CHECK (length(price_currency) = 3 AND price_currency = upper(price_currency)),
    source TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    UNIQUE (instrument_id, price_date, revision)
) STRICT;

CREATE UNIQUE INDEX uq_active_security_price_revision
    ON security_price_revisions(instrument_id, price_date) WHERE active = 1;
CREATE INDEX idx_price_as_of
    ON security_price_revisions(instrument_id, price_date DESC) WHERE active = 1;

CREATE TABLE fx_resolutions (
    fx_resolution_id TEXT PRIMARY KEY,
    owner_type TEXT NOT NULL CHECK (owner_type IN ('event', 'posting', 'valuation')),
    owner_id TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose IN ('transaction', 'fee', 'valuation')),
    target_date TEXT NOT NULL CHECK (target_date = date(target_date, '+0 days')),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    base_currency TEXT NOT NULL CHECK (length(base_currency) = 3 AND base_currency = upper(base_currency)),
    auto_rate_revision_id TEXT REFERENCES fx_rate_revisions(fx_rate_revision_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    override_value TEXT,
    override_reason TEXT,
    final_rate TEXT NOT NULL,
    calculation_version TEXT NOT NULL,
    created_at_utc TEXT NOT NULL,
    CHECK ((override_value IS NULL AND override_reason IS NULL) OR (override_value IS NOT NULL AND length(trim(override_reason)) > 0)),
    UNIQUE (owner_type, owner_id, purpose, currency, base_currency)
) STRICT;

CREATE TABLE ledger_postings (
    posting_id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    posting_ordinal INTEGER NOT NULL CHECK (posting_ordinal > 0),
    posting_kind TEXT NOT NULL CHECK (posting_kind IN (
        'cash', 'cash-reversal', 'security-quantity', 'security-cost', 'realized-trade-pnl',
        'net-dividend', 'independent-expense'
    )),
    account_id TEXT REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    portfolio_id TEXT REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    instrument_id TEXT REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    quantity_delta TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    base_value TEXT,
    base_currency TEXT NOT NULL CHECK (length(base_currency) = 3 AND base_currency = upper(base_currency)),
    calculation_version TEXT NOT NULL,
    CHECK (account_id IS NOT NULL OR instrument_id IS NOT NULL),
    UNIQUE (event_id, posting_ordinal)
) STRICT;

CREATE TABLE audit_events (
    audit_event_id TEXT PRIMARY KEY,
    business_event_id TEXT REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    actor TEXT NOT NULL CHECK (actor = 'local-user'),
    action TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    entity_revision INTEGER NOT NULL CHECK (entity_revision > 0),
    occurred_at_utc TEXT NOT NULL,
    reason TEXT
) STRICT;

CREATE TABLE projection_metadata (
    projection_name TEXT PRIMARY KEY,
    projection_version TEXT NOT NULL,
    calculation_version TEXT NOT NULL,
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    available INTEGER NOT NULL DEFAULT 1 CHECK (available IN (0, 1)),
    rebuilt_at_utc TEXT
) STRICT;

CREATE TABLE cash_balance_projection (
    account_id TEXT PRIMARY KEY REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE CASCADE,
    balance TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    calculation_version TEXT NOT NULL
) STRICT;

CREATE TABLE monthly_cash_flow_projection (
    month TEXT NOT NULL CHECK (length(month) = 7),
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    income TEXT NOT NULL,
    expense TEXT NOT NULL,
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    calculation_version TEXT NOT NULL,
    PRIMARY KEY (month, currency)
) STRICT;

CREATE TABLE cash_data_quality_projection (
    event_id TEXT NOT NULL REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE CASCADE,
    issue_code TEXT NOT NULL,
    currency TEXT NOT NULL CHECK (length(currency) = 3 AND currency = upper(currency)),
    target_date TEXT NOT NULL CHECK (target_date = date(target_date, '+0 days')),
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    calculation_version TEXT NOT NULL,
    PRIMARY KEY (event_id, issue_code, currency)
) STRICT;

CREATE TABLE expense_daily_projection (
    effective_date TEXT NOT NULL CHECK (effective_date = date(effective_date, '+0 days')),
    bucket_id TEXT NOT NULL,
    semantic_role TEXT NOT NULL CHECK (semantic_role = 'normal'),
    valuation_state TEXT NOT NULL CHECK (valuation_state IN ('valued', 'unvalued')),
    amount TEXT NOT NULL,
    distinct_event_count INTEGER NOT NULL CHECK (distinct_event_count >= 0),
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    calculation_version TEXT NOT NULL,
    PRIMARY KEY (effective_date, bucket_id, semantic_role, valuation_state)
) STRICT;

CREATE TABLE expense_daily_summary_projection (
    effective_date TEXT NOT NULL CHECK (effective_date = date(effective_date, '+0 days')),
    measure_role TEXT NOT NULL CHECK (measure_role IN ('expense', 'refund', 'reimbursement')),
    valuation_state TEXT NOT NULL CHECK (valuation_state IN ('valued', 'unvalued')),
    amount TEXT NOT NULL,
    distinct_event_count INTEGER NOT NULL CHECK (distinct_event_count >= 0),
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    calculation_version TEXT NOT NULL,
    PRIMARY KEY (effective_date, measure_role, valuation_state)
) STRICT;

CREATE TABLE expense_daily_event_bucket_projection (
    effective_date TEXT NOT NULL CHECK (effective_date = date(effective_date, '+0 days')),
    event_id TEXT NOT NULL REFERENCES business_events(event_id) ON UPDATE RESTRICT ON DELETE CASCADE,
    bucket_id TEXT NOT NULL,
    valuation_state TEXT NOT NULL CHECK (valuation_state IN ('valued', 'unvalued')),
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    calculation_version TEXT NOT NULL,
    PRIMARY KEY (effective_date, event_id, bucket_id, valuation_state)
) STRICT;

CREATE INDEX idx_expense_daily_bucket
    ON expense_daily_projection(effective_date, bucket_id, valuation_state);
CREATE INDEX idx_expense_daily_event_bucket
    ON expense_daily_event_bucket_projection(effective_date, bucket_id, event_id);

CREATE TABLE holding_projection (
    portfolio_id TEXT NOT NULL REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE CASCADE,
    instrument_id TEXT NOT NULL REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE CASCADE,
    as_of_date TEXT NOT NULL CHECK (as_of_date = date(as_of_date, '+0 days')),
    quantity TEXT NOT NULL,
    carrying_cost TEXT NOT NULL,
    realized_trade_pnl TEXT NOT NULL,
    net_dividend TEXT NOT NULL,
    independent_expense TEXT NOT NULL,
    unrealized_pnl TEXT,
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    projection_version TEXT NOT NULL,
    calculation_version TEXT NOT NULL,
    PRIMARY KEY (portfolio_id, instrument_id)
) STRICT;

CREATE TABLE valuation_snapshots (
    valuation_snapshot_id TEXT PRIMARY KEY,
    supersedes_snapshot_id TEXT REFERENCES valuation_snapshots(valuation_snapshot_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    valuation_date TEXT NOT NULL CHECK (valuation_date = date(valuation_date, '+0 days')),
    base_currency TEXT NOT NULL CHECK (length(base_currency) = 3 AND base_currency = upper(base_currency)),
    calculation_version TEXT NOT NULL,
    event_watermark INTEGER NOT NULL CHECK (event_watermark >= 0),
    market_data_watermark INTEGER NOT NULL CHECK (market_data_watermark >= 0),
    summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
    created_at_utc TEXT NOT NULL
) STRICT;

CREATE TABLE valuation_snapshot_lines (
    valuation_snapshot_line_id TEXT PRIMARY KEY,
    valuation_snapshot_id TEXT NOT NULL REFERENCES valuation_snapshots(valuation_snapshot_id) ON UPDATE RESTRICT ON DELETE CASCADE,
    asset_type TEXT NOT NULL CHECK (asset_type IN ('cash-account', 'holding')),
    account_id TEXT REFERENCES cash_accounts(account_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    portfolio_id TEXT REFERENCES portfolios(portfolio_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    instrument_id TEXT REFERENCES security_instruments(instrument_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    native_value TEXT NOT NULL,
    native_currency TEXT NOT NULL CHECK (length(native_currency) = 3 AND native_currency = upper(native_currency)),
    price_revision_id TEXT REFERENCES security_price_revisions(security_price_revision_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    fx_resolution_id TEXT REFERENCES fx_resolutions(fx_resolution_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
    base_value TEXT,
    base_currency TEXT NOT NULL CHECK (length(base_currency) = 3 AND base_currency = upper(base_currency)),
    valuation_state TEXT NOT NULL CHECK (valuation_state IN ('valued', 'unvalued')),
    unvalued_reason TEXT,
    CHECK ((asset_type = 'cash-account' AND account_id IS NOT NULL AND portfolio_id IS NULL AND instrument_id IS NULL) OR (asset_type = 'holding' AND account_id IS NULL AND portfolio_id IS NOT NULL AND instrument_id IS NOT NULL)),
    CHECK ((valuation_state = 'valued' AND base_value IS NOT NULL AND unvalued_reason IS NULL) OR (valuation_state = 'unvalued' AND base_value IS NULL AND length(trim(unvalued_reason)) > 0))
) STRICT;

CREATE TABLE backup_status (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    protection_state TEXT NOT NULL CHECK (
        protection_state IN ('not-configured', 'pending', 'protected', 'failed')
    ),
    last_attempt_at_utc TEXT,
    last_success_at_utc TEXT,
    last_verified_schema_version INTEGER,
    last_error_code TEXT,
    external_target_configured INTEGER NOT NULL DEFAULT 0 CHECK (external_target_configured IN (0, 1))
) STRICT;

CREATE TABLE migration_history (
    schema_version INTEGER PRIMARY KEY CHECK (schema_version > 0),
    applied_at_utc TEXT NOT NULL,
    application_version TEXT NOT NULL,
    schema_hash TEXT NOT NULL
) STRICT;

CREATE INDEX idx_business_events_activity
    ON business_events(effective_date DESC, status, sequence DESC, event_id DESC);
CREATE INDEX idx_business_events_revision_target
    ON business_events(supersedes_event_id) WHERE supersedes_event_id IS NOT NULL;
CREATE INDEX idx_business_events_reversal_target
    ON business_events(reverses_event_id) WHERE reverses_event_id IS NOT NULL;
CREATE INDEX idx_income_expense_category
    ON income_expense_details(event_id, category_id);
CREATE INDEX idx_ledger_postings_event_kind
    ON ledger_postings(event_id, posting_kind);
CREATE INDEX idx_security_trade_holding
    ON security_trade_details(portfolio_id, instrument_id, event_id);
CREATE INDEX idx_import_rows_batch_status
    ON import_rows(import_batch_id, status, sheet_name, source_row_number);
CREATE INDEX idx_valuation_snapshots_as_of
    ON valuation_snapshots(valuation_date DESC, created_at_utc DESC);

CREATE TRIGGER trg_freeze_base_currency
BEFORE UPDATE OF base_currency ON app_settings
WHEN OLD.base_currency <> NEW.base_currency AND (
    EXISTS (SELECT 1 FROM cash_accounts LIMIT 1)
    OR EXISTS (SELECT 1 FROM security_instruments LIMIT 1)
    OR EXISTS (SELECT 1 FROM business_events LIMIT 1)
    OR EXISTS (SELECT 1 FROM fx_rate_revisions LIMIT 1)
    OR EXISTS (SELECT 1 FROM security_price_revisions LIMIT 1)
    OR EXISTS (SELECT 1 FROM valuation_snapshots LIMIT 1)
)
BEGIN
    SELECT RAISE(ABORT, 'BASE_CURRENCY_FROZEN');
END;

CREATE TRIGGER trg_freeze_cash_account_currency
BEFORE UPDATE OF currency ON cash_accounts
WHEN OLD.currency <> NEW.currency AND EXISTS (
    SELECT 1 FROM ledger_postings WHERE account_id = OLD.account_id LIMIT 1
)
BEGIN
    SELECT RAISE(ABORT, 'CASH_ACCOUNT_CURRENCY_FROZEN');
END;

CREATE TRIGGER trg_freeze_instrument_trade_currency
BEFORE UPDATE OF trade_currency ON security_instruments
WHEN OLD.trade_currency <> NEW.trade_currency AND (
    EXISTS (SELECT 1 FROM security_trade_details WHERE instrument_id = OLD.instrument_id LIMIT 1)
    OR EXISTS (SELECT 1 FROM security_price_revisions WHERE instrument_id = OLD.instrument_id LIMIT 1)
)
BEGIN
    SELECT RAISE(ABORT, 'INSTRUMENT_TRADE_CURRENCY_FROZEN');
END;
";

pub fn schema_hash() -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(SCHEMA_CURRENT.as_bytes());
    let hex = digest
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        });
    format!("sha256:{hex}")
}
