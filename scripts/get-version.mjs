#!/usr/bin/env node
// Prints the app version.  The single source of truth is the top-level
// "version" in src-tauri/tauri.conf.json:
//   * Tauri uses it directly for bundle names, NSIS metadata and the Windows
//     exe file resources (tauri-build only overrides them from the config),
//   * the Rust code reports it through app.package_info().version, which the
//     codegen resolves from the same config field,
//   * src-tauri/Cargo.toml deliberately declares NO version, so there is no
//     second source to drift; the package.json entry is a mirror kept in sync
//     by scripts/set-version.mjs and read by no shipped artifact.
//
// Usage: node scripts/get-version.mjs        -> 0.1.2
import { readFileSync } from "node:fs";

const configPath = new URL("../src-tauri/tauri.conf.json", import.meta.url);
const config = JSON.parse(readFileSync(configPath, "utf8"));

if (typeof config.version !== "string" || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(config.version)) {
  console.error('[version] src-tauri/tauri.conf.json must declare a top-level "version" like "0.1.2"');
  process.exit(1);
}

console.log(config.version);
