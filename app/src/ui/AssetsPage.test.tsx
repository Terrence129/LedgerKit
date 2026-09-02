import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { InvestmentWorkspace, LedgerStatus } from "../command-client/contracts";
import { AssetsPage } from "./AssetsPage";

const id = "019d0000-0000-7000-8000-000000000001";
const status: LedgerStatus = { appVersion: "0.1.0", uiLocale: "en-US", ledgerState: "open", ledgerId: id, schemaVersion: 4, baseCurrency: "CNY", eventWatermark: 1, projectionWatermark: 1, calculationVersion: "ledger-calculation-v1", blockedReason: null, databaseLocation: null, backupProtectionState: "not-configured", deviceLossProtected: false, localOnly: true, privilegedOperationCount: 23, catalog: { asOfDate: "2026-09-03", baseCurrency: "CNY", institutions: [], accounts: [], categories: [], qualityIssues: [], portfolios: [{ id, businessId: "p", name: "Portfolio", details: [id, id, "brokerage"], enabled: true }], instruments: [{ id, businessId: "i", name: "Alpha", details: ["ALPHA", "USD"], enabled: true }], fxRevisions: [], priceRevisions: [{ id, ownerId: id, date: "2026-09-01", value: "12", currency: "USD", source: "synthetic", revision: 1, active: true }] } };
const workspace: InvestmentWorkspace = { asOfDate: "2026-09-03", baseCurrency: "CNY", holdings: [{ portfolioId: id, portfolioName: "Portfolio", instrumentId: id, instrumentName: "Alpha", currency: "USD", asOfDate: "2026-09-03", quantity: "2", carryingCost: "20", averageCost: "10", realizedTradePnl: "0", netDividend: "0", independentExpense: "0", marketPrice: "12", priceRevisionId: id, priceDate: "2026-09-01", priceAgeDays: 2, marketValue: "24", fxRate: "7", fxRevisionId: id, baseMarketValue: "168", unrealizedPnl: "4", totalReturn: "4", valuationState: "valued", unvaluedReason: null, warningCodes: [] }], portfolioExpenses: [], eventWatermark: 1, projectionVersion: "holding-projection-v1", calculationVersion: "ledger-calculation-v1" };

describe("assets page", () => {
  it.each(["en-US", "zh-CN"] as const)("renders semantic portfolio, instrument, price, and holding views in %s", (locale) => {
    const html = renderToStaticMarkup(<AssetsPage locale={locale} status={{ ...status, uiLocale: locale }} onLoad={async () => workspace} />);
    expect(html).toContain("Portfolio");
    expect(html).toContain("Alpha");
    expect(html).toContain("2026-09-01");
    expect(html).toContain("scope=\"col\"");
    expect(html).toContain("type=\"date\"");
  });
});
