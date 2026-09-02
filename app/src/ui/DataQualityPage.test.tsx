import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DataQualityPage } from "./DataQualityPage";

describe("data quality page", () => {
  it.each([
    ["zh-CN", "待处理项目"],
    ["en-US", "Items to resolve"],
  ] as const)("renders its labelled, keyboard-operable entry in %s", (locale, heading) => {
    const markup = renderToStaticMarkup(
      <DataQualityPage
        locale={locale}
        asOfDate="2026-09-03"
        onLoad={async () => ({
          contract: "ledgerkit-data-quality-v1",
          asOfDate: "2026-09-03",
          blockerCount: 0,
          warningCount: 0,
          eventWatermark: 0,
          calculationVersion: "ledger-calculation-v1",
          issues: [],
        })}
        onFix={() => undefined}
      />,
    );
    expect(markup).toContain(heading);
    expect(markup).toContain('type="date"');
    expect(markup).toContain('role="status"');
    expect(markup).toContain('aria-live="polite"');
  });
});
