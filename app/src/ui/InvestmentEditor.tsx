import { useEffect, useRef, useState, type FormEvent } from "react";
import type {
  InvestmentEventPreview,
  InvestmentEventRequest,
  LedgerStatus,
  PostedInvestmentEvent,
} from "../command-client/contracts";
import { translate, type SupportedLocale } from "./i18n";
import { LatestRequestGate } from "./queryGate";

type Draft = {
  effectiveDate: string; sequence: number; eventType: InvestmentEventRequest["eventType"];
  portfolioId: string; instrumentId: string; settlementAccountId: string;
  quantity: string; unitPrice: string; tradeFee: string; grossCashAmount: string;
  withholdingTax: string; feeAmount: string; amount: string;
  feeScope: "instrument" | "portfolio"; settlementOverrideReason: string;
};

type Props = {
  locale: SupportedLocale; status: LedgerStatus; busy: boolean;
  onPreview: (request: InvestmentEventRequest) => Promise<InvestmentEventPreview>;
  onPost: (request: InvestmentEventRequest) => Promise<PostedInvestmentEvent>;
};

const decimalPattern = /^\d+(?:\.\d+)?$/u;

export function createInvestmentDraft(date: string, sequence: number): Draft {
  return { effectiveDate: date, sequence, eventType: "SecurityBuy", portfolioId: "", instrumentId: "", settlementAccountId: "", quantity: "", unitPrice: "", tradeFee: "0", grossCashAmount: "", withholdingTax: "0", feeAmount: "0", amount: "", feeScope: "instrument", settlementOverrideReason: "" };
}

export function validateInvestmentDraft(draft: Draft): string[] {
  const errors: string[] = [];
  const positive = (value: string) => decimalPattern.test(value) && Number(value) > 0;
  const nonNegative = (value: string) => decimalPattern.test(value) && Number(value) >= 0;
  if (!draft.effectiveDate) errors.push("investmentDate");
  if (!draft.portfolioId) errors.push("investmentPortfolioId");
  if (!draft.settlementAccountId) errors.push("investmentSettlementAccountId");
  if (draft.eventType !== "InvestmentExpense" || draft.feeScope === "instrument") {
    if (!draft.instrumentId) errors.push("investmentInstrumentId");
  }
  if (draft.eventType === "SecurityBuy" || draft.eventType === "SecuritySell") {
    if (!positive(draft.quantity)) errors.push("investmentQuantity");
    if (!positive(draft.unitPrice)) errors.push("investmentUnitPrice");
    if (!nonNegative(draft.tradeFee)) errors.push("investmentTradeFee");
  } else if (draft.eventType === "Dividend") {
    if (!positive(draft.grossCashAmount)) errors.push("investmentGross");
    if (!nonNegative(draft.withholdingTax)) errors.push("investmentTax");
    if (!nonNegative(draft.feeAmount)) errors.push("investmentFee");
  } else if (!positive(draft.amount)) errors.push("investmentAmount");
  return errors;
}

export function toInvestmentRequest(draft: Draft): InvestmentEventRequest {
  const base: InvestmentEventRequest = {
    effectiveDate: draft.effectiveDate,
    sequence: draft.sequence,
    eventType: draft.eventType,
    portfolioId: draft.portfolioId,
    settlementAccountId: draft.settlementAccountId,
    ...(draft.instrumentId ? { instrumentId: draft.instrumentId } : {}),
    ...(draft.settlementOverrideReason.trim() ? { settlementOverrideReason: draft.settlementOverrideReason.trim() } : {}),
  };
  if (draft.eventType === "SecurityBuy" || draft.eventType === "SecuritySell") {
    return { ...base, quantity: draft.quantity, unitPrice: draft.unitPrice, tradeFee: draft.tradeFee };
  }
  if (draft.eventType === "Dividend") {
    return { ...base, grossCashAmount: draft.grossCashAmount, withholdingTax: draft.withholdingTax, feeAmount: draft.feeAmount };
  }
  return { ...base, amount: draft.amount, feeScope: draft.feeScope };
}

export function InvestmentEditor({ locale, status, busy, onPreview, onPost }: Props) {
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  const date = status.catalog?.asOfDate ?? new Date().toISOString().slice(0, 10);
  const [draft, setDraft] = useState(() => createInvestmentDraft(date, status.eventWatermark + 1));
  const [preview, setPreview] = useState<InvestmentEventPreview | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const [failure, setFailure] = useState<string | null>(null);
  const previewGate = useRef(new LatestRequestGate());
  useEffect(() => {
    setDraft((current) => ({ ...current, sequence: status.eventWatermark + 1 }));
    previewGate.current.invalidate();
    setPreview(null);
  }, [status.eventWatermark]);
  const catalog = status.catalog;
  if (!catalog) return null;
  const patch = (value: Partial<Draft>) => { previewGate.current.invalidate(); setDraft((current) => ({ ...current, ...value })); setPreview(null); setFailure(null); };

  async function previewEvent(event: FormEvent): Promise<void> {
    event.preventDefault();
    const nextErrors = validateInvestmentDraft(draft);
    setErrors(nextErrors);
    if (nextErrors.length > 0) { document.getElementById(nextErrors[0] ?? "investmentDate")?.focus(); return; }
    const generation = previewGate.current.begin();
    try {
      const next = await onPreview(toInvestmentRequest(draft));
      if (previewGate.current.isLatest(generation)) setPreview(next);
    }
    catch (error: unknown) {
      if (previewGate.current.isLatest(generation)) setFailure(typeof error === "object" && error !== null && "code" in error ? String(error.code) : "UNEXPECTED_ERROR");
    }
  }

  async function postEvent(): Promise<void> {
    if (!preview) return;
    try {
      await onPost(toInvestmentRequest(draft));
      setDraft(createInvestmentDraft(date, status.eventWatermark + 2));
      setPreview(null); setErrors([]);
    } catch (error: unknown) { setFailure(typeof error === "object" && error !== null && "code" in error ? String(error.code) : "UNEXPECTED_ERROR"); }
  }

  const trade = draft.eventType === "SecurityBuy" || draft.eventType === "SecuritySell";
  const instrumentRequired = draft.eventType !== "InvestmentExpense" || draft.feeScope === "instrument";
  return <section className="cash-editor" aria-labelledby="investment-editor-title">
    <div className="section-title"><div><p className="eyebrow">{t("investment.editorEyebrow")}</p><h2 id="investment-editor-title">{t("investment.editorTitle")}</h2></div></div>
    {failure ? <p className="inline-error" role="alert"><code>{failure}</code></p> : null}
    {errors.length ? <p className="inline-error" role="alert">{t("activity.validation")}</p> : null}
    <form onSubmit={(event) => void previewEvent(event)} noValidate>
      <div className="editor-grid">
        <label>{t("activity.eventType")}<select value={draft.eventType} onChange={(event) => patch({ eventType: event.currentTarget.value as Draft["eventType"] })}><option value="SecurityBuy">{t("event.securityBuy")}</option><option value="SecuritySell">{t("event.securitySell")}</option><option value="Dividend">{t("event.dividend")}</option><option value="InvestmentExpense">{t("event.investmentExpense")}</option></select></label>
        <label>{t("field.date")}<input id="investmentDate" type="date" value={draft.effectiveDate} aria-invalid={errors.includes("investmentDate")} onChange={(event) => patch({ effectiveDate: event.currentTarget.value })} /></label>
        <label>{t("investment.portfolio")}<select id="investmentPortfolioId" value={draft.portfolioId} aria-invalid={errors.includes("investmentPortfolioId")} onChange={(event) => patch({ portfolioId: event.currentTarget.value })}><option value="">—</option>{catalog.portfolios.filter((item) => item.enabled).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
        {instrumentRequired ? <label>{t("field.instrument")}<select id="investmentInstrumentId" value={draft.instrumentId} aria-invalid={errors.includes("investmentInstrumentId")} onChange={(event) => patch({ instrumentId: event.currentTarget.value })}><option value="">—</option>{catalog.instruments.filter((item) => item.enabled).map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label> : null}
        <label>{t("field.settlementAccount")}<select id="investmentSettlementAccountId" value={draft.settlementAccountId} aria-invalid={errors.includes("investmentSettlementAccountId")} onChange={(event) => patch({ settlementAccountId: event.currentTarget.value })}><option value="">—</option>{catalog.accounts.filter((item) => item.enabled).map((item) => <option key={item.id} value={item.id}>{item.name} · {item.details[2]}</option>)}</select></label>
        {trade ? <><label>{t("investment.quantity")}<input id="investmentQuantity" inputMode="decimal" value={draft.quantity} aria-invalid={errors.includes("investmentQuantity")} onChange={(event) => patch({ quantity: event.currentTarget.value })} /></label><label>{t("investment.unitPrice")}<input id="investmentUnitPrice" inputMode="decimal" value={draft.unitPrice} aria-invalid={errors.includes("investmentUnitPrice")} onChange={(event) => patch({ unitPrice: event.currentTarget.value })} /></label><label>{t("investment.tradeFee")}<input id="investmentTradeFee" inputMode="decimal" value={draft.tradeFee} aria-invalid={errors.includes("investmentTradeFee")} onChange={(event) => patch({ tradeFee: event.currentTarget.value })} /></label></> : null}
        {draft.eventType === "Dividend" ? <><label>{t("investment.grossDividend")}<input id="investmentGross" inputMode="decimal" value={draft.grossCashAmount} onChange={(event) => patch({ grossCashAmount: event.currentTarget.value })} /></label><label>{t("investment.withholdingTax")}<input id="investmentTax" inputMode="decimal" value={draft.withholdingTax} onChange={(event) => patch({ withholdingTax: event.currentTarget.value })} /></label><label>{t("investment.dividendFee")}<input id="investmentFee" inputMode="decimal" value={draft.feeAmount} onChange={(event) => patch({ feeAmount: event.currentTarget.value })} /></label></> : null}
        {draft.eventType === "InvestmentExpense" ? <><label>{t("activity.amount")}<input id="investmentAmount" inputMode="decimal" value={draft.amount} onChange={(event) => patch({ amount: event.currentTarget.value })} /></label><label>{t("investment.feeScope")}<select value={draft.feeScope} onChange={(event) => patch({ feeScope: event.currentTarget.value as Draft["feeScope"], instrumentId: event.currentTarget.value === "portfolio" ? "" : draft.instrumentId })}><option value="instrument">{t("investment.instrumentScope")}</option><option value="portfolio">{t("investment.portfolioScope")}</option></select></label></> : null}
        <label className="wide-field">{t("investment.overrideReason")}<input value={draft.settlementOverrideReason} onChange={(event) => patch({ settlementOverrideReason: event.currentTarget.value })} /></label>
      </div>
      <div className="form-actions"><button type="submit" disabled={busy}>{t("activity.preview")}</button>{preview ? <button type="button" disabled={busy} onClick={() => void postEvent()}>{t("activity.confirmPost")}</button> : null}</div>
    </form>
    {preview ? <div className="investment-preview" role="status"><dl className="preview-facts"><div><dt>{t("investment.quantityAfter")}</dt><dd>{preview.quantityAfter ?? "—"}</dd></div><div><dt>{t("investment.carryingAfter")}</dt><dd>{preview.carryingCostAfter ?? "—"}</dd></div><div><dt>{t("investment.averageAfter")}</dt><dd>{preview.averageCostAfter ?? "—"}</dd></div><div><dt>{t("investment.realizedAfter")}</dt><dd>{preview.realizedTradePnlAfter ?? "—"}</dd></div></dl><div className="table-scroll"><table><thead><tr><th>{t("activity.postingRole")}</th><th>{t("activity.nativeChange")}</th><th>{t("activity.baseValue")}</th></tr></thead><tbody>{preview.postings.map((posting, index) => <tr key={`${posting.postingKind}-${index}`}><td>{posting.postingKind}</td><td>{posting.quantityDelta} {posting.currency}</td><td>{posting.baseValue ?? t("activity.unvalued")}</td></tr>)}</tbody></table></div></div> : null}
  </section>;
}
