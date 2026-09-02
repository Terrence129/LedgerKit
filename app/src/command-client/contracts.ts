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

export type FxOverrideRequest = { currency: string; value: string; reason: string };
export type CashEventRequest = {
  effectiveDate: string;
  sequence: number;
  eventType: "OpeningBalance" | "Income" | "Expense" | "Adjustment" | "Transfer" | "CurrencyExchange";
  accountId?: string; fromAccountId?: string; toAccountId?: string;
  amount?: string; toAmount?: string; categoryId?: string;
  semanticRole?: "normal" | "refund" | "reimbursement";
  merchant?: string; note?: string; feeAccountId?: string; feeAmount?: string;
  cutoverDate?: string; migrationPolicy?: "full_history" | "explicit_cutover";
  fxOverrides?: FxOverrideRequest[];
  currencyPrecisionConfirmed?: boolean;
};
export type FxResolutionResult = {
  purpose: string;
  currency: string;
  baseCurrency: string;
  targetDate: string;
  automaticCandidateRevisionId: string | null;
  overrideValue: string | null;
  overrideReason: string | null;
  finalRate: string | null;
  calculationVersion: string;
  valuationState: "valued" | "unvalued";
};
export type PostingPreview = { accountId: string; quantityDelta: string; currency: string; baseValue: string | null; baseCurrency: string; role: string };
export type EventPreview = { eventType: string; effectiveDate: string; sequence: number; postings: PostingPreview[]; fxResolutions: FxResolutionResult[]; qualityIssueCodes: string[] };
export type PostedEvent = { eventId: string; eventWatermark: number; revision: number; preview: EventPreview };
export type DrilldownContext = { start_date: string; end_date: string; event_watermark: number; calculation_version: string; expense_policy_version: string; bucket_id?: string; semantic_role?: string; member_rank_gt?: number; valuation_state: "valued" | "unvalued" | "all" };
export type ExpenseBucket = { bucket_id: string; bucket_kind: "category" | "system"; label: string; archived: boolean; amount: string; distinct_event_count: number; drilldown_context: DrilldownContext };
export type ExpenseTopItem = { bucket_id: string; label: string; amount: string; distinct_event_count: number; drilldown_context: DrilldownContext };
export type ExpenseMeasure = { amount: string; distinct_event_count: number; unvalued_count: number; drilldown_context: DrilldownContext };
export type ExpenseAnalysis = {
  contract: "expense-analysis-query-result/v1";
  query: { start_date: string; end_date: string; base_currency: string };
  summary: { label: string; total_expense: string | null; valued_subtotal: string; global_distinct_event_count: number; largest_category: { bucket_id: string; amount: string } | null };
  buckets: ExpenseBucket[];
  top10: { items: ExpenseTopItem[]; other: ExpenseTopItem | null };
  refunds: { refund: ExpenseMeasure; reimbursement: ExpenseMeasure };
  unvalued: { expense_count: number; drilldown_context: DrilldownContext };
  watermarks: { event: number; master_data: number };
  versions: { calculation: string; expense_policy: string; bucket_policy: string; refund_policy: string };
  canonicalization: string;
  canonical_hash: string;
};
export type ActivityItem = { eventId: string; eventOrder: number; eventType: string; effectiveDate: string; amount: string; currency: string; categoryId: string | null; semanticRole: string; valuationState: "valued" | "unvalued" };
export type ActivityPage = { items: ActivityItem[]; nextCursor: number | null };

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
  previewEvent(request: CashEventRequest): Promise<EventPreview>;
  postEvent(request: CashEventRequest): Promise<PostedEvent>;
  reviseEvent(request: { targetEventId: string; reason: string; replacement: CashEventRequest }): Promise<PostedEvent>;
  reverseEvent(request: { targetEventId: string; reason: string; effectiveDate: string; sequence: number }): Promise<PostedEvent>;
  getExpenseAnalysis(request: { startDate: string; endDate: string; eventWatermark?: number }): Promise<ExpenseAnalysis>;
  getActivity(request: { context: DrilldownContext; cursor?: number; limit: number }): Promise<ActivityPage>;
}
