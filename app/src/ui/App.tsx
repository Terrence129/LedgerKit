import { useEffect, useRef, useState } from "react";
import { ledgerKitCommands } from "../command-client/client";
import type {
  LedgerStatus,
  ImportAnalysis,
  SaveCashAccountRequest,
  SaveCategoryRequest,
  SaveFxRevisionRequest,
  SaveInstitutionRequest,
  SaveInstrumentRequest,
  SavePortfolioRequest,
  SavePriceRevisionRequest,
} from "../command-client/contracts";
import { ActivityPage } from "./ActivityPage";
import { HealthHome, type WorkspaceView } from "./HealthHome";
import { ImportWizard } from "./ImportWizard";
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
  const [activeView, setActiveView] = useState<WorkspaceView>("activity");
  const [importAnalysis, setImportAnalysis] = useState<ImportAnalysis | null>(null);
  const busyRef = useRef(false);
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

  async function execute<T>(action: () => Promise<T>, refreshAfter = true): Promise<T> {
    if (busyRef.current) throw { code: "COMMAND_ALREADY_RUNNING", field: null };
    busyRef.current = true;
    setBusy(true);
    setFailure(null);
    try {
      const result = await action();
      if (refreshAfter) await refresh();
      return result;
    } catch (error: unknown) {
      const nextFailure = toFailure(error);
      setFailure(nextFailure);
      if (nextFailure?.field) document.getElementById(nextFailure.field)?.focus();
      throw error;
    } finally {
      busyRef.current = false;
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

  const save = <T,>(command: (request: T) => Promise<unknown>) => (request: T) => execute(() => command(request)).then(() => undefined);

  return (
    <HealthHome
      locale={locale}
      status={status}
      failure={failure}
      busy={busy}
      activeView={activeView}
      onNavigate={setActiveView}
      activityContent={status?.ledgerState === "open" && status.catalog ? <ActivityPage
        locale={locale}
        status={status}
        busy={busy}
        onLoad={(request) => ledgerKitCommands.getActivity(request)}
        onPreview={(request) => execute(() => ledgerKitCommands.previewEvent(request), false)}
        onPost={(request) => execute(() => ledgerKitCommands.postEvent(request))}
        onRevise={(request) => execute(() => ledgerKitCommands.reviseEvent(request))}
        onReverse={(request) => execute(() => ledgerKitCommands.reverseEvent(request))}
      /> : null}
      importContent={<ImportWizard
        locale={locale}
        busy={busy}
        analysis={importAnalysis}
        onAnalyze={() => execute(() => ledgerKitCommands.analyzeImport(), false).then((analysis) => {
          setImportAnalysis(analysis);
          return analysis;
        })}
        onCommit={(batchId) => execute(() => ledgerKitCommands.commitImport({ batchId, confirmed: true })).then(() => {
          setImportAnalysis(null);
        })}
      />}
      onLocaleChange={(nextLocale) => void changeLocale(nextLocale).catch(() => undefined)}
      onCreateLedger={(baseCurrency) => execute(() => ledgerKitCommands.createLedger({ baseCurrency, uiLocale: locale })).then(() => undefined)}
      onOpenLedger={() => execute(() => ledgerKitCommands.openLedger()).then(() => undefined)}
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
