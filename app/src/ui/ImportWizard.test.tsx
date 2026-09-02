import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { ImportAnalysis } from "../command-client/contracts";
import { ImportWizard } from "./ImportWizard";

const analysis: ImportAnalysis = {
  batchId: "019d0000-0000-7000-8000-000000000001",
  sourceSha256: `sha256:${"a".repeat(64)}`,
  templateVersion: "ledgerkit-workbook-v1.3",
  importerVersion: "ledgerkit-xlsx-cash-v1",
  targetSchemaVersion: 3,
  status: "needs-review",
  rowCount: 2,
  validRowCount: 1,
  blockerCount: 1,
  warningCount: 0,
  issues: [{ code: "IMPORT_REFERENCE_INVALID", severity: "blocker", sheet: "收支流水", row: 3, field: "account_legacy_id" }],
  mappings: [{ entityType: "account", legacyId: "synthetic-account", targetId: "019d0000-0000-7000-8000-000000000002", migrationPolicy: "explicit_cutover" }],
  proposedEvents: [],
  reconciliation: { balances: [], differenceBridge: [], canonicalResultSha256: `sha256:${"b".repeat(64)}`, balanced: true },
  canCommit: false,
  reusedStaging: false,
};

describe("ImportWizard", () => {
  it("renders located blockers, mappings, and a disabled confirmation boundary", () => {
    const html = renderToStaticMarkup(
      <ImportWizard locale="zh-CN" busy={false} analysis={analysis} onAnalyze={async () => analysis} onCommit={async () => undefined} />,
    );
    expect(html).toContain("收支流水");
    expect(html).toContain("IMPORT_REFERENCE_INVALID");
    expect(html).toContain("synthetic-account");
    expect(html).toContain("disabled");
    expect(html).not.toContain("sourceSha256");
  });
});
