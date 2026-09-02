import { useEffect, useState } from "react";
import { ledgerKitCommands } from "../command-client/client";
import type { LedgerStatus } from "../command-client/contracts";
import { HealthHome } from "./HealthHome";
import {
  applyDocumentLocale,
  localeFromSystemHint,
  systemLocaleHint,
  type SupportedLocale,
} from "./i18n";
import "./styles.css";

export function App() {
  const [locale, setLocale] = useState<SupportedLocale>(() => localeFromSystemHint(systemLocaleHint()));
  const [status, setStatus] = useState<LedgerStatus | null>(null);
  const [failure, setFailure] = useState(false);
  const [savingLocale, setSavingLocale] = useState(false);

  useEffect(() => {
    void ledgerKitCommands
      .getLedgerStatus({ systemLocale: systemLocaleHint() })
      .then((nextStatus) => {
        setLocale(nextStatus.uiLocale);
        applyDocumentLocale(nextStatus.uiLocale);
        setStatus(nextStatus);
      })
      .catch(() => setFailure(true));
  }, []);

  async function changeLocale(nextLocale: SupportedLocale): Promise<void> {
    if (!status || nextLocale === locale) return;
    const previousLocale = locale;
    setLocale(nextLocale);
    applyDocumentLocale(nextLocale);
    setSavingLocale(true);
    try {
      const result = await ledgerKitCommands.updateSettings({ uiLocale: nextLocale });
      setStatus({ ...status, uiLocale: result.uiLocale });
    } catch {
      setLocale(previousLocale);
      applyDocumentLocale(previousLocale);
      setFailure(true);
    } finally {
      setSavingLocale(false);
    }
  }

  return (
    <HealthHome
      locale={locale}
      status={status}
      failure={failure}
      savingLocale={savingLocale}
      onLocaleChange={(nextLocale) => void changeLocale(nextLocale)}
    />
  );
}
