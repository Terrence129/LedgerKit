import type { SupportedLocale } from "../shared-contracts/locales";

export type LedgerStatusRequest = {
  systemLocale: string | null;
};

export type LedgerStatus = {
  appVersion: string;
  uiLocale: SupportedLocale;
  ledgerState: "not-created";
  localOnly: true;
  privilegedOperationCount: number;
};

export type UpdateSettingsRequest = {
  uiLocale: SupportedLocale;
};

export type UpdateSettingsResult = {
  uiLocale: SupportedLocale;
  persisted: boolean;
};

export interface LedgerKitCommands {
  getLedgerStatus(request: LedgerStatusRequest): Promise<LedgerStatus>;
  updateSettings(request: UpdateSettingsRequest): Promise<UpdateSettingsResult>;
}
