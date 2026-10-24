#!/usr/bin/env node
/**
 * Extract the inline fixtures from FileZilla's C++ directory-listing test
 * (tests/dirparsertest.cpp) into JSON that the Rust test suite can consume.
 *
 * Usage:
 *   node scripts/extract-cpp-listing-tests.mjs <path/to/dirparsertest.cpp> <out.json>
 */
import fs from "node:fs";

const [, , inputPath, outputPath] = process.argv;
if (!inputPath || !outputPath) {
  console.error("usage: extract-cpp-listing-tests.mjs <dirparsertest.cpp> <out.json>");
  process.exit(1);
}
const source = fs.readFileSync(inputPath, "utf8");

function skipSpace(text, index) {
  while (index < text.length && /\s/.test(text[index])) index++;
  return index;
}

function readStringLiteral(text, index) {
  let i = index;
  if (text[i] === "L" || text[i] === "u" || text[i] === "U") i++;
  if (text[i] !== '"') throw new Error(`expected string literal at ${index}`);
  i++;
  const bytes = [];
  while (i < text.length && text[i] !== '"') {
    let ch = text[i];
    if (ch !== "\\") {
      for (const byte of Buffer.from(ch, "utf8")) bytes.push(byte);
      i++;
      continue;
    }
    i++;
    ch = text[i++];
    switch (ch) {
      case "n": bytes.push(10); break;
      case "r": bytes.push(13); break;
      case "t": bytes.push(9); break;
      case "0": bytes.push(0); break;
      case "\\": bytes.push(92); break;
      case '"': bytes.push(34); break;
      case "'": bytes.push(39); break;
      case "x": {
        let hex = "";
        while (i < text.length && /[0-9a-fA-F]/.test(text[i])) hex += text[i++];
        bytes.push(parseInt(hex || "0", 16) & 0xff);
        break;
      }
      case "u":
      case "U": {
        const length = ch === "u" ? 4 : 8;
        const hex = text.slice(i, i + length);
        i += length;
        for (const byte of Buffer.from(String.fromCodePoint(parseInt(hex, 16)), "utf8")) bytes.push(byte);
        break;
      }
      default:
        bytes.push(ch.charCodeAt(0));
    }
  }
  return { value: Buffer.from(bytes).toString("utf8"), next: i + 1 };
}

function readStringSequence(text, index) {
  let i = skipSpace(text, index);
  let value = "";
  let found = false;
  while (text[i] === '"' || ((text[i] === "L" || text[i] === "u" || text[i] === "U") && text[i + 1] === '"')) {
    const literal = readStringLiteral(text, i);
    value += literal.value;
    found = true;
    i = skipSpace(text, literal.next);
  }
  return found ? { value, next: i } : null;
}

function extractParenthesized(text, start) {
  // start points at '('
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    const ch = text[i];
    if (ch === '"') {
      const literal = readStringLiteral(text, i);
      i = literal.next - 1;
      continue;
    }
    if (ch === "(") depth++;
    else if (ch === ")") {
      depth--;
      if (depth === 0) return text.slice(start + 1, i);
    }
  }
  throw new Error("unbalanced parentheses");
}

function splitTopLevel(text) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (ch === '"') {
      const literal = readStringLiteral(text, i);
      i = literal.next - 1;
      continue;
    }
    if (ch === "(" || ch === "{" || ch === "[") depth++;
    else if (ch === ")" || ch === "}" || ch === "]") depth--;
    else if (ch === "," && depth === 0) {
      parts.push(text.slice(start, i).trim());
      start = i + 1;
    }
  }
  const tail = text.slice(start).trim();
  if (tail) parts.push(tail);
  return parts;
}

function calculateYear(month, day, now = new Date()) {
  const currentYear = now.getFullYear();
  const currentMonth = now.getMonth() + 1;
  const currentDay = now.getDate();
  const dayOfYear = month * 31 + day;
  const currentDayOfYear = currentMonth * 31 + currentDay;
  return dayOfYear > currentDayOfYear + 1 ? currentYear - 1 : currentYear;
}

function parseDateTime(expression) {
  const match = expression.match(/fz::datetime\s*\((.*)\)/s);
  if (!match) return null;
  const args = splitTopLevel(match[1]);
  if (args.length < 4) return null;
  const numeric = args.slice(1).map((arg) => {
    const calc = arg.match(/calcYear\s*\(\s*(\d+)\s*,\s*(\d+)\s*\)/);
    if (calc) return calculateYear(Number(calc[1]), Number(calc[2]));
    return Number(arg);
  });
  const [year, month, day, hour = 0, minute = 0, second = 0] = numeric;
  if (![year, month, day].every(Number.isFinite)) return null;
  return {
    year, month, day, hour, minute, second,
    iso: `${String(year).padStart(4, "0")}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}T${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}:${String(second).padStart(2, "0")}Z`,
  };
}

function extractLiteral(field) {
  if (!field) return null;
  const index = field.search(/L?"/);
  if (index < 0) return null;
  return readStringSequence(field, index)?.value ?? null;
}

const cases = [];
const skipped = [];
const marker = "m_entries.emplace_back(t_entry({";
let cursor = 0;
while (true) {
  const start = source.indexOf(marker, cursor);
  if (start < 0) break;
  const open = source.indexOf("{", start) + 1;
  // find matching close for the t_entry initializer
  let depth = 1;
  let end = open;
  for (; end < source.length; end++) {
    const ch = source[end];
    if (ch === '"') {
      const literal = readStringLiteral(source, end);
      end = literal.next - 1;
      continue;
    }
    if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) break;
    }
  }
  const body = source.slice(open, end);
  cursor = end + 1;

  // First string sequence before the first inner '{' is the raw line.
  const innerOpen = body.indexOf("{");
  const head = body.slice(0, innerOpen).replace(/\/\/[^\n]*/g, "");
  const line = readStringSequence(head, 0)?.value;
  if (line === undefined) {
    skipped.push({ at: start, preview: body.slice(0, 160).replace(/\s+/g, " ") });
    continue;
  }

  let innerDepth = 1;
  let innerEnd = innerOpen + 1;
  for (; innerEnd < body.length; innerEnd++) {
    const ch = body[innerEnd];
    if (ch === '"') {
      const literal = readStringLiteral(body, innerEnd);
      innerEnd = literal.next - 1;
      continue;
    }
    if (ch === "{") innerDepth++;
    else if (ch === "}") {
      innerDepth--;
      if (innerDepth === 0) break;
    }
  }
  const fields = splitTopLevel(body.slice(innerOpen + 1, innerEnd));
  const tail = body.slice(innerEnd + 1);
  const serverType = tail.match(/\b(UNIX|DOS|MVS|HPNONSTOP|ZVM|VMS|EPLF|OTHER)\b/)?.[1] ?? "UNIX";

  const [nameField, sizeField, permissionField, ownerField, linkField, timeField, flagField] = fields;
  cases.push({
    line,
    name: extractLiteral(nameField),
    size: Number(sizeField ?? 0),
    permissions: extractLiteral(permissionField),
    owner: extractLiteral(ownerField),
    link: extractLiteral(linkField),
    modified: parseDateTime(timeField ?? "")?.iso ?? null,
    is_dir: /flag_dir/.test(flagField ?? ""),
    is_link: /flag_link/.test(flagField ?? ""),
    is_unsure: /flag_unsure/.test(flagField ?? ""),
    server_type: serverType,
    raw_flags: (flagField ?? "").trim(),
  });
}

fs.writeFileSync(outputPath, JSON.stringify({ source: inputPath, cases }, null, 2));
console.log(`extracted ${cases.length} cases -> ${outputPath}`);
if (skipped.length) console.log(`skipped ${skipped.length} block(s):`, skipped);
