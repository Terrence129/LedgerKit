import { useEffect, useState } from "react";
import { ledgerKitCommands } from "../command-client/client";
import type {
  LedgerStatus,
  SaveCashAccountRequest,
  SaveCategoryRequest,
  SaveFxRevisionRequest,
  SaveInstitutionRequest,
  SaveInstrumentRequest,
  SavePortfolioRequest,
  SavePriceRevisionRequest,
} from "../command-client/contracts";
import { HealthHome } from "./HealthHome";
import { applyDocumentLocale, localeFromSystemHint, systemLocaleHint, type SupportedLocale } from "./i18n";
import "./styles.css";

type UiFailure = { code: string; field: string | null } | null;

function today(): string {
  const date = new Date();
  const year = String(date.getFullYear()).padStart(4, "0");
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function toFailure(error: unknown): UiFailure {
  if (typeof error === "object" && error !== null && "code" in error) {
    const value = error as { code: unknown; field?: unknown };
    return {
      code: typeof value.code === "string" ? value.code : "UNEXPECTED_ERROR",
      field: typeof value.field === "string" ? value.field : null,
    };
  }
  return { code: "UNEXPECTED_ERROR", field: null };
}

export function App() {
  const [locale, setLocale] = useState<SupportedLocale>(() => localeFromSystemHint(systemLocaleHint()));
  const [status, setStatus] = useState<LedgerStatus | null>(null);
  const [failure, setFailure] = useState<UiFailure>(null);
  const [busy, setBusy] = useState(false);
  const asOfDate = today();

  async function refresh(): Promise<void> {
    const nextStatus = await ledgerKitCommands.getLedgerStatus({ systemLocale: systemLocaleHint(), asOfDate });
    setLocale(nextStatus.uiLocale);
    applyDocumentLocale(nextStatus.uiLocale);
    setStatus(nextStatus);
  }

  useEffect(() => {
    void refresh().catch((error: unknown) => setFailure(toFailure(error)));
  }, []);

  async function execute(action: () => Promise<unknown>): Promise<void> {
    setBusy(true);
    setFailure(null);
    try {
      await action();
      await refresh();
    } catch (error: unknown) {
      const nextFailure = toFailure(error);
      setFailure(nextFailure);
      if (nextFailure?.field) document.getElementById(nextFailure.field)?.focus();
    } finally {
      setBusy(false);
    }
  }

  async function changeLocale(nextLocale: SupportedLocale): Promise<void> {
    if (!status || nextLocale === locale) return;
    const previousLocale = locale;
    setLocale(nextLocale);
    applyDocumentLocale(nextLocale);
    await execute(async () => {
      try {
        await ledgerKitCommands.updateSettings({ uiLocale: nextLocale });
      } catch (error: unknown) {
        setLocale(previousLocale);
        applyDocumentLocale(previousLocale);
        throw error;
      }
    });
  }

  const save = <T,>(command: (request: T) => Promise<unknown>) => (request: T) => execute(() => command(request));

  return (
    <HealthHome
      locale={locale}
      status={status}
      failure={failure}
      busy={busy}
      onLocaleChange={(nextLocale) => void changeLocale(nextLocale)}
      onCreateLedger={(baseCurrency) => execute(() => ledgerKitCommands.createLedger({ baseCurrency, uiLocale: locale }))}
      onOpenLedger={() => execute(() => ledgerKitCommands.openLedger())}
      onSaveInstitution={save<SaveInstitutionRequest>((request) => ledgerKitCommands.saveInstitution(request))}
      onSaveCashAccount={save<SaveCashAccountRequest>((request) => ledgerKitCommands.saveCashAccount(request))}
      onSaveCategory={save<SaveCategoryRequest>((request) => ledgerKitCommands.saveCategory(request))}
      onSavePortfolio={save<SavePortfolioRequest>((request) => ledgerKitCommands.savePortfolio(request))}
      onSaveInstrument={save<SaveInstrumentRequest>((request) => ledgerKitCommands.saveInstrument(request))}
      onSaveFxRevision={save<SaveFxRevisionRequest>((request) => ledgerKitCommands.saveFxRevision(request))}
      onSavePriceRevision={save<SavePriceRevisionRequest>((request) => ledgerKitCommands.savePriceRevision(request))}
    />
  );
}
