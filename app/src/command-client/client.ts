import { invoke } from "@tauri-apps/api/core";
import type {
  CreateLedgerRequest,
  LedgerKitCommands,
  LedgerStatus,
  LedgerStatusRequest,
  UpdateSettingsRequest,
  UpdateSettingsResult,
  SaveInstitutionRequest,
  SaveCashAccountRequest,
  SaveCategoryRequest,
  SavePortfolioRequest,
  SaveInstrumentRequest,
  SaveFxRevisionRequest,
  SavePriceRevisionRequest,
  SaveResult,
  CashEventRequest,
  EventPreview,
  PostedEvent,
  ExpenseAnalysis,
  ActivityRequest,
  ActivityPage,
  ImportAnalysis,
  ImportCommitResult,
} from "./contracts";

class TauriLedgerKitCommands implements LedgerKitCommands {
  createLedger(request: CreateLedgerRequest): Promise<LedgerStatus> {
    return invoke<LedgerStatus>("create_ledger", { request });
  }

  openLedger(): Promise<LedgerStatus> {
    return invoke<LedgerStatus>("open_ledger");
  }

  getLedgerStatus(request: LedgerStatusRequest): Promise<LedgerStatus> {
    return invoke<LedgerStatus>("get_ledger_status", { request });
  }

  updateSettings(request: UpdateSettingsRequest): Promise<UpdateSettingsResult> {
    return invoke<UpdateSettingsResult>("update_settings", { request });
  }

  saveInstitution(request: SaveInstitutionRequest): Promise<SaveResult> { return invoke("save_institution", { request }); }
  saveCashAccount(request: SaveCashAccountRequest): Promise<SaveResult> { return invoke("save_cash_account", { request }); }
  saveCategory(request: SaveCategoryRequest): Promise<SaveResult> { return invoke("save_category", { request }); }
  savePortfolio(request: SavePortfolioRequest): Promise<SaveResult> { return invoke("save_portfolio", { request }); }
  saveInstrument(request: SaveInstrumentRequest): Promise<SaveResult> { return invoke("save_instrument", { request }); }
  saveFxRevision(request: SaveFxRevisionRequest): Promise<SaveResult> { return invoke("save_fx_revision", { request }); }
  savePriceRevision(request: SavePriceRevisionRequest): Promise<SaveResult> { return invoke("save_price_revision", { request }); }
  previewEvent(request: CashEventRequest): Promise<EventPreview> { return invoke("preview_event", { request }); }
  postEvent(request: CashEventRequest): Promise<PostedEvent> { return invoke("post_event", { request }); }
  reviseEvent(request: { targetEventId: string; reason: string; replacement: CashEventRequest }): Promise<PostedEvent> { return invoke("revise_event", { request }); }
  reverseEvent(request: { targetEventId: string; reason: string; effectiveDate: string; sequence: number }): Promise<PostedEvent> { return invoke("reverse_event", { request }); }
  getExpenseAnalysis(request: { startDate: string; endDate: string; eventWatermark?: number }): Promise<ExpenseAnalysis> { return invoke("get_expense_analysis", { request }); }
  getActivity(request: ActivityRequest): Promise<ActivityPage> { return invoke("get_activity", { request }); }
  analyzeImport(): Promise<ImportAnalysis> { return invoke("analyze_import"); }
  commitImport(request: { batchId: string; confirmed: boolean }): Promise<ImportCommitResult> { return invoke("commit_import", { request }); }
}

export const ledgerKitCommands: LedgerKitCommands = new TauriLedgerKitCommands();
