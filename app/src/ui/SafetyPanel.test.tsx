import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { BackupStatus } from "../command-client/contracts";
import { SafetyPanel } from "./SafetyPanel";

const status: BackupStatus = {
  protectionState: "protected",
  externalTargetConfigured: true,
  externalTargetLabel: "Backups",
  lastAttemptAtUtc: "2026-09-03T00:00:00Z",
  lastSuccessAtUtc: "2026-09-03T00:00:00Z",
  lastVerifiedSchemaVersion: 6,
  lastErrorCode: null,
  deviceLossProtected: true,
  recoverySecretState: "locked",
  dailyRetention: 7,
  weeklyRetention: 4,
};

describe("SafetyPanel", () => {
  it.each([
    ["zh-CN", "让账本始终可恢复"],
    ["en-US", "Keep your ledger recoverable"],
  ] as const)("renders recovery and explicit privacy controls in %s", (locale, heading) => {
    const html = renderToStaticMarkup(<SafetyPanel
      locale={locale}
      busy={false}
      ledgerOpen
      onGetStatus={async () => status}
      onCreateBackup={async () => ({ fileName: "backup.lkbackup", backupId: "synthetic", createdAtUtc: "2026-09-03T00:00:00Z", schemaVersion: 6, verified: true, protectionState: "protected" })}
      onRestoreBackup={async () => ({ backupId: "synthetic", ledgerId: "synthetic-ledger", schemaVersion: 7, eventWatermark: 0, settingsLocale: locale, preRestoreBackupVerified: true })}
      onExportData={async ({ format }) => ({ fileName: `export.${format}`, format, rowCount: 0, contentSha256: "sha256:synthetic" })}
    />);
    expect(html).toContain(heading);
    expect(html).toContain('type="password"');
    expect(html).toContain("XLSX");
    expect(html).toContain("CSV");
    expect(html).toContain('type="checkbox"');
    expect(html).not.toContain("D:/");
    expect(html).not.toContain("backupPassword\":");
  });

  it("allows restore on a fresh device while hiding live-ledger-only actions", () => {
    const html = renderToStaticMarkup(<SafetyPanel
      locale="en-US"
      busy={false}
      ledgerOpen={false}
      onGetStatus={async () => status}
      onCreateBackup={async () => { throw new Error("not called"); }}
      onRestoreBackup={async () => ({ backupId: "synthetic", ledgerId: "synthetic-ledger", schemaVersion: 7, eventWatermark: 0, settingsLocale: "en-US", preRestoreBackupVerified: true })}
      onExportData={async () => { throw new Error("not called"); }}
    />);
    expect(html).toContain("Choose backup and restore");
    expect(html).not.toContain("Create portable backup");
    expect(html).not.toContain("Standalone export");
  });
});
