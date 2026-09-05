import { useEffect, useRef, useState, type CSSProperties, type FormEvent } from "react";
import type {
  CompositionItem,
  DrilldownContext,
  ExpenseAnalysis,
  ExpenseTopItem,
  Overview,
} from "../command-client/contracts";
import { translate, type SupportedLocale } from "./i18n";
import { LatestRequestGate } from "./queryGate";

type OverviewTab = "assets" | "expenses";
export type ExpenseUiState = "invalid" | "incomplete" | "unvalued-only" | "no-valued-spend" | "normal";

type OverviewPageProps = {
  locale: SupportedLocale;
  asOfDate: string;
  refreshVersion?: number;
  onLoadOverview: (request: { asOfDate: string }) => Promise<Overview>;
  onLoadExpense: (request: { startDate: string; endDate: string; eventWatermark?: number }) => Promise<ExpenseAnalysis>;
  onDrilldown: (context: DrilldownContext) => void;
  onOpenQuality: () => void;
};

function validLocalDate(value: string): boolean {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/u.exec(value);
  if (!match) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  if (year < 1 || month < 1 || month > 12 || day < 1) return false;
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return day <= (days[month - 1] ?? 0);
}

export function invalidExpenseDateFields(startDate: string, endDate: string): string[] {
  const fields: string[] = [];
  if (!validLocalDate(startDate)) fields.push("expenseStartDate");
  if (!validLocalDate(endDate)) fields.push("expenseEndDate");
  if (fields.length === 0 && startDate > endDate) fields.push("expenseStartDate", "expenseEndDate");
  return fields;
}

function decimalStringIsZero(value: string): boolean {
  return /^-?0+(?:\.0+)?$/u.test(value);
}

export function expenseUiState(result: ExpenseAnalysis | null, invalid: boolean): ExpenseUiState {
  if (invalid) return "invalid";
  if (!result) return "normal";
  const unvalued = result.unvalued.expense_count
    + result.refunds.refund.unvalued_count
    + result.refunds.reimbursement.unvalued_count;
  if (unvalued > 0) {
    return decimalStringIsZero(result.summary.valued_subtotal) && result.unvalued.expense_count > 0
      ? "unvalued-only"
      : "incomplete";
  }
  return decimalStringIsZero(result.summary.valued_subtotal) ? "no-valued-spend" : "normal";
}

export function formatBasisPoints(value: number): string {
  const whole = Math.trunc(value / 100);
  const fraction = String(value % 100).padStart(2, "0");
  return `${whole}.${fraction}%`;
}

function errorCode(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && typeof error.code === "string") return error.code;
  return "UNEXPECTED_ERROR";
}

function globalExpenseContext(result: ExpenseAnalysis): DrilldownContext {
  return {
    start_date: result.query.start_date,
    end_date: result.query.end_date,
    event_watermark: result.watermarks.event,
    calculation_version: result.versions.calculation,
    expense_policy_version: result.versions.expense_policy,
    semantic_role: "expense",
    valuation_state: "valued",
  };
}

function bucketLabel(item: { bucket_id: string; label: string }, t: (key: Parameters<typeof translate>[1]) => string): string {
  const key = item.bucket_id === "system:uncategorized"
    ? "activity.uncategorized"
    : item.bucket_id === "system:ordinary-fee"
      ? "activity.ordinaryFees"
      : item.bucket_id === "system:fx-fee"
        ? "activity.fxFees"
        : item.bucket_id === "system:top10-other"
          ? "expense.otherCategories"
          : null;
  return key ? t(key) : item.label;
}

export function OverviewPage(props: OverviewPageProps) {
  const t = (key: Parameters<typeof translate>[1]) => translate(props.locale, key);
  const [tab, setTab] = useState<OverviewTab>("assets");
  const [valuationDate, setValuationDate] = useState(props.asOfDate);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [overviewLoading, setOverviewLoading] = useState(false);
  const [overviewError, setOverviewError] = useState<string | null>(null);
  const [startDate, setStartDate] = useState(`${props.asOfDate.slice(0, 7)}-01`);
  const [endDate, setEndDate] = useState(props.asOfDate);
  const [dateErrors, setDateErrors] = useState<string[]>([]);
  const [expense, setExpense] = useState<ExpenseAnalysis | null>(null);
  const [expenseLoading, setExpenseLoading] = useState(false);
  const [expenseError, setExpenseError] = useState<string | null>(null);
  const expenseLoaded = useRef(false);
  const overviewGate = useRef(new LatestRequestGate());
  const expenseGate = useRef(new LatestRequestGate());

  async function loadOverview(date: string): Promise<void> {
    const generation = overviewGate.current.begin();
    setOverviewLoading(true);
    setOverviewError(null);
    try {
      const next = await props.onLoadOverview({ asOfDate: date });
      if (overviewGate.current.isLatest(generation)) setOverview(next);
    } catch (error: unknown) {
      if (overviewGate.current.isLatest(generation)) {
        setOverview(null);
        setOverviewError(errorCode(error));
      }
    } finally {
      if (overviewGate.current.isLatest(generation)) setOverviewLoading(false);
    }
  }

  async function loadExpense(): Promise<void> {
    const invalidFields = invalidExpenseDateFields(startDate, endDate);
    setDateErrors(invalidFields);
    if (invalidFields.length > 0) {
      expenseGate.current.invalidate();
      setExpense(null);
      setExpenseError(null);
      setExpenseLoading(false);
      requestAnimationFrame(() => document.getElementById(invalidFields[0] ?? "expenseStartDate")?.focus());
      return;
    }
    const generation = expenseGate.current.begin();
    setExpenseLoading(true);
    setExpenseError(null);
    try {
      const next = await props.onLoadExpense({ startDate, endDate });
      if (expenseGate.current.isLatest(generation)) {
        setExpense(next);
        expenseLoaded.current = true;
      }
    } catch (error: unknown) {
      if (expenseGate.current.isLatest(generation)) {
        setExpense(null);
        setExpenseError(errorCode(error));
      }
    } finally {
      if (expenseGate.current.isLatest(generation)) setExpenseLoading(false);
    }
  }

  useEffect(() => {
    if (validLocalDate(valuationDate)) void loadOverview(valuationDate);
    if (expenseLoaded.current || tab === "expenses") void loadExpense();
    return () => { overviewGate.current.invalidate(); expenseGate.current.invalidate(); };
  }, [props.asOfDate, props.refreshVersion]);

  function chooseTab(next: OverviewTab): void {
    setTab(next);
    if (next === "expenses" && !expenseLoaded.current && !expenseLoading) void loadExpense();
  }

  function submitOverview(event: FormEvent): void {
    event.preventDefault();
    if (!validLocalDate(valuationDate)) {
      overviewGate.current.invalidate();
      setOverviewLoading(false);
      setOverview(null);
      setOverviewError("LOCAL_DATE_INVALID");
      document.getElementById("overviewValuationDate")?.focus();
      return;
    }
    void loadOverview(valuationDate);
  }

  return <section className="overview-page">
    <header className="dashboard-heading">
      <div><p className="eyebrow">{t("overview.eyebrow")}</p><h1>{t("overview.title")}</h1><p>{t("overview.description")}</p></div>
      <div className="overview-tabs" role="tablist" aria-label={t("overview.tabsLabel")}>
        <button type="button" role="tab" aria-selected={tab === "assets"} onClick={() => chooseTab("assets")}>{t("overview.assetTab")}</button>
        <button type="button" role="tab" aria-selected={tab === "expenses"} onClick={() => chooseTab("expenses")}>{t("overview.expenseTab")}</button>
      </div>
    </header>
    <div className="sr-status" role="status" aria-live="polite">
      {overviewLoading || expenseLoading ? t("common.loading") : expenseError || overviewError ? t("overview.queryFailed") : t("overview.resultsUpdated")}
    </div>
    {tab === "assets" ? <div role="tabpanel">
      <form className="date-toolbar" onSubmit={submitOverview}>
        <label htmlFor="overviewValuationDate">{t("assets.valuationDate")}<input id="overviewValuationDate" type="date" value={valuationDate} aria-invalid={overviewError === "LOCAL_DATE_INVALID"} onChange={(event) => setValuationDate(event.currentTarget.value)} /></label>
        <button type="submit" disabled={overviewLoading}>{t("overview.refresh")}</button>
      </form>
      {overviewError ? <p className="inline-error" role="alert">{t("overview.queryFailed")}: <code>{overviewError}</code></p> : null}
      {overview ? <AssetOverview result={overview} locale={props.locale} onOpenQuality={props.onOpenQuality} /> : null}
    </div> : <div role="tabpanel">
      <form className="date-toolbar expense-date-toolbar" noValidate onSubmit={(event) => { event.preventDefault(); void loadExpense(); }}>
        <label htmlFor="expenseStartDate">{t("activity.startDate")}<input id="expenseStartDate" type="date" value={startDate} aria-invalid={dateErrors.includes("expenseStartDate")} aria-describedby={dateErrors.includes("expenseStartDate") ? "expenseDateError" : undefined} onChange={(event) => { setStartDate(event.currentTarget.value); setDateErrors([]); }} /></label>
        <label htmlFor="expenseEndDate">{t("activity.endDate")}<input id="expenseEndDate" type="date" value={endDate} aria-invalid={dateErrors.includes("expenseEndDate")} aria-describedby={dateErrors.includes("expenseEndDate") ? "expenseDateError" : undefined} onChange={(event) => { setEndDate(event.currentTarget.value); setDateErrors([]); }} /></label>
        <button type="submit" disabled={expenseLoading}>{t("expense.apply")}</button>
      </form>
      {dateErrors.length > 0 ? <p id="expenseDateError" className="inline-error" role="alert">{t("expense.invalidRange")}</p> : null}
      {expenseError ? <p className="inline-error" role="alert">{t("overview.queryFailed")}: <code>{expenseError}</code></p> : null}
      <ExpenseAnalysisView locale={props.locale} result={expense} state={expenseUiState(expense, dateErrors.length > 0)} onDrilldown={props.onDrilldown} onOpenQuality={props.onOpenQuality} />
    </div>}
  </section>;
}

function AssetOverview({ result, locale, onOpenQuality }: { result: Overview; locale: SupportedLocale; onOpenQuality: () => void }) {
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  const metrics = [
    [t("overview.valuedNetAssets"), result.valuedNetAssets],
    [t("overview.valuedCash"), result.valuedCash],
    [t("overview.valuedHoldings"), result.valuedHoldings],
    [t("overview.mtdExpense"), result.mtdExpense],
  ];
  return <>
    <dl className="dashboard-kpis">{metrics.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{result.baseCurrency} {value}</dd></div>)}</dl>
    <p className="dashboard-meta">{t("overview.valuationAsOf")}: <strong>{result.valuationDate}</strong> · {t("overview.mtdRange")}: {result.mtdStartDate}–{result.mtdEndDate}</p>
    <div className="composition-grid">
      <CompositionTable title={t("overview.byInstitution")} items={result.composition.institutions} currency={result.baseCurrency} />
      <CompositionTable title={t("overview.byCurrency")} items={result.composition.currencies} currency={result.baseCurrency} />
      <CompositionTable title={t("overview.byCashAccount")} items={result.composition.cashAccounts} currency={result.baseCurrency} />
      <CompositionTable title={t("overview.byHolding")} items={result.composition.holdings} currency={result.baseCurrency} />
    </div>
    <section className="quality-card" aria-labelledby="overview-quality-title"><div><h2 id="overview-quality-title">{t("overview.qualityTodo")}</h2><button type="button" className="secondary" onClick={onOpenQuality}>{t("overview.openQuality")}</button></div>
      {result.unvaluedAssets.length === 0 && result.anomalyCodes.length === 0 && result.mtdUnvaluedExpenseCount === 0 ? <p className="empty-state">{t("quality.empty")}</p> : <>
        <p>{t("overview.unvaluedAssets")}: <strong>{result.unvaluedAssets.length}</strong> · {t("overview.unvaluedExpenses")}: <strong>{result.mtdUnvaluedExpenseCount}</strong></p>
        <ul className="issue-list">{result.unvaluedAssets.map((asset) => <li key={`${asset.assetType}-${asset.entityId}`}><strong>{asset.reason}</strong><code>{asset.entityId}</code><span>{asset.nativeCurrency} {asset.nativeValue}</span></li>)}</ul>
        {result.anomalyCodes.length > 0 ? <p><strong>{t("overview.anomalies")}</strong>: {result.anomalyCodes.join(", ")}</p> : null}
      </>}
    </section>
  </>;
}

function CompositionTable({ title, items, currency }: { title: string; items: CompositionItem[]; currency: string }) {
  return <section className="composition-card"><h2>{title}</h2>{items.length === 0 ? <p className="empty-state">—</p> : <div className="table-scroll"><table><thead><tr><th scope="col">{title}</th><th scope="col">{currency}</th></tr></thead><tbody>{items.map((item) => <tr key={item.id}><th scope="row">{item.label}</th><td>{item.baseValue}</td></tr>)}</tbody></table></div>}</section>;
}

export function ExpenseAnalysisView({ locale, result, state, onDrilldown, onOpenQuality }: { locale: SupportedLocale; result: ExpenseAnalysis | null; state: ExpenseUiState; onDrilldown: (context: DrilldownContext) => void; onOpenQuality: () => void }) {
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  if (state === "invalid" || !result) return null;
  const totalLabel = result.summary.total_expense === null ? t("expense.valuedSubtotal") : t("expense.total");
  const totalValue = result.summary.total_expense ?? result.summary.valued_subtotal;
  const largest = result.summary.largest_category;
  const largestRow = largest ? result.buckets.find((item) => item.bucket_id === largest.bucket_id) : null;
  const chartItems = [...result.top10.items, ...(result.top10.other ? [result.top10.other] : [])];
  return <>
    {state === "incomplete" || state === "unvalued-only" ? <div className="quality-warning" role="status"><strong>{t(state === "unvalued-only" ? "expense.unvaluedOnly" : "expense.incomplete")}</strong><button type="button" className="secondary" onClick={onOpenQuality}>{t("overview.openQuality")}</button></div> : null}
    {state === "no-valued-spend" ? <p className="notice">{t("expense.noSpend")}</p> : null}
    <dl className="dashboard-kpis expense-kpis">
      <KpiButton label={totalLabel} value={`${result.query.base_currency} ${totalValue}`} onClick={() => onDrilldown(globalExpenseContext(result))} />
      <KpiButton label={t("expense.distinctEvents")} value={String(result.summary.global_distinct_event_count)} onClick={() => onDrilldown(globalExpenseContext(result))} />
      <KpiButton label={t("expense.largestCategory")} value={largestRow ? `${bucketLabel(largestRow, t)} · ${result.query.base_currency} ${largest?.amount}` : "—"} disabled={!largestRow} onClick={() => largestRow && onDrilldown(largestRow.drilldown_context)} />
      <KpiButton label={t("expense.refunds")} value={`${result.query.base_currency} ${result.refunds.refund.amount} · ${result.refunds.refund.distinct_event_count} · ${t("expense.unvaluedShort")} ${result.refunds.refund.unvalued_count}`} onClick={() => onDrilldown(result.refunds.refund.drilldown_context)} />
      <KpiButton label={t("expense.reimbursements")} value={`${result.query.base_currency} ${result.refunds.reimbursement.amount} · ${result.refunds.reimbursement.distinct_event_count} · ${t("expense.unvaluedShort")} ${result.refunds.reimbursement.unvalued_count}`} onClick={() => onDrilldown(result.refunds.reimbursement.drilldown_context)} />
      <KpiButton label={t("expense.unvaluedCount")} value={String(result.unvalued.expense_count)} disabled={result.unvalued.expense_count === 0} onClick={() => onDrilldown(result.unvalued.drilldown_context)} />
    </dl>
    <p className="dashboard-meta">{result.query.start_date}–{result.query.end_date} · {result.versions.calculation} · {t("common.watermark")} {result.watermarks.event}</p>
    <div className="expense-layout">
      <section className="expense-chart" aria-labelledby="expense-ranking-title"><h2 id="expense-ranking-title">{t("expense.ranking")}</h2>
        {chartItems.length === 0 ? <p className="empty-state">{t("expense.noSpend")}</p> : <ol className="expense-bars" aria-hidden="true">{chartItems.map((item) => <ExpenseBar key={item.bucket_id} item={item} currency={result.query.base_currency} label={bucketLabel(item, t)} />)}</ol>}
      </section>
      <section className="expense-table" aria-labelledby="expense-detail-title"><h2 id="expense-detail-title">{t("expense.detail")}</h2>
        <div className="table-scroll"><table><thead><tr><th scope="col">{t("expense.category")}</th><th scope="col">{t("expense.amount")}</th><th scope="col">{t("expense.share")}</th><th scope="col">{t("expense.count")}</th></tr></thead><tbody>{result.buckets.map((item) => <tr key={item.bucket_id}><th scope="row"><button type="button" className="table-link" onClick={() => onDrilldown(item.drilldown_context)}>{bucketLabel(item, t)}{item.archived ? ` (${t("common.archived")})` : ""}</button></th><td>{result.query.base_currency} {item.amount}</td><td>{formatBasisPoints(item.share_basis_points)}</td><td>{item.distinct_event_count}</td></tr>)}</tbody></table></div>
      </section>
    </div>
  </>;
}

function KpiButton({ label, value, onClick, disabled = false }: { label: string; value: string; onClick: () => void; disabled?: boolean }) {
  return <div><dt>{label}</dt><dd><button type="button" className="kpi-link" disabled={disabled} onClick={onClick}>{value}</button></dd></div>;
}

function ExpenseBar({ item, currency, label }: { item: ExpenseTopItem; currency: string; label: string }) {
  const style = { "--share-bps": item.share_basis_points } as CSSProperties;
  return <li><span className="expense-bar-label">{label}</span><span className="expense-bar-track"><span className="expense-bar-fill" style={style} /></span><span>{currency} {item.amount} · {formatBasisPoints(item.share_basis_points)} · {item.distinct_event_count}</span></li>;
}
