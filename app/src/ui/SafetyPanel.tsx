import { useCallback, useEffect, useState, type FormEvent } from "react";
import type {
  BackupResult,
  BackupStatus,
  CreateBackupRequest,
  ExportFormat,
  ExportResult,
  RestoreBackupRequest,
  RestoreResult,
} from "../command-client/contracts";
import { translate, type SupportedLocale } from "./i18n";

type SafetyPanelProps = {
  locale: SupportedLocale;
  busy: boolean;
  ledgerOpen: boolean;
  onGetStatus: () => Promise<BackupStatus>;
  onCreateBackup: (request: CreateBackupRequest) => Promise<BackupResult>;
  onRestoreBackup: (request: RestoreBackupRequest) => Promise<RestoreResult>;
  onExportData: (request: { format: ExportFormat }) => Promise<ExportResult>;
};

function errorCode(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && typeof error.code === "string") return error.code;
  return "UNEXPECTED_ERROR";
}

export function SafetyPanel(props: SafetyPanelProps) {
  const t = (key: Parameters<typeof translate>[1]) => translate(props.locale, key);
  const [status, setStatus] = useState<BackupStatus | null>(null);
  const [password, setPassword] = useState("");
  const [restoreConfirmed, setRestoreConfirmed] = useState(false);
  const [privacyConfirmed, setPrivacyConfirmed] = useState(false);
  const [format, setFormat] = useState<ExportFormat>("xlsx");
  const [message, setMessage] = useState<string | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => {
    if (!props.ledgerOpen) {
      setStatus(null);
      return;
    }
    try {
      setStatus(await props.onGetStatus());
      setLocalError(null);
    } catch (error: unknown) {
      setLocalError(errorCode(error));
    }
  }, [props.ledgerOpen, props.onGetStatus]);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  async function create(configureExternalTarget: boolean): Promise<void> {
    const secret = password;
    setPassword("");
    setMessage(null);
    setLocalError(null);
    try {
      const result = await props.onCreateBackup({ password: secret, configureExternalTarget });
      setMessage(`${t("safety.backupCreated")}: ${result.fileName}`);
      await refreshStatus();
    } catch (error: unknown) {
      setLocalError(errorCode(error));
    }
  }

  async function restore(event: FormEvent): Promise<void> {
    event.preventDefault();
    const secret = password;
    setPassword("");
    setMessage(null);
    setLocalError(null);
    try {
      const result = await props.onRestoreBackup({ password: secret });
      setRestoreConfirmed(false);
      setMessage(`${t("safety.restoreCompleted")}: ${result.backupId}`);
      await refreshStatus();
    } catch (error: unknown) {
      setLocalError(errorCode(error));
    }
  }

  async function exportSelected(): Promise<void> {
    setMessage(null);
    setLocalError(null);
    try {
      const result = await props.onExportData({ format });
      setMessage(`${t("safety.exportCreated")}: ${result.fileName}`);
    } catch (error: unknown) {
      setLocalError(errorCode(error));
    }
  }

  return (
    <section className="safety-panel" aria-labelledby="safety-title">
      <div>
        <p className="eyebrow">{t("safety.eyebrow")}</p>
        <h2 id="safety-title">{t("safety.title")}</h2>
        <p>{t("safety.description")}</p>
      </div>

      {status ? <dl className="safety-status">
        <div><dt>{t("safety.deviceLoss")}</dt><dd className={status.deviceLossProtected ? "state-ok" : "state-warning"}>{status.deviceLossProtected ? t("protection.protected") : t("protection.notProtected")}</dd></div>
        <div><dt>{t("safety.externalTarget")}</dt><dd>{status.externalTargetLabel ?? t("safety.notConfigured")}</dd></div>
        <div><dt>{t("safety.lastVerified")}</dt><dd>{status.lastSuccessAtUtc ?? "—"}</dd></div>
        <div><dt>{t("safety.recoverySecret")}</dt><dd>{status.recoverySecretState === "unlocked-for-session" ? t("safety.unlocked") : t("safety.locked")}</dd></div>
        <div><dt>{t("safety.retention")}</dt><dd>{status.dailyRetention} / {status.weeklyRetention}</dd></div>
      </dl> : null}

      <form className="safety-actions" onSubmit={(event) => void restore(event)}>
        <label htmlFor="backupPassword">{t("safety.password")}<input id="backupPassword" type="password" autoComplete="new-password" minLength={12} maxLength={1024} required value={password} onChange={(event) => setPassword(event.currentTarget.value)} /></label>
        <small>{t("safety.passwordHelp")}</small>
        {props.ledgerOpen ? <div className="form-actions">
          <button type="button" disabled={props.busy || password.length < 12} onClick={() => void create(false)}>{t("safety.manualBackup")}</button>
          <button type="button" className="secondary" disabled={props.busy || password.length < 12} onClick={() => void create(true)}>{t("safety.configureExternal")}</button>
        </div> : null}
        <label className="checkbox-field"><input type="checkbox" checked={restoreConfirmed} onChange={(event) => setRestoreConfirmed(event.currentTarget.checked)} />{t("safety.restoreConfirm")}</label>
        <button className="danger" type="submit" disabled={props.busy || !restoreConfirmed || password.length < 12}>{t("safety.restore")}</button>
      </form>

      {props.ledgerOpen ? <div className="export-actions">
        <p><strong>{t("safety.exportTitle")}</strong></p>
        <p>{t("safety.privacyNotice")}</p>
        <label htmlFor="exportFormat">{t("safety.exportFormat")}<select id="exportFormat" value={format} onChange={(event) => setFormat(event.currentTarget.value as ExportFormat)}><option value="xlsx">XLSX</option><option value="csv">CSV</option><option value="reconciliation">{t("safety.reconciliation")}</option><option value="diagnostics">{t("safety.diagnostics")}</option></select></label>
        <label className="checkbox-field"><input type="checkbox" checked={privacyConfirmed} onChange={(event) => setPrivacyConfirmed(event.currentTarget.checked)} />{t("safety.privacyConfirm")}</label>
        <button type="button" disabled={props.busy || !privacyConfirmed} onClick={() => void exportSelected()}>{t("safety.export")}</button>
      </div> : null}

      {message ? <p className="success-banner" role="status">{message}</p> : null}
      {localError ? <p className="error-banner" role="alert"><code>{localError}</code></p> : null}
    </section>
  );
}
