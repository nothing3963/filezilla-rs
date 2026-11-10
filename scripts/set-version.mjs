#!/usr/bin/env node
// Bumps the app version.  tauri.conf.json is the authority (see
// scripts/get-version.mjs); the package.json entry is only a mirror kept in
// sync here so tooling and humans never see two different numbers.
// src-tauri/Cargo.toml has no version at all, so it cannot drift.
//
// Usage:
//   pnpm version:set 0.2.0
//   pnpm version:set 0.2.0 --dry-run
import { readFileSync, writeFileSync } from "node:fs";

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const target = args.find((arg) => arg !== "--dry-run");

if (!target || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(target)) {
  console.error("usage: pnpm version:set <major.minor.patch>   e.g. pnpm version:set 0.2.0");
  process.exit(1);
}

const configPath = new URL("../src-tauri/tauri.conf.json", import.meta.url);
const packagePath = new URL("../package.json", import.meta.url);

// --- tauri.conf.json: the authority.  The top-level field sits at two-space
// indentation; nested version keys (wix/nsis) are deeper and never match.
const configText = readFileSync(configPath, "utf8");
const configMatch = configText.match(/^ {2}"version":\s*"([^"]+)"/m);
if (!configMatch) {
  console.error('[version] no top-level "version" found in src-tauri/tauri.conf.json');
  process.exit(1);
}
const nextConfigText = configText.replace(
  configMatch[0],
  `  "version": "${target}"`
);

// --- package.json: decorative mirror.
const pkg = readFileSync(packagePath, "utf8");
const pkgMatch = pkg.match(/^(\s*"version"\s*:\s*")([^"]+)("\s*,?)/m);
if (!pkgMatch) {
  console.error('[version] no "version" field found in package.json');
  process.exit(1);
}
const nextPkg = pkg.replace(pkgMatch[0], `${pkgMatch[1]}${target}${pkgMatch[3]}`);

console.log(`[version] tauri.conf.json (authority): ${configMatch[1]} -> ${target}`);
console.log(`[version] package.json (mirror):       ${pkgMatch[2]} -> ${target}`);

if (dryRun) {
  console.log("[version] dry run, nothing written");
  process.exit(0);
}

writeFileSync(configPath, nextConfigText);
writeFileSync(packagePath, nextPkg);
console.log("[version] done. Now run .\\build-win.cmd (or pnpm tauri build); no other file carries a version.");
