import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { LedgerStatus } from "../command-client/contracts";
import { HealthHome } from "./HealthHome";

const status: LedgerStatus = {
  appVersion: "0.1.0",
  uiLocale: "en-US",
  ledgerState: "not-created",
  localOnly: true,
  privilegedOperationCount: 2,
};

describe("HealthHome", () => {
  it.each([
    ["zh-CN", "你的私人账本从这里开始"],
    ["en-US", "Your private ledger starts here"],
  ] as const)("renders the complete health page in %s", (locale, heading) => {
    const html = renderToStaticMarkup(
      <HealthHome
        locale={locale}
        status={{ ...status, uiLocale: locale }}
        failure={false}
        savingLocale={false}
        onLocaleChange={() => undefined}
      />,
    );

    expect(html).toContain(heading);
    expect(html).toContain("<select");
    expect(html).toContain(">2</dd>");
  });
});
