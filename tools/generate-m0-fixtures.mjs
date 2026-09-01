import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";

const CALCULATION_VERSION = "ledger-calculation-v1";
const PROJECTION_VERSION = "projection-v1";
const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");
const fixturesRoot = join(repositoryRoot, "fixtures", "sanitized");
const checkOnly = process.argv.includes("--check");

function compareUnicodeScalar(left, right) {
  const a = Array.from(left, (character) => character.codePointAt(0));
  const b = Array.from(right, (character) => character.codePointAt(0));
  for (let index = 0; index < Math.min(a.length, b.length); index += 1) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return a.length - b.length;
}

function canonicalize(value) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new Error(`Canonical JSON only permits non-negative safe integers: ${value}`);
    }
    return String(value);
  }
  if (typeof value === "string") return JSON.stringify(value.normalize("NFC"));
  if (Array.isArray(value)) return `[${value.map(canonicalize).join(",")}]`;
  if (typeof value === "object") {
    return `{${Object.keys(value)
      .sort(compareUnicodeScalar)
      .map((key) => `${JSON.stringify(key.normalize("NFC"))}:${canonicalize(value[key])}`)
      .join(",")}}`;
  }
  throw new Error(`Unsupported canonical JSON value: ${typeof value}`);
}

function sha256(value) {
  return `sha256:${createHash("sha256").update(canonicalize(value), "utf8").digest("hex")}`;
}

function withHash(value) {
  const copy = structuredClone(value);
  delete copy.canonical_hash;
  return { ...copy, canonical_hash: sha256(copy) };
}

function decimalParts(value) {
  const match = /^(-?)(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match) throw new Error(`Invalid decimal in generator: ${value}`);
  const scale = (match[3] ?? "").length;
  const coefficient = BigInt(`${match[1]}${match[2]}${match[3] ?? ""}`);
  return { coefficient, scale };
}

function compareDecimal(left, right) {
  const a = decimalParts(left);
  const b = decimalParts(right);
  const scale = Math.max(a.scale, b.scale);
  const ac = a.coefficient * 10n ** BigInt(scale - a.scale);
  const bc = b.coefficient * 10n ** BigInt(scale - b.scale);
  return ac < bc ? -1 : ac > bc ? 1 : 0;
}

function sumDecimals(values) {
  const parsed = values.map(decimalParts);
  const scale = Math.max(0, ...parsed.map((value) => value.scale));
  const total = parsed.reduce(
    (sum, value) => sum + value.coefficient * 10n ** BigInt(scale - value.scale),
    0n,
  );
  const negative = total < 0n;
  const digits = (negative ? -total : total).toString().padStart(scale + 1, "0");
  if (scale === 0) return `${negative ? "-" : ""}${digits}`;
  const integer = digits.slice(0, -scale);
  const fraction = digits.slice(-scale).replace(/0+$/, "");
  return `${negative ? "-" : ""}${integer}${fraction ? `.${fraction}` : ""}`;
}

const command = (commandType, data) => ({ commandType, data });

function event(caseNumber, kind, index, eventType, effectiveDate, sequence, detail, extra = {}) {
  return {
    event_id: `evt-${String(caseNumber).padStart(2, "0")}-${kind}-${String(index).padStart(2, "0")}`,
    event_type: eventType,
    effective_date: effectiveDate,
    sequence,
    status: "posted",
    detail,
    calculation_version: CALCULATION_VERSION,
    ...extra,
  };
}

function posting(eventId, index, postingKind, data = {}) {
  return {
    posting_id: `post-${eventId}-${String(index).padStart(2, "0")}`,
    event_id: eventId,
    posting_kind: postingKind,
    calculation_version: CALCULATION_VERSION,
    ...data,
  };
}

const diagnostic = (code, field) => ({
  code,
  field,
  message_key: `ledgerkit.${code.toLowerCase()}`,
});

function scenario(kind, commands, events = [], postings = [], state = {}, options = {}) {
  return {
    scenarioId: kind,
    kind,
    commands,
    events,
    postings,
    state,
    errors: options.errors ?? [],
    warnings: options.warnings ?? [],
    watermark: options.watermark ?? events.filter((item) => item.status === "posted").length,
  };
}

function makeExpenseResult({
  startDate = "2026-02-01",
  endDate = "2026-02-28",
  buckets = [],
  complete = true,
  globalCount = 0,
  unvaluedExpenseCount = 0,
  refundAmount = "0",
  refundCount = 0,
  unvaluedRefundCount = 0,
  reimbursementAmount = "0",
  reimbursementCount = 0,
  unvaluedReimbursementCount = 0,
  eventWatermark = 0,
  masterDataWatermark = 1,
}) {
  const contextBase = {
    start_date: startDate,
    end_date: endDate,
    event_watermark: eventWatermark,
    calculation_version: CALCULATION_VERSION,
    expense_policy_version: "expense-policy-v1",
  };
  const orderedBuckets = buckets
    .map((bucket) => ({
      bucket_id: bucket.bucket_id,
      bucket_kind: bucket.bucket_kind ?? "category",
      label: bucket.label,
      archived: bucket.archived ?? false,
      amount: bucket.amount,
      distinct_event_count: bucket.distinct_event_count,
      drilldown_context: {
        ...contextBase,
        bucket_id: bucket.bucket_id,
        valuation_state: "valued",
      },
    }))
    .sort((left, right) => compareDecimal(right.amount, left.amount) || compareUnicodeScalar(left.bucket_id, right.bucket_id));
  const positive = orderedBuckets.filter((bucket) => compareDecimal(bucket.amount, "0") > 0);
  const topItems = positive.slice(0, 10).map((bucket) => ({
    bucket_id: bucket.bucket_id,
    label: bucket.label,
    amount: bucket.amount,
    distinct_event_count: bucket.distinct_event_count,
    drilldown_context: bucket.drilldown_context,
  }));
  let other = null;
  if (positive.length > 10) {
    const remaining = positive.slice(10);
    other = {
      bucket_id: "system:top10-other",
      label: "Other categories",
      amount: sumDecimals(remaining.map((bucket) => bucket.amount)),
      distinct_event_count: remaining.reduce((sum, bucket) => sum + bucket.distinct_event_count, 0),
      drilldown_context: {
        ...contextBase,
        bucket_id: "system:top10-other",
        member_rank_gt: 10,
        valuation_state: "valued",
      },
    };
  }
  const valuedSubtotal = sumDecimals(orderedBuckets.map((bucket) => bucket.amount));
  const largestCategory = positive.find((bucket) => bucket.bucket_kind === "category") ?? null;
  return withHash({
    contract: "expense-analysis-query-result/v1",
    query: { start_date: startDate, end_date: endDate, base_currency: "CNY" },
    summary: {
      label: complete ? "Total expense" : "Valued expense subtotal",
      total_expense: complete ? valuedSubtotal : null,
      valued_subtotal: valuedSubtotal,
      global_distinct_event_count: globalCount,
      largest_category: largestCategory
        ? { bucket_id: largestCategory.bucket_id, amount: largestCategory.amount }
        : null,
    },
    buckets: orderedBuckets,
    top10: { items: topItems, other },
    refunds: {
      refund: {
        amount: refundAmount,
        distinct_event_count: refundCount,
        unvalued_count: unvaluedRefundCount,
        drilldown_context: { ...contextBase, semantic_role: "refund", valuation_state: "all" },
      },
      reimbursement: {
        amount: reimbursementAmount,
        distinct_event_count: reimbursementCount,
        unvalued_count: unvaluedReimbursementCount,
        drilldown_context: { ...contextBase, semantic_role: "reimbursement", valuation_state: "all" },
      },
    },
    unvalued: {
      expense_count: unvaluedExpenseCount,
      drilldown_context: { ...contextBase, semantic_role: "expense", valuation_state: "unvalued" },
    },
    watermarks: { event: eventWatermark, master_data: masterDataWatermark },
    versions: {
      calculation: CALCULATION_VERSION,
      expense_policy: "expense-policy-v1",
      bucket_policy: "expense-bucket-policy-v1",
      refund_policy: "refund-policy-v1",
    },
    canonicalization: "ledgerkit-canonical-json-v1",
  });
}

const cases = [];
const addCase = (number, slug, title, rules, adrs, scenarios, options = {}) =>
  cases.push({ number, slug, title, rules, adrs, scenarios, ...options });

{
  const income = event(1, "normal", 1, "Income", "2026-01-05", 1, { account_id: "cash-cny-1", amount: "100.00", currency: "CNY", category_id: "cat-salary" });
  const expense = event(1, "normal", 2, "Expense", "2026-01-06", 1, { account_id: "cash-cny-1", amount: "25.50", currency: "CNY", category_id: "cat-food" });
  const boundary = event(1, "boundary", 1, "Expense", "2026-01-31", 1, { account_id: "cash-cny-1", amount: "0.00000001", currency: "CNY", category_id: "cat-rounding", currency_precision_confirmed: true });
  addCase(1, "cny-income-expense", "CNY income, expense, Decimal limits and signs", ["NUM-001", "NUM-002", "NUM-003", "NUM-004", "NUM-005", "NUM-006", "DATE-001", "CASH-001", "CASH-002"], ["ADR-0003", "ADR-0004"], [
    scenario("normal", [command("post_event", income.detail), command("post_event", expense.detail)], [income, expense], [
      posting(income.event_id, 1, "cash", { account_id: "cash-cny-1", quantity_delta: "100.00", currency: "CNY", base_value: "100.00", base_currency: "CNY" }),
      posting(expense.event_id, 1, "cash", { account_id: "cash-cny-1", quantity_delta: "-25.50", currency: "CNY", base_value: "-25.50", base_currency: "CNY" }),
    ], { account_balances: [{ account_id: "cash-cny-1", balance: "74.50", currency: "CNY" }], period_income: "100.00", period_expense: "25.50" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "cash", { account_id: "cash-cny-1", quantity_delta: "-0.00000001", currency: "CNY", base_value: "-0.00000001", base_currency: "CNY" })], { account_balances: [{ account_id: "cash-cny-1", balance: "-0.00000001", currency: "CNY" }], display_balance: "0.00" }, { warnings: [diagnostic("CURRENCY_PRECISION_CONFIRMATION_REQUIRED", "amount")] }),
    scenario("failure", [command("post_event", { account_id: "cash-cny-1", amount: "1.000000000", currency: "CNY" })], [], [], { unchanged: true }, { errors: [diagnostic("DECIMAL_SCALE_EXCEEDED", "amount")] }),
  ]);
}

{
  const normal = event(2, "normal", 1, "Expense", "2026-01-10", 1, { account_id: "cash-usd-1", amount: "10.00", currency: "USD", category_id: "cat-travel", fx_resolution: { target_date: "2026-01-10", selected_revision_id: "fx-usd-20260101-r1", final_rate: "7.10", base_value: "71.0000" } });
  const boundary = event(2, "boundary", 1, "Income", "2026-01-15", 1, { account_id: "cash-usd-1", amount: "1.00", currency: "USD", category_id: "cat-other-income", fx_resolution: { target_date: "2026-01-15", selected_revision_id: "fx-usd-20260115-r1", final_rate: "7.20", base_value: "7.2000" } });
  addCase(2, "fx-as-of-order-independent", "Foreign-currency as-of selection is row-order independent", ["DATE-003", "FX-001", "FX-002", "FX-004"], ["ADR-0004", "ADR-0012"], [
    scenario("normal", [command("post_event", { ...normal.detail, fx_revisions_in_physical_order: [{ revision_id: "fx-usd-20260115-r1", date: "2026-01-15", rate_to_base: "7.20", active: true }, { revision_id: "fx-usd-20260101-r1", date: "2026-01-01", rate_to_base: "7.10", active: true }] })], [normal], [posting(normal.event_id, 1, "cash", { account_id: "cash-usd-1", quantity_delta: "-10.00", currency: "USD", base_value: "-71.0000", base_currency: "CNY" })], { selected_fx_revision_id: "fx-usd-20260101-r1", valued_expense: "71.0000" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "cash", { account_id: "cash-usd-1", quantity_delta: "1.00", currency: "USD", base_value: "7.2000", base_currency: "CNY" })], { selected_fx_revision_id: "fx-usd-20260115-r1", valued_income: "7.2000" }),
    scenario("failure", [command("post_event", { account_id: "cash-usd-1", amount: "2.00", currency: "USD", effective_date: "2025-12-31", fx_revisions: [{ date: "2026-01-01", rate_to_base: "7.10", active: true }] })], [], [], { unvalued_count: 1 }, { errors: [diagnostic("FX_MISSING_AS_OF", "fx_resolution")] }),
  ]);
}

{
  const normal = event(3, "normal", 1, "Expense", "2026-02-01", 1, { account_id: "cash-usd-1", amount: "5.00", currency: "USD", fx_resolution: { target_date: "2026-02-01", automatic_candidate_revision_id: "fx-usd-r2", override_value: null, override_reason: null, final_rate: "7.25", calculation_version: CALCULATION_VERSION } });
  const boundary = event(3, "boundary", 1, "Expense", "2026-02-01", 2, { account_id: "cash-usd-1", amount: "5.00", currency: "USD", fx_resolution: { target_date: "2026-02-01", automatic_candidate_revision_id: "fx-usd-r2", override_value: "7.30", override_reason: "Synthetic bank receipt rate", final_rate: "7.30", calculation_version: CALCULATION_VERSION } });
  addCase(3, "fx-revision-missing-override", "Active FX revisions, missing FX and audited override", ["FX-002", "FX-003", "FX-005"], ["ADR-0004", "ADR-0012"], [
    scenario("normal", [command("post_event", { ...normal.detail, same_day_revisions: [{ revision_id: "fx-usd-r1", rate_to_base: "7.20", active: false }, { revision_id: "fx-usd-r2", rate_to_base: "7.25", active: true }] })], [normal], [posting(normal.event_id, 1, "cash", { quantity_delta: "-5.00", currency: "USD", base_value: "-36.2500", base_currency: "CNY" })], { selected_revision_id: "fx-usd-r2", valued_expense: "36.2500" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "cash", { quantity_delta: "-5.00", currency: "USD", base_value: "-36.5000", base_currency: "CNY" })], { automatic_candidate_rate: "7.25", final_rate: "7.30", valued_expense: "36.5000" }),
    scenario("failure", [command("post_event", { account_id: "cash-eur-1", amount: "5.00", currency: "EUR", effective_date: "2026-02-01", override_value: "7.80", override_reason: "" })], [], [], { unvalued_count: 1 }, { errors: [diagnostic("FX_OVERRIDE_REASON_REQUIRED", "override_reason")] }),
  ]);
}

{
  const normal = event(4, "normal", 1, "Transfer", "2026-02-02", 1, { from_account_id: "cash-cny-1", to_account_id: "cash-cny-2", amount: "50.00", currency: "CNY" });
  const boundary = event(4, "boundary", 1, "Transfer", "2026-02-02", 2, { from_account_id: "cash-cny-1", to_account_id: "cash-cny-2", amount: "0.00000001", currency: "CNY", currency_precision_confirmed: true });
  addCase(4, "same-currency-transfer", "Same-currency transfer conserves cash", ["CASH-001", "CASH-003"], ["ADR-0003", "ADR-0004"], [
    scenario("normal", [command("post_event", normal.detail)], [normal], [posting(normal.event_id, 1, "transfer-out", { account_id: "cash-cny-1", quantity_delta: "-50.00", currency: "CNY", base_value: "-50.00", base_currency: "CNY" }), posting(normal.event_id, 2, "transfer-in", { account_id: "cash-cny-2", quantity_delta: "50.00", currency: "CNY", base_value: "50.00", base_currency: "CNY" })], { net_cash_change: "0", expense: "0", income: "0" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "transfer-out", { account_id: "cash-cny-1", quantity_delta: "-0.00000001", currency: "CNY", base_value: "-0.00000001", base_currency: "CNY" }), posting(boundary.event_id, 2, "transfer-in", { account_id: "cash-cny-2", quantity_delta: "0.00000001", currency: "CNY", base_value: "0.00000001", base_currency: "CNY" })], { net_cash_change: "0" }, { warnings: [diagnostic("CURRENCY_PRECISION_CONFIRMATION_REQUIRED", "amount")] }),
    scenario("failure", [command("post_event", { from_account_id: "cash-cny-1", to_account_id: "cash-usd-1", amount: "50.00", currency: "CNY" })], [], [], { unchanged: true }, { errors: [diagnostic("TRANSFER_CURRENCY_MISMATCH", "to_account_id")] }),
  ]);
}

{
  const normal = event(5, "normal", 1, "CurrencyExchange", "2026-02-03", 1, { from_account_id: "cash-cny-1", to_account_id: "cash-usd-1", from_amount: "710.00", to_amount: "100.00", fee_account_id: "cash-cny-1", fee_amount: "2.00", from_currency: "CNY", to_currency: "USD" });
  const boundary = event(5, "boundary", 1, "CurrencyExchange", "2026-02-03", 2, { from_account_id: "cash-usd-1", to_account_id: "cash-cny-1", from_amount: "1.00000000", to_amount: "7.10000000", fee_account_id: null, fee_amount: "0", from_currency: "USD", to_currency: "CNY" });
  addCase(5, "cross-currency-exchange-fee", "Cross-currency exchange excludes principal and buckets fee", ["CASH-004", "FX-004", "EXP-001", "EXP-002", "EXP-010"], ["ADR-0003", "ADR-0004", "ADR-0014"], [
    scenario("normal", [command("post_event", normal.detail)], [normal], [posting(normal.event_id, 1, "exchange-out", { account_id: "cash-cny-1", quantity_delta: "-710.00", currency: "CNY", base_value: "-710.00", base_currency: "CNY" }), posting(normal.event_id, 2, "exchange-in", { account_id: "cash-usd-1", quantity_delta: "100.00", currency: "USD", base_value: "710.00", base_currency: "CNY" }), posting(normal.event_id, 3, "fx-fee", { account_id: "cash-cny-1", quantity_delta: "-2.00", currency: "CNY", base_value: "-2.00", base_currency: "CNY" })], { principal_expense: "0", fee_expense: "2.00", expense_bucket_id: "system:fx-fee" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "exchange-out", { quantity_delta: "-1.00000000", currency: "USD", base_value: "-7.10000000", base_currency: "CNY" }), posting(boundary.event_id, 2, "exchange-in", { quantity_delta: "7.10000000", currency: "CNY", base_value: "7.10000000", base_currency: "CNY" })], { principal_expense: "0", fee_expense: "0" }),
    scenario("failure", [command("post_event", { from_account_id: "cash-cny-1", to_account_id: "cash-cny-2", from_amount: "10.00", to_amount: "10.00", from_currency: "CNY", to_currency: "CNY" })], [], [], { unchanged: true }, { errors: [diagnostic("EXCHANGE_REQUIRES_DIFFERENT_CURRENCIES", "to_account_id")] }),
  ]);
}

{
  const normal = event(6, "normal", 1, "SecurityBuy", "2026-02-04", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", settlement_account_id: "cash-usd-1", quantity: "10.000000000000", unit_price: "12.340000000000", trade_fee: "1.60", currency: "USD" });
  const boundary = event(6, "boundary", 1, "SecurityBuy", "2026-02-04", 2, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", settlement_account_id: "cash-usd-1", quantity: "0.000000000001", unit_price: "1000000.000000000000", trade_fee: "0", currency: "USD" });
  addCase(6, "security-buy", "Security buy reduces settlement cash and capitalizes fee", ["INV-002", "INV-003", "INV-004"], ["ADR-0004", "ADR-0005"], [
    scenario("normal", [command("post_event", normal.detail)], [normal], [posting(normal.event_id, 1, "settlement-cash", { account_id: "cash-usd-1", quantity_delta: "-125.00", currency: "USD", base_value: "-887.5000", base_currency: "CNY" }), posting(normal.event_id, 2, "security-quantity", { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", quantity_delta: "10.000000000000" }), posting(normal.event_id, 3, "holding-cost", { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", cost_delta: "125.00", currency: "USD" })], { quantity: "10.000000000000", carrying_cost: "125.00", average_cost: "12.500000000000000000" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "settlement-cash", { quantity_delta: "-0.000001000000000000", currency: "USD", base_value: "-0.000007100000000000", base_currency: "CNY" }), posting(boundary.event_id, 2, "security-quantity", { quantity_delta: "0.000000000001" }), posting(boundary.event_id, 3, "holding-cost", { cost_delta: "0.000001000000000000", currency: "USD" })], { quantity: "0.000000000001", carrying_cost: "0.000001000000000000", average_cost: "1000000.000000000000000000" }),
    scenario("failure", [command("post_event", { ...normal.detail, settlement_account_id: "cash-cny-1" })], [], [], { unchanged: true }, { errors: [diagnostic("TRADE_CURRENCY_MISMATCH", "settlement_account_id")] }),
  ]);
}

{
  const normal = event(7, "normal", 1, "SecuritySell", "2026-02-05", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", quantity: "4.000000000000", unit_price: "15.000000000000", trade_fee: "1.00", currency: "USD", pre_quantity: "10.000000000000", pre_carrying_cost: "125.00" });
  const boundary = event(7, "boundary", 1, "SecuritySell", "2026-02-05", 2, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", quantity: "0.000000000001", unit_price: "15.000000000000", trade_fee: "0", currency: "USD", pre_quantity: "6.000000000001", pre_carrying_cost: "75.000000000012500000" });
  addCase(7, "partial-security-sale", "Partial sale locks moving-average cost and realized PnL", ["INV-004", "INV-006", "INV-007"], ["ADR-0004", "ADR-0005"], [
    scenario("normal", [command("post_event", normal.detail)], [normal], [posting(normal.event_id, 1, "settlement-cash", { quantity_delta: "59.00", currency: "USD", base_value: "418.9000", base_currency: "CNY" }), posting(normal.event_id, 2, "security-quantity", { quantity_delta: "-4.000000000000" }), posting(normal.event_id, 3, "holding-cost", { cost_delta: "-50.000000000000000000", currency: "USD" }), posting(normal.event_id, 4, "realized-pnl", { realized_pnl: "9.000000000000000000", currency: "USD" })], { quantity: "6.000000000000", carrying_cost: "75.000000000000000000", average_cost: "12.500000000000000000", realized_pnl: "9.000000000000000000" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "settlement-cash", { quantity_delta: "0.000000000015", currency: "USD", base_value: "0.000000000106500000", base_currency: "CNY" }), posting(boundary.event_id, 2, "security-quantity", { quantity_delta: "-0.000000000001" }), posting(boundary.event_id, 3, "holding-cost", { cost_delta: "-0.000000000012500000", currency: "USD" }), posting(boundary.event_id, 4, "realized-pnl", { realized_pnl: "0.000000000002500000", currency: "USD" })], { quantity: "6.000000000000", carrying_cost: "75.000000000000000000", realized_pnl: "0.000000000002500000" }),
    scenario("failure", [command("post_event", { ...normal.detail, quantity: "10.000000000001" })], [], [], { unchanged: true }, { errors: [diagnostic("NEGATIVE_HOLDING_NOT_ALLOWED", "quantity")] }),
  ]);
}

{
  const close1 = event(8, "normal", 1, "SecuritySell", "2026-02-06", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", quantity: "2.000000000000", unit_price: "13.000000000000", trade_fee: "0", pre_quantity: "3.000000000000", pre_carrying_cost: "30.000000000000000001" });
  const close2 = event(8, "normal", 2, "SecuritySell", "2026-02-07", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", quantity: "1.000000000000", unit_price: "14.000000000000", trade_fee: "0", pre_quantity: "1.000000000000", pre_carrying_cost: "10.000000000000000001" });
  const reopen = event(8, "normal", 3, "SecurityBuy", "2026-02-08", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", quantity: "1.000000000000", unit_price: "20.000000000000", trade_fee: "0" });
  const boundary = event(8, "boundary", 1, "SecuritySell", "2026-02-09", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-beta", quantity: "0.000000000001", unit_price: "1.000000000000", trade_fee: "0", pre_quantity: "0.000000000001", pre_carrying_cost: "0.000000000000000001" });
  addCase(8, "close-and-reopen-position", "Full close zeros cost; reopen preserves historical PnL", ["INV-007", "INV-008"], ["ADR-0004", "ADR-0005"], [
    scenario("normal", [command("post_event", close1.detail), command("post_event", close2.detail), command("post_event", reopen.detail)], [close1, close2, reopen], [posting(close1.event_id, 1, "holding-cost", { cost_delta: "-20.000000000000000000", currency: "USD" }), posting(close1.event_id, 2, "realized-pnl", { realized_pnl: "5.999999999999999999", currency: "USD" }), posting(close2.event_id, 1, "holding-cost", { cost_delta: "-10.000000000000000001", currency: "USD" }), posting(close2.event_id, 2, "realized-pnl", { realized_pnl: "3.999999999999999999", currency: "USD" }), posting(reopen.event_id, 1, "holding-cost", { cost_delta: "20.000000000000000000", currency: "USD" })], { quantity: "1.000000000000", carrying_cost: "20.000000000000000000", average_cost: "20.000000000000000000", historical_realized_pnl: "9.999999999999999998", closed_state_before_reopen: { quantity: "0", carrying_cost: "0", average_cost: null } }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "holding-cost", { cost_delta: "-0.000000000000000001", currency: "USD" }), posting(boundary.event_id, 2, "security-quantity", { quantity_delta: "-0.000000000001" })], { quantity: "0", carrying_cost: "0", average_cost: null }),
    scenario("failure", [command("post_event", { ...close2.detail, quantity: "1.000000000001" })], [], [], { unchanged: true }, { errors: [diagnostic("NEGATIVE_HOLDING_NOT_ALLOWED", "quantity")] }),
  ]);
}

{
  const dividend = event(9, "normal", 1, "Dividend", "2026-02-10", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", settlement_account_id: "cash-usd-1", gross_cash_amount: "10.00", withholding_tax: "1.50", fee_amount: "0.50", currency: "USD" });
  const expense = event(9, "normal", 2, "InvestmentExpense", "2026-02-10", 2, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", settlement_account_id: "cash-usd-1", fee_scope: "instrument", amount: "2.00", currency: "USD" });
  const boundary = event(9, "boundary", 1, "InvestmentExpense", "2026-02-10", 3, { portfolio_id: "portfolio-a", instrument_id: null, settlement_account_id: "cash-usd-1", fee_scope: "portfolio", amount: "0.01", currency: "USD" });
  addCase(9, "dividend-investment-expense", "Dividend, withholding and independent investment expense", ["INV-002", "INV-005", "INV-008", "EXP-002"], ["ADR-0005", "ADR-0014"], [
    scenario("normal", [command("post_event", dividend.detail), command("post_event", expense.detail)], [dividend, expense], [posting(dividend.event_id, 1, "settlement-cash", { quantity_delta: "8.00", currency: "USD" }), posting(dividend.event_id, 2, "net-dividend", { net_dividend: "8.00", currency: "USD" }), posting(expense.event_id, 1, "settlement-cash", { quantity_delta: "-2.00", currency: "USD" }), posting(expense.event_id, 2, "independent-expense", { independent_expense: "2.00", currency: "USD" })], { quantity_delta: "0", net_dividend: "8.00", independent_expense: "2.00", total_return_delta: "6.00", p0_expense: "0" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "settlement-cash", { quantity_delta: "-0.01", currency: "USD" }), posting(boundary.event_id, 2, "portfolio-independent-expense", { independent_expense: "0.01", currency: "USD" })], { instrument_allocation: null, portfolio_return_delta: "-0.01", p0_expense: "0" }),
    scenario("failure", [command("post_event", { ...expense.detail, fee_scope: "instrument", instrument_id: null })], [], [], { unchanged: true }, { errors: [diagnostic("INSTRUMENT_REQUIRED_FOR_FEE_SCOPE", "instrument_id")] }),
  ]);
}

{
  const normal = event(10, "normal", 1, "ValuationRequest", "2026-02-11", 1, { valuation_date: "2026-02-11", instrument_id: "instrument-no-price", quantity: "3.000000000000" });
  const boundary = event(10, "boundary", 1, "ValuationRequest", "2026-02-11", 2, { valuation_date: "2026-02-11", instrument_id: "instrument-alpha", quantity: "3.000000000000", price_revision: { date: "2026-02-11", price: "10.000000000000" } });
  addCase(10, "missing-security-price", "Missing security price stays unvalued", ["FX-005", "VAL-001", "VAL-002"], ["ADR-0012"], [
    scenario("normal", [command("get_overview", normal.detail)], [], [], { valued_net_assets: "0", unvalued_items: [{ instrument_id: "instrument-no-price", reason: "PRICE_MISSING_AS_OF", quantity: "3.000000000000" }] }, { warnings: [diagnostic("PRICE_MISSING_AS_OF", "price_revision")], watermark: 1 }),
    scenario("boundary", [command("get_overview", boundary.detail)], [], [], { valued_net_assets: "30.000000000000000000", unvalued_items: [] }, { watermark: 1 }),
    scenario("failure", [command("save_price_revision", { instrument_id: "instrument-alpha", date: "2026-02-11", price: "0" })], [], [], { unchanged: true }, { errors: [diagnostic("PRICE_MUST_BE_POSITIVE", "price")] }),
  ]);
}

{
  const normal = event(11, "normal", 1, "SecuritySell", "2026-02-12", 1, { quantity: "1.000000000000", available_quantity: "2.000000000000", unit_price: "5.000000000000" });
  const boundary = event(11, "boundary", 1, "SecuritySell", "2026-02-12", 2, { quantity: "2.000000000000", available_quantity: "2.000000000000", unit_price: "5.000000000000" });
  addCase(11, "negative-holding-blocked", "Negative holdings are blocked", ["INV-006"], ["ADR-0005"], [
    scenario("normal", [command("post_event", normal.detail)], [normal], [posting(normal.event_id, 1, "security-quantity", { quantity_delta: "-1.000000000000" })], { quantity: "1.000000000000" }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "security-quantity", { quantity_delta: "-2.000000000000" })], { quantity: "0", carrying_cost: "0" }),
    scenario("failure", [command("post_event", { quantity: "2.000000000001", available_quantity: "2.000000000000", unit_price: "5.000000000000" })], [], [], { quantity: "2.000000000000" }, { errors: [diagnostic("NEGATIVE_HOLDING_NOT_ALLOWED", "quantity")] }),
  ]);
}

{
  const normal = event(12, "normal", 1, "ValuationRequest", "2026-02-15", 1, { valuation_date: "2026-02-15", quantity: "2.000000000000", price_revisions: [{ id: "price-old", date: "2026-02-10", price: "10.000000000000", active: true }, { id: "price-future", date: "2026-02-16", price: "99.000000000000", active: true }], fx_revisions: [{ id: "fx-asof", date: "2026-02-14", rate_to_base: "7.00", active: true }, { id: "fx-future", date: "2026-02-16", rate_to_base: "8.00", active: true }] });
  const boundary = event(12, "boundary", 1, "ValuationRequest", "2026-02-20", 1, { valuation_date: "2026-02-20", quantity: "2.000000000000", price_revision: { id: "price-stale", date: "2026-02-10", price: "10.000000000000", active: true }, fx_revision: { id: "fx-current", date: "2026-02-20", rate_to_base: "7.00", active: true } });
  addCase(12, "valuation-as-of-stale", "Valuation uses valuation-date as-of and flags stale prices", ["FX-002", "FX-004", "VAL-001", "VAL-002", "VAL-003", "VAL-004"], ["ADR-0012"], [
    scenario("normal", [command("get_overview", normal.detail)], [], [], { selected_price_revision_id: "price-old", selected_fx_revision_id: "fx-asof", market_value: "20.000000000000000000", base_value: "140.000000000000000000", future_values_excluded: true }, { watermark: 2 }),
    scenario("boundary", [command("get_overview", boundary.detail)], [], [], { selected_price_revision_id: "price-stale", selected_fx_revision_id: "fx-current", price_age_days: 10, base_value: "140.000000000000000000" }, { warnings: [diagnostic("STALE_PRICE", "price_revision")], watermark: 2 }),
    scenario("failure", [command("get_overview", { valuation_date: "2026-02-01", quantity: "2.000000000000", first_price_date: "2026-02-02", first_fx_date: "2026-02-02" })], [], [], { valued_net_assets: "0", unvalued_items: 1 }, { errors: [diagnostic("PRICE_MISSING_AS_OF", "price_revision"), diagnostic("FX_MISSING_AS_OF", "fx_resolution")] }),
  ]);
}

{
  const full = event(13, "normal", 1, "OpeningBalance", "2025-01-01", 1, { account_id: "cash-cny-history", balance_amount: "1000.00", cutover_date: "2025-01-01", migration_policy: "full_history" });
  const buy = event(13, "normal", 2, "SecurityBuy", "2025-01-02", 1, { portfolio_id: "portfolio-history", instrument_id: "instrument-alpha", quantity: "10.000000000000", unit_price: "10.000000000000", trade_fee: "0", migration_policy: "full_history" });
  const openingBalance = event(13, "boundary", 1, "OpeningBalance", "2026-01-01", 1, { account_id: "cash-cny-cutover", balance_amount: "900.00", cutover_date: "2026-01-01", migration_policy: "explicit_cutover" });
  const openingPosition = event(13, "boundary", 2, "OpeningPosition", "2026-01-01", 2, { portfolio_id: "portfolio-cutover", instrument_id: "instrument-closed", quantity: "0", carrying_cost: "0", cost_currency: "CNY", cutover_date: "2026-01-01", migration_policy: "explicit_cutover" });
  const openingPerformance = event(13, "boundary", 3, "OpeningPerformance", "2026-01-01", 3, { portfolio_id: "portfolio-cutover", instrument_id: "instrument-closed", realized_pnl: "25.00", net_dividend: "5.00", independent_expense: "2.00", currency: "CNY", cutover_date: "2026-01-01" });
  addCase(13, "history-and-cutover", "Full-history and explicit cut-over migration both close", ["CASH-005", "MIG-004", "MIG-005", "MIG-006", "MIG-007"], ["ADR-0003", "ADR-0005", "ADR-0011"], [
    scenario("normal", [command("commit_import", { account_policy: "full_history", portfolio_policy: "full_history", source_evidence_complete: true })], [full, buy], [posting(full.event_id, 1, "opening-cash", { quantity_delta: "1000.00", currency: "CNY" }), posting(buy.event_id, 1, "settlement-cash", { quantity_delta: "-100.00", currency: "CNY" }), posting(buy.event_id, 2, "security-quantity", { quantity_delta: "10.000000000000" }), posting(buy.event_id, 3, "holding-cost", { cost_delta: "100.000000000000000000", currency: "CNY" })], { cash_balance: "900.00", quantity: "10.000000000000", carrying_cost: "100.000000000000000000", reconciliation_difference: "0" }),
    scenario("boundary", [command("commit_import", { account_policy: "explicit_cutover", portfolio_policy: "explicit_cutover", cutover_date: "2026-01-01", pre_cutover_rows: "evidence-only" })], [openingBalance, openingPosition, openingPerformance], [posting(openingBalance.event_id, 1, "opening-cash", { quantity_delta: "900.00", currency: "CNY" }), posting(openingPosition.event_id, 1, "opening-quantity", { quantity_delta: "0" }), posting(openingPerformance.event_id, 1, "opening-realized-pnl", { realized_pnl: "25.00", currency: "CNY" }), posting(openingPerformance.event_id, 2, "opening-net-dividend", { net_dividend: "5.00", currency: "CNY" }), posting(openingPerformance.event_id, 3, "opening-independent-expense", { independent_expense: "2.00", currency: "CNY" })], { cash_balance: "900.00", quantity: "0", carrying_cost: "0", realized_pnl: "25.00", net_dividend: "5.00", independent_expense: "2.00", zero_position_history_preserved: true, reconciliation_difference: "0" }),
    scenario("failure", [command("commit_import", { account_policy: null, portfolio_policy: null })], [], [], { committed: false }, { errors: [diagnostic("MIGRATION_POLICY_REQUIRED", "migration_policy")] }),
  ]);
}

{
  const normal = event(14, "normal", 1, "ValuationRequest", "2026-03-15", 1, { valuation_date: "2026-03-15", period_mode: "mtd", events: [{ effective_date: "2026-03-01", amount: "10.00" }, { effective_date: "2026-03-15", amount: "20.00" }, { effective_date: "2026-03-16", amount: "30.00" }] });
  const boundary = event(14, "boundary", 1, "ValuationRequest", "2026-03-01", 1, { valuation_date: "2026-03-01", period_mode: "mtd", events: [{ effective_date: "2026-03-01", amount: "10.00" }] });
  addCase(14, "valuation-date-mtd", "Valuation-date movement resolves MTD explicitly", ["DATE-002", "VAL-004"], ["ADR-0012", "ADR-0014"], [
    scenario("normal", [command("get_overview", normal.detail)], [], [], { resolved_start_date: "2026-03-01", resolved_end_date: "2026-03-15", mtd_expense: "30.00", excluded_future_expense: "30.00" }, { watermark: 3 }),
    scenario("boundary", [command("get_overview", boundary.detail)], [], [], { resolved_start_date: "2026-03-01", resolved_end_date: "2026-03-01", mtd_expense: "10.00" }, { watermark: 1 }),
    scenario("failure", [command("get_overview", { valuation_date: "2026-02-30", period_mode: "mtd" })], [], [], { previous_result_cleared: true }, { errors: [diagnostic("INVALID_LOCAL_DATE", "valuation_date")] }),
  ]);
}

{
  const normal = event(15, "normal", 1, "Expense", "2024-02-29", 1, { amount: "1.00", currency: "CNY" });
  const first = event(15, "boundary", 1, "SecurityBuy", "2026-01-31", 10, { quantity: "1.000000000000", unit_price: "10.000000000000" });
  const second = event(15, "boundary", 2, "SecuritySell", "2026-01-31", 11, { quantity: "1.000000000000", unit_price: "11.000000000000" });
  addCase(15, "calendar-and-sequence", "Month-end, leap-day and same-day trade sequence", ["DATE-001", "DATE-003"], ["ADR-0003", "ADR-0005"], [
    scenario("normal", [command("post_event", normal.detail)], [normal], [posting(normal.event_id, 1, "cash", { quantity_delta: "-1.00", currency: "CNY" })], { accepted_local_date: "2024-02-29" }),
    scenario("boundary", [command("commit_import", { source_rows: [{ row: 10, event_type: "BUY" }, { row: 11, event_type: "SELL" }] })], [first, second], [posting(first.event_id, 1, "security-quantity", { quantity_delta: "1.000000000000" }), posting(second.event_id, 1, "security-quantity", { quantity_delta: "-1.000000000000" })], { ordered_event_ids: [first.event_id, second.event_id], quantity: "0" }),
    scenario("failure", [command("commit_import", { events: [{ effective_date: "2026-01-31", sequence: 10 }, { effective_date: "2026-01-31", sequence: 10 }] })], [], [], { committed: false }, { errors: [diagnostic("DUPLICATE_EVENT_SEQUENCE", "sequence")] }),
  ]);
}

{
  const buyA = event(16, "normal", 1, "SecurityBuy", "2026-02-16", 1, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", settlement_account_id: "cash-usd-a", quantity: "2.000000000000", unit_price: "10.000000000000", trade_fee: "0" });
  const buyB = event(16, "normal", 2, "SecurityBuy", "2026-02-16", 2, { portfolio_id: "portfolio-b", instrument_id: "instrument-alpha", settlement_account_id: "cash-usd-b", quantity: "3.000000000000", unit_price: "20.000000000000", trade_fee: "0" });
  const boundary = event(16, "boundary", 1, "SecurityBuy", "2026-02-16", 3, { portfolio_id: "portfolio-a", instrument_id: "instrument-alpha", settlement_account_id: "cash-usd-alternate", settlement_account_override_reason: "Synthetic segregated cash account", quantity: "1.000000000000", unit_price: "11.000000000000", trade_fee: "0" });
  addCase(16, "cross-portfolio-isolation", "Same instrument stays isolated across portfolios", ["INV-001", "INV-003"], ["ADR-0005"], [
    scenario("normal", [command("post_event", buyA.detail), command("post_event", buyB.detail)], [buyA, buyB], [posting(buyA.event_id, 1, "holding-cost", { portfolio_id: "portfolio-a", cost_delta: "20.000000000000000000", currency: "USD" }), posting(buyB.event_id, 1, "holding-cost", { portfolio_id: "portfolio-b", cost_delta: "60.000000000000000000", currency: "USD" })], { holdings: [{ portfolio_id: "portfolio-a", quantity: "2.000000000000", carrying_cost: "20.000000000000000000" }, { portfolio_id: "portfolio-b", quantity: "3.000000000000", carrying_cost: "60.000000000000000000" }] }),
    scenario("boundary", [command("post_event", boundary.detail)], [boundary], [posting(boundary.event_id, 1, "holding-cost", { portfolio_id: "portfolio-a", cost_delta: "11.000000000000000000", currency: "USD" })], { override_audit_reason: "Synthetic segregated cash account", portfolio_a_cost: "11.000000000000000000" }),
    scenario("failure", [command("post_event", { ...boundary.detail, settlement_account_override_reason: "" })], [], [], { unchanged: true }, { errors: [diagnostic("SETTLEMENT_OVERRIDE_REASON_REQUIRED", "settlement_account_override_reason")] }),
  ]);
}

{
  const original = event(17, "normal", 1, "Expense", "2026-02-17", 1, { amount: "25.00", currency: "CNY", category_id: "cat-food" });
  const reversal = event(17, "normal", 2, "Reversal", "2026-02-17", 2, { reason: "Synthetic duplicate entry" }, { reverses_event_id: original.event_id });
  const boundaryOriginal = event(17, "boundary", 1, "Income", "2026-02-17", 3, { amount: "0.00000001", currency: "CNY" });
  const boundaryReversal = event(17, "boundary", 2, "Reversal", "2026-02-17", 4, { reason: "Synthetic precision boundary reversal" }, { reverses_event_id: boundaryOriginal.event_id });
  addCase(17, "reversal-zero-net", "Reversal preserves audit and nets exactly to zero", ["EVT-001", "EVT-002", "EVT-003", "EVT-004", "EVT-005", "EVT-006"], ["ADR-0003", "ADR-0006"], [
    scenario("normal", [command("post_event", original.detail), command("reverse_event", { event_id: original.event_id, reason: reversal.detail.reason })], [original, reversal], [posting(original.event_id, 1, "cash", { quantity_delta: "-25.00", currency: "CNY" }), posting(reversal.event_id, 1, "cash-reversal", { quantity_delta: "25.00", currency: "CNY" })], { net_cash_change: "0", expense: "0", audit_event_ids: [original.event_id, reversal.event_id], rebuild_matches: true }),
    scenario("boundary", [command("post_event", boundaryOriginal.detail), command("reverse_event", { event_id: boundaryOriginal.event_id, reason: boundaryReversal.detail.reason })], [boundaryOriginal, boundaryReversal], [posting(boundaryOriginal.event_id, 1, "cash", { quantity_delta: "0.00000001", currency: "CNY" }), posting(boundaryReversal.event_id, 1, "cash-reversal", { quantity_delta: "-0.00000001", currency: "CNY" })], { net_cash_change: "0" }),
    scenario("failure", [command("reverse_event", { event_id: original.event_id, reason: "" })], [], [], { unchanged: true }, { errors: [diagnostic("REVERSAL_REASON_REQUIRED", "reason")] }),
  ]);
}

{
  const normalInput = { source_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", importer_version: "importer-v1", schema_version: 1, import_key: "synthetic-key-a", rerun_count: 2, synthetic_rows: [{ sheet: "Synthetic expenses", row: 2, amount: "5.00", currency: "CNY" }] };
  const proposed = event(18, "normal", 1, "Expense", "2026-02-18", 1, { account_id: "cash-cny-1", amount: "5.00", currency: "CNY", import_key: "synthetic-key-a" }, { status: "evidence-only" });
  const boundaryInput = { source_sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", importer_version: "importer-v1", schema_version: 1, import_key: "synthetic-key-b", rerun_count: 1, synthetic_rows: [] };
  addCase(18, "import-idempotency", "Identical-byte import reruns are idempotent", ["MIG-001", "MIG-002", "MIG-003"], ["ADR-0011"], [
    scenario("normal", [command("analyze_import", normalInput), command("analyze_import", normalInput)], [proposed], [], { staged_rows: 1, duplicate_rows: 0, proposed_event_count: 1, identical_result_hash: true }),
    scenario("boundary", [command("analyze_import", boundaryInput)], [], [], { staged_rows: 0, proposed_event_count: 0, empty_workbook_accepted_for_analysis: true }),
    scenario("failure", [command("commit_import", { existing_ledger_import_hash: normalInput.source_sha256, candidate_source_sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", mode: "incremental-merge" })], [], [], { committed: false }, { errors: [diagnostic("MODIFIED_WORKBOOK_INCREMENTAL_MERGE_FORBIDDEN", "source_sha256")] }),
  ]);
}

{
  const normal = event(19, "normal", 1, "RestoreAttempt", "2026-02-19", 1, { package_id: "synthetic-backup-valid", package_hash: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd", schema_version: 1 });
  const boundary = event(19, "boundary", 1, "RestoreAttempt", "2026-02-19", 2, { package_id: "synthetic-backup-truncated", package_hash: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", schema_version: 1 });
  addCase(19, "backup-safe-failure", "Damaged, future-version and failed restore preserve live ledger", ["EVT-004"], ["ADR-0002"], [
    scenario("normal", [command("restore_backup", normal.detail)], [], [], { candidate_validated: true, live_ledger_replaced: true, live_ledger_hash_after: normal.detail.package_hash }),
    scenario("boundary", [command("restore_backup", boundary.detail)], [], [], { candidate_validated: false, live_ledger_replaced: false, live_ledger_hash_after: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" }, { errors: [diagnostic("BACKUP_TRUNCATED", "package_hash")] }),
    scenario("failure", [command("restore_backup", { package_id: "synthetic-backup-future", schema_version: 999, package_hash: "1111111111111111111111111111111111111111111111111111111111111111" }), command("restore_backup", { package_id: "synthetic-backup-switch-failure", schema_version: 1, simulate_atomic_switch_failure: true })], [], [], { candidate_validated: false, live_ledger_replaced: false, live_ledger_hash_after: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff" }, { errors: [diagnostic("BACKUP_SCHEMA_TOO_NEW", "schema_version"), diagnostic("RESTORE_SWITCH_FAILED", "atomic_switch")] }),
  ], { noFinancial: true });
}

{
  const normal = event(20, "normal", 1, "EncryptedRestoreAttempt", "2026-02-20", 1, { package_id: "synthetic-encrypted-backup", format_version: 1, kdf: "argon2id-reviewed-placeholder", authenticated: true, password_case: "correct" });
  const boundary = event(20, "boundary", 1, "EncryptedRestoreAttempt", "2026-02-20", 2, { package_id: "synthetic-encrypted-backup", format_version: 1, kdf: "argon2id-reviewed-placeholder", authenticated: false, password_case: "wrong" });
  addCase(20, "encrypted-backup", "Encrypted backup authenticates before live-ledger switch", ["EVT-004"], ["ADR-0002"], [
    scenario("normal", [command("restore_backup", normal.detail)], [], [], { authenticated: true, candidate_validated: true, live_ledger_replaced: true }),
    scenario("boundary", [command("restore_backup", boundary.detail)], [], [], { authenticated: false, candidate_validated: false, live_ledger_replaced: false }, { errors: [diagnostic("BACKUP_AUTHENTICATION_FAILED", "password")] }),
    scenario("failure", [command("restore_backup", { package_id: "synthetic-encrypted-backup", format_version: 2, kdf: "unknown-kdf", authenticated: false })], [], [], { candidate_validated: false, live_ledger_replaced: false }, { errors: [diagnostic("BACKUP_KDF_OR_VERSION_UNSUPPORTED", "kdf")] }),
  ], { noFinancial: true });
}

{
  const normalResult = makeExpenseResult({ startDate: "2024-02-29", endDate: "2024-02-29", buckets: [{ bucket_id: "cat-food", label: "Food", amount: "12.00", distinct_event_count: 1 }], globalCount: 1, eventWatermark: 1 });
  const boundaryResult = makeExpenseResult({ startDate: "2026-01-31", endDate: "2026-02-01", buckets: [{ bucket_id: "cat-food", label: "Food", amount: "20.00", distinct_event_count: 2 }], globalCount: 2, eventWatermark: 2 });
  addCase(21, "expense-date-validation", "Expense dates include endpoints and invalid input clears stale results", ["DATE-002", "EXP-008", "EXP-009"], ["ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { start_date: "2024-02-29", end_date: "2024-02-29" })], [], [], { expense_analysis_query_result: normalResult }, { watermark: 1 }),
    scenario("boundary", [command("get_expense_analysis", { start_date: "2026-01-31", end_date: "2026-02-01", resolved_from_local_today: false })], [], [], { expense_analysis_query_result: boundaryResult }, { watermark: 2 }),
    scenario("failure", [command("get_expense_analysis", { start_date: "2026-03-02", end_date: "2026-03-01", excel_date_has_unconfirmed_time_fraction: true })], [], [], { previous_result_cleared: true }, { errors: [diagnostic("EXPENSE_DATE_RANGE_INVALID", "start_date")] }),
  ]);
}

{
  const normalResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-food", label: "Food", amount: "100.00", distinct_event_count: 1 }, { bucket_id: "system:ordinary-fee", bucket_kind: "system", label: "Ordinary fees", amount: "2.00", distinct_event_count: 1 }, { bucket_id: "system:fx-fee", bucket_kind: "system", label: "FX fees", amount: "3.00", distinct_event_count: 1 }], globalCount: 3, eventWatermark: 9 });
  const boundaryResult = makeExpenseResult({ buckets: [{ bucket_id: "system:uncategorized", bucket_kind: "system", label: "Uncategorized", amount: "0.01", distinct_event_count: 1 }], globalCount: 1, eventWatermark: 10 });
  addCase(22, "expense-fee-matrix", "P0 expense fee matrix includes each contribution exactly once", ["EXP-001", "EXP-002", "EXP-003", "EXP-004", "EXP-010"], ["ADR-0005", "ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { included: ["ordinary-expense", "ordinary-fee", "fx-fee"], excluded: ["income", "adjustment", "transfer-principal", "exchange-principal", "security-principal", "trade-fee", "dividend-fee", "withholding-tax", "investment-expense"] })], [], [], { expense_analysis_query_result: normalResult, excluded_investment_return_effect: "-4.00" }, { watermark: 9 }),
    scenario("boundary", [command("get_expense_analysis", { uncategorized_expense_amount: "0.01" })], [], [], { expense_analysis_query_result: boundaryResult }, { watermark: 10 }),
    scenario("failure", [command("get_expense_analysis", { contribution_id: "synthetic-contribution-1", assigned_bucket_ids: ["cat-food", "system:ordinary-fee"] })], [], [], { result_emitted: false }, { errors: [diagnostic("EXPENSE_CONTRIBUTION_BUCKET_CARDINALITY", "assigned_bucket_ids")] }),
  ]);
}

{
  const normalResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-food", label: "Food", amount: "10.00", distinct_event_count: 1 }, { bucket_id: "system:ordinary-fee", bucket_kind: "system", label: "Ordinary fees", amount: "1.00", distinct_event_count: 1 }], globalCount: 1, eventWatermark: 1 });
  const boundaryResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-a", label: "A", amount: "1.00", distinct_event_count: 1 }, { bucket_id: "cat-b", label: "B", amount: "1.00", distinct_event_count: 1 }, { bucket_id: "cat-c", label: "C", amount: "1.00", distinct_event_count: 1 }], globalCount: 2, eventWatermark: 2 });
  addCase(23, "expense-distinct-counts", "Global and bucket distinct event counts have different cardinality", ["EXP-007", "EXP-008", "EXP-009"], ["ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { synthetic_event_contributions: [{ event_id: "evt-shared", bucket_id: "cat-food" }, { event_id: "evt-shared", bucket_id: "system:ordinary-fee" }] })], [], [], { expense_analysis_query_result: normalResult, bucket_count_sum: 2, global_distinct_count: 1 }, { watermark: 1 }),
    scenario("boundary", [command("get_expense_analysis", { synthetic_event_contributions: [{ event_id: "evt-shared", bucket_id: "cat-a" }, { event_id: "evt-shared", bucket_id: "cat-b" }, { event_id: "evt-second", bucket_id: "cat-c" }] })], [], [], { expense_analysis_query_result: boundaryResult, bucket_count_sum: 3, global_distinct_count: 2 }, { watermark: 2 }),
    scenario("failure", [command("get_expense_analysis", { count_strategy: "sum-bucket-counts-as-global" })], [], [], { result_emitted: false }, { errors: [diagnostic("EXPENSE_GLOBAL_COUNT_NOT_DISTINCT", "count_strategy")] }),
  ]);
}

{
  const normalResult = makeExpenseResult({ startDate: "2026-02-01", endDate: "2026-02-28", buckets: [{ bucket_id: "cat-food", label: "Food", amount: "100.00", distinct_event_count: 1 }], globalCount: 1, refundAmount: "20.00", refundCount: 1, reimbursementAmount: "5.00", reimbursementCount: 1, eventWatermark: 3 });
  const boundaryResult = makeExpenseResult({ startDate: "2026-03-01", endDate: "2026-03-31", buckets: [], globalCount: 0, refundAmount: "7.00", refundCount: 1, unvaluedRefundCount: 1, eventWatermark: 4 });
  addCase(24, "refund-reimbursement", "Refund and reimbursement are separate gross semantic roles", ["EXP-005", "EXP-009"], ["ADR-0006", "ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { original_expense_date: "2026-01-20", refund_date: "2026-02-10", reimbursement_date: "2026-02-12" })], [], [], { expense_analysis_query_result: normalResult, original_category_gross_expense: "100.00" }, { watermark: 3 }),
    scenario("boundary", [command("get_expense_analysis", { refund_without_link: "7.00", foreign_currency_refund_missing_fx: "1.00" })], [], [], { expense_analysis_query_result: boundaryResult }, { watermark: 4 }),
    scenario("failure", [command("get_expense_analysis", { semantic_role: "refund", implementation_strategy: "net-original-category" })], [], [], { result_emitted: false }, { errors: [diagnostic("REFUND_NETTING_POLICY_FORBIDDEN", "semantic_role")] }),
  ]);
}

{
  const normalResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-stable-1", label: "Renamed dining", archived: true, amount: "30.00", distinct_event_count: 2 }], globalCount: 2, eventWatermark: 2, masterDataWatermark: 4 });
  const boundaryResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-stable-1", label: "Dining", archived: false, amount: "30.00", distinct_event_count: 2 }, { bucket_id: "cat-stable-2", label: "Travel", archived: true, amount: "30.00", distinct_event_count: 1 }], globalCount: 3, eventWatermark: 3, masterDataWatermark: 5 });
  addCase(25, "category-lifecycle", "Category lifecycle preserves stable historical identity", ["EXP-006", "EXP-011"], ["ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { category_id: "cat-stable-1", operations: ["disable", "rename", "reorder", "enable"] })], [], [], { expense_analysis_query_result: normalResult, drilldown_filter_bucket_id: "cat-stable-1" }, { watermark: 2 }),
    scenario("boundary", [command("get_expense_analysis", { equal_amount_categories: ["cat-stable-2", "cat-stable-1"] })], [], [], { expense_analysis_query_result: boundaryResult, tie_order: ["cat-stable-1", "cat-stable-2"] }, { watermark: 3 }),
    scenario("failure", [command("save_category", { category_id: "cat-stable-1", operation: "physical-delete", has_posted_events: true })], [], [], { unchanged: true }, { errors: [diagnostic("CATEGORY_WITH_HISTORY_CANNOT_BE_DELETED", "category_id")] }),
  ]);
}

{
  const missingResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-food", label: "Food", amount: "50.00", distinct_event_count: 1 }], complete: false, globalCount: 1, unvaluedExpenseCount: 1, eventWatermark: 2 });
  const completeResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-food", label: "Food", amount: "50.00", distinct_event_count: 1 }, { bucket_id: "cat-travel", label: "Travel", amount: "70.00", distinct_event_count: 1 }], complete: true, globalCount: 2, eventWatermark: 3 });
  addCase(26, "expense-missing-fx", "Missing FX reports valued subtotal and deterministically completes later", ["FX-005", "EXP-003", "EXP-004", "EXP-008", "EXP-012"], ["ADR-0012", "ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { valued_expense: "50.00", unvalued_usd_expense: "10.00", fx_available: false })], [], [], { expense_analysis_query_result: missingResult }, { watermark: 2 }),
    scenario("boundary", [command("get_expense_analysis", { valued_expense: "50.00", usd_expense: "10.00", final_rate: "7.00", fx_available: true })], [], [], { expense_analysis_query_result: completeResult }, { watermark: 3 }),
    scenario("failure", [command("save_fx_revision", { currency: "USD", date: "2026-02-28", rate_to_base: "0" })], [], [], { previous_result_preserved: true }, { errors: [diagnostic("FX_RATE_MUST_BE_POSITIVE", "rate_to_base")] }),
  ]);
}

{
  const twelveBuckets = Array.from({ length: 12 }, (_, index) => ({ bucket_id: `cat-${String(index + 1).padStart(2, "0")}`, label: `Category ${String(index + 1).padStart(2, "0")}`, amount: String(120 - index * 10), distinct_event_count: 1 }));
  const tieBuckets = [{ bucket_id: "cat-b", label: "B", amount: "10.00", distinct_event_count: 1 }, { bucket_id: "cat-a", label: "A", amount: "10.00", distinct_event_count: 1 }];
  const normalResult = makeExpenseResult({ buckets: twelveBuckets, globalCount: 12, eventWatermark: 12 });
  const boundaryResult = makeExpenseResult({ buckets: tieBuckets, globalCount: 2, eventWatermark: 2 });
  addCase(27, "expense-top10", "Top 10 and other use stable amount/id ordering", ["EXP-008", "EXP-011"], ["ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { positive_category_count: 12 })], [], [], { expense_analysis_query_result: normalResult, top10_item_count: 10, other_amount: normalResult.top10.other.amount }, { watermark: 12 }),
    scenario("boundary", [command("get_expense_analysis", { equal_amount_bucket_ids: ["cat-b", "cat-a"] })], [], [], { expense_analysis_query_result: boundaryResult, stable_order: ["cat-a", "cat-b"] }, { watermark: 2 }),
    scenario("failure", [command("get_expense_analysis", { top10_strategy: "database-physical-row-order" })], [], [], { result_emitted: false }, { errors: [diagnostic("EXPENSE_SORT_NOT_CANONICAL", "top10_strategy")] }),
  ]);
}

{
  const dynamicBuckets = Array.from({ length: 22 }, (_, index) => ({ bucket_id: `cat-dynamic-${String(index + 1).padStart(2, "0")}`, label: `Synthetic category ${String(index + 1).padStart(2, "0")}`, archived: index === 21, amount: "1.00", distinct_event_count: 1 }));
  dynamicBuckets.push({ bucket_id: "system:uncategorized", bucket_kind: "system", label: "Uncategorized", amount: "2.00", distinct_event_count: 1 });
  dynamicBuckets.push({ bucket_id: "system:fx-fee", bucket_kind: "system", label: "FX fees", amount: "3.00", distinct_event_count: 1 });
  const normalResult = makeExpenseResult({ buckets: dynamicBuckets, globalCount: 24, eventWatermark: 24, masterDataWatermark: 22 });
  const boundaryResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-dynamic-22", label: "Synthetic archived category", archived: true, amount: "1.00", distinct_event_count: 1 }, { bucket_id: "system:fx-fee", bucket_kind: "system", label: "FX fees", amount: "3.00", distinct_event_count: 1 }], globalCount: 2, eventWatermark: 2 });
  addCase(28, "expense-excel-range-gap", "Dynamic categories fix Excel fixed-range amount/count gaps", ["EXP-006", "EXP-007", "EXP-008", "EXP-010", "EXP-011"], ["ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { synthetic_category_count: 22, include_archived: true, include_uncategorized: true, include_fx_fee: true, excel_amount_rows: "O7:O62", excel_count_rows: "P7:P27" })], [], [], { expense_analysis_query_result: normalResult, excel_visible_amount: "27.00", excel_visible_count: 21, application_amount: "27.00", application_count: 24, difference_event_count: 3 }, { watermark: 24 }),
    scenario("boundary", [command("get_expense_analysis", { only_fixed_range_gap_items: true })], [], [], { expense_analysis_query_result: boundaryResult, excel_visible_count: 0, application_count: 2 }, { watermark: 2 }),
    scenario("failure", [command("get_expense_analysis", { category_scan: "fixed-row-range" })], [], [], { result_emitted: false }, { errors: [diagnostic("EXPENSE_FIXED_RANGE_FORBIDDEN", "category_scan")] }),
  ]);
}

{
  const oldResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-food", label: "Food", amount: "10.00", distinct_event_count: 1 }], globalCount: 1, refundAmount: "0", eventWatermark: 1 });
  const revisedResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-food", label: "Food", amount: "12.00", distinct_event_count: 1 }], globalCount: 1, refundAmount: "5.00", refundCount: 1, eventWatermark: 3 });
  const original = event(29, "normal", 1, "Expense", "2026-02-10", 1, { amount: "10.00", category_id: "cat-food" });
  const revision = event(29, "normal", 2, "Expense", "2026-02-10", 2, { amount: "12.00", category_id: "cat-food", reason: "Synthetic corrected receipt" }, { supersedes_event_id: original.event_id });
  const refund = event(29, "normal", 3, "Income", "2026-03-01", 1, { amount: "5.00", semantic_role: "refund" });
  addCase(29, "revision-watermark", "Revision watermarks and cross-period refunds remain reproducible", ["EVT-001", "EVT-005", "EVT-006", "EXP-005", "EXP-012"], ["ADR-0006", "ADR-0014"], [
    scenario("normal", [command("revise_event", { event_id: original.event_id, replacement_amount: "12.00", reason: revision.detail.reason }), command("post_event", refund.detail)], [original, revision, refund], [posting(original.event_id, 1, "cash", { quantity_delta: "-10.00", currency: "CNY" }), posting(revision.event_id, 1, "cash", { quantity_delta: "-12.00", currency: "CNY" }), posting(refund.event_id, 1, "cash", { quantity_delta: "5.00", currency: "CNY" })], { old_export: oldResult, current_result: revisedResult, old_export_reproduced: true }, { watermark: 3 }),
    scenario("boundary", [command("get_expense_analysis", { event_watermark: 1, calculation_version: CALCULATION_VERSION, expense_policy_version: "expense-policy-v1" })], [], [], { expense_analysis_query_result: oldResult }, { watermark: 1 }),
    scenario("failure", [command("revise_event", { event_id: original.event_id, replacement_amount: "12.00", reason: "" })], [], [], { unchanged: true }, { errors: [diagnostic("REVISION_REASON_REQUIRED", "reason")] }),
  ]);
}

{
  const deterministic = makeExpenseResult({ buckets: [{ bucket_id: "cat-a", label: "A", amount: "10.00", distinct_event_count: 1 }, { bucket_id: "cat-b", label: "B", amount: "10.00", distinct_event_count: 1 }, { bucket_id: "system:ordinary-fee", bucket_kind: "system", label: "Ordinary fees", amount: "1.00", distinct_event_count: 1 }], globalCount: 2, eventWatermark: 3, masterDataWatermark: 2 });
  const newer = makeExpenseResult({ buckets: [{ bucket_id: "cat-a", label: "A", amount: "11.00", distinct_event_count: 2 }, { bucket_id: "cat-b", label: "B", amount: "10.00", distinct_event_count: 1 }, { bucket_id: "system:ordinary-fee", bucket_kind: "system", label: "Ordinary fees", amount: "1.00", distinct_event_count: 1 }], globalCount: 3, eventWatermark: 4, masterDataWatermark: 2 });
  addCase(30, "canonical-row-order", "Canonical query hash ignores physical row and import order", ["EVT-005", "EXP-008", "EXP-009", "EXP-012"], ["ADR-0003", "ADR-0014"], [
    scenario("normal", [command("get_expense_analysis", { physical_row_order: [3, 1, 2], import_batch_order: [2, 1] }), command("get_expense_analysis", { physical_row_order: [1, 2, 3], import_batch_order: [1, 2] })], [], [], { result_from_order_a: deterministic, result_from_order_b: deterministic, hashes_equal: true }, { watermark: 3 }),
    scenario("boundary", [command("get_expense_analysis", { event_watermark: 4, previous_event_watermark: 3 })], [], [], { previous_result: deterministic, new_result: newer, previous_export_metadata_preserved: true }, { watermark: 4 }),
    scenario("failure", [command("get_expense_analysis", { serialization: "locale-dependent", physical_row_order_used_as_tiebreaker: true })], [], [], { result_emitted: false }, { errors: [diagnostic("CANONICAL_SERIALIZATION_VIOLATION", "serialization")] }),
  ]);
}

{
  const applicationResult = makeExpenseResult({ buckets: [{ bucket_id: "cat-visible", label: "Visible category", amount: "20.00", distinct_event_count: 1 }, { bucket_id: "cat-archived", label: "Archived category", archived: true, amount: "5.00", distinct_event_count: 1 }, { bucket_id: "system:uncategorized", bucket_kind: "system", label: "Uncategorized", amount: "3.00", distinct_event_count: 1 }, { bucket_id: "system:fx-fee", bucket_kind: "system", label: "FX fees", amount: "2.00", distinct_event_count: 1 }], globalCount: 4, refundAmount: "4.00", refundCount: 1, eventWatermark: 5, masterDataWatermark: 3 });
  const boundaryResult = makeExpenseResult({ buckets: [], complete: false, globalCount: 0, unvaluedExpenseCount: 1, eventWatermark: 1 });
  addCase(31, "expense-excel-difference-bridge", "Excel-visible and application expense policies reconcile by event", ["MIG-006", "EXP-001", "EXP-004", "EXP-005", "EXP-006", "EXP-007", "EXP-012"], ["ADR-0011", "ADR-0014"], [
    scenario("normal", [command("analyze_import", { reference_period_start: "2026-02-01", reference_period_end: "2026-02-28", excel_visible_amount: "24.00", excel_visible_count: 1 })], [], [], { expense_analysis_query_result: applicationResult, difference_bridge: [{ synthetic_event_key: "bridge-archived", excel_amount: "0", application_amount: "5.00", reason_code: "ARCHIVED_CATEGORY_INCLUDED" }, { synthetic_event_key: "bridge-uncategorized", excel_amount: "0", application_amount: "3.00", reason_code: "UNCATEGORIZED_DYNAMIC_BUCKET" }, { synthetic_event_key: "bridge-fx-fee", excel_amount: "2.00", application_amount: "2.00", reason_code: "AMOUNT_INCLUDED_COUNT_FIXED" }, { synthetic_event_key: "bridge-refund", excel_amount: "2.00", application_amount: "4.00", reason_code: "SEMANTIC_ROLE_NOT_NAME" }], application_amount: "30.00", unexplained_difference: "0" }, { watermark: 5 }),
    scenario("boundary", [command("analyze_import", { reference_period_start: "2026-02-01", reference_period_end: "2026-02-28", foreign_expense_missing_fx: "10.00" })], [], [], { expense_analysis_query_result: boundaryResult, difference_bridge: [{ synthetic_event_key: "bridge-unvalued", excel_amount: "0", application_amount: "0", reason_code: "UNVALUED_EXPLICIT" }], unexplained_difference: "0" }, { watermark: 1 }),
    scenario("failure", [command("commit_import", { unexplained_difference: "0.01", user_confirmed: false })], [], [], { committed: false }, { errors: [diagnostic("MIGRATION_RECONCILIATION_UNEXPLAINED", "unexplained_difference")] }),
  ]);
}

function renderFixture(spec) {
  const fixtureId = `${String(spec.number).padStart(2, "0")}-${spec.slug}`;
  const scenarioIds = spec.scenarios.map((item) => item.scenarioId);
  const metadata = withHash({
    fixture_id: fixtureId,
    case_number: spec.number,
    title: spec.title,
    synthetic_data: true,
    related_rules: [...new Set(spec.rules)].sort(compareUnicodeScalar),
    related_adrs: [...new Set(spec.adrs)].sort(compareUnicodeScalar),
    coverage: {
      normal: { scenario_ids: scenarioIds.filter((id) => id === "normal"), rule_ids: [...new Set(spec.rules)].sort(compareUnicodeScalar) },
      boundary: { scenario_ids: scenarioIds.filter((id) => id === "boundary"), rule_ids: [...new Set(spec.rules)].sort(compareUnicodeScalar) },
      failure: { scenario_ids: scenarioIds.filter((id) => id === "failure"), rule_ids: [...new Set(spec.rules)].sort(compareUnicodeScalar) },
    },
    decimal_contract: {
      id: "decimal-contract-v1",
      max_significant_digits: 28,
      max_scale: { amount: 8, quantity: 12, unit_price: 12, fx_rate: 15, internal: 18 },
    },
    calculation_version: CALCULATION_VERSION,
    policy_versions: { expense: "expense-policy-v1", bucket: "expense-bucket-policy-v1", refund: "refund-policy-v1", market_data: "market-data-policy-v1" },
    rounding_boundaries: spec.noFinancial ? ["none"] : ["internal-scale-18-half-up", "display-currency-half-up"],
    tolerances: { internal: "0", display: "0.01" },
    generation: { method: "programmatically-generated-synthetic-data", generator: "tools/generate-m0-fixtures.mjs", reviewed: true },
  });
  const input = withHash({
    fixture_id: fixtureId,
    base_currency: "CNY",
    scenarios: spec.scenarios.map((item) => ({
      scenario_id: item.scenarioId,
      kind: item.kind,
      commands: item.commands.map((entry, index) => ({ command_id: `cmd-${String(spec.number).padStart(2, "0")}-${item.kind}-${String(index + 1).padStart(2, "0")}`, command_type: entry.commandType, data: entry.data })),
    })),
  });
  const normalizedEvents = withHash({
    fixture_id: fixtureId,
    scenarios: spec.scenarios.map((item) => ({ scenario_id: item.scenarioId, events: item.events })),
  });
  const expectedPostings = withHash({
    fixture_id: fixtureId,
    scenarios: spec.scenarios.map((item) => ({ scenario_id: item.scenarioId, postings: item.postings, sequence_hash: sha256(item.postings) })),
  });
  const expectedProjection = withHash({
    fixture_id: fixtureId,
    scenarios: spec.scenarios.map((item) => ({ scenario_id: item.scenarioId, projection_version: PROJECTION_VERSION, event_watermark: item.watermark, state: item.state })),
  });
  const expectedErrors = withHash({
    fixture_id: fixtureId,
    scenarios: spec.scenarios.map((item) => ({ scenario_id: item.scenarioId, errors: item.errors, warnings: item.warnings })),
  });
  return new Map([
    ["metadata.json", metadata],
    ["input.json", input],
    ["normalized-events.json", normalizedEvents],
    ["expected-postings.json", expectedPostings],
    ["expected-projection.json", expectedProjection],
    ["expected-errors.json", expectedErrors],
  ]);
}

if (cases.length !== 31 || cases.some((item, index) => item.number !== index + 1)) {
  throw new Error("Generator must define plan section 14.2 cases 1 through 31 exactly once and in order.");
}

const drift = [];
for (const spec of cases) {
  const fixtureId = `${String(spec.number).padStart(2, "0")}-${spec.slug}`;
  const directory = join(fixturesRoot, fixtureId);
  if (!checkOnly) mkdirSync(directory, { recursive: true });
  for (const [fileName, value] of renderFixture(spec)) {
    const expected = `${JSON.stringify(value, null, 2)}\n`;
    const filePath = join(directory, fileName);
    if (checkOnly) {
      let actual = null;
      try {
        actual = readFileSync(filePath, "utf8");
      } catch {
        drift.push(filePath);
      }
      if (actual !== null && actual !== expected) drift.push(filePath);
    } else {
      writeFileSync(filePath, expected, "utf8");
    }
  }
}

if (drift.length > 0) {
  throw new Error(`Generated fixture drift:\n${[...new Set(drift)].join("\n")}`);
}

console.log(checkOnly ? "M0 fixture generation is reproducible." : `Generated ${cases.length} synthetic M0 fixture groups.`);
