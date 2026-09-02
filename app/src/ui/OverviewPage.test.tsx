import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { DrilldownContext, ExpenseAnalysis, ExpenseBucket, ExpenseTopItem } from "../command-client/contracts";
import {
  ExpenseAnalysisView,
  expenseUiState,
  formatBasisPoints,
  invalidExpenseDateFields,
} from "./OverviewPage";
import { LatestRequestGate } from "./queryGate";

const context = (bucketId?: string): DrilldownContext => ({
  start_date: "2026-09-01",
  end_date: "2026-09-03",
  event_watermark: 21,
  calculation_version: "ledger-calculation-v1",
  expense_policy_version: "expense-policy-v1",
  ...(bucketId ? { bucket_id: bucketId } : {}),
  valuation_state: "valued",
});

function expenseResult(overrides: { valued?: string; unvalued?: number; refundUnvalued?: number; bucketCount?: number } = {}): ExpenseAnalysis {
  const bucketCount = overrides.bucketCount ?? 2;
  const buckets: ExpenseBucket[] = Array.from({ length: bucketCount }, (_, index) => ({
    bucket_id: index === 10 ? "system:uncategorized" : index === 11 ? "system:ordinary-fee" : `cat-${String(index + 1).padStart(2, "0")}`,
    bucket_kind: index >= 10 ? "system" : "category",
    label: index === 10 ? "Uncategorized" : index === 11 ? "Ordinary fees" : `Category ${index + 1}`,
    archived: index === 1,
    amount: String(bucketCount - index),
    share_basis_points: Math.max(1, 1000 - index * 50),
    distinct_event_count: index + 1,
    drilldown_context: context(index === 10 ? "system:uncategorized" : index === 11 ? "system:ordinary-fee" : `cat-${String(index + 1).padStart(2, "0")}`),
  }));
  const topItems: ExpenseTopItem[] = buckets.slice(0, 10).map((bucket) => ({
    bucket_id: bucket.bucket_id,
    label: bucket.label,
    amount: bucket.amount,
    share_basis_points: bucket.share_basis_points,
    distinct_event_count: bucket.distinct_event_count,
    drilldown_context: bucket.drilldown_context,
  }));
  const other: ExpenseTopItem | null = bucketCount > 10 ? {
    bucket_id: "system:top10-other",
    label: "Other categories",
    amount: "3",
    share_basis_points: 175,
    distinct_event_count: 23,
    drilldown_context: { ...context("system:top10-other"), member_rank_gt: 10 },
  } : null;
  return {
    contract: "expense-analysis-query-result/v1",
    query: { start_date: "2026-09-01", end_date: "2026-09-03", base_currency: "CNY" },
    summary: {
      label: overrides.unvalued || overrides.refundUnvalued ? "Valued expense subtotal" : "Total expense",
      total_expense: overrides.unvalued || overrides.refundUnvalued ? null : (overrides.valued ?? "3"),
      valued_subtotal: overrides.valued ?? "3",
      global_distinct_event_count: 2,
      largest_category: buckets[0] ? { bucket_id: buckets[0].bucket_id, amount: buckets[0].amount } : null,
    },
    buckets,
    top10: { items: topItems, other },
    refunds: {
      refund: { amount: "1", distinct_event_count: 1, unvalued_count: overrides.refundUnvalued ?? 0, drilldown_context: { ...context(), semantic_role: "refund", valuation_state: "all" } },
      reimbursement: { amount: "0", distinct_event_count: 0, unvalued_count: 0, drilldown_context: { ...context(), semantic_role: "reimbursement", valuation_state: "all" } },
    },
    unvalued: { expense_count: overrides.unvalued ?? 0, drilldown_context: { ...context(), semantic_role: "expense", valuation_state: "unvalued" } },
    watermarks: { event: 21, master_data: 7 },
    versions: { calculation: "ledger-calculation-v1", expense_policy: "expense-policy-v1", bucket_policy: "expense-bucket-policy-v1", refund_policy: "refund-policy-v1" },
    canonicalization: "ledgerkit-canonical-json-v1",
    canonical_hash: "sha256:synthetic",
  };
}

describe("expense analysis UI contract", () => {
  it("validates exact inclusive local-date inputs and clears invalid ordering", () => {
    expect(invalidExpenseDateFields("2026-02-01", "2026-02-28")).toEqual([]);
    expect(invalidExpenseDateFields("2026-02-30", "2026-03-01")).toEqual(["expenseStartDate"]);
    expect(invalidExpenseDateFields("2026-03-02", "2026-03-01")).toEqual(["expenseStartDate", "expenseEndDate"]);
  });

  it("applies the required state priority", () => {
    expect(expenseUiState(expenseResult(), true)).toBe("invalid");
    expect(expenseUiState(expenseResult({ valued: "2", unvalued: 1 }), false)).toBe("incomplete");
    expect(expenseUiState(expenseResult({ valued: "0.00", unvalued: 1 }), false)).toBe("unvalued-only");
    expect(expenseUiState(expenseResult({ valued: "0.00" }), false)).toBe("no-valued-spend");
    expect(expenseUiState(expenseResult({ valued: "2" }), false)).toBe("normal");
  });

  it("renders at most eleven repeated bars and a complete semantic table from one result", () => {
    const result = expenseResult({ bucketCount: 12 });
    const markup = renderToStaticMarkup(<ExpenseAnalysisView locale="en-US" result={result} state="normal" onDrilldown={() => undefined} onOpenQuality={() => undefined} />);
    expect(markup.match(/expense-bar-fill/g)).toHaveLength(11);
    expect(markup.match(/<tr>/g)).toHaveLength(13);
    expect(markup).toContain('scope="col"');
    expect(markup).toContain('scope="row"');
    expect(markup).toContain("Category 2 (Archived)");
    expect(markup).toContain("Uncategorized");
    expect(markup).toContain("Ordinary fees");
    expect(markup).toContain("10.00%");
  });

  it("formats Core-provided integer basis points without parsing Decimal strings", () => {
    expect(formatBasisPoints(0)).toBe("0.00%");
    expect(formatBasisPoints(9091)).toBe("90.91%");
  });
});

describe("latest request gate", () => {
  it("rejects a late result after a newer request begins", () => {
    const gate = new LatestRequestGate();
    const older = gate.begin();
    const newer = gate.begin();
    expect(gate.isLatest(older)).toBe(false);
    expect(gate.isLatest(newer)).toBe(true);
    gate.invalidate();
    expect(gate.isLatest(newer)).toBe(false);
  });
});
