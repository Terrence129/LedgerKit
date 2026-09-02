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
}

export const ledgerKitCommands: LedgerKitCommands = new TauriLedgerKitCommands();
