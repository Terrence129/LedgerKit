import { useEffect, useRef, useState, type FormEvent } from "react";
import type {
  ActivityItem,
  ActivityPage as ActivityPageResult,
  ActivityPosting,
  ActivityRequest,
  CashEventRequest,
  CatalogRecord,
  EventPreview,
  LedgerStatus,
  PostedEvent,
  InvestmentEventPreview,
  InvestmentEventRequest,
  PostedInvestmentEvent,
  DrilldownContext,
} from "../command-client/contracts";
import { translate, type SupportedLocale } from "./i18n";
import { InvestmentEditor } from "./InvestmentEditor";
import { LatestRequestGate } from "./queryGate";

type CashEventType = CashEventRequest["eventType"];
type FxOverrideDraft = { currency: string; value: string; reason: string };

export type CashDraft = {
  effectiveDate: string;
  sequence: number;
  eventType: CashEventType;
  accountId: string;
  fromAccountId: string;
  toAccountId: string;
  amount: string;
  toAmount: string;
  categoryId: string;
  semanticRole: "normal" | "refund" | "reimbursement";
  merchant: string;
  note: string;
  feeAccountId: string;
  feeAmount: string;
  cutoverDate: string;
  migrationPolicy: "full_history" | "explicit_cutover";
  fxOverrides: FxOverrideDraft[];
  currencyPrecisionConfirmed: boolean;
};

type ActivityPageProps = {
  locale: SupportedLocale;
  status: LedgerStatus;
  busy: boolean;
  initialContext?: DrilldownContext | null;
  onLoad: (request: ActivityRequest) => Promise<ActivityPageResult>;
  onPreview: (request: CashEventRequest) => Promise<EventPreview>;
  onPost: (request: CashEventRequest) => Promise<PostedEvent>;
  onRevise: (request: { targetEventId: string; reason: string; replacement: CashEventRequest }) => Promise<PostedEvent>;
  onReverse: (request: { targetEventId: string; reason: string; effectiveDate: string; sequence: number }) => Promise<PostedEvent>;
  onPreviewInvestment?: (request: InvestmentEventRequest) => Promise<InvestmentEventPreview>;
  onPostInvestment?: (request: InvestmentEventRequest) => Promise<PostedInvestmentEvent>;
};

const decimalPattern = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/u;

export function createCashDraft(effectiveDate: string, sequence: number): CashDraft {
  return {
    effectiveDate,
    sequence,
    eventType: "Expense",
    accountId: "",
    fromAccountId: "",
    toAccountId: "",
    amount: "",
    toAmount: "",
    categoryId: "",
    semanticRole: "normal",
    merchant: "",
    note: "",
    feeAccountId: "",
    feeAmount: "",
    cutoverDate: "",
    migrationPolicy: "explicit_cutover",
    fxOverrides: [],
    currencyPrecisionConfirmed: false,
  };
}

function decimalIsZero(value: string): boolean {
  return value.replace(/^[+-]/u, "").replace(".", "").replace(/0/gu, "") === "";
}

export function validateCashDraft(draft: CashDraft): string[] {
  const errors: string[] = [];
  if (!draft.effectiveDate) errors.push("date");
  if (!decimalPattern.test(draft.amount)) errors.push("amount");
  if (["Income", "Expense", "Transfer", "CurrencyExchange"].includes(draft.eventType)
      && (draft.amount.startsWith("-") || decimalIsZero(draft.amount))) errors.push("amount");
  if (draft.eventType === "Adjustment" && decimalPattern.test(draft.amount) && decimalIsZero(draft.amount)) errors.push("amount");
  if (["Income", "Expense", "Adjustment", "OpeningBalance"].includes(draft.eventType) && !draft.accountId) errors.push("accountId");
  if (["Transfer", "CurrencyExchange"].includes(draft.eventType)) {
    if (!draft.fromAccountId) errors.push("fromAccountId");
    if (!draft.toAccountId || draft.toAccountId === draft.fromAccountId) errors.push("toAccountId");
  }
  if (draft.eventType === "CurrencyExchange"
      && (!decimalPattern.test(draft.toAmount) || draft.toAmount.startsWith("-") || decimalIsZero(draft.toAmount))) errors.push("toAmount");
  if (draft.eventType === "OpeningBalance" && (!draft.cutoverDate || !draft.migrationPolicy)) errors.push("cutoverDate");
  if (draft.feeAmount && (!decimalPattern.test(draft.feeAmount) || draft.feeAmount.startsWith("-") || decimalIsZero(draft.feeAmount))) errors.push("feeAmount");
  if (draft.feeAmount && !draft.feeAccountId) errors.push("feeAccountId");
  draft.fxOverrides.forEach((override, index) => {
    if (!/^[A-Z]{3}$/u.test(override.currency) || !decimalPattern.test(override.value) || !override.reason.trim()) {
      errors.push(`fxOverride-${index}`);
    }
  });
  return [...new Set(errors)];
}

export function toCashEventRequest(draft: CashDraft): CashEventRequest {
  const request: CashEventRequest = {
    effectiveDate: draft.effectiveDate,
    sequence: draft.sequence,
    eventType: draft.eventType,
    semanticRole: draft.semanticRole,
    currencyPrecisionConfirmed: draft.currencyPrecisionConfirmed,
  };
  const assign = (key: "accountId" | "fromAccountId" | "toAccountId" | "amount" | "toAmount" | "categoryId" | "merchant" | "note" | "feeAccountId" | "feeAmount" | "cutoverDate", value: string) => {
    const trimmed = value.trim();
    if (trimmed) request[key] = trimmed;
  };
  assign("accountId", draft.accountId);
  assign("fromAccountId", draft.fromAccountId);
  assign("toAccountId", draft.toAccountId);
  assign("amount", draft.amount);
  assign("toAmount", draft.toAmount);
  assign("categoryId", draft.categoryId);
  assign("merchant", draft.merchant);
  assign("note", draft.note);
  assign("feeAccountId", draft.feeAccountId);
  assign("feeAmount", draft.feeAmount);
  assign("cutoverDate", draft.cutoverDate);
  if (draft.eventType === "OpeningBalance") request.migrationPolicy = draft.migrationPolicy;
  if (draft.fxOverrides.length > 0) request.fxOverrides = draft.fxOverrides;
  return request;
}

function draftFromActivity(item: ActivityItem): CashDraft | null {
  if (item.eventType === "Reversal") return null;
  const eventType = item.eventType === "BalanceAdjustment" ? "Adjustment" : item.eventType as CashEventType;
  return {
    ...createCashDraft(item.effectiveDate, item.sequence),
    eventType,
    accountId: item.content.accountId ?? "",
    fromAccountId: item.content.fromAccountId ?? "",
    toAccountId: item.content.toAccountId ?? "",
    amount: item.content.amount ?? "",
    toAmount: item.content.toAmount ?? "",
    categoryId: item.content.categoryId ?? "",
    semanticRole: item.content.semanticRole === "refund" || item.content.semanticRole === "reimbursement" ? item.content.semanticRole : "normal",
    merchant: item.content.merchant ?? "",
    note: item.content.note ?? "",
    feeAccountId: item.content.feeAccountId ?? "",
    feeAmount: item.content.feeAmount ?? "",
    cutoverDate: item.content.cutoverDate ?? "",
    migrationPolicy: item.content.migrationPolicy === "full_history" ? "full_history" : "explicit_cutover",
    fxOverrides: item.fxResolutions.flatMap((resolution) => resolution.overrideValue === null ? [] : [{
      currency: resolution.currency,
      value: resolution.overrideValue,
      reason: resolution.overrideReason ?? "",
    }]),
  };
}

function errorCode(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && typeof error.code === "string") return error.code;
  return "UNEXPECTED_ERROR";
}

function focusField(field: string): void {
  requestAnimationFrame(() => document.getElementById(field)?.focus());
}

function displayEventType(value: string, t: (key: Parameters<typeof translate>[1]) => string): string {
  const key = value === "BalanceAdjustment" ? "event.adjustment" : `event.${value.charAt(0).toLowerCase()}${value.slice(1)}`;
  return t(key as Parameters<typeof translate>[1]);
}

export function ActivityPage(props: ActivityPageProps) {
  const { locale, status, busy } = props;
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  const catalog = status.catalog;
  const today = catalog?.asOfDate ?? new Date().toISOString().slice(0, 10);
  const [draft, setDraft] = useState(() => createCashDraft(today, status.eventWatermark + 1));
  const [preview, setPreview] = useState<EventPreview | null>(null);
  const [editingTarget, setEditingTarget] = useState<ActivityItem | null>(null);
  const [revisionReason, setRevisionReason] = useState("");
  const [formErrors, setFormErrors] = useState<string[]>([]);
  const [editorError, setEditorError] = useState<string | null>(null);
  const [startDate, setStartDate] = useState(`${today.slice(0, 7)}-01`);
  const [endDate, setEndDate] = useState(today);
  const [eventType, setEventType] = useState<ActivityRequest["eventType"] | "">("");
  const [accountId, setAccountId] = useState("");
  const [categoryId, setCategoryId] = useState("");
  const [search, setSearch] = useState("");
  const [drilldownContext, setDrilldownContext] = useState<DrilldownContext | null>(props.initialContext ?? null);
  const [page, setPage] = useState<ActivityPageResult>({ items: [], nextCursor: null });
  const [loading, setLoading] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ActivityItem | null>(null);
  const [reverseTarget, setReverseTarget] = useState<ActivityItem | null>(null);
  const [reverseReason, setReverseReason] = useState("");
  const [reverseDate, setReverseDate] = useState(today);
  const loadedKey = useRef("");
  const queryGate = useRef(new LatestRequestGate());

  const accountOptions = catalog?.accounts.filter((item) => item.enabled) ?? [];
  const categoryOptions = catalog?.categories.filter((item) => item.enabled) ?? [];

  function request(cursor?: number, context = drilldownContext): ActivityRequest {
    return {
      startDate,
      endDate,
      ...(context ? { context } : {}),
      ...(eventType ? { eventType } : {}),
      ...(accountId ? { accountId } : {}),
      ...(categoryId ? { categoryId } : {}),
      ...(search.trim() ? { search: search.trim() } : {}),
      ...(cursor === undefined ? {} : { cursor }),
      limit: 25,
    };
  }

  async function load(reset: boolean, context = drilldownContext): Promise<void> {
    const generation = queryGate.current.begin();
    setLoading(true);
    setListError(null);
    try {
      const next = await props.onLoad(request(reset ? undefined : page.nextCursor ?? undefined, context));
      if (!queryGate.current.isLatest(generation)) return;
      setPage((current) => ({
        items: reset ? next.items : [...current.items, ...next.items],
        nextCursor: next.nextCursor,
      }));
    } catch (error: unknown) {
      if (queryGate.current.isLatest(generation)) setListError(errorCode(error));
    } finally {
      if (queryGate.current.isLatest(generation)) setLoading(false);
    }
  }

  useEffect(() => {
    const key = `${status.ledgerId}:${status.eventWatermark}`;
    if (loadedKey.current === key) return;
    loadedKey.current = key;
    void load(true);
  }, [status.ledgerId, status.eventWatermark]);

  useEffect(() => {
    const context = props.initialContext;
    if (!context) {
      setDrilldownContext(null);
      return;
    }
    setStartDate(context.start_date);
    setEndDate(context.end_date);
    setEventType("");
    setAccountId("");
    setCategoryId("");
    setSearch("");
    setDrilldownContext(context);
    const generation = queryGate.current.begin();
    setLoading(true);
    setListError(null);
    void props.onLoad({ startDate: context.start_date, endDate: context.end_date, context, limit: 25 })
      .then((next) => {
        if (queryGate.current.isLatest(generation)) setPage(next);
      })
      .catch((error: unknown) => {
        if (queryGate.current.isLatest(generation)) setListError(errorCode(error));
      })
      .finally(() => {
        if (queryGate.current.isLatest(generation)) setLoading(false);
      });
  }, [props.initialContext]);

  function clearDrilldown(): void {
    setDrilldownContext(null);
  }

  function updateDraft(patch: Partial<CashDraft>): void {
    setDraft((current) => ({ ...current, ...patch }));
    setPreview(null);
    setEditorError(null);
  }

  async function previewDraft(event: FormEvent): Promise<void> {
    event.preventDefault();
    const errors = validateCashDraft(draft);
    if (editingTarget && !revisionReason.trim()) errors.push("reason");
    setFormErrors(errors);
    if (errors.length > 0) {
      focusField(errors[0] ?? "amount");
      return;
    }
    setEditorError(null);
    try {
      setPreview(await props.onPreview(toCashEventRequest(draft)));
    } catch (error: unknown) {
      setEditorError(errorCode(error));
    }
  }

  async function savePreviewed(): Promise<void> {
    if (!preview) return;
    try {
      if (editingTarget) {
        await props.onRevise({
          targetEventId: editingTarget.eventId,
          reason: revisionReason.trim(),
          replacement: toCashEventRequest(draft),
        });
      } else {
        await props.onPost(toCashEventRequest(draft));
      }
      setDraft(createCashDraft(today, status.eventWatermark + 2));
      setPreview(null);
      setEditingTarget(null);
      setRevisionReason("");
      setFormErrors([]);
    } catch (error: unknown) {
      setEditorError(errorCode(error));
    }
  }

  function beginRevision(item: ActivityItem): void {
    const replacement = draftFromActivity(item);
    if (!replacement) return;
    setDraft(replacement);
    setEditingTarget(item);
    setRevisionReason("");
    setPreview(null);
    setEditorError(null);
    focusField("amount");
  }

  async function confirmReversal(event: FormEvent): Promise<void> {
    event.preventDefault();
    if (!reverseTarget || !reverseReason.trim() || !reverseDate) {
      focusField(!reverseReason.trim() ? "reverseReason" : "reverseDate");
      return;
    }
    try {
      await props.onReverse({
        targetEventId: reverseTarget.eventId,
        reason: reverseReason.trim(),
        effectiveDate: reverseDate,
        sequence: status.eventWatermark + 1,
      });
      setReverseTarget(null);
      setReverseReason("");
    } catch (error: unknown) {
      setEditorError(errorCode(error));
    }
  }

  if (!catalog) return null;
  const singleAccount = ["Income", "Expense", "Adjustment", "OpeningBalance"].includes(draft.eventType);
  const twoAccounts = draft.eventType === "Transfer" || draft.eventType === "CurrencyExchange";
  const canChange = (item: ActivityItem) => item.eventType !== "Reversal"
    && item.relations.supersededByEventId === null
    && item.relations.reversedByEventId === null;

  return (
    <section className="activity-page" aria-labelledby="activity-title">
      <div className="page-heading activity-heading">
        <div><p className="eyebrow">{t("activity.eyebrow")}</p><h1 id="activity-title">{t("activity.title")}</h1><p className="lede">{t("activity.description")}</p></div>
        <a className="primary-link" href="#cash-editor">{t("activity.add")}</a>
      </div>

      <form id="cash-editor" className="cash-editor" onSubmit={(event) => void previewDraft(event)} noValidate>
        <div className="section-title"><div><p className="eyebrow">{editingTarget ? t("activity.reviseEyebrow") : t("activity.addEyebrow")}</p><h2>{editingTarget ? t("activity.reviseTitle") : t("activity.addTitle")}</h2></div>{editingTarget ? <button type="button" className="secondary" onClick={() => { setEditingTarget(null); setRevisionReason(""); setDraft(createCashDraft(today, status.eventWatermark + 1)); setPreview(null); }}>{t("common.cancel")}</button> : null}</div>
        {editorError ? <p className="inline-error" role="alert">{t("common.error")}: <code>{editorError}</code></p> : null}
        {formErrors.length > 0 ? <p className="inline-error" role="alert">{t("activity.validation")}</p> : null}
        <div className="editor-grid">
          <label htmlFor="eventType">{t("activity.eventType")}<select id="eventType" value={draft.eventType} disabled={Boolean(editingTarget)} onChange={(event) => updateDraft({ eventType: event.currentTarget.value as CashEventType })}>
            {draft.eventType === "OpeningBalance" ? <option value="OpeningBalance">{t("event.openingBalance")}</option> : null}
            <option value="Income">{t("event.income")}</option><option value="Expense">{t("event.expense")}</option><option value="Adjustment">{t("event.adjustment")}</option><option value="Transfer">{t("event.transfer")}</option><option value="CurrencyExchange">{t("event.currencyExchange")}</option>
          </select></label>
          <label htmlFor="date">{t("field.date")}<input id="date" type="date" value={draft.effectiveDate} aria-invalid={formErrors.includes("date")} onChange={(event) => updateDraft({ effectiveDate: event.currentTarget.value })} /></label>
          {singleAccount ? <AccountSelect id="accountId" label={t("activity.account")} value={draft.accountId} options={accountOptions} invalid={formErrors.includes("accountId")} onChange={(value) => updateDraft({ accountId: value })} /> : null}
          {twoAccounts ? <><AccountSelect id="fromAccountId" label={t("activity.fromAccount")} value={draft.fromAccountId} options={accountOptions} invalid={formErrors.includes("fromAccountId")} onChange={(value) => updateDraft({ fromAccountId: value })} /><AccountSelect id="toAccountId" label={t("activity.toAccount")} value={draft.toAccountId} options={accountOptions} invalid={formErrors.includes("toAccountId")} onChange={(value) => updateDraft({ toAccountId: value })} /></> : null}
          <label htmlFor="amount">{draft.eventType === "Adjustment" ? t("activity.delta") : draft.eventType === "CurrencyExchange" ? t("activity.fromAmount") : t("activity.amount")}<input id="amount" inputMode="decimal" value={draft.amount} aria-invalid={formErrors.includes("amount")} onChange={(event) => updateDraft({ amount: event.currentTarget.value })} /></label>
          {draft.eventType === "CurrencyExchange" ? <label htmlFor="toAmount">{t("activity.toAmount")}<input id="toAmount" inputMode="decimal" value={draft.toAmount} aria-invalid={formErrors.includes("toAmount")} onChange={(event) => updateDraft({ toAmount: event.currentTarget.value })} /></label> : null}
          {["Income", "Expense", "Adjustment"].includes(draft.eventType) ? <><label htmlFor="categoryId">{t("activity.category")}<select id="categoryId" value={draft.categoryId} onChange={(event) => updateDraft({ categoryId: event.currentTarget.value })}><option value="">{t("activity.uncategorized")}</option>{categoryOptions.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label><label htmlFor="semanticRole">{t("field.semanticRole")}<select id="semanticRole" value={draft.semanticRole} onChange={(event) => updateDraft({ semanticRole: event.currentTarget.value as CashDraft["semanticRole"] })}><option value="normal">{t("value.normal")}</option><option value="refund">{t("value.refund")}</option><option value="reimbursement">{t("value.reimbursement")}</option></select></label><label htmlFor="merchant">{t("activity.merchant")}<input id="merchant" value={draft.merchant} onChange={(event) => updateDraft({ merchant: event.currentTarget.value })} /></label><label htmlFor="note">{t("activity.note")}<input id="note" value={draft.note} onChange={(event) => updateDraft({ note: event.currentTarget.value })} /></label></> : null}
          {draft.eventType !== "OpeningBalance" && draft.eventType !== "Transfer" ? <><AccountSelect id="feeAccountId" label={t("activity.feeAccount")} value={draft.feeAccountId} options={accountOptions} invalid={formErrors.includes("feeAccountId")} optional onChange={(value) => updateDraft({ feeAccountId: value })} /><label htmlFor="feeAmount">{t("activity.feeAmount")}<input id="feeAmount" inputMode="decimal" value={draft.feeAmount} aria-invalid={formErrors.includes("feeAmount")} onChange={(event) => updateDraft({ feeAmount: event.currentTarget.value })} /></label></> : null}
          {draft.eventType === "OpeningBalance" ? <><label htmlFor="cutoverDate">{t("activity.cutoverDate")}<input id="cutoverDate" type="date" value={draft.cutoverDate} onChange={(event) => updateDraft({ cutoverDate: event.currentTarget.value })} /></label><label htmlFor="migrationPolicy">{t("activity.migrationPolicy")}<select id="migrationPolicy" value={draft.migrationPolicy} onChange={(event) => updateDraft({ migrationPolicy: event.currentTarget.value as CashDraft["migrationPolicy"] })}><option value="full_history">{t("activity.fullHistory")}</option><option value="explicit_cutover">{t("activity.explicitCutover")}</option></select></label></> : null}
          {editingTarget ? <label htmlFor="reason" className="wide-field">{t("activity.revisionReason")}<textarea id="reason" value={revisionReason} aria-invalid={formErrors.includes("reason")} onChange={(event) => { setRevisionReason(event.currentTarget.value); setPreview(null); }} /></label> : null}
        </div>
        <fieldset className="fx-overrides"><legend>{t("activity.fxOverrides")}</legend><p>{t("activity.fxOverrideHelp")}</p>{draft.fxOverrides.map((override, index) => <div className="override-row" key={`${index}-${override.currency}`}><label htmlFor={`fxOverride-${index}`}>{t("field.currency")}<input id={`fxOverride-${index}`} value={override.currency} maxLength={3} onChange={(event) => updateDraft({ fxOverrides: draft.fxOverrides.map((item, itemIndex) => itemIndex === index ? { ...item, currency: event.currentTarget.value.toUpperCase() } : item) })} /></label><label>{t("activity.rate")}<input inputMode="decimal" value={override.value} onChange={(event) => updateDraft({ fxOverrides: draft.fxOverrides.map((item, itemIndex) => itemIndex === index ? { ...item, value: event.currentTarget.value } : item) })} /></label><label>{t("activity.overrideReason")}<input value={override.reason} onChange={(event) => updateDraft({ fxOverrides: draft.fxOverrides.map((item, itemIndex) => itemIndex === index ? { ...item, reason: event.currentTarget.value } : item) })} /></label><button type="button" className="secondary" onClick={() => updateDraft({ fxOverrides: draft.fxOverrides.filter((_, itemIndex) => itemIndex !== index) })}>{t("activity.removeOverride")}</button></div>)}<button type="button" className="secondary" onClick={() => updateDraft({ fxOverrides: [...draft.fxOverrides, { currency: "", value: "", reason: "" }] })}>{t("activity.addOverride")}</button></fieldset>
        <label className="checkbox-field precision-confirmation"><input type="checkbox" checked={draft.currencyPrecisionConfirmed} onChange={(event) => updateDraft({ currencyPrecisionConfirmed: event.currentTarget.checked })} />{t("activity.precisionConfirmed")}</label>
        <div className="form-actions"><button type="submit" disabled={busy}>{t("activity.preview")}</button>{preview ? <button type="button" disabled={busy} onClick={() => void savePreviewed()}>{editingTarget ? t("activity.confirmRevision") : t("activity.confirmPost")}</button> : null}</div>
      </form>

      {preview ? <PreviewPanel preview={preview} categoryLabel={preview.categoryId ? categoryOptions.find((item) => item.id === preview.categoryId)?.name ?? preview.categoryId : t("activity.uncategorized")} t={t} /> : null}

      {props.onPreviewInvestment && props.onPostInvestment ? <InvestmentEditor locale={locale} status={status} busy={busy} onPreview={props.onPreviewInvestment} onPost={props.onPostInvestment} /> : null}

      <section className="timeline-section" aria-labelledby="timeline-title">
        <div className="section-title"><div><p className="eyebrow">{t("activity.timelineEyebrow")}</p><h2 id="timeline-title">{t("activity.timelineTitle")}</h2></div></div>
        <form className="filter-grid" onSubmit={(event) => { event.preventDefault(); void load(true); }}>
          {drilldownContext ? <p className="notice wide-field">{t("activity.drilldownActive")}</p> : null}
          <label htmlFor="dateRange">{t("activity.startDate")}<input id="dateRange" type="date" value={startDate} onChange={(event) => { clearDrilldown(); setStartDate(event.currentTarget.value); }} /></label><label>{t("activity.endDate")}<input type="date" value={endDate} onChange={(event) => { clearDrilldown(); setEndDate(event.currentTarget.value); }} /></label>
          <label>{t("activity.eventType")}<select value={eventType} onChange={(event) => { clearDrilldown(); setEventType(event.currentTarget.value as ActivityRequest["eventType"] | ""); }}><option value="">{t("activity.all")}</option><option value="Income">{t("event.income")}</option><option value="Expense">{t("event.expense")}</option><option value="Adjustment">{t("event.adjustment")}</option><option value="Transfer">{t("event.transfer")}</option><option value="CurrencyExchange">{t("event.currencyExchange")}</option><option value="SecurityBuy">{t("event.securityBuy")}</option><option value="SecuritySell">{t("event.securitySell")}</option><option value="Dividend">{t("event.dividend")}</option><option value="InvestmentExpense">{t("event.investmentExpense")}</option><option value="Reversal">{t("event.reversal")}</option></select></label>
          <AccountSelect id="activityAccount" label={t("activity.account")} value={accountId} options={accountOptions} optional onChange={(value) => { clearDrilldown(); setAccountId(value); }} />
          <label>{t("activity.category")}<select value={categoryId} onChange={(event) => { clearDrilldown(); setCategoryId(event.currentTarget.value); }}><option value="">{t("activity.all")}</option><option value="system:uncategorized">{t("activity.uncategorized")}</option><option value="system:ordinary-fee">{t("activity.ordinaryFees")}</option><option value="system:fx-fee">{t("activity.fxFees")}</option>{categoryOptions.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
          <label className="wide-field">{t("activity.search")}<input id="search" type="search" maxLength={200} value={search} onChange={(event) => { clearDrilldown(); setSearch(event.currentTarget.value); }} /></label><button type="submit" disabled={loading}>{t("activity.applyFilters")}</button>
        </form>
        <div className="sr-status" role="status" aria-live="polite">{loading ? t("activity.loading") : t("activity.loaded")}</div>
        {listError ? <p className="inline-error" role="alert">{t("activity.loadFailed")}: <code>{listError}</code></p> : null}
        {page.items.length === 0 && !loading ? <p className="empty-state">{t("activity.empty")}</p> : <ol className="timeline-list">{page.items.map((item) => <li key={item.eventId}><button type="button" className="timeline-item" onClick={() => setSelected(item)}><span className="event-glyph" aria-hidden="true">{item.eventType === "Reversal" ? "↶" : item.eventType === "Income" ? "+" : item.eventType === "Expense" ? "−" : "↔"}</span><span><strong>{displayEventType(item.eventType, t)}</strong><small>{item.effectiveDate} · r{item.revision}</small><span>{item.content.merchant ?? item.content.note ?? item.content.amount ?? item.eventId}</span></span><span className="timeline-value">{item.content.amount ?? item.content.toAmount ?? "—"}</span></button></li>)}</ol>}
        {page.nextCursor !== null ? <button type="button" className="secondary load-more" disabled={loading} onClick={() => void load(false)}>{t("activity.loadMore")}</button> : null}
      </section>

      {selected ? <ActivityDetails item={selected} accounts={catalog.accounts} categories={catalog.categories} t={t} canChange={canChange(selected)} onClose={() => setSelected(null)} onRevise={() => { beginRevision(selected); setSelected(null); }} onReverse={() => { setReverseTarget(selected); setReverseDate(today); setSelected(null); }} /> : null}
      {reverseTarget ? <form className="detail-panel reversal-panel" aria-labelledby="reversal-title" onSubmit={(event) => void confirmReversal(event)}><div className="detail-header"><h2 id="reversal-title">{t("activity.reverseTitle")}</h2><button type="button" className="secondary" onClick={() => setReverseTarget(null)}>{t("common.cancel")}</button></div><p>{t("activity.reverseDescription")}</p><PostingTable postings={reverseTarget.reversalPreview} t={t} /><label htmlFor="reverseReason">{t("activity.reversalReason")}<textarea id="reverseReason" value={reverseReason} onChange={(event) => setReverseReason(event.currentTarget.value)} /></label><label htmlFor="reverseDate">{t("field.date")}<input id="reverseDate" type="date" value={reverseDate} onChange={(event) => setReverseDate(event.currentTarget.value)} /></label><button type="submit" disabled={busy}>{t("activity.confirmReversal")}</button></form> : null}
    </section>
  );
}

function AccountSelect({ id, label, value, options, onChange, optional = false, invalid = false }: { id: string; label: string; value: string; options: CatalogRecord[]; onChange: (value: string) => void; optional?: boolean; invalid?: boolean }) {
  return <label htmlFor={id}>{label}<select id={id} value={value} required={!optional} aria-invalid={invalid} onChange={(event) => onChange(event.currentTarget.value)}><option value="">—</option>{options.map((item) => <option key={item.id} value={item.id}>{item.name} · {item.details[2] ?? ""}</option>)}</select></label>;
}

function PostingTable({ postings, t }: { postings: ActivityPosting[] | EventPreview["postings"]; t: (key: Parameters<typeof translate>[1]) => string }) {
  return <div className="table-scroll"><table><thead><tr><th>{t("activity.account")}</th><th>{t("activity.nativeChange")}</th><th>{t("activity.baseValue")}</th><th>{t("activity.postingRole")}</th></tr></thead><tbody>{postings.map((posting, index) => <tr key={`${posting.accountId}-${index}`}><td><code>{posting.accountId ?? ("portfolioId" in posting ? posting.portfolioId : null) ?? "—"}</code></td><td>{posting.quantityDelta} {posting.currency}</td><td>{posting.baseValue ?? t("activity.unvalued")} {posting.baseValue ? posting.baseCurrency : ""}</td><td>{"postingKind" in posting ? posting.postingKind : posting.role}</td></tr>)}</tbody></table></div>;
}

function PreviewPanel({ preview, categoryLabel, t }: { preview: EventPreview; categoryLabel: string; t: (key: Parameters<typeof translate>[1]) => string }) {
  return <section className="preview-panel" aria-labelledby="preview-title"><div className="section-title"><div><p className="eyebrow">{t("activity.authoritativePreview")}</p><h2 id="preview-title">{t("activity.previewTitle")}</h2></div><strong>{preview.effectiveDate}</strong></div><dl className="preview-facts"><div><dt>{t("activity.category")}</dt><dd>{categoryLabel}</dd></div><div><dt>{t("field.semanticRole")}</dt><dd>{preview.semanticRole}</dd></div><div><dt>{t("activity.feeAmount")}</dt><dd>{preview.feeAmount ?? "—"}</dd></div><div><dt>{t("activity.feeAccount")}</dt><dd><code>{preview.feeAccountId ?? "—"}</code></dd></div></dl><PostingTable postings={preview.postings} t={t} /><h3>{t("activity.fxResolution")}</h3>{preview.fxResolutions.length === 0 ? <p className="empty-state">{t("activity.noFx")}</p> : <ul className="resolution-list">{preview.fxResolutions.map((resolution) => <li key={`${resolution.purpose}-${resolution.currency}`}><strong>{resolution.currency} → {resolution.baseCurrency}</strong><span>{resolution.finalRate ?? t("activity.unvalued")}</span><small>{resolution.overrideValue ? `${t("activity.manualOverride")} · ${resolution.overrideReason ?? "—"}` : `${t("activity.automaticRate")} · ${resolution.automaticCandidateRevisionId ?? "1:1"}`} · {resolution.targetDate} · {resolution.calculationVersion}</small></li>)}</ul>}{preview.qualityIssueCodes.length > 0 ? <div className="quality-warning" role="status"><strong>{t("activity.incomplete")}</strong>{preview.qualityIssueCodes.map((code) => <code key={code}>{code}</code>)}</div> : <p className="quality-ok">{t("activity.complete")}</p>}</section>;
}

export function ActivityDetails({ item, accounts, categories, t, canChange, onClose, onRevise, onReverse }: { item: ActivityItem; accounts: CatalogRecord[]; categories: CatalogRecord[]; t: (key: Parameters<typeof translate>[1]) => string; canChange: boolean; onClose: () => void; onRevise: () => void; onReverse: () => void }) {
  const accountName = (id: string | null) => accounts.find((account) => account.id === id)?.name ?? id ?? "—";
  const categoryName = categories.find((category) => category.id === item.content.categoryId)?.name ?? item.content.categoryId ?? t("activity.uncategorized");
  const relations = Object.entries(item.relations).filter(([, value]) => value !== null);
  const investment = item.content.portfolioId ? <><div><dt>{t("investment.portfolio")}</dt><dd><code>{item.content.portfolioId}</code></dd></div><div><dt>{t("field.instrument")}</dt><dd><code>{item.content.instrumentId ?? "—"}</code></dd></div><div><dt>{t("investment.quantity")}</dt><dd>{item.content.quantity ?? "—"}</dd></div><div><dt>{t("investment.unitPrice")}</dt><dd>{item.content.unitPrice ?? "—"}</dd></div><div><dt>{t("investment.tradeFee")}</dt><dd>{item.content.tradeFee ?? item.content.investmentFeeAmount ?? "—"}</dd></div><div><dt>{t("activity.amount")}</dt><dd>{item.content.grossCashAmount ?? item.content.investmentExpenseAmount ?? "—"}</dd></div></> : null;
  return <aside className="detail-panel" aria-labelledby="detail-title"><div className="detail-header"><div><p className="eyebrow">{t("activity.detailEyebrow")}</p><h2 id="detail-title">{displayEventType(item.eventType, t)}</h2></div><button type="button" className="secondary" onClick={onClose}>{t("activity.closeDetails")}</button></div><dl className="detail-grid"><div><dt>{t("field.date")}</dt><dd>{item.effectiveDate}</dd></div><div><dt>{t("activity.revision")}</dt><dd>r{item.revision}</dd></div>{investment}<div><dt>{t("activity.account")}</dt><dd>{accountName(item.content.accountId ?? item.content.fromAccountId ?? item.content.settlementAccountId ?? null)}</dd></div><div><dt>{t("activity.toAccount")}</dt><dd>{accountName(item.content.toAccountId)}</dd></div><div><dt>{t("activity.amount")}</dt><dd>{item.content.amount ?? "—"}</dd></div><div><dt>{t("activity.toAmount")}</dt><dd>{item.content.toAmount ?? "—"}</dd></div><div><dt>{t("activity.category")}</dt><dd>{categoryName}</dd></div><div><dt>{t("field.semanticRole")}</dt><dd>{item.content.semanticRole}</dd></div><div><dt>{t("activity.merchant")}</dt><dd>{item.content.merchant ?? "—"}</dd></div><div><dt>{t("activity.note")}</dt><dd>{item.content.note ?? item.content.settlementOverrideReason ?? "—"}</dd></div></dl><h3>{t("activity.postings")}</h3><PostingTable postings={item.postings} t={t} /><h3>{t("activity.fxResolution")}</h3>{item.fxResolutions.length === 0 ? <p className="empty-state">{t("activity.noFx")}</p> : <ul className="resolution-list">{item.fxResolutions.map((resolution) => <li key={`${resolution.purpose}-${resolution.currency}`}><strong>{resolution.currency} → {resolution.baseCurrency}</strong><span>{resolution.finalRate}</span><small>{resolution.overrideValue ? t("activity.manualOverride") : t("activity.automaticRate")} · {resolution.targetDate} · {resolution.calculationVersion}</small></li>)}</ul>}<h3>{t("activity.history")}</h3>{relations.length === 0 ? <p className="empty-state">{t("activity.noRelations")}</p> : <ul className="relation-list">{relations.map(([kind, value]) => <li key={kind}><strong>{kind}</strong><code>{value}</code></li>)}</ul>}<p className="audit-line"><strong>{t("activity.audit")}</strong> {item.audit.action} · {item.audit.occurredAtUtc}{item.audit.reason ? ` · ${item.audit.reason}` : ""}</p>{canChange ? <div className="form-actions">{item.content.portfolioId ? null : <button type="button" onClick={onRevise}>{t("activity.revise")}</button>}<button type="button" className="danger" onClick={onReverse}>{t("activity.reverse")}</button></div> : <p className="empty-state">{t("activity.immutableHistory")}</p>}</aside>;
}
