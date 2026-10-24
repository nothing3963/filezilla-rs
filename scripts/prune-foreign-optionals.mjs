/**
 * pnpm's hoisted linker installs optional native binaries for every platform.
 * Several tools (notably @tailwindcss/oxide) probe those packages by name and
 * fail on a Mach-O/ELF binary before reaching the current platform's build.
 *
 * Keep only packages whose `os`/`cpu` metadata matches this machine.  This runs
 * from `postinstall` and is a no-op when the package manager already filtered
 * optional dependencies.
 */
import fs from "node:fs";
import path from "node:path";

const nodeModules = path.resolve("node_modules");
const scopes = ["@tailwindcss", "@esbuild", "@rolldown", "@oxc-project", "@rollup"];
const removed = [];

for (const scope of scopes) {
  const scopeDir = path.join(nodeModules, scope);
  if (!fs.existsSync(scopeDir)) continue;
  for (const name of fs.readdirSync(scopeDir)) {
    const dir = path.join(scopeDir, name);
    const manifest = path.join(dir, "package.json");
    if (!fs.existsSync(manifest)) continue;
    try {
      const pkg = JSON.parse(fs.readFileSync(manifest, "utf8"));
      const osMismatch = Array.isArray(pkg.os) && !pkg.os.includes(process.platform);
      const cpuMismatch = Array.isArray(pkg.cpu) && !pkg.cpu.includes(process.arch);
      if (osMismatch || cpuMismatch) {
        fs.rmSync(dir, { recursive: true, force: true });
        removed.push(`${scope}/${name}`);
      }
    } catch {
      // Ignore malformed optional packages; they are not used by the app.
    }
  }
}

if (removed.length > 0) {
  console.log(`[filezilla-rs] pruned ${removed.length} foreign optional package(s):`);
  for (const name of removed) console.log(`  - ${name}`);
}
