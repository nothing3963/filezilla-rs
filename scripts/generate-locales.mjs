#!/usr/bin/env node
/**
 * Build one translation overlay file per language from the original
 * FileZilla gettext catalogs.
 *
 * The UI uses semantic keys (e.g. "settings.theme"); `en-US.json` holds the
 * English source and every other language file uses the same keys, so a
 * translated file only needs its values replaced. This script maps each
 * English UI string to its translation in the .po files and writes
 * `src/i18n/locales/<code>.json`, then regenerates
 * `src/i18n/locales/index.ts`. Missing strings fall back to English at
 * runtime.
 *
 * Usage:
 *   node scripts/generate-locales.mjs ../origin/locales src/i18n/locales
 */
import fs from "node:fs";
import path from "node:path";

const [, , localesDir = "../locales", outputDir = "src/i18n/locales"] = process.argv;

function unquote(value) {
  const text = value.trim();
  if (!text.startsWith('"')) return "";
  const body = text.slice(1, text.lastIndexOf('"'));
  let out = "";
  for (let i = 0; i < body.length; i++) {
    if (body[i] !== "\\") {
      out += body[i];
      continue;
    }
    const next = body[++i];
    switch (next) {
      case "n": out += "\n"; break;
      case "r": out += "\r"; break;
      case "t": out += "\t"; break;
      case '"': out += '"'; break;
      case "\\": out += "\\"; break;
      default: out += next ?? "";
    }
  }
  return out;
}

function cleanTranslation(value) {
  return value.replace(/&&/g, "\u0000").replace(/&/g, "").replace(/\u0000/g, "&").trim();
}

function normalizeText(value) {
  return value
    .replace(/&/g, "")
    .replace(/\.\.\.$/, "")
    .replace(/:$/, "")
    .trim()
    .toLowerCase();
}

function parsePo(source) {
  const map = new Map();
  const lines = source.split(/\r?\n/);
  let msgid = null;
  let msgstr = null;
  let fuzzy = false;

  const commit = () => {
    if (msgid && msgstr && !fuzzy) {
      const id = msgid;
      if (!map.has(id)) map.set(id, msgstr);
    }
    msgid = null;
    msgstr = null;
    fuzzy = false;
  };

  for (let i = 0; i < lines.length; i++) {
    let line = lines[i];
    if (line.startsWith("#,")) {
      if (/fuzzy/.test(line)) fuzzy = true;
      continue;
    }
    if (line.startsWith("#") || line.trim() === "") {
      if (msgid && msgstr) commit();
      continue;
    }
    if (line.startsWith("msgid ")) {
      if (msgid && msgstr) commit();
      msgid = unquote(line.slice(6));
      while (lines[i + 1]?.startsWith('"')) msgid += unquote(lines[++i]);
    } else if (line.startsWith("msgstr ")) {
      msgstr = unquote(line.slice(7));
      while (lines[i + 1]?.startsWith('"')) msgstr += unquote(lines[++i]);
    } else if (line.startsWith("msgid_plural ")) {
      // Plural forms are skipped; the UI keys are singular.
      fuzzy = true;
    } else if (line.startsWith("msgstr[")) {
      fuzzy = true;
    }
  }
  commit();
  return map;
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return {};
  }
}

// Every locale file uses a region-qualified code (de-DE, fr-FR, ...), matching
// the naming of en-US / zh-CN. Bare .po codes are mapped here.
const CANONICAL = {
  ar: "ar-SA",
  da: "da-DK",
  de: "de-DE",
  el: "el-GR",
  es: "es-ES",
  fr: "fr-FR",
  it: "it-IT",
  nl: "nl-NL",
  ru: "ru-RU",
  sv: "sv-SE",
  tr: "tr-TR",
};

function normalizeCode(code) {
  const base = code.replace("@", "-").replace("_", "-");
  try {
    return Intl.getCanonicalLocales(base)[0];
  } catch {
    return code;
  }
}

const english = readJson(path.join(outputDir, "en-US.json"));
const files = fs.readdirSync(localesDir).filter((file) => file.endsWith(".po"));

// Only mainstream languages are exposed in the UI. The rest of the upstream
// catalogs would be mostly English because the modern UI uses new strings.
const KEEP = new Set([
  "ar-SA", "bg-BG", "cs-CZ", "da-DK", "de-DE", "el-GR", "es-ES", "fi-FI", "fr-FR",
  "hu-HU", "id-ID", "it-IT", "ja-JP", "ko-KR", "nb-NO", "nl-NL", "pl-PL", "pt-BR",
  "ru-RU", "sv-SE", "th-TH", "tr-TR", "uk-UA", "vi-VN", "zh-TW",
]);

const languages = [];
fs.mkdirSync(outputDir, { recursive: true });

for (const file of files) {
  const base = normalizeCode(path.basename(file, ".po"));
  const code = CANONICAL[base] ?? base;
  if (code === "zh-CN" || code === "en-US" || !KEEP.has(code)) continue;
  const catalog = parsePo(fs.readFileSync(path.join(localesDir, file), "utf8"));
  const normalized = new Map();
  for (const [id, translation] of catalog) {
    const key = normalizeText(id);
    if (key && !normalized.has(key)) normalized.set(key, translation);
  }
  const target = path.join(outputDir, `${code}.json`);
  // Keep translations that were added manually or in an earlier run.
  const translated = { ...readJson(target) };
  for (const [key, value] of Object.entries(english)) {
    const hit = catalog.get(value) ?? normalized.get(normalizeText(value));
    if (hit && hit.trim() && hit !== value && !hit.includes("%")) translated[key] = cleanTranslation(hit);
  }
  const count = Object.keys(translated).length;
  if (count === 0) continue;
  fs.writeFileSync(target, `${JSON.stringify(translated, null, 2)}\n`);
  languages.push({ code, file, translated: count });
}

languages.sort((a, b) => a.code.localeCompare(b.code));

const index = `// Generated by scripts/generate-locales.mjs. Do not edit by hand.
//
// Each file holds one language and uses the same semantic keys as
// en-US.json ("settings.theme"), so only the values differ.
//
// Only en-US (fallback) and zh-CN (default) are imported eagerly; every other
// language is loaded on demand by translate.ts, so switching or starting the
// app does not parse all catalogs.
import enUS from "./en-US.json";
import zhCN from "./zh-CN.json";

export interface LocaleInfo {
  code: string;
  file: string;
  translated: number;
}

export const eagerMessages: Record<string, Record<string, string>> = {
  "en-US": enUS,
  "zh-CN": zhCN,
};

export const localeLoaders: Record<string, () => Promise<Record<string, string>>> = {
${languages
  .map(
    (lang) =>
      `  ${JSON.stringify(lang.code)}: () => import("./${lang.code}.json").then((module) => module.default),`,
  )
  .join("\n")}
};

export const supportedLanguages: LocaleInfo[] = [
${languages
  .map(
    (lang) =>
      `  { code: ${JSON.stringify(lang.code)}, file: ${JSON.stringify(`${lang.code}.json`)}, translated: ${lang.translated} },`,
  )
  .join("\n")}
];
`;

fs.writeFileSync(path.join(outputDir, "index.ts"), index);
console.log(`mapped ${Object.keys(english).length} UI keys across ${languages.length} locales -> ${outputDir}`);
console.log(languages.map((lang) => `${lang.code}:${lang.translated}`).join("  "));
