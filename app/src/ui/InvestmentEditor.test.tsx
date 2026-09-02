import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { LedgerStatus } from "../command-client/contracts";
import { InvestmentEditor, createInvestmentDraft, toInvestmentRequest, validateInvestmentDraft } from "./InvestmentEditor";

const portfolioId = "019d0000-0000-7000-8000-000000000001";
const instrumentId = "019d0000-0000-7000-8000-000000000002";
const accountId = "019d0000-0000-7000-8000-000000000003";
const status: LedgerStatus = { appVersion: "0.1.0", uiLocale: "en-US", ledgerState: "open", ledgerId: "019d0000-0000-7000-8000-000000000004", schemaVersion: 4, baseCurrency: "CNY", eventWatermark: 2, projectionWatermark: 2, calculationVersion: "ledger-calculation-v1", blockedReason: null, databaseLocation: "C:/local/ledger.sqlite3", backupProtectionState: "not-configured", deviceLossProtected: false, localOnly: true, privilegedOperationCount: 23, catalog: { asOfDate: "2026-09-03", baseCurrency: "CNY", institutions: [], categories: [], fxRevisions: [], priceRevisions: [], qualityIssues: [], accounts: [{ id: accountId, businessId: "usd", name: "USD cash", details: ["broker", "settlement", "USD"], enabled: true }], portfolios: [{ id: portfolioId, businessId: "portfolio", name: "Portfolio", details: ["broker", accountId, "brokerage"], enabled: true }], instruments: [{ id: instrumentId, businessId: "alpha", name: "Alpha", details: ["ALPHA", "USD"], enabled: true }] } };

describe("investment editor", () => {
  it.each(["SecurityBuy", "SecuritySell"] as const)("builds a high-level %s command without accepting postings", (eventType) => {
    const draft = createInvestmentDraft("2026-09-03", 3);
    Object.assign(draft, { eventType, portfolioId, instrumentId, settlementAccountId: accountId, quantity: "1.000000000001", unitPrice: "12.34", tradeFee: "0.50" });
    expect(validateInvestmentDraft(draft)).toEqual([]);
    const request = toInvestmentRequest(draft);
    expect(request).toMatchObject({ eventType, portfolioId, instrumentId, settlementAccountId: accountId, quantity: "1.000000000001" });
    expect(request).not.toHaveProperty("postings");
    expect(request).not.toHaveProperty("carryingCost");
  });

  it("requires an instrument only for instrument-scoped expenses", () => {
    const draft = createInvestmentDraft("2026-09-03", 3);
    Object.assign(draft, { eventType: "InvestmentExpense", portfolioId, settlementAccountId: accountId, amount: "2", feeScope: "portfolio" });
    expect(validateInvestmentDraft(draft)).toEqual([]);
    expect(toInvestmentRequest(draft).instrumentId).toBeUndefined();
  });

  it.each(["en-US", "zh-CN"] as const)("renders all four investment event choices in %s", (locale) => {
    const html = renderToStaticMarkup(<InvestmentEditor locale={locale} status={{ ...status, uiLocale: locale }} busy={false} onPreview={async () => { throw new Error("not called"); }} onPost={async () => { throw new Error("not called"); }} />);
    for (const value of ["SecurityBuy", "SecuritySell", "Dividend", "InvestmentExpense"]) expect(html).toContain(`value="${value}"`);
    expect(html).toContain("<form");
    expect(html).toContain("type=\"date\"");
  });
});
