import { useEffect, useRef, useState } from "react";
import type { InvestmentWorkspace, LedgerStatus } from "../command-client/contracts";
import { translate, type SupportedLocale } from "./i18n";

type Props = {
  locale: SupportedLocale;
  status: LedgerStatus;
  refreshVersion?: number;
  onLoad: (request: { asOfDate: string }) => Promise<InvestmentWorkspace>;
};

export function AssetsPage({ locale, status, onLoad, refreshVersion }: Props) {
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  const [asOfDate, setAsOfDate] = useState(status.catalog?.asOfDate ?? new Date().toISOString().slice(0, 10));
  const [workspace, setWorkspace] = useState<InvestmentWorkspace | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const requestId = useRef(0);

  async function load(): Promise<void> {
    const current = ++requestId.current;
    setFailure(null);
    try {
      const result = await onLoad({ asOfDate });
      if (current === requestId.current) setWorkspace(result);
    } catch (error: unknown) {
      if (current === requestId.current) {
        setWorkspace(null);
        setSelected(null);
        setFailure(typeof error === "object" && error !== null && "code" in error ? String(error.code) : "UNEXPECTED_ERROR");
      }
    }
  }

  useEffect(() => {
    void load();
    return () => { requestId.current += 1; };
  }, [status.ledgerId, status.eventWatermark, refreshVersion]);
  const holding = workspace?.holdings.find((item) => `${item.portfolioId}:${item.instrumentId}` === selected) ?? null;
  const catalog = status.catalog;
  if (!catalog) return null;

  return <section className="assets-page" aria-labelledby="assets-title">
    <div className="page-heading activity-heading"><div><p className="eyebrow">{t("assets.eyebrow")}</p><h1 id="assets-title">{t("assets.title")}</h1><p className="lede">{t("assets.description")}</p></div><form onSubmit={(event) => { event.preventDefault(); void load(); }}><label>{t("assets.valuationDate")}<input type="date" value={asOfDate} onChange={(event) => setAsOfDate(event.currentTarget.value)} /></label><button type="submit">{t("assets.refresh")}</button></form></div>
    {failure ? <p className="inline-error" role="alert"><code>{failure}</code></p> : null}
    <div className="asset-reference-grid"><article><h2>{t("catalog.portfolios")}</h2><ul>{catalog.portfolios.map((item) => <li key={item.id}><strong>{item.name}</strong><small>{item.details[2]}</small></li>)}</ul></article><article><h2>{t("catalog.instruments")}</h2><ul>{catalog.instruments.map((item) => <li key={item.id}><strong>{item.name}</strong><small>{item.details[0]} · {item.details[1]}</small></li>)}</ul></article><article><h2>{t("assets.priceHistory")}</h2><ul>{catalog.priceRevisions.map((item) => <li key={item.id}><strong>{item.date}</strong><span>{item.value} {item.currency}</span><small>{item.source} · r{item.revision}</small></li>)}</ul></article></div>
    <section className="timeline-section" aria-labelledby="holding-list-title"><h2 id="holding-list-title">{t("assets.holdings")}</h2>{!workspace || workspace.holdings.length === 0 ? <p className="empty-state">{t("assets.empty")}</p> : null}<div className="table-scroll"><table><thead><tr><th scope="col">{t("investment.portfolio")}</th><th scope="col">{t("field.instrument")}</th><th scope="col">{t("investment.quantity")}</th><th scope="col">{t("assets.carryingCost")}</th><th scope="col">{t("assets.marketValue")}</th><th scope="col">{t("assets.totalReturn")}</th><th scope="col">{t("assets.state")}</th></tr></thead><tbody>{workspace?.holdings.map((item) => <tr key={`${item.portfolioId}:${item.instrumentId}`}><td>{item.portfolioName}</td><td><button type="button" className="table-link" onClick={() => setSelected(`${item.portfolioId}:${item.instrumentId}`)}>{item.instrumentName}</button></td><td>{item.quantity}</td><td>{item.carryingCost} {item.currency}</td><td>{item.baseMarketValue === null ? t("activity.unvalued") : `${item.baseMarketValue} ${workspace.baseCurrency}`}</td><td>{item.totalReturn ?? "—"}</td><td>{item.valuationState === "valued" ? t("assets.valued") : t("assets.unvalued")}</td></tr>)}</tbody></table></div></section>
    {workspace && workspace.portfolioExpenses.length > 0 ? <section className="quality-card"><h2>{t("assets.portfolioExpenses")}</h2><ul className="record-list">{workspace.portfolioExpenses.map((item) => <li key={`${item.portfolioId}:${item.currency}`}><strong>{item.portfolioName}</strong><span>{item.amount} {item.currency}</span></li>)}</ul></section> : null}
    {holding ? <aside className="detail-panel" aria-labelledby="holding-title"><div className="detail-header"><div><p className="eyebrow">{t("assets.detail")}</p><h2 id="holding-title">{holding.instrumentName}</h2></div><button type="button" className="secondary" onClick={() => setSelected(null)}>{t("activity.closeDetails")}</button></div><dl className="detail-grid"><div><dt>{t("investment.portfolio")}</dt><dd>{holding.portfolioName}</dd></div><div><dt>{t("investment.averageAfter")}</dt><dd>{holding.averageCost ?? "—"}</dd></div><div><dt>{t("assets.realized")}</dt><dd>{holding.realizedTradePnl}</dd></div><div><dt>{t("assets.netDividend")}</dt><dd>{holding.netDividend}</dd></div><div><dt>{t("assets.independentExpense")}</dt><dd>{holding.independentExpense}</dd></div><div><dt>{t("assets.unrealized")}</dt><dd>{holding.unrealizedPnl ?? "—"}</dd></div><div><dt>{t("assets.priceEvidence")}</dt><dd>{holding.priceRevisionId ?? holding.unvaluedReason ?? "—"} {holding.priceDate ? `· ${holding.priceDate}` : ""}</dd></div><div><dt>{t("assets.fxEvidence")}</dt><dd>{holding.fxRevisionId ?? (holding.fxRate === "1" ? "1:1" : holding.unvaluedReason ?? "—")}</dd></div></dl>{holding.warningCodes.map((code) => <p key={code} className="quality-warning"><code>{code}</code></p>)}</aside> : null}
  </section>;
}
