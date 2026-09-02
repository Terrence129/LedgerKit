import { useState } from "react";
import type { ImportAnalysis } from "../command-client/contracts";
import { translate, type SupportedLocale } from "./i18n";

type ImportWizardProps = {
  locale: SupportedLocale;
  busy: boolean;
  analysis: ImportAnalysis | null;
  onAnalyze: () => Promise<ImportAnalysis>;
  onCommit: (batchId: string) => Promise<void>;
};

export function ImportWizard(props: ImportWizardProps) {
  const t = (key: Parameters<typeof translate>[1]) => translate(props.locale, key);
  const [confirmed, setConfirmed] = useState(false);
  const analysis = props.analysis;
  return (
    <section className="import-wizard" aria-labelledby="import-title">
      <p className="eyebrow">{t("import.eyebrow")}</p>
      <h2 id="import-title">{t("import.title")}</h2>
      <p>{t("import.description")}</p>
      <button type="button" disabled={props.busy} onClick={() => void props.onAnalyze().catch(() => undefined)}>
        {props.busy ? t("import.working") : t("import.choose")}
      </button>
      {analysis ? (
        <div className="import-review" aria-live="polite">
          <dl className="summary-grid">
            <div><dt>{t("import.template")}</dt><dd>{analysis.templateVersion}</dd></div>
            <div><dt>{t("import.rows")}</dt><dd>{analysis.validRowCount}/{analysis.rowCount}</dd></div>
            <div><dt>{t("import.events")}</dt><dd>{analysis.proposedEvents.length}</dd></div>
            <div><dt>{t("import.reconciliation")}</dt><dd>{analysis.reconciliation.balanced ? t("import.balanced") : t("import.unbalanced")}</dd></div>
          </dl>
          {analysis.reusedStaging ? <p className="notice">{t("import.reused")}</p> : null}
          <details open={analysis.issues.length > 0}>
            <summary>{t("import.issues")} ({analysis.blockerCount + analysis.warningCount})</summary>
            {analysis.issues.length === 0 ? <p>{t("import.noIssues")}</p> : (
              <ul className="issue-list">{analysis.issues.map((issue, index) => (
                <li key={`${issue.sheet}-${issue.row}-${issue.code}-${index}`}>
                  <strong>{issue.code}</strong><span>{issue.sheet} · {t("import.row")} {issue.row} · {issue.field}</span>
                </li>
              ))}</ul>
            )}
          </details>
          <details>
            <summary>{t("import.mappings")} ({analysis.mappings.length})</summary>
            <ul className="mapping-list">{analysis.mappings.map((mapping) => (
              <li key={`${mapping.entityType}-${mapping.legacyId}`}><span>{mapping.entityType}: {mapping.legacyId}</span><code>{mapping.targetId}</code>{mapping.migrationPolicy ? <small>{mapping.migrationPolicy}</small> : null}</li>
            ))}</ul>
          </details>
          <details>
            <summary>{t("import.balances")} ({analysis.reconciliation.balances.length})</summary>
            <div className="table-wrap"><table><thead><tr><th>{t("import.account")}</th><th>{t("import.currency")}</th><th>{t("import.sourceBalance")}</th><th>{t("import.proposedBalance")}</th><th>{t("import.difference")}</th></tr></thead>
              <tbody>{analysis.reconciliation.balances.map((balance) => <tr key={balance.accountId}><td><code>{balance.accountId}</code></td><td>{balance.currency}</td><td>{balance.sourceBalance}</td><td>{balance.proposedBalance}</td><td>{balance.difference}</td></tr>)}</tbody>
            </table></div>
          </details>
          <label className="confirmation"><input type="checkbox" checked={confirmed} disabled={!analysis.canCommit || props.busy} onChange={(event) => setConfirmed(event.currentTarget.checked)} />{t("import.confirm")}</label>
          <button type="button" disabled={!analysis.canCommit || !confirmed || props.busy} onClick={() => void props.onCommit(analysis.batchId).catch(() => undefined)}>{t("import.commit")}</button>
          {!analysis.canCommit ? <p role="alert">{t("import.blocked")}</p> : null}
        </div>
      ) : null}
    </section>
  );
}
