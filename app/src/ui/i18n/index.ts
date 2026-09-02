import enUS from "./resources/en-US.json";
import zhCN from "./resources/zh-CN.json";
import { supportedLocales, type SupportedLocale } from "../../shared-contracts/locales";

export { supportedLocales };
export type { SupportedLocale };
export type MessageKey = keyof typeof enUS;

const resources: Record<SupportedLocale, Record<MessageKey, string>> = {
  "en-US": enUS,
  "zh-CN": zhCN,
};

export function translate(locale: SupportedLocale, key: MessageKey): string {
  return resources[locale][key] ?? resources["en-US"][key];
}

export function applyDocumentLocale(locale: SupportedLocale): void {
  document.documentElement.lang = locale;
}

export function systemLocaleHint(): string | null {
  return navigator.languages.at(0) ?? navigator.language ?? null;
}

export function localeFromSystemHint(locale: string | null): SupportedLocale {
  return locale?.split(/[-_]/u).at(0)?.toLowerCase() === "zh" ? "zh-CN" : "en-US";
}

export function resourceKeySets(): Record<SupportedLocale, string[]> {
  return {
    "en-US": Object.keys(resources["en-US"]).sort(),
    "zh-CN": Object.keys(resources["zh-CN"]).sort(),
  };
}
