import { invoke } from "@tauri-apps/api/core";
import type {
  CreateLedgerRequest,
  LedgerKitCommands,
  LedgerStatus,
  LedgerStatusRequest,
  UpdateSettingsRequest,
  UpdateSettingsResult,
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
}

export const ledgerKitCommands: LedgerKitCommands = new TauriLedgerKitCommands();
