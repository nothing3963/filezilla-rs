/**
 * Sandbox shim for Vite's Windows network-drive detection.
 *
 * Vite runs `net use` through `child_process.exec` the first time it resolves
 * a path.  In the DSH Windows sandbox, capturing a child process's stdout is
 * denied (EPERM), which aborts the build even though the result is irrelevant
 * for a local workspace.  This shim short-circuits that one probe.
 *
 * It is only used for local verification, never by the app at runtime:
 *   NODE_OPTIONS="--import ./scripts/vite-sandbox-shim.mjs" pnpm build
 */
import cp from "node:child_process";
import { syncBuiltinESMExports } from "node:module";

const originalExec = cp.exec;

cp.exec = function patchedExec(command, options, callback) {
  const args = [...arguments];
  let cb = callback;
  let opts = options;
  if (typeof options === "function") {
    cb = options;
    opts = undefined;
  }
  if (typeof command === "string" && command.trim().toLowerCase() === "net use") {
    if (typeof cb === "function") process.nextTick(() => cb(null, "", ""));
    return { on() {}, once() {}, kill() {}, stdout: null, stderr: null };
  }
  return originalExec.apply(this, args);
};

syncBuiltinESMExports();

export {};
