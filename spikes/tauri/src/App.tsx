import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type LedgerStatus = {
  schemaVersion: number;
  sqliteVersion: string;
  eventWatermark: number;
  projectionWatermark: number;
  databaseBytes: number;
  calculationVersion: string;
  defaultNetworkEnabled: boolean;
};

type Overview = {
  baseCurrency: string;
  netWorth: string;
  cashValue: string;
  securityValue: string;
  valuedRatioPercent: number;
  eventWatermark: number;
};

type EventRecord = {
  eventId: string;
  eventType: string;
  effectiveDate: string;
  sequence: number;
  accountId: string;
  amount: string;
  signedAmount: string;
  currency: string;
  categoryId: string | null;
  categoryLabel: string | null;
  note: string | null;
  eventWatermark: number;
};

type ActivityPage = {
  items: EventRecord[];
  page: number;
  pageSize: number;
  totalCount: number;
  hasMore: boolean;
};

type ExpenseBucket = {
  bucket_id: string;
  label: string;
  amount: string;
  distinct_event_count: number;
};

type ExpenseAnalysis = {
  queryResult: ExpenseQueryResult;
  chartRows: ExpenseChartRow[];
};

type ExpenseQueryResult = {
  summary: { total_expense: string; global_distinct_event_count: number };
  buckets: ExpenseBucket[];
  top10: { items: ExpenseBucket[]; other: ExpenseBucket | null };
  canonical_hash: string;
};

type ExpenseChartRow = ExpenseBucket & { widthBasisPoints: number };

type BackupSummary = { backupId: string; packageBytes: number; verified: boolean };
type CommandFailure = { code?: string; message?: string };

const appStartedAt = performance.now();
const pageSize = 6;

function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const failure = error as CommandFailure;
    return [failure.code, failure.message].filter(Boolean).join(": ");
  }
  return "Unexpected command failure";
}

function App() {
  const [status, setStatus] = useState<LedgerStatus | null>(null);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [activity, setActivity] = useState<ActivityPage | null>(null);
  const [analysis, setAnalysis] = useState<ExpenseAnalysis | null>(null);
  const [page, setPage] = useState(1);
  const [amount, setAmount] = useState("12.34");
  const [password, setPassword] = useState("synthetic-password");
  const [latestBackup, setLatestBackup] = useState<BackupSummary | null>(null);
  const [message, setMessage] = useState("Loading local ledger…");
  const [busy, setBusy] = useState(false);
  const readyReported = useRef(false);
  const expenseRequestAt = useRef(performance.now());

  const chartRows = useMemo(() => analysis?.chartRows ?? [], [analysis]);

  async function refresh(targetPage = page) {
    const [nextStatus, nextOverview, nextActivity, nextAnalysis] = await Promise.all([
      invoke<LedgerStatus>("get_ledger_status"),
      invoke<Overview>("get_overview"),
      invoke<ActivityPage>("get_activity", {
        request: { page: targetPage, pageSize },
      }),
      invoke<ExpenseAnalysis>("get_expense_analysis", {
        request: { startDate: "2026-02-01", endDate: "2026-02-28" },
      }),
    ]);
    setStatus(nextStatus);
    setOverview(nextOverview);
    setActivity(nextActivity);
    setAnalysis(nextAnalysis);
  }

  useEffect(() => {
    expenseRequestAt.current = performance.now();
    refresh(1)
      .then(() => setMessage("Local-only ledger ready"))
      .catch((error) => setMessage(errorText(error)));
  }, []);

  useEffect(() => {
    if (!analysis || readyReported.current) return;
    readyReported.current = true;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        void invoke("mark_frontend_ready", {
          metrics: {
            firstRenderMs: performance.now() - appStartedAt,
            expenseRenderMs: performance.now() - expenseRequestAt.current,
          },
        });
      });
    });
  }, [analysis]);

  async function perform(label: string, operation: () => Promise<string>) {
    setBusy(true);
    setMessage(`${label}…`);
    try {
      setMessage(await operation());
    } catch (error) {
      setMessage(errorText(error));
    } finally {
      setBusy(false);
    }
  }

  async function submitEvent(event: FormEvent) {
    event.preventDefault();
    await perform("Posting synthetic expense", async () => {
      await invoke("post_event", {
        request: {
          eventType: "Expense",
          effectiveDate: "2026-02-20",
          accountId: "cash-cny-1",
          amount,
          currency: "CNY",
          categoryId: "cat-01",
          currencyPrecisionConfirmed: false,
          note: "Synthetic UI spike event",
        },
      });
      setPage(1);
      await refresh(1);
      return "Synthetic expense committed with posting and projection";
    });
  }

  async function selectPage(nextPage: number) {
    await perform("Loading activity", async () => {
      setPage(nextPage);
      const next = await invoke<ActivityPage>("get_activity", {
        request: { page: nextPage, pageSize },
      });
      setActivity(next);
      return `Showing page ${nextPage}`;
    });
  }

  return (
    <main>
      <header className="hero">
        <div>
          <p className="eyebrow">Disposable M1 vertical spike</p>
          <h1>LedgerKit local ledger</h1>
          <p>One desktop process tree, one SQLite authority, named IPC only.</p>
        </div>
        <dl className="status-grid" aria-label="Runtime status">
          <div><dt>SQLite</dt><dd>{status?.sqliteVersion ?? "—"}</dd></div>
          <div><dt>Schema</dt><dd>v{status?.schemaVersion ?? "—"}</dd></div>
          <div><dt>Watermark</dt><dd>{status?.eventWatermark ?? "—"}</dd></div>
          <div><dt>Network default</dt><dd>{status?.defaultNetworkEnabled ? "Enabled" : "Off"}</dd></div>
        </dl>
      </header>

      <p className="live-message" role="status" aria-live="polite">{message}</p>

      <section className="card" aria-labelledby="worth-heading">
        <div className="section-heading">
          <div><p className="eyebrow">Overview</p><h2 id="worth-heading">Net worth</h2></div>
          <strong className="money">{overview?.netWorth ?? "—"} {overview?.baseCurrency}</strong>
        </div>
        <div className="worth-bar" role="img" aria-label={`Cash ${overview?.cashValue ?? "0"} CNY, securities ${overview?.securityValue ?? "0"} CNY`}>
          <span style={{ width: `${overview?.valuedRatioPercent ?? 0}%` }} />
        </div>
        <p className="muted">Cash {overview?.cashValue ?? "—"} · Securities {overview?.securityValue ?? "—"} · valued {overview?.valuedRatioPercent ?? 0}%</p>
      </section>

      <div className="two-column">
        <section className="card" aria-labelledby="expense-heading">
          <div className="section-heading">
            <div><p className="eyebrow">2026-02</p><h2 id="expense-heading">Top 10 + other</h2></div>
            <strong>{analysis?.queryResult.summary.total_expense ?? "—"} CNY</strong>
          </div>
          <ol className="bar-list" aria-label="Top expense categories">
            {chartRows.map((row) => (
              <li key={row.bucket_id}>
                <div><span>{row.label}</span><strong>{row.amount}</strong></div>
                <span className="bar-track"><span style={{ width: `${row.widthBasisPoints / 100}%` }} /></span>
              </li>
            ))}
          </ol>
          <p className="hash">Canonical result: {analysis?.queryResult.canonical_hash ?? "—"}</p>
        </section>

        <section className="card" aria-labelledby="activity-heading">
          <div className="section-heading">
            <div><p className="eyebrow">Paged query</p><h2 id="activity-heading">Activity</h2></div>
            <span>{activity?.totalCount ?? 0} events</span>
          </div>
          <ul className="activity-list">
            {activity?.items.map((item) => (
              <li key={item.eventId}>
                <span><strong>{item.eventType}</strong><small>{item.effectiveDate} · {item.categoryLabel ?? "Uncategorized"}</small></span>
                <strong className={item.eventType === "Expense" ? "negative" : "positive"}>{item.signedAmount} {item.currency}</strong>
              </li>
            ))}
          </ul>
          <nav className="pagination" aria-label="Activity pages">
            <button disabled={busy || page === 1} onClick={() => void selectPage(page - 1)}>Previous</button>
            <span>Page {page}</span>
            <button disabled={busy || !activity?.hasMore} onClick={() => void selectPage(page + 1)}>Next</button>
          </nav>
        </section>
      </div>

      <section className="card" aria-labelledby="table-heading">
        <div className="section-heading"><div><p className="eyebrow">Same Core query</p><h2 id="table-heading">Semantic expense table</h2></div></div>
        <div className="table-scroll">
          <table>
            <caption>All expense buckets from the same result used by the chart</caption>
            <thead><tr><th scope="col">Category</th><th scope="col">Amount (CNY)</th><th scope="col">Distinct events</th><th scope="col">Bucket ID</th></tr></thead>
            <tbody>{analysis?.queryResult.buckets.map((row) => <tr key={row.bucket_id}><th scope="row">{row.label}</th><td>{row.amount}</td><td>{row.distinct_event_count}</td><td><code>{row.bucket_id}</code></td></tr>)}</tbody>
          </table>
        </div>
      </section>

      <section className="card" aria-labelledby="actions-heading">
        <div className="section-heading"><div><p className="eyebrow">Bounded commands</p><h2 id="actions-heading">Vertical slice actions</h2></div></div>
        <form className="post-form" onSubmit={(event) => void submitEvent(event)}>
          <label htmlFor="amount">Synthetic expense amount</label>
          <input id="amount" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.currentTarget.value)} />
          <button disabled={busy} type="submit">Post event</button>
        </form>
        <div className="actions">
          <button disabled={busy} onClick={() => void perform("Selecting workbook", async () => { const result = await invoke<{rowCount: number; elapsedMs: number}>("analyze_import"); return `Validated ${result.rowCount} rows in ${result.elapsedMs} ms`; })}>Analyze 10k XLSX</button>
          <button disabled={busy} onClick={() => void perform("Exporting", async () => { const result = await invoke<{fileName: string; rowCount: number}>("export_data"); return `Exported ${result.rowCount} rows to ${result.fileName}`; })}>Export XLSX</button>
          <button disabled={busy} onClick={() => void perform("Selecting attachment", async () => { const authorization = await invoke<{authorizationToken: string; displayName: string}>("authorize_attachment"); const copied = await invoke<{relativeLocation: string}>("copy_attachment", { request: { authorizationToken: authorization.authorizationToken } }); return `Copied ${authorization.displayName} to ${copied.relativeLocation}`; })}>Copy attachment</button>
        </div>
        <div className="backup-row">
          <label htmlFor="password">Backup password</label>
          <input id="password" type="password" autoComplete="new-password" value={password} onChange={(event) => setPassword(event.currentTarget.value)} />
          <button disabled={busy} onClick={() => void perform("Creating encrypted backup", async () => { const backup = await invoke<BackupSummary>("create_backup", { request: { password } }); setLatestBackup(backup); return `Backup ${backup.backupId} verified (${backup.packageBytes} bytes)`; })}>Create backup</button>
          <button disabled={busy || !latestBackup} onClick={() => void perform("Restoring encrypted backup", async () => { await invoke("restore_backup", { request: { backupId: latestBackup?.backupId, password } }); await refresh(page); return `Backup ${latestBackup?.backupId} restored and verified`; })}>Restore latest</button>
        </div>
      </section>
    </main>
  );
}

export default App;
