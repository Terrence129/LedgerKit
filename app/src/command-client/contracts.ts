import type { SupportedLocale } from "../shared-contracts/locales";

export type LedgerStatusRequest = {
  systemLocale: string | null;
};

export type LedgerStatus = {
  appVersion: string;
  uiLocale: SupportedLocale;
  ledgerState: "not-created" | "closed" | "open" | "blocked";
  ledgerId: string | null;
  schemaVersion: number | null;
  baseCurrency: string | null;
  eventWatermark: number;
  projectionWatermark: number;
  calculationVersion: string;
  blockedReason: string | null;
  localOnly: true;
  privilegedOperationCount: number;
};

export type CreateLedgerRequest = {
  baseCurrency: string;
  uiLocale: SupportedLocale;
};

export type UpdateSettingsRequest = {
  uiLocale: SupportedLocale;
  baseCurrency?: string;
  valuationDefaults?: Record<string, unknown>;
};

export type UpdateSettingsResult = {
  uiLocale: SupportedLocale;
  baseCurrency: string | null;
  persisted: boolean;
};

export interface LedgerKitCommands {
  createLedger(request: CreateLedgerRequest): Promise<LedgerStatus>;
  openLedger(): Promise<LedgerStatus>;
  getLedgerStatus(request: LedgerStatusRequest): Promise<LedgerStatus>;
  updateSettings(request: UpdateSettingsRequest): Promise<UpdateSettingsResult>;
}
