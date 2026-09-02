use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ledgerkit_tauri_spike_lib::excel::{analyze_known_template, export_standardized};
use ledgerkit_tauri_spike_lib::ledger::{LedgerStore, PostEventRequest};
use rusqlite::params;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "ledgerkit-m1-bench-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    std::fs::create_dir_all(&root)?;
    let result = run(&root);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn run(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let open_started = Instant::now();
    let store = LedgerStore::open(root.join("benchmark.sqlite"))?;
    let migration_open_ms = elapsed_ms(open_started);
    store.initialize_demo()?;

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/sanitized/m1/ledgerkit-known-template-10000.xlsx");
    let import = analyze_known_template(&fixture)?;

    let mut write_samples = Vec::new();
    for index in 0..30 {
        let started = Instant::now();
        store.post_event(&PostEventRequest {
            event_type: "Expense".to_owned(),
            effective_date: "2026-02-20".to_owned(),
            account_id: "cash-cny-1".to_owned(),
            amount: "1.00".to_owned(),
            currency: "CNY".to_owned(),
            category_id: Some("cat-01".to_owned()),
            currency_precision_confirmed: false,
            note: Some(format!("Synthetic benchmark write {index:02}")),
        })?;
        write_samples.push(elapsed_ms(started));
    }

    let mut page_samples = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let _ = store.activity(1, 20)?;
        page_samples.push(elapsed_ms(started));
    }
    let export_path = root.join("standardized.xlsx");
    let export_started = Instant::now();
    let export = export_standardized(&store.all_activity()?, &export_path)?;
    let export_ms = elapsed_ms(export_started);
    store.with_connection(|connection| {
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    })?;
    let current_database_bytes = std::fs::metadata(store.database_path())?.len();
    let mut current_expense_samples = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let _ = store.expense_analysis("2026-02-01", "2026-02-28")?;
        current_expense_samples.push(elapsed_ms(started));
    }

    let seed_started = Instant::now();
    store.with_connection(|connection| {
        let transaction = connection.transaction()?;
        for index in 1..=20 {
            transaction.execute(
                "INSERT OR REPLACE INTO categories(category_id, label, archived) VALUES (?1, ?2, 0)",
                params![format!("perf-cat-{index:02}"), format!("Performance category {index:02}")],
            )?;
        }
        transaction.execute_batch(
            r#"
            WITH RECURSIVE sequence(n) AS (
                SELECT 1 UNION ALL SELECT n + 1 FROM sequence WHERE n < 100000
            )
            INSERT INTO business_events(
                event_id, event_type, effective_date, sequence, account_id, amount,
                signed_amount, currency, category_id, note, calculation_version
            )
            SELECT
                printf('perf-event-%06d', n), 'Expense',
                printf('2026-%02d-%02d', ((n - 1) % 12) + 1, ((n - 1) % 28) + 1),
                n, 'cash-cny-1', '1.00', '-1.00', 'CNY',
                printf('perf-cat-%02d', ((n - 1) % 20) + 1),
                'Synthetic 100k query benchmark', 'ledger-calculation-v1'
            FROM sequence;

            INSERT INTO ledger_postings(
                posting_id, event_id, posting_kind, account_id, quantity_delta,
                currency, base_value, base_currency, calculation_version
            )
            SELECT
                'post-' || event_id || '-01', event_id, 'cash', account_id,
                signed_amount, currency, signed_amount, 'CNY', calculation_version
            FROM business_events WHERE event_id LIKE 'perf-event-%';

            UPDATE projection_state
            SET event_watermark = (SELECT MAX(event_order) FROM business_events),
                calculation_version = 'ledger-calculation-v1'
            WHERE projection_name = 'cash-balances';
            "#,
        )?;
        transaction.commit()?;
        connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    })?;
    store.rebuild_expense_projection()?;
    let seed_100k_ms = elapsed_ms(seed_started);

    let mut timeline_100k_samples = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let _ = store.activity(1, 50)?;
        timeline_100k_samples.push(elapsed_ms(started));
    }

    let database_path = store.database_path().to_owned();
    let mut cold_samples = Vec::new();
    let mut cold_result = None;
    for _ in 0..30 {
        let cold_store = LedgerStore::open(&database_path)?;
        let started = Instant::now();
        let result = cold_store.expense_analysis("2026-01-01", "2026-12-31")?;
        cold_samples.push(elapsed_ms(started));
        cold_result.get_or_insert(result);
    }
    let mut warm_samples = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let _ = store.expense_analysis("2026-01-01", "2026-12-31")?;
        warm_samples.push(elapsed_ms(started));
    }
    let response_bytes = serde_json::to_vec(&cold_result.ok_or("missing cold result")?)?.len();
    let database_bytes = std::fs::metadata(store.database_path())?.len();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "migration_open_ms": migration_open_ms,
            "import_10k_ms": import.elapsed_ms,
            "import_10k_rows": import.row_count,
            "write_ms_raw": write_samples,
            "write_p95_ms": p95(&write_samples),
            "page_ms_raw": page_samples,
            "page_p95_ms": p95(&page_samples),
            "export_ms": export_ms,
            "export_rows": export.row_count,
            "current_database_bytes": current_database_bytes,
            "current_expense_ms_raw": current_expense_samples,
            "current_expense_p95_ms": p95(&current_expense_samples),
            "seed_100k_ms": seed_100k_ms,
            "timeline_100k_ms_raw": timeline_100k_samples,
            "timeline_100k_p95_ms": p95(&timeline_100k_samples),
            "query_100k_cold_ms_raw": cold_samples,
            "query_100k_cold_p95_ms": p95(&cold_samples),
            "query_100k_warm_ms_raw": warm_samples,
            "query_100k_warm_p95_ms": p95(&warm_samples),
            "query_100k_response_bytes": response_bytes,
            "database_100k_bytes": database_bytes,
            "sqlite_version": store.status()?.sqlite_version,
        }))?
    );
    Ok(())
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn p95(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() as f64 * 0.95).ceil() as usize).saturating_sub(1)]
}
