import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { LedgerStatus } from "../command-client/contracts";
import { HealthHome } from "./HealthHome";

const status: LedgerStatus = {
  appVersion: "0.1.0",
  uiLocale: "en-US",
  ledgerState: "not-created",
  ledgerId: null,
  schemaVersion: null,
  baseCurrency: null,
  eventWatermark: 0,
  projectionWatermark: 0,
  calculationVersion: "ledger-calculation-v1",
  blockedReason: null,
  databaseLocation: "C:/local/LedgerKit/ledger.sqlite3",
  backupProtectionState: "not-configured",
  deviceLossProtected: false,
  catalog: null,
  localOnly: true,
  privilegedOperationCount: 17,
};

describe("HealthHome", () => {
  it.each([
    ["zh-CN", "创建本地账本"],
    ["en-US", "Create your local ledger"],
  ] as const)("renders the complete health page in %s", (locale, heading) => {
    const html = renderToStaticMarkup(
      <HealthHome
        locale={locale}
        status={{ ...status, uiLocale: locale }}
        failure={null}
        busy={false}
        onLocaleChange={() => undefined}
        onCreateLedger={async () => undefined}
        onOpenLedger={async () => undefined}
        onSaveInstitution={async () => undefined}
        onSaveCashAccount={async () => undefined}
        onSaveCategory={async () => undefined}
        onSavePortfolio={async () => undefined}
        onSaveInstrument={async () => undefined}
        onSaveFxRevision={async () => undefined}
        onSavePriceRevision={async () => undefined}
      />,
    );

    expect(html).toContain(heading);
    expect(html).toContain("<select");
    expect(html).toContain("ledger.sqlite3");
  });

  it.each([
    ["zh-CN", "精确维护参考数据"],
    ["en-US", "Reference data, kept precise"],
  ] as const)("renders keyboard-operable catalog forms in %s", (locale, heading) => {
    const openStatus: LedgerStatus = {
      ...status,
      uiLocale: locale,
      ledgerState: "open",
      ledgerId: "019d0000-0000-7000-8000-000000000001",
      schemaVersion: 1,
      baseCurrency: "CNY",
      catalog: {
        asOfDate: "2026-09-02",
        baseCurrency: "CNY",
        institutions: [{ id: "019d0000-0000-7000-8000-000000000002", businessId: "bank", name: "原文机构", details: ["SG", "bank"], enabled: true }],
        accounts: [], categories: [], portfolios: [], instruments: [], fxRevisions: [], priceRevisions: [],
        qualityIssues: [{ code: "FX_MISSING_AS_OF", entityType: "cash-account", entityId: "019d0000-0000-7000-8000-000000000003", fixOperation: "save_fx_revision", fixField: "currency" }],
      },
    };
    const html = renderToStaticMarkup(
      <HealthHome
        locale={locale} status={openStatus} failure={null} busy={false}
        onLocaleChange={() => undefined} onCreateLedger={async () => undefined} onOpenLedger={async () => undefined}
        onSaveInstitution={async () => undefined} onSaveCashAccount={async () => undefined} onSaveCategory={async () => undefined}
        onSavePortfolio={async () => undefined} onSaveInstrument={async () => undefined} onSaveFxRevision={async () => undefined}
        onSavePriceRevision={async () => undefined}
      />,
    );
    expect(html).toContain(heading);
    expect(html).toContain("<form");
    expect(html).toContain("<button");
    expect(html).toContain("原文机构");
    expect(html).toContain("019d0000-0000-7000-8000-000000000003");
  });
});
