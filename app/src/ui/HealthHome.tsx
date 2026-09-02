import type { LedgerStatus } from "../command-client/contracts";
import {
  supportedLocales,
  translate,
  type SupportedLocale,
} from "./i18n";

type HealthHomeProps = {
  locale: SupportedLocale;
  status: LedgerStatus | null;
  failure: boolean;
  savingLocale: boolean;
  onLocaleChange: (locale: SupportedLocale) => void;
};

export function HealthHome({
  locale,
  status,
  failure,
  savingLocale,
  onLocaleChange,
}: HealthHomeProps) {
  const t = (key: Parameters<typeof translate>[1]) => translate(locale, key);
  const statusText = failure
    ? t("health.error")
    : status
      ? savingLocale
        ? t("language.saving")
        : t("health.ready")
      : t("health.loading");

  return (
    <main className="shell">
      <header className="masthead">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">L</span>
          <span>
            <strong>{t("app.name")}</strong>
            <small>{t("app.tagline")}</small>
          </span>
        </div>
        <label className="language-control" htmlFor="ui-locale">
          <span>{t("language.label")}</span>
          <select
            id="ui-locale"
            value={locale}
            disabled={!status || savingLocale}
            onChange={(event) => onLocaleChange(event.currentTarget.value as SupportedLocale)}
          >
            {supportedLocales.map((item) => (
              <option key={item} value={item}>
                {item === "zh-CN" ? t("language.zhCN") : t("language.enUS")}
              </option>
            ))}
          </select>
        </label>
      </header>

      <section className="hero" aria-labelledby="health-title">
        <p className="eyebrow">{t("health.eyebrow")}</p>
        <h1 id="health-title">{t("health.title")}</h1>
        <p className="lede">{t("health.description")}</p>
        <p className={failure ? "health-pill health-pill--error" : "health-pill"} role="status" aria-live="polite">
          <span aria-hidden="true" />
          {statusText}
        </p>
      </section>

      <section className="status-card" aria-label={t("health.statusLabel")}>
        <dl className="status-grid">
          <div>
            <dt>{t("health.localOnlyLabel")}</dt>
            <dd>{status?.localOnly ? t("health.localOnlyValue") : "—"}</dd>
          </div>
          <div>
            <dt>{t("health.ledgerLabel")}</dt>
            <dd>{status?.ledgerState === "not-created" ? t("health.ledgerNotCreated") : "—"}</dd>
          </div>
          <div>
            <dt>{t("health.boundaryLabel")}</dt>
            <dd>{status?.privilegedOperationCount ?? "—"}</dd>
          </div>
          <div>
            <dt>{t("health.versionLabel")}</dt>
            <dd>{status?.appVersion ?? "—"}</dd>
          </div>
        </dl>
      </section>

      <section className="next-card" aria-labelledby="next-title">
        <div className="next-number" aria-hidden="true">02</div>
        <div>
          <p className="eyebrow">M2</p>
          <h2 id="next-title">{t("health.nextTitle")}</h2>
          <p>{t("health.nextDescription")}</p>
        </div>
      </section>
    </main>
  );
}
