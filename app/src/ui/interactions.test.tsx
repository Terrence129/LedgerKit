// @vitest-environment jsdom
import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ActivityItem, ActivityRequest, DataQualityReport, DrilldownContext, EventPreview, ImportAnalysis, InvestmentEventPreview, InvestmentWorkspace, LedgerStatus, Overview } from "../command-client/contracts";
import { ActivityPage } from "./ActivityPage";
import { AssetsPage } from "./AssetsPage";
import { DataQualityPage } from "./DataQualityPage";
import { OverviewPage } from "./OverviewPage";
import { focusField } from "./focusField";
import { ImportWizard } from "./ImportWizard";
import { InvestmentEditor } from "./InvestmentEditor";
import { SafetyPanel } from "./SafetyPanel";

const date = "2026-09-05";
const status: LedgerStatus = {
  appVersion: "1.0.0-beta.1", uiLocale: "en-US", ledgerState: "open", ledgerId: "synthetic-ledger",
  schemaVersion: 4, baseCurrency: "CNY", eventWatermark: 1, projectionWatermark: 1,
  calculationVersion: "ledger-calculation-v1", blockedReason: null, databaseLocation: null,
  backupProtectionState: "not-configured", deviceLossProtected: false, localOnly: true, privilegedOperationCount: 25,
  catalog: { asOfDate: date, baseCurrency: "CNY", institutions: [],
    accounts: [{ id: "cash", businessId: "cash", name: "Synthetic Cash", details: ["bank", "daily", "CNY"], enabled: true },
      { id: "archived", businessId: "old", name: "Archived Cash", details: ["bank", "daily", "CNY"], enabled: false }],
    categories: [{ id: "old-category", businessId: null, name: "Archived Category", details: ["expense", "normal", "1"], enabled: false }],
    portfolios: [], instruments: [], fxRevisions: [], priceRevisions: [], qualityIssues: [] },
};
const overview: Overview = {
  contract: "ledgerkit-overview-v1", valuationDate: date, mtdStartDate: "2026-09-01", mtdEndDate: date,
  baseCurrency: "CNY", valuedNetAssets: "123", valuedCash: "123", valuedHoldings: "0", mtdExpense: "0", mtdUnvaluedExpenseCount: 0,
  composition: { institutions: [], currencies: [], cashAccounts: [], holdings: [] }, unvaluedAssets: [], anomalyCodes: [],
  watermarks: { event: 1, marketData: 1 }, calculationVersion: "ledger-calculation-v1", snapshotVersion: "v1",
};
const quality: DataQualityReport = { contract: "ledgerkit-data-quality-v1", asOfDate: date, blockerCount: 0, warningCount: 0, issues: [], eventWatermark: 1, calculationVersion: "ledger-calculation-v1" };
const workspace: InvestmentWorkspace = { asOfDate: date, baseCurrency: "CNY", holdings: [], portfolioExpenses: [{ portfolioId: "portfolio", portfolioName: "Synthetic fee marker", currency: "CNY", amount: "9" }], eventWatermark: 1, projectionVersion: "holding-projection-v1", calculationVersion: "ledger-calculation-v1" };
const preview: EventPreview = { eventType: "Expense", effectiveDate: date, sequence: 2, categoryId: null, semanticRole: "normal", feeAccountId: null, feeAmount: null, postings: [], fxResolutions: [], qualityIssueCodes: [] };
const context: DrilldownContext = { start_date: "2026-09-01", end_date: date, event_watermark: 1, calculation_version: "ledger-calculation-v1", expense_policy_version: "expense-policy-v1", valuation_state: "valued" };

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => { callback(0); return 0; });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
async function render(node: ReactNode) { await act(async () => root.render(node)); }
async function click(node: HTMLElement) { await act(async () => node.click()); }
async function input(selector: string, value: string) {
  const node = host.querySelector<HTMLInputElement | HTMLSelectElement>(selector)!;
  const prototype = node instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  await act(async () => {
    Object.getOwnPropertyDescriptor(prototype, "value")!.set!.call(node, value);
    node.dispatchEvent(new Event(node instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
  });
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}
function activityProps() {
  return { locale: "en-US" as const, status, busy: false, onLoad: vi.fn(async (_request: ActivityRequest) => ({ items: [], nextCursor: null })),
    onPreview: vi.fn(async () => preview), onPost: vi.fn(), onRevise: vi.fn(), onReverse: vi.fn() };
}

describe("interactive regression checks", () => {
  it("refreshes overview after writes while preserving the selected valuation date", async () => {
    const load = vi.fn(async () => overview);
    const props = { locale: "en-US" as const, asOfDate: date, onLoadOverview: load, onLoadExpense: vi.fn(), onDrilldown: vi.fn(), onOpenQuality: vi.fn() };
    await render(<OverviewPage {...props} refreshVersion={1} />);
    await input("#overviewValuationDate", "2026-08-31");
    await act(async () => host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    await render(<OverviewPage {...props} refreshVersion={2} />);
    expect(load).toHaveBeenCalledTimes(3);
    expect(load).toHaveBeenLastCalledWith({ asOfDate: "2026-08-31" });
  });

  it("refreshes data-quality reports even when the date and event watermark have not changed", async () => {
    const load = vi.fn(async () => quality);
    const props = { locale: "en-US" as const, asOfDate: date, onLoad: load, onFix: vi.fn() };
    await render(<DataQualityPage {...props} refreshVersion={1} />);
    await render(<DataQualityPage {...props} refreshVersion={2} />);
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("refreshes market valuations and removes stale figures on query failure", async () => {
    const load = vi.fn(async () => workspace);
    await render(<AssetsPage locale="en-US" status={status} onLoad={load} refreshVersion={1} />);
    expect(host.textContent).toContain("Synthetic fee marker");
    load.mockRejectedValueOnce({ code: "LOCAL_DATE_INVALID" });
    await render(<AssetsPage locale="en-US" status={status} onLoad={load} refreshVersion={2} />);
    expect(load).toHaveBeenCalledTimes(2);
    expect(host.textContent).toContain("LOCAL_DATE_INVALID");
    expect(host.textContent).not.toContain("Synthetic fee marker");
  });

  it("ignores out-of-order overview responses", async () => {
    const first = deferred<Overview>();
    const load = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue({ ...overview, valuedNetAssets: "456" });
    const props = { locale: "en-US" as const, asOfDate: date, onLoadOverview: load, onLoadExpense: vi.fn(), onDrilldown: vi.fn(), onOpenQuality: vi.fn() };
    await render(<OverviewPage {...props} refreshVersion={1} />);
    await render(<OverviewPage {...props} refreshVersion={2} />);
    await act(async () => first.resolve(overview));
    expect(host.querySelector(".dashboard-kpis dd")?.textContent).toBe("CNY 456");
  });

  it("reloads ordinary activity when leaving a drilldown", async () => {
    const props = activityProps();
    await render(<ActivityPage {...props} initialContext={context} />);
    expect(props.onLoad.mock.lastCall?.[0]).toMatchObject({ context });
    await render(<ActivityPage {...props} initialContext={null} />);
    expect(props.onLoad.mock.lastCall?.[0]).not.toHaveProperty("context");
  });

  it("keeps the FX currency input mounted and focused while typing", async () => {
    await render(<ActivityPage {...activityProps()} />);
    await click(host.querySelector("summary")!);
    await click(host.querySelector<HTMLButtonElement>(".fx-overrides button")!);
    const field = host.querySelector<HTMLInputElement>("#fxOverride-0")!;
    field.focus();
    await input("#fxOverride-0", "U");
    expect(host.querySelector("#fxOverride-0")).toBe(field);
    expect(document.activeElement).toBe(field);
    await input("#fxOverride-0", "USD");
    expect(field.value).toBe("USD");
  });

  it("allows archived accounts in history filters, but not new cash events", async () => {
    await render(<ActivityPage {...activityProps()} />);
    expect(host.querySelector('#activityAccount option[value="archived"]')).not.toBeNull();
    expect(host.querySelector('#accountId option[value="archived"]')).toBeNull();
    expect(host.querySelector('.filter-grid option[value="old-category"]')).not.toBeNull();
  });

  it("discards a cash preview if its draft changes while the request is pending", async () => {
    const pending = deferred<EventPreview>();
    const props = { ...activityProps(), onPreview: vi.fn(() => pending.promise) };
    await render(<ActivityPage {...props} />);
    await input("#accountId", "cash"); await input("#amount", "10");
    await act(async () => host.querySelector("#cash-editor")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    expect(props.onPreview).toHaveBeenCalledTimes(1);
    await input("#amount", "20");
    await act(async () => pending.resolve(preview));
    expect(host.querySelector(".preview-panel")).toBeNull();
    expect(props.onPost).not.toHaveBeenCalled();
  });

  it("assigns a fresh sequence to revisions instead of duplicating the original event", async () => {
    const item: ActivityItem = {
      eventId: "synthetic-event", eventOrder: 1, eventType: "Expense", effectiveDate: date, sequence: 1, revision: 1,
      content: { accountId: "cash", fromAccountId: null, toAccountId: null, amount: "10", toAmount: null,
        categoryId: null, semanticRole: "normal", merchant: null, note: null, feeAccountId: null, feeAmount: null, cutoverDate: null, migrationPolicy: null },
      postings: [], reversalPreview: [], fxResolutions: [],
      relations: { supersedesEventId: null, reversesEventId: null, supersededByEventId: null, reversedByEventId: null },
      audit: { action: "post", occurredAtUtc: "2026-09-05 00:00:00", reason: null },
    };
    const props = { ...activityProps(), onLoad: vi.fn(async () => ({ items: [item], nextCursor: null })) };
    await render(<ActivityPage {...props} />);
    await click(host.querySelector<HTMLButtonElement>(".timeline-item")!);
    await click(host.querySelector<HTMLButtonElement>(".detail-panel .form-actions button")!);
    const reason = host.querySelector<HTMLTextAreaElement>("#reason")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(reason, "Synthetic correction");
      reason.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => host.querySelector("#cash-editor")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    await click(host.querySelector<HTMLButtonElement>("#cash-editor .form-actions button[type=button]")!);
    expect(props.onRevise).toHaveBeenCalledWith(expect.objectContaining({ replacement: expect.objectContaining({ sequence: 2 }) }));
  });

  it("updates a cash draft sequence after another editor posts without losing entered values", async () => {
    const props = activityProps();
    await render(<ActivityPage {...props} />);
    await input("#accountId", "cash"); await input("#amount", "10");
    await render(<ActivityPage {...props} status={{ ...status, eventWatermark: 9 }} />);
    await act(async () => host.querySelector("#cash-editor")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    expect(props.onPreview).toHaveBeenCalledWith(expect.objectContaining({ amount: "10", sequence: 10 }));
  });

  it("reveals collapsed controls before focusing a quality repair target", async () => {
    await render(<details><summary>Import</summary><div id="import-review" tabIndex={-1}>Review</div></details>);
    focusField("import-review");
    expect(host.querySelector("details")?.open).toBe(true);
    expect(document.activeElement?.id).toBe("import-review");
  });

  it("requires fresh confirmation for each import batch and each analysis attempt", async () => {
    const analysis: ImportAnalysis = { batchId: "synthetic-batch", sourceSha256: "synthetic-hash", templateVersion: "v1.3",
      importerVersion: "v1", targetSchemaVersion: 4, status: "needs-review", rowCount: 1, validRowCount: 1,
      blockerCount: 0, warningCount: 0, issues: [], mappings: [], proposedEvents: [],
      reconciliation: { balances: [], metrics: [], differenceBridge: [], differenceItems: [], canonicalResultSha256: "synthetic-hash", balanced: true },
      canCommit: true, reusedStaging: false };
    const props = { locale: "en-US" as const, busy: false, onAnalyze: vi.fn(async () => analysis), onCommit: vi.fn() };
    await render(<ImportWizard {...props} analysis={analysis} />);
    const commit = () => host.querySelector<HTMLButtonElement>(".import-review > button")!;
    expect(commit().disabled).toBe(true);
    await click(host.querySelector<HTMLInputElement>('input[type="checkbox"]')!);
    expect(commit().disabled).toBe(false);
    await render(<ImportWizard {...props} analysis={{ ...analysis, batchId: "next-batch" }} />);
    expect(commit().disabled).toBe(true);
    await click(host.querySelector<HTMLInputElement>('input[type="checkbox"]')!);
    await click(host.querySelector<HTMLButtonElement>(".import-wizard > button")!);
    expect(commit().disabled).toBe(true);
    expect(props.onCommit).not.toHaveBeenCalled();
  });

  it("rejects restore submission without explicit confirmation, including programmatic form submit", async () => {
    const props = { locale: "en-US" as const, busy: false, ledgerOpen: false, onGetStatus: vi.fn(), onCreateBackup: vi.fn(), onRestoreBackup: vi.fn(), onExportData: vi.fn() };
    await render(<SafetyPanel {...props} />);
    await input("#backupPassword", "x".repeat(12));
    await act(async () => host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    expect(props.onRestoreBackup).not.toHaveBeenCalled();
  });

  it("discards an investment preview if the quantity changes before it returns", async () => {
    const pending = deferred<InvestmentEventPreview>();
    const investmentStatus: LedgerStatus = { ...status, catalog: { ...status.catalog!,
      portfolios: [{ id: "portfolio", businessId: "p", name: "Synthetic Portfolio", details: [], enabled: true }],
      instruments: [{ id: "instrument", businessId: "i", name: "Synthetic Instrument", details: [], enabled: true }] } };
    const onPreview = vi.fn(() => pending.promise);
    await render(<InvestmentEditor locale="en-US" status={investmentStatus} busy={false} onPreview={onPreview} onPost={vi.fn()} />);
    await input("#investmentPortfolioId", "portfolio"); await input("#investmentInstrumentId", "instrument");
    await input("#investmentSettlementAccountId", "cash"); await input("#investmentQuantity", "2"); await input("#investmentUnitPrice", "10");
    await act(async () => host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    expect(onPreview).toHaveBeenCalledTimes(1);
    await input("#investmentQuantity", "3");
    await act(async () => pending.resolve({ postings: [] } as unknown as InvestmentEventPreview));
    expect(host.querySelector(".investment-preview")).toBeNull();
  });
});
