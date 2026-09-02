import { useEffect, useRef, useState, type FormEvent } from "react";
import type { DataQualityReport, FixContext } from "../command-client/contracts";
import { translate, type SupportedLocale } from "./i18n";
import { LatestRequestGate } from "./queryGate";

type DataQualityPageProps = {
  locale: SupportedLocale;
  asOfDate: string;
  onLoad: (request: { asOfDate: string }) => Promise<DataQualityReport>;
  onFix: (context: FixContext) => void;
};

function errorCode(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error && typeof error.code === "string") return error.code;
  return "UNEXPECTED_ERROR";
}

export function DataQualityPage(props: DataQualityPageProps) {
  const t = (key: Parameters<typeof translate>[1]) => translate(props.locale, key);
  const [asOfDate, setAsOfDate] = useState(props.asOfDate);
  const [report, setReport] = useState<DataQualityReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const gate = useRef(new LatestRequestGate());

  async function load(): Promise<void> {
    const generation = gate.current.begin();
    setLoading(true);
    setFailure(null);
    try {
      const next = await props.onLoad({ asOfDate });
      if (gate.current.isLatest(generation)) setReport(next);
    } catch (error: unknown) {
      if (gate.current.isLatest(generation)) {
        setReport(null);
        setFailure(errorCode(error));
      }
    } finally {
      if (gate.current.isLatest(generation)) setLoading(false);
    }
  }

  useEffect(() => { void load(); }, [props.asOfDate]);

  function submit(event: FormEvent): void {
    event.preventDefault();
    void load();
  }

  return <section className="quality-page" aria-labelledby="quality-page-title">
    <header className="dashboard-heading"><div><p className="eyebrow">{t("quality.eyebrow")}</p><h1 id="quality-page-title">{t("quality.title")}</h1><p>{t("quality.description")}</p></div></header>
    <form className="date-toolbar" onSubmit={submit}><label htmlFor="qualityAsOfDate">{t("assets.valuationDate")}<input id="qualityAsOfDate" required type="date" value={asOfDate} onChange={(event) => setAsOfDate(event.currentTarget.value)} /></label><button type="submit" disabled={loading}>{t("quality.refresh")}</button></form>
    <div className="sr-status" role="status" aria-live="polite">{loading ? t("common.loading") : failure ? t("quality.failed") : t("quality.updated")}</div>
    {failure ? <p className="inline-error" role="alert">{t("quality.failed")}: <code>{failure}</code></p> : null}
    {report ? <>
      <dl className="dashboard-kpis quality-kpis"><div><dt>{t("quality.blockers")}</dt><dd>{report.blockerCount}</dd></div><div><dt>{t("quality.warnings")}</dt><dd>{report.warningCount}</dd></div><div><dt>{t("assets.valuationDate")}</dt><dd>{report.asOfDate}</dd></div></dl>
      {report.issues.length === 0 ? <p className="notice">{t("quality.empty")}</p> : <div className="table-scroll"><table className="quality-table"><thead><tr><th scope="col">{t("quality.issue")}</th><th scope="col">{t("quality.reason")}</th><th scope="col">{t("quality.location")}</th><th scope="col">{t("quality.status")}</th><th scope="col">{t("quality.action")}</th></tr></thead><tbody>{report.issues.map((issue) => <tr key={issue.issueId}><th scope="row"><code>{issue.code}</code><small>{issue.severity === "blocker" ? t("quality.blocker") : t("quality.warning")}</small></th><td>{t(`quality.${issue.code}` as Parameters<typeof translate>[1]) || issue.code}</td><td><code>{issue.context.entityType}:{issue.context.entityId}</code><small>{issue.context.field}</small></td><td>{issue.status === "open" ? t("quality.open") : issue.status}</td><td><button type="button" onClick={() => props.onFix(issue.context)}>{t("quality.fixNow")}</button></td></tr>)}</tbody></table></div>}
      <p className="dashboard-meta">{report.calculationVersion} · {t("common.watermark")} {report.eventWatermark}</p>
    </> : null}
  </section>;
}
