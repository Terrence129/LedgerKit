import { describe, expect, it } from "vitest";
import { localeFromSystemHint, resourceKeySets, supportedLocales, translate } from ".";

describe("localized resources", () => {
  it("keeps zh-CN and en-US key sets identical", () => {
    const keys = resourceKeySets();
    expect(keys["zh-CN"]).toEqual(keys["en-US"]);
    expect(keys["en-US"].length).toBeGreaterThan(0);
  });

  it.each(supportedLocales)("has no blank values in %s", (locale) => {
    for (const key of resourceKeySets()[locale]) {
      expect(translate(locale, key as Parameters<typeof translate>[1]).trim()).not.toBe("");
    }
  });

  it("uses Chinese for Chinese system hints and English as the safe fallback", () => {
    expect(localeFromSystemHint("zh-Hans-CN")).toBe("zh-CN");
    expect(localeFromSystemHint("en-SG")).toBe("en-US");
    expect(localeFromSystemHint(null)).toBe("en-US");
  });
});
