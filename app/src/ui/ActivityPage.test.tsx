import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ActivityItem, CashEventRequest, LedgerStatus } from "../command-client/contracts";
import { ActivityDetails, ActivityPage, createCashDraft, toCashEventRequest, validateCashDraft } from "./ActivityPage";
import { translate } from "./i18n";

const accountId = "019d0000-0000-7000-8000-000000000001";
const secondAccountId = "019d0000-0000-7000-8000-000000000002";
const categoryId = "019d0000-0000-7000-8000-000000000003";

const status: LedgerStatus = {
  appVersion: "0.1.0",
  uiLocale: "en-US",
  ledgerState: "open",
  ledgerId: "019d0000-0000-7000-8000-000000000004",
  schemaVersion: 2,
  baseCurrency: "CNY",
  eventWatermark: 7,
  projectionWatermark: 7,
  calculationVersion: "ledger-calculation-v1",
  blockedReason: null,
  databaseLocation: "C:/local/LedgerKit/ledger.sqlite3",
  backupProtectionState: "not-configured",
  deviceLossProtected: false,
  localOnly: true,
  privilegedOperationCount: 17,
  catalog: {
    asOfDate: "2026-09-02",
    baseCurrency: "CNY",
    institutions: [],
    accounts: [
      { id: accountId, businessId: "daily-cny", name: "Daily CNY", details: ["bank", "daily", "CNY"], enabled: true },
      { id: secondAccountId, businessId: "daily-usd", name: "Daily USD", details: ["bank", "daily", "USD"], enabled: true },
    ],
    categories: [{ id: categoryId, businessId: null, name: "Food", details: ["expense", "normal", "1"], enabled: true }],
    portfolios: [], instruments: [], fxRevisions: [], priceRevisions: [], qualityIssues: [],
  },
};

const item: ActivityItem = {
  eventId: "019d0000-0000-7000-8000-000000000005",
  eventOrder: 7,
  eventType: "Expense",
  effectiveDate: "2026-09-02",
  sequence: 7,
  revision: 2,
  content: {
    accountId, fromAccountId: null, toAccountId: null, amount: "12.50", toAmount: null,
    categoryId, semanticRole: "normal", merchant: "Synthetic Market", note: "sample",
    feeAccountId: accountId, feeAmount: "0.50", cutoverDate: null, migrationPolicy: null,
  },
  postings: [{ postingKind: "cash", accountId, quantityDelta: "-12.50", currency: "CNY", baseValue: "-12.50", baseCurrency: "CNY" }],
  reversalPreview: [{ postingKind: "cash-reversal", accountId, quantityDelta: "12.50", currency: "CNY", baseValue: "12.50", baseCurrency: "CNY" }],
  fxResolutions: [{ purpose: "transaction", currency: "CNY", baseCurrency: "CNY", targetDate: "2026-09-02", automaticCandidateRevisionId: null, overrideValue: null, overrideReason: null, finalRate: "1", calculationVersion: "ledger-calculation-v1" }],
  relations: { supersedesEventId: "019d0000-0000-7000-8000-000000000000", reversesEventId: null, supersededByEventId: null, reversedByEventId: null },
  audit: { action: "revise", occurredAtUtc: "2026-09-02 12:00:00", reason: "correct amount" },
};

function validRequest(eventType: CashEventRequest["eventType"]): CashEventRequest {
  const draft = createCashDraft("2026-09-02", 8);
  draft.eventType = eventType;
  draft.accountId = accountId;
  draft.fromAccountId = accountId;
  draft.toAccountId = secondAccountId;
  draft.amount = eventType === "Adjustment" ? "-1.25" : "10";
  draft.toAmount = "1.4";
  if (eventType === "OpeningBalance") draft.cutoverDate = "2026-09-02";
  expect(validateCashDraft(draft)).toEqual([]);
  return toCashEventRequest(draft);
}

describe("cash activity UI", () => {
  it.each(["Income", "Expense", "Adjustment", "Transfer", "CurrencyExchange"] as const)("builds the %s fast-path command without derived financial fields", (eventType) => {
    const request = validRequest(eventType);
    expect(request.eventType).toBe(eventType);
    expect(request).not.toHaveProperty("currency");
    expect(request).not.toHaveProperty("postings");
    expect(request).not.toHaveProperty("status");
    expect(request).not.toHaveProperty("baseValue");
  });

  it("requires complete exchange and FX-fee override inputs before preview", () => {
    const draft = createCashDraft("2026-09-02", 8);
    draft.eventType = "CurrencyExchange";
    draft.fromAccountId = accountId;
    draft.toAccountId = secondAccountId;
    draft.amount = "10";
    draft.toAmount = "1.4";
    draft.feeAccountId = secondAccountId;
    draft.feeAmount = "0.1";
    draft.fxOverrides = [{ currency: "USD", value: "7.1", reason: "broker receipt" }];
    expect(validateCashDraft(draft)).toEqual([]);
    expect(toCashEventRequest(draft).fxOverrides).toEqual(draft.fxOverrides);
  });

  it.each(["en-US", "zh-CN"] as const)("renders labelled entry, bounded filters, keyboard controls, and a live region in %s", (locale) => {
    const html = renderToStaticMarkup(<ActivityPage
      locale={locale}
      status={{ ...status, uiLocale: locale }}
      busy={false}
      onLoad={async () => ({ items: [], nextCursor: null })}
      onPreview={async () => { throw new Error("not called during SSR"); }}
      onPost={async () => { throw new Error("not called during SSR"); }}
      onRevise={async () => { throw new Error("not called during SSR"); }}
      onReverse={async () => { throw new Error("not called during SSR"); }}
    />);
    expect(html).toContain(translate(locale, "activity.title"));
    expect(html).toContain("preview_event".replace("preview_event", translate(locale, "activity.preview")));
    expect(html).toContain("aria-live=\"polite\"");
    expect(html).toContain("maxLength=\"200\"");
    expect(html).toContain("type=\"date\"");
  });

  it("renders business content, posting, FX, version chain, sanitized audit, and correction controls", () => {
    const t = (key: Parameters<typeof translate>[1]) => translate("en-US", key);
    const html = renderToStaticMarkup(<ActivityDetails
      item={item}
      accounts={status.catalog?.accounts ?? []}
      categories={status.catalog?.categories ?? []}
      t={t}
      canChange
      onClose={() => undefined}
      onRevise={() => undefined}
      onReverse={() => undefined}
    />);
    for (const expected of ["Synthetic Market", "12.50", "Daily CNY", "correct amount", "cash", "ledger-calculation-v1", "Revise", "Reverse"]) {
      expect(html).toContain(expected);
    }
    expect(html).not.toContain("local-user");
    expect(html).not.toContain("databaseLocation");
  });
});
