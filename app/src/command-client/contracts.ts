import type { SupportedLocale } from "../shared-contracts/locales";

export type LedgerStatusRequest = {
  systemLocale: string | null;
  asOfDate?: string;
};

export type CatalogRecord = {
  id: string;
  businessId: string | null;
  name: string;
  details: string[];
  enabled: boolean;
};

export type MarketRevision = {
  id: string;
  ownerId: string;
  date: string;
  value: string;
  currency: string;
  source: string;
  revision: number;
  active: boolean;
};

export type QualityIssue = {
  code: string;
  entityType: string;
  entityId: string;
  fixOperation: string;
  fixField: string;
};

export type CatalogSnapshot = {
  asOfDate: string;
  baseCurrency: string;
  institutions: CatalogRecord[];
  accounts: CatalogRecord[];
  categories: CatalogRecord[];
  portfolios: CatalogRecord[];
  instruments: CatalogRecord[];
  fxRevisions: MarketRevision[];
  priceRevisions: MarketRevision[];
  qualityIssues: QualityIssue[];
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
  databaseLocation: string | null;
  backupProtectionState: "not-configured" | "pending" | "protected" | "failed";
  deviceLossProtected: boolean;
  catalog: CatalogSnapshot | null;
  localOnly: true;
  privilegedOperationCount: number;
};

export type SaveInstitutionRequest = { institutionId?: string; businessId: string; name: string; region?: string | undefined; institutionType: string; enabled: boolean };
export type SaveCashAccountRequest = { accountId?: string; businessId: string; institutionId: string; name: string; purpose: string; currency: string; openedOn?: string | undefined; enabled: boolean };
export type SaveCategoryRequest = { categoryId?: string; name: string; kind: "income" | "expense"; semanticRole: "normal" | "refund" | "reimbursement"; sortOrder: number; enabled: boolean };
export type SavePortfolioRequest = { portfolioId?: string; businessId: string; institutionId: string; settlementAccountId: string; name: string; portfolioType: string; enabled: boolean };
export type SaveInstrumentRequest = { instrumentId?: string; businessId: string; code: string; name: string; tradeCurrency: string; enabled: boolean };
export type SaveFxRevisionRequest = { revisionId?: string; rateDate: string; currency: string; rateToBase: string; source: string; active: boolean };
export type SavePriceRevisionRequest = { revisionId?: string; instrumentId: string; priceDate: string; price: string; priceCurrency: string; source: string; active: boolean };
export type SaveResult = { id: string };

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
  saveInstitution(request: SaveInstitutionRequest): Promise<SaveResult>;
  saveCashAccount(request: SaveCashAccountRequest): Promise<SaveResult>;
  saveCategory(request: SaveCategoryRequest): Promise<SaveResult>;
  savePortfolio(request: SavePortfolioRequest): Promise<SaveResult>;
  saveInstrument(request: SaveInstrumentRequest): Promise<SaveResult>;
  saveFxRevision(request: SaveFxRevisionRequest): Promise<SaveResult>;
  savePriceRevision(request: SavePriceRevisionRequest): Promise<SaveResult>;
}
