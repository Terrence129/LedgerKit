import { useState, type FormEvent, type ReactNode } from "react";
import type {
  CatalogRecord,
  LedgerStatus,
  SaveCashAccountRequest,
  SaveCategoryRequest,
  SaveFxRevisionRequest,
  SaveInstitutionRequest,
  SaveInstrumentRequest,
  SavePortfolioRequest,
  SavePriceRevisionRequest,
} from "../command-client/contracts";
import { supportedLocales, translate, type SupportedLocale } from "./i18n";

type UiFailure = { code: string; field: string | null } | null;
export type WorkspaceView = "overview" | "activity" | "assets" | "quality" | "settings";

type HealthHomeProps = {
  locale: SupportedLocale;
  status: LedgerStatus | null;
  failure: UiFailure;
  busy: boolean;
  activeView: WorkspaceView;
  activityContent: ReactNode;
  overviewContent?: ReactNode;
  assetsContent?: ReactNode;
  qualityContent?: ReactNode;
  importContent?: ReactNode;
  onNavigate: (view: WorkspaceView) => void;
  onLocaleChange: (locale: SupportedLocale) => void;
  onCreateLedger: (baseCurrency: string) => Promise<void>;
  onOpenLedger: () => Promise<void>;
  onSaveInstitution: (request: SaveInstitutionRequest) => Promise<void>;
  onSaveCashAccount: (request: SaveCashAccountRequest) => Promise<void>;
  onSaveCategory: (request: SaveCategoryRequest) => Promise<void>;
  onSavePortfolio: (request: SavePortfolioRequest) => Promise<void>;
  onSaveInstrument: (request: SaveInstrumentRequest) => Promise<void>;
  onSaveFxRevision: (request: SaveFxRevisionRequest) => Promise<void>;
  onSavePriceRevision: (request: SavePriceRevisionRequest) => Promise<void>;
};

const initialInstitution: SaveInstitutionRequest = { businessId: "", name: "", institutionType: "bank", enabled: true };
const initialAccount: SaveCashAccountRequest = { businessId: "", institutionId: "", name: "", purpose: "daily", currency: "CNY", enabled: true };
const initialCategory: SaveCategoryRequest = { name: "", kind: "expense", semanticRole: "normal", sortOrder: 0, enabled: true };
const initialPortfolio: SavePortfolioRequest = { businessId: "", institutionId: "", settlementAccountId: "", name: "", portfolioType: "brokerage", enabled: true };
const initialInstrument: SaveInstrumentRequest = { businessId: "", code: "", name: "", tradeCurrency: "CNY", enabled: true };
const initialFx: SaveFxRevisionRequest = { rateDate: "", currency: "USD", rateToBase: "", source: "manual", active: true };
const initialPrice: SavePriceRevisionRequest = { instrumentId: "", priceDate: "", price: "", priceCurrency: "CNY", source: "manual", active: true };

export function HealthHome(props: HealthHomeProps) {
  const { locale, status, failure, busy, onLocaleChange } = props;
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  const [baseCurrency, setBaseCurrency] = useState("CNY");
  const [institution, setInstitution] = useState(initialInstitution);
  const [account, setAccount] = useState(initialAccount);
  const [category, setCategory] = useState(initialCategory);
  const [portfolio, setPortfolio] = useState(initialPortfolio);
  const [instrument, setInstrument] = useState(initialInstrument);
  const [fx, setFx] = useState(initialFx);
  const [price, setPrice] = useState(initialPrice);
  const catalog = status?.catalog;

  const submit = <T,>(event: FormEvent, action: (request: T) => Promise<void>, value: T, reset: () => void) => {
    event.preventDefault();
    void action(value).then(reset).catch(() => undefined);
  };

  return (
    <main className="shell">
      <header className="masthead">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">L</span>
          <span><strong>{t("app.name")}</strong><small>{t("app.tagline")}</small></span>
        </div>
        <label className="language-control" htmlFor="uiLocale">
          <span>{t("language.label")}</span>
          <select id="uiLocale" value={locale} disabled={!status || busy} onChange={(event) => onLocaleChange(event.currentTarget.value as SupportedLocale)}>
            {supportedLocales.map((item) => <option key={item} value={item}>{item === "zh-CN" ? t("language.zhCN") : t("language.enUS")}</option>)}
          </select>
        </label>
      </header>

      <div className="sr-status" role="status" aria-live="polite">
        {busy ? t("common.saving") : failure ? `${t("common.error")}: ${failure.code}` : status ? t("common.ready") : t("common.loading")}
      </div>
      {failure ? <div className="error-banner" role="alert"><strong>{t("common.error")}</strong><code>{failure.code}</code></div> : null}

      {!status ? <section className="hero"><h1>{t("setup.loadingTitle")}</h1><p>{t("common.loading")}</p></section> : null}
      {status?.ledgerState === "blocked" ? <section className="hero"><h1>{t("setup.blockedTitle")}</h1><p><code>{status.blockedReason}</code></p></section> : null}
      {status?.ledgerState === "not-created" ? (
        <section className="onboarding" aria-labelledby="setup-title">
          <p className="eyebrow">{t("setup.eyebrow")}</p>
          <h1 id="setup-title">{t("setup.title")}</h1>
          <p className="lede">{t("setup.description")}</p>
          <form className="compact-form" onSubmit={(event) => submit(event, props.onCreateLedger, baseCurrency, () => undefined)}>
            <label htmlFor="currency">{t("field.baseCurrency")}</label>
            <input id="currency" value={baseCurrency} required pattern="[A-Z]{3}" maxLength={3} onChange={(event) => setBaseCurrency(event.currentTarget.value.toUpperCase())} aria-describedby="base-currency-help" />
            <small id="base-currency-help">{t("setup.currencyHelp")}</small>
            <button disabled={busy} type="submit">{t("setup.create")}</button>
          </form>
          {props.importContent}
          <Protection status={status} t={t} />
        </section>
      ) : null}
      {status?.ledgerState === "closed" ? (
        <section className="onboarding"><h1>{t("setup.closedTitle")}</h1><p>{t("setup.closedDescription")}</p><button disabled={busy} onClick={() => void props.onOpenLedger().catch(() => undefined)}>{t("setup.open")}</button><Protection status={status} t={t} /></section>
      ) : null}

      {status?.ledgerState === "open" && catalog ? (
        <>
          <nav className="workspace-nav" aria-label={t("nav.label")}><ul>{(["overview", "activity", "assets", "quality", "settings"] as const).map((view) => <li key={view}><button type="button" className={props.activeView === view ? "active" : ""} aria-current={props.activeView === view ? "page" : undefined} onClick={() => props.onNavigate(view)}>{t(`nav.${view}` as Parameters<typeof translate>[1])}</button></li>)}</ul></nav>
          <div hidden={props.activeView !== "activity"}>{props.activityContent}</div>
          <div hidden={props.activeView !== "overview"}>{props.overviewContent ?? <WorkspacePlaceholder eyebrow={t("overview.eyebrow")} title={t("overview.title")} description={t("overview.description")} />}</div>
          <div hidden={props.activeView !== "assets"}>{props.assetsContent ?? <WorkspacePlaceholder eyebrow={t("assets.eyebrow")} title={t("assets.title")} description={t("assets.description")} />}</div>
          <div hidden={props.activeView !== "quality"}>{props.qualityContent ?? <WorkspacePlaceholder eyebrow={t("quality.eyebrow")} title={t("quality.title")} description={t("quality.empty")} />}</div>
          {props.activeView === "settings" ? <>
          <section className="page-heading"><p className="eyebrow">{t("catalog.eyebrow")}</p><h1>{t("catalog.title")}</h1><p className="lede">{t("catalog.description")}</p></section>
          <Protection status={status} t={t} />
          <section className="quality-card" aria-labelledby="quality-title">
            <div><p className="eyebrow">{t("quality.eyebrow")}</p><h2 id="quality-title">{t("quality.title")}</h2></div>
            {catalog.qualityIssues.length === 0 ? <p className="empty-state">{t("quality.empty")}</p> : <ul className="issue-list">{catalog.qualityIssues.map((issue) => <li key={`${issue.code}-${issue.entityId}`}><strong>{t(`quality.${issue.code}` as Parameters<typeof translate>[1])}</strong><code>{issue.entityId}</code><span>{t("quality.fix")}: {issue.fixField}</span></li>)}</ul>}
          </section>
          <div id="import-review" tabIndex={-1}>{props.importContent}</div>

          <div className="catalog-grid">
            <CatalogSection title={t("catalog.institutions")} records={catalog.institutions} t={t} onEdit={(record) => {
              const [region, institutionType] = record.details;
              setInstitution({ institutionId: record.id, businessId: record.businessId ?? "", name: record.name, ...(region ? { region } : {}), institutionType: institutionType ?? "bank", enabled: record.enabled });
            }}>
              <form onSubmit={(event) => submit(event, props.onSaveInstitution, institution, () => setInstitution(initialInstitution))}>
                <TextField id="businessId" label={t("field.businessId")} value={institution.businessId} onChange={(businessId) => setInstitution({ ...institution, businessId })} />
                <TextField id="institutionName" label={t("field.name")} value={institution.name} onChange={(name) => setInstitution({ ...institution, name })} />
                <TextField id="institutionRegion" label={t("field.region")} value={institution.region ?? ""} required={false} onChange={(region) => setInstitution({ ...institution, ...(region ? { region } : { region: undefined }) })} />
                <TextField id="institutionType" label={t("field.type")} value={institution.institutionType} onChange={(institutionType) => setInstitution({ ...institution, institutionType })} />
                <EnabledField t={t} value={institution.enabled} onChange={(enabled) => setInstitution({ ...institution, enabled })} />
                <FormActions busy={busy} editing={Boolean(institution.institutionId)} t={t} reset={() => setInstitution(initialInstitution)} />
              </form>
            </CatalogSection>

            <CatalogSection title={t("catalog.accounts")} records={catalog.accounts} t={t} onEdit={(record) => {
              const [institutionId, purpose, currency, openedOn] = record.details;
              setAccount({ accountId: record.id, businessId: record.businessId ?? "", institutionId: institutionId ?? "", name: record.name, purpose: purpose ?? "daily", currency: currency ?? "CNY", ...(openedOn ? { openedOn } : {}), enabled: record.enabled });
            }}>
              <form onSubmit={(event) => submit(event, props.onSaveCashAccount, account, () => setAccount(initialAccount))}>
                <TextField id="accountBusinessId" label={t("field.businessId")} value={account.businessId} onChange={(businessId) => setAccount({ ...account, businessId })} />
                <TextField id="accountName" label={t("field.name")} value={account.name} onChange={(name) => setAccount({ ...account, name })} />
                <SelectField id="institutionId" label={t("field.institution")} value={account.institutionId} options={catalog.institutions} onChange={(institutionId) => setAccount({ ...account, institutionId })} />
                <TextField id="purpose" label={t("field.purpose")} value={account.purpose} onChange={(purpose) => setAccount({ ...account, purpose })} />
                <TextField id="currency" label={t("field.currency")} value={account.currency} pattern="[A-Z]{3}" onChange={(currency) => setAccount({ ...account, currency: currency.toUpperCase() })} />
                <TextField id="openedOn" label={t("field.openedOn")} type="date" value={account.openedOn ?? ""} required={false} onChange={(openedOn) => setAccount({ ...account, ...(openedOn ? { openedOn } : { openedOn: undefined }) })} />
                <EnabledField t={t} value={account.enabled} onChange={(enabled) => setAccount({ ...account, enabled })} />
                <FormActions busy={busy} editing={Boolean(account.accountId)} t={t} reset={() => setAccount(initialAccount)} />
              </form>
            </CatalogSection>

            <CatalogSection title={t("catalog.categories")} records={catalog.categories} t={t} onEdit={(record) => {
              const [kind, semanticRole, sortOrder] = record.details;
              setCategory({ categoryId: record.id, name: record.name, kind: kind === "income" ? "income" : "expense", semanticRole: semanticRole === "refund" || semanticRole === "reimbursement" ? semanticRole : "normal", sortOrder: Number(sortOrder ?? 0), enabled: record.enabled });
            }}>
              <form onSubmit={(event) => submit(event, props.onSaveCategory, category, () => setCategory(initialCategory))}>
                <TextField id="categoryName" label={t("field.name")} value={category.name} onChange={(name) => setCategory({ ...category, name })} />
                <label>{t("field.kind")}<select id="kind" value={category.kind} onChange={(event) => setCategory({ ...category, kind: event.currentTarget.value as SaveCategoryRequest["kind"] })}><option value="expense">{t("value.expense")}</option><option value="income">{t("value.income")}</option></select></label>
                <label>{t("field.semanticRole")}<select id="semanticRole" value={category.semanticRole} onChange={(event) => setCategory({ ...category, semanticRole: event.currentTarget.value as SaveCategoryRequest["semanticRole"] })}><option value="normal">{t("value.normal")}</option><option value="refund">{t("value.refund")}</option><option value="reimbursement">{t("value.reimbursement")}</option></select></label>
                <label>{t("field.sortOrder")}<input id="sortOrder" type="number" min={0} value={category.sortOrder} onChange={(event) => setCategory({ ...category, sortOrder: Number(event.currentTarget.value) })} /></label>
                <EnabledField t={t} value={category.enabled} onChange={(enabled) => setCategory({ ...category, enabled })} />
                <FormActions busy={busy} editing={Boolean(category.categoryId)} t={t} reset={() => setCategory(initialCategory)} />
              </form>
            </CatalogSection>

            <CatalogSection title={t("catalog.portfolios")} records={catalog.portfolios} t={t} onEdit={(record) => {
              const [institutionId, settlementAccountId, portfolioType] = record.details;
              setPortfolio({ portfolioId: record.id, businessId: record.businessId ?? "", institutionId: institutionId ?? "", settlementAccountId: settlementAccountId ?? "", name: record.name, portfolioType: portfolioType ?? "brokerage", enabled: record.enabled });
            }}>
              <form onSubmit={(event) => submit(event, props.onSavePortfolio, portfolio, () => setPortfolio(initialPortfolio))}>
                <TextField id="portfolioBusinessId" label={t("field.businessId")} value={portfolio.businessId} onChange={(businessId) => setPortfolio({ ...portfolio, businessId })} />
                <TextField id="portfolioName" label={t("field.name")} value={portfolio.name} onChange={(name) => setPortfolio({ ...portfolio, name })} />
                <SelectField id="portfolioInstitutionId" label={t("field.institution")} value={portfolio.institutionId} options={catalog.institutions} onChange={(institutionId) => setPortfolio({ ...portfolio, institutionId })} />
                <SelectField id="settlementAccountId" label={t("field.settlementAccount")} value={portfolio.settlementAccountId} options={catalog.accounts} onChange={(settlementAccountId) => setPortfolio({ ...portfolio, settlementAccountId })} />
                <TextField id="portfolioType" label={t("field.type")} value={portfolio.portfolioType} onChange={(portfolioType) => setPortfolio({ ...portfolio, portfolioType })} />
                <EnabledField t={t} value={portfolio.enabled} onChange={(enabled) => setPortfolio({ ...portfolio, enabled })} />
                <FormActions busy={busy} editing={Boolean(portfolio.portfolioId)} t={t} reset={() => setPortfolio(initialPortfolio)} />
              </form>
            </CatalogSection>

            <CatalogSection title={t("catalog.instruments")} records={catalog.instruments} t={t} onEdit={(record) => {
              const [code, tradeCurrency] = record.details;
              setInstrument({ instrumentId: record.id, businessId: record.businessId ?? "", code: code ?? "", name: record.name, tradeCurrency: tradeCurrency ?? "CNY", enabled: record.enabled });
            }}>
              <form onSubmit={(event) => submit(event, props.onSaveInstrument, instrument, () => setInstrument(initialInstrument))}>
                <TextField id="instrumentBusinessId" label={t("field.businessId")} value={instrument.businessId} onChange={(businessId) => setInstrument({ ...instrument, businessId })} />
                <TextField id="code" label={t("field.code")} value={instrument.code} onChange={(code) => setInstrument({ ...instrument, code })} />
                <TextField id="instrumentName" label={t("field.name")} value={instrument.name} onChange={(name) => setInstrument({ ...instrument, name })} />
                <TextField id="tradeCurrency" label={t("field.tradeCurrency")} value={instrument.tradeCurrency} pattern="[A-Z]{3}" onChange={(tradeCurrency) => setInstrument({ ...instrument, tradeCurrency: tradeCurrency.toUpperCase() })} />
                <EnabledField t={t} value={instrument.enabled} onChange={(enabled) => setInstrument({ ...instrument, enabled })} />
                <FormActions busy={busy} editing={Boolean(instrument.instrumentId)} t={t} reset={() => setInstrument(initialInstrument)} />
              </form>
            </CatalogSection>
          </div>

          <section className="market-section" aria-labelledby="market-title"><p className="eyebrow">{t("market.eyebrow")}</p><h2 id="market-title">{t("market.title")}</h2><p>{t("market.description")}</p>
            <div className="catalog-grid">
              <article className="catalog-card"><h3>{t("market.fx")}</h3><form onSubmit={(event) => submit(event, props.onSaveFxRevision, fx, () => setFx(initialFx))}>
                <TextField id="rateDate" label={t("field.date")} type="date" value={fx.rateDate} onChange={(rateDate) => setFx({ ...fx, rateDate })} />
                <TextField id="fxCurrency" label={t("field.currency")} value={fx.currency} pattern="[A-Z]{3}" onChange={(currency) => setFx({ ...fx, currency: currency.toUpperCase() })} />
                <TextField id="rateToBase" label={t("field.rateToBase")} value={fx.rateToBase} inputMode="decimal" onChange={(rateToBase) => setFx({ ...fx, rateToBase })} />
                <TextField id="fxSource" label={t("field.source")} value={fx.source} onChange={(source) => setFx({ ...fx, source })} />
                <EnabledField t={t} value={fx.active} active onChange={(active) => setFx({ ...fx, active })} /><FormActions busy={busy} editing={Boolean(fx.revisionId)} t={t} reset={() => setFx(initialFx)} />
              </form><RevisionList records={catalog.fxRevisions} t={t} onEdit={(revision) => setFx({ revisionId: revision.id, rateDate: revision.date, currency: revision.ownerId, rateToBase: revision.value, source: revision.source, active: revision.active })} /></article>
              <article className="catalog-card"><h3>{t("market.prices")}</h3><form onSubmit={(event) => submit(event, props.onSavePriceRevision, price, () => setPrice(initialPrice))}>
                <SelectField id="instrumentId" label={t("field.instrument")} value={price.instrumentId} options={catalog.instruments} onChange={(instrumentId) => setPrice({ ...price, instrumentId })} />
                <TextField id="priceDate" label={t("field.date")} type="date" value={price.priceDate} onChange={(priceDate) => setPrice({ ...price, priceDate })} />
                <TextField id="price" label={t("field.price")} value={price.price} inputMode="decimal" onChange={(nextPrice) => setPrice({ ...price, price: nextPrice })} />
                <TextField id="priceCurrency" label={t("field.currency")} value={price.priceCurrency} pattern="[A-Z]{3}" onChange={(priceCurrency) => setPrice({ ...price, priceCurrency: priceCurrency.toUpperCase() })} />
                <TextField id="priceSource" label={t("field.source")} value={price.source} onChange={(source) => setPrice({ ...price, source })} />
                <EnabledField t={t} value={price.active} active onChange={(active) => setPrice({ ...price, active })} /><FormActions busy={busy} editing={Boolean(price.revisionId)} t={t} reset={() => setPrice(initialPrice)} />
              </form><RevisionList records={catalog.priceRevisions} t={t} onEdit={(revision) => setPrice({ revisionId: revision.id, instrumentId: revision.ownerId, priceDate: revision.date, price: revision.value, priceCurrency: revision.currency, source: revision.source, active: revision.active })} /></article>
            </div>
          </section>
          </> : null}
        </>
      ) : null}
    </main>
  );
}

function WorkspacePlaceholder({ eyebrow, title, description }: { eyebrow: string; title: string; description: string }) {
  return <section className="page-heading workspace-placeholder"><p className="eyebrow">{eyebrow}</p><h1>{title}</h1><p className="lede">{description}</p></section>;
}

type T = (key: Parameters<typeof translate>[1]) => string;

function Protection({ status, t }: { status: LedgerStatus; t: T }) {
  return <section className="protection-card" aria-label={t("protection.title")}><div><strong>{t("protection.location")}</strong><code>{status.databaseLocation ?? "—"}</code></div><div><strong>{t("protection.title")}</strong><span className={status.deviceLossProtected ? "state-ok" : "state-warning"}>{status.deviceLossProtected ? t("protection.protected") : t("protection.notProtected")}</span></div></section>;
}

function CatalogSection({ title, records, children, onEdit, t }: { title: string; records: CatalogRecord[]; children: React.ReactNode; onEdit: (record: CatalogRecord) => void; t: T }) {
  return <article className="catalog-card"><h2>{title}</h2>{children}{records.length === 0 ? <p className="empty-state">{t("common.empty")}</p> : <ul className="record-list">{records.map((record) => <li key={record.id}><div><strong>{record.name}</strong><small>{record.businessId ?? record.id}</small><span>{record.enabled ? t("common.enabled") : t("common.archived")}</span></div><button type="button" onClick={() => onEdit(record)}>{t("common.edit")}</button></li>)}</ul>}</article>;
}

function RevisionList({ records, onEdit, t }: { records: NonNullable<LedgerStatus["catalog"]>["fxRevisions"]; onEdit: (record: NonNullable<LedgerStatus["catalog"]>["fxRevisions"][number]) => void; t: T }) {
  return records.length === 0 ? <p className="empty-state">{t("common.empty")}</p> : <ul className="record-list">{records.map((record) => <li key={record.id}><div><strong>{record.ownerId} · {record.value}</strong><small>{record.date} · {record.source} · r{record.revision}</small><span>{record.active ? t("common.active") : t("common.inactive")}</span></div><button type="button" onClick={() => onEdit(record)}>{t("common.select")}</button></li>)}</ul>;
}

function TextField({ id, label, value, onChange, required = true, type = "text", pattern, inputMode }: { id: string; label: string; value: string; onChange: (value: string) => void; required?: boolean; type?: string; pattern?: string; inputMode?: "decimal" }) {
  return <label htmlFor={id}>{label}<input id={id} value={value} required={required} type={type} pattern={pattern} inputMode={inputMode} onChange={(event) => onChange(event.currentTarget.value)} /></label>;
}

function SelectField({ id, label, value, options, onChange }: { id: string; label: string; value: string; options: CatalogRecord[]; onChange: (value: string) => void }) {
  return <label htmlFor={id}>{label}<select id={id} value={value} required onChange={(event) => onChange(event.currentTarget.value)}><option value="" disabled>—</option>{options.filter((item) => item.enabled || item.id === value).map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}</select></label>;
}

function EnabledField({ value, onChange, t, active = false }: { value: boolean; onChange: (value: boolean) => void; t: T; active?: boolean }) {
  return <label className="checkbox-field"><input type="checkbox" checked={value} onChange={(event) => onChange(event.currentTarget.checked)} />{active ? t("field.active") : t("field.enabled")}</label>;
}

function FormActions({ busy, editing, reset, t }: { busy: boolean; editing: boolean; reset: () => void; t: T }) {
  return <div className="form-actions"><button disabled={busy} type="submit">{editing ? t("common.update") : t("common.add")}</button>{editing ? <button className="secondary" type="button" onClick={reset}>{t("common.cancel")}</button> : null}</div>;
}
