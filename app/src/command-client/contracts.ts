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
export type PostingPreview = { accountId: string | null; quantityDelta: string; currency: string; baseValue: string | null; baseCurrency: string; role: string };
export type EventPreview = { eventType: string; effectiveDate: string; sequence: number; categoryId: string | null; semanticRole: string; feeAccountId: string | null; feeAmount: string | null; postings: PostingPreview[]; fxResolutions: FxResolutionResult[]; qualityIssueCodes: string[] };
export type PostedEvent = { eventId: string; eventWatermark: number; revision: number; preview: EventPreview };
export type DrilldownContext = { start_date: string; end_date: string; event_watermark: number; calculation_version: string; expense_policy_version: string; bucket_id?: string; semantic_role?: string; member_rank_gt?: number; valuation_state: "valued" | "unvalued" | "all" };
export type ExpenseBucket = { bucket_id: string; bucket_kind: "category" | "system"; label: string; archived: boolean; amount: string; share_basis_points: number; distinct_event_count: number; drilldown_context: DrilldownContext };
export type ExpenseTopItem = { bucket_id: string; label: string; amount: string; share_basis_points: number; distinct_event_count: number; drilldown_context: DrilldownContext };
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
export type ActivityPosting = { postingKind: string; accountId: string | null; portfolioId?: string | null; instrumentId?: string | null; quantityDelta: string; currency: string; baseValue: string | null; baseCurrency: string };
export type ActivityFxResolution = {
  purpose: string; currency: string; baseCurrency: string; targetDate: string;
  automaticCandidateRevisionId: string | null; overrideValue: string | null;
  overrideReason: string | null; finalRate: string; calculationVersion: string;
};
export type ActivityEventContent = {
  accountId: string | null; fromAccountId: string | null; toAccountId: string | null;
  amount: string | null; toAmount: string | null; categoryId: string | null;
  semanticRole: string; merchant: string | null; note: string | null;
  feeAccountId: string | null; feeAmount: string | null;
  cutoverDate: string | null; migrationPolicy: string | null;
  portfolioId?: string | null; instrumentId?: string | null; settlementAccountId?: string | null;
  tradeType?: "BUY" | "SELL" | null; quantity?: string | null; unitPrice?: string | null;
  tradeFee?: string | null; grossCashAmount?: string | null; withholdingTax?: string | null;
  investmentFeeAmount?: string | null; investmentExpenseAmount?: string | null;
  feeScope?: "instrument" | "portfolio" | null; settlementOverrideReason?: string | null;
  carryingCost?: string | null; realizedTradePnl?: string | null; netDividend?: string | null;
  independentExpense?: string | null; costCurrency?: string | null;
};
export type ActivityItem = {
  eventId: string; eventOrder: number; eventType: string; effectiveDate: string;
  sequence: number; revision: number; content: ActivityEventContent;
  postings: ActivityPosting[]; reversalPreview: ActivityPosting[];
  fxResolutions: ActivityFxResolution[];
  relations: { supersedesEventId: string | null; reversesEventId: string | null; supersededByEventId: string | null; reversedByEventId: string | null };
  audit: { action: string; occurredAtUtc: string; reason: string | null };
};
export type ActivityPage = { items: ActivityItem[]; nextCursor: number | null };
export type ActivityRequest = {
  startDate: string; endDate: string; context?: DrilldownContext;
  eventType?: CashEventRequest["eventType"] | "SecurityBuy" | "SecuritySell" | "Dividend" | "InvestmentExpense" | "OpeningPosition" | "OpeningPerformance" | "Reversal";
  accountId?: string; categoryId?: string; search?: string;
  cursor?: number; limit: number;
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

export type ImportIssue = { code: string; severity: "blocker" | "warning"; sheet: string; row: number; field: string };
export type ImportMapping = { entityType: string; legacyId: string; targetId: string; migrationPolicy: string | null };
export type ImportPosting = { accountId: string; portfolioId: string | null; instrumentId: string | null; quantityDelta: string; currency: string; baseValue: string | null; role: string };
export type ImportProposedEvent = { sourceSheet: string; sourceRow: number; eventType: string; effectiveDate: string; sequence: number; postings: ImportPosting[] };
export type ImportBalance = { accountId: string; currency: string; sourceBalance: string; proposedBalance: string; difference: string };
export type ImportMetric = { scope: string; entityId: string; metric: string; sourceValue: string; proposedValue: string; difference: string; asOfDate: string | null };
export type ImportDifference = { scope: string; key: string; excelValue: string; applicationValue: string; difference: string; explanation: string };
export type ImportAnalysis = {
  batchId: string; sourceSha256: string; templateVersion: string; importerVersion: string;
  targetSchemaVersion: number; status: "ready" | "needs-review" | "committed";
  rowCount: number; validRowCount: number; blockerCount: number; warningCount: number;
  issues: ImportIssue[]; mappings: ImportMapping[]; proposedEvents: ImportProposedEvent[];
  reconciliation: { balances: ImportBalance[]; metrics: ImportMetric[]; differenceBridge: string[]; differenceItems: ImportDifference[]; canonicalResultSha256: string; balanced: boolean };
  canCommit: boolean; reusedStaging: boolean;
};
export type ImportCommitResult = {
  batchId: string; sourceSha256: string; status: "committed"; ledgerId: string;
  eventWatermark: number; canonicalPostingSha256: string; alreadyCommitted: boolean;
};

export type InvestmentEventRequest = {
  effectiveDate: string; sequence: number;
  eventType: "SecurityBuy" | "SecuritySell" | "Dividend" | "InvestmentExpense" | "OpeningPosition" | "OpeningPerformance";
  portfolioId: string; instrumentId?: string; settlementAccountId: string;
  quantity?: string; unitPrice?: string; tradeFee?: string;
  grossCashAmount?: string; withholdingTax?: string; feeAmount?: string;
  amount?: string; feeScope?: "instrument" | "portfolio";
  carryingCost?: string; realizedTradePnl?: string; netDividend?: string;
  independentExpense?: string; costCurrency?: string; cutoverDate?: string;
  migrationPolicy?: "full_history" | "explicit_cutover";
  settlementOverrideReason?: string; fxOverrides?: FxOverrideRequest[];
};
export type InvestmentPostingPreview = {
  postingKind: string; accountId: string | null; portfolioId: string;
  instrumentId: string | null; quantityDelta: string; currency: string;
  baseValue: string | null; baseCurrency: string;
};
export type InvestmentEventPreview = {
  eventType: InvestmentEventRequest["eventType"]; effectiveDate: string; sequence: number;
  postings: InvestmentPostingPreview[]; quantityAfter: string | null;
  carryingCostAfter: string | null; averageCostAfter: string | null;
  realizedTradePnlAfter: string | null; qualityIssueCodes: string[];
};
export type PostedInvestmentEvent = { eventId: string; eventWatermark: number; revision: number; preview: InvestmentEventPreview };
export type HoldingPosition = {
  portfolioId: string; portfolioName: string; instrumentId: string; instrumentName: string;
  currency: string; asOfDate: string; quantity: string; carryingCost: string;
  averageCost: string | null; realizedTradePnl: string; netDividend: string;
  independentExpense: string; marketPrice: string | null; priceRevisionId: string | null;
  priceDate: string | null; priceAgeDays: number | null; marketValue: string | null;
  fxRate: string | null; fxRevisionId: string | null; baseMarketValue: string | null;
  unrealizedPnl: string | null; totalReturn: string | null;
  valuationState: "valued" | "unvalued"; unvaluedReason: string | null; warningCodes: string[];
};
export type InvestmentWorkspace = {
  asOfDate: string; baseCurrency: string; holdings: HoldingPosition[];
  portfolioExpenses: { portfolioId: string; portfolioName: string; amount: string; currency: string }[];
  eventWatermark: number; projectionVersion: string; calculationVersion: string;
};

export type CompositionItem = { id: string; label: string; baseValue: string };
export type Overview = {
  contract: "ledgerkit-overview-v1"; valuationDate: string; mtdStartDate: string; mtdEndDate: string;
  baseCurrency: string; valuedNetAssets: string; valuedCash: string; valuedHoldings: string;
  mtdExpense: string; mtdUnvaluedExpenseCount: number;
  composition: { institutions: CompositionItem[]; currencies: CompositionItem[]; cashAccounts: CompositionItem[]; holdings: CompositionItem[] };
  unvaluedAssets: { assetType: string; entityId: string; nativeValue: string; nativeCurrency: string; reason: string }[];
  anomalyCodes: string[]; watermarks: { event: number; marketData: number };
  calculationVersion: string; snapshotVersion: string;
};
export type FixContext = { operation: string; field: string; entityType: string; entityId: string; asOfDate: string };
export type DataQualityIssue = {
  issueId: string; code: string; severity: "blocker" | "warning"; status: "open";
  context: FixContext;
};
export type DataQualityReport = {
  contract: "ledgerkit-data-quality-v1"; asOfDate: string; blockerCount: number; warningCount: number;
  issues: DataQualityIssue[]; eventWatermark: number; calculationVersion: string;
};

export type CreateBackupRequest = { password: string; configureExternalTarget: boolean };
export type RestoreBackupRequest = { password: string };
export type ExportFormat = "xlsx" | "csv" | "reconciliation" | "diagnostics";
export type BackupResult = {
  fileName: string; backupId: string; createdAtUtc: string; schemaVersion: number;
  verified: true; protectionState: LedgerStatus["backupProtectionState"];
};
export type RestoreResult = {
  backupId: string; ledgerId: string; schemaVersion: number; eventWatermark: number;
  settingsLocale: SupportedLocale; preRestoreBackupVerified: boolean;
};
export type BackupStatus = {
  protectionState: LedgerStatus["backupProtectionState"];
  externalTargetConfigured: boolean; externalTargetLabel: string | null;
  lastAttemptAtUtc: string | null; lastSuccessAtUtc: string | null;
  lastVerifiedSchemaVersion: number | null; lastErrorCode: string | null;
  deviceLossProtected: boolean; recoverySecretState: "locked" | "unlocked-for-session";
  dailyRetention: number; weeklyRetention: number;
};
export type ExportResult = { fileName: string; format: ExportFormat; rowCount: number; contentSha256: string };

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
  getActivity(request: ActivityRequest): Promise<ActivityPage>;
  analyzeImport(): Promise<ImportAnalysis>;
  commitImport(request: { batchId: string; confirmed: boolean }): Promise<ImportCommitResult>;
  previewInvestmentEvent(request: InvestmentEventRequest): Promise<InvestmentEventPreview>;
  postInvestmentEvent(request: InvestmentEventRequest): Promise<PostedInvestmentEvent>;
  reviseInvestmentEvent(request: { targetEventId: string; reason: string; replacement: InvestmentEventRequest }): Promise<PostedInvestmentEvent>;
  getInvestmentWorkspace(request: { asOfDate: string }): Promise<InvestmentWorkspace>;
  getOverview(request: { asOfDate: string }): Promise<Overview>;
  getDataQuality(request: { asOfDate: string }): Promise<DataQualityReport>;
  createBackup(request: CreateBackupRequest): Promise<BackupResult>;
  restoreBackup(request: RestoreBackupRequest): Promise<RestoreResult>;
  getBackupStatus(): Promise<BackupStatus>;
  exportData(request: { format: ExportFormat }): Promise<ExportResult>;
}
