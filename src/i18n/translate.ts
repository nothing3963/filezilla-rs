import { eagerMessages, localeLoaders, supportedLanguages } from "./locales";

export type Translate = (key: string, vars?: Record<string, string | number>) => string;

export { supportedLanguages };

// Only the active language is kept in memory; en-US and zh-CN are always
// present (fallback and default UI language).
const cache: Record<string, Record<string, string>> = { ...eagerMessages };

/** Loads a catalog on demand. Unknown codes are ignored. */
export async function loadLanguage(code: string): Promise<void> {
  if (cache[code]) return;
  const loader = localeLoaders[code];
  if (!loader) return;
  cache[code] = await loader();
}

export function isSupportedLanguage(code: string): boolean {
  return code in eagerMessages || Object.prototype.hasOwnProperty.call(localeLoaders, code);
}

/**
 * Picks the best supported catalog for the given system language tags.
 * Used when the user has not chosen a language yet (settings store "").
 */
export function resolveSystemLanguage(candidates: readonly string[]): string {
  const supported = ["zh-CN", "en-US", ...supportedLanguages.map((item) => item.code)];
  const lookup = new Map(supported.map((code) => [code.toLowerCase(), code]));
  for (const raw of candidates) {
    if (!raw) continue;
    const tag = raw.replace("_", "-").toLowerCase();
    const exact = lookup.get(tag);
    if (exact) return exact;
    const parts = tag.split("-");
    if (parts[0] === "zh") {
      return parts.some((part) => part === "hant" || part === "hk" || part === "mo" || part === "tw")
        ? "zh-TW"
        : "zh-CN";
    }
    const base = supported.find((code) => code.split("-")[0].toLowerCase() === parts[0]);
    if (base) return base;
  }
  return "en-US";
}

export function languageDisplayName(code: string, uiLanguage = "zh-CN"): string {
  // Chinese needs the script name, not the region; every other language is
  // shown without a "(Country)" suffix unless a code has its own rule here.
  if (code === "zh-TW") return "繁體中文";
  const base = code.split("-")[0];
  const capitalize = (name: string) => name.charAt(0).toUpperCase() + name.slice(1);
  try {
    const names = new Intl.DisplayNames([uiLanguage, "en"], { type: "language" });
    return capitalize(names.of(base) ?? code);
  } catch {
    try {
      const names = new Intl.DisplayNames(["en"], { type: "language" });
      return capitalize(names.of(base) ?? code);
    } catch {
      return code;
    }
  }
}

export function translate(
  language: string,
  key: string,
  vars?: Record<string, string | number>,
): string {
  const english = eagerMessages["en-US"] ?? {};
  const table = cache[language];
  let text = table?.[key] ?? english[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}
