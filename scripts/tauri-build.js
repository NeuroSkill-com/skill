#!/usr/bin/env node
/**
 * Tauri wrapper — pre-builds the espeak-ng static library for the current
 * platform before delegating to the Tauri CLI.
 *
 * Handles: dev, build (and passes everything else straight through).
 *
 * Usage (via npm — all standard Tauri flags work as normal):
 *   npm run tauri dev
 *   npm run tauri build
 *   npm run tauri build -- --debug
 *   npm run tauri build -- --target x86_64-pc-windows-gnu
 *   npm run tauri info
 *
 * Platform behaviour for `dev` and `build`:
 *   macOS         → bash scripts/build-espeak-static.sh
 *                   `build` also adds --target aarch64-apple-darwin --no-sign
 *   Windows MSVC  → PowerShell scripts\build-espeak-static.ps1
 *   Linux         → bash scripts/build-espeak-static.sh
 *   *-windows-gnu → bash scripts/build-espeak-static-mingw.sh
 *                   (cross-compile from Linux/macOS, or native MSYS2)
 */

import { execSync } from "child_process";
import { platform } from "os";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

const isMac = platform() === "darwin";
const isWin = platform() === "win32";

// ── Parse arguments ───────────────────────────────────────────────────────────
// argv: ["node", "tauri-build.js", subcommand?, ...rest]
const [subcommand = "", ...subArgs] = process.argv.slice(2);

// Subcommands that need espeak pre-built before Tauri runs.
const needsEspeak = subcommand === "dev" || subcommand === "build";

// ── Pass-through for subcommands that don't need espeak ───────────────────────
if (!needsEspeak) {
  const cmd = ["npx", "tauri", subcommand, ...subArgs]
    .filter(Boolean)
    .join(" ");
  execSync(cmd, { cwd: root, stdio: "inherit" });
  process.exit(0);
}

// ── Parse --target from subArgs ───────────────────────────────────────────────
let explicitTarget = null;
for (let i = 0; i < subArgs.length; i++) {
  if (subArgs[i] === "--target" && i + 1 < subArgs.length) {
    explicitTarget = subArgs[i + 1];
    break;
  }
  if (subArgs[i].startsWith("--target=")) {
    explicitTarget = subArgs[i].slice("--target=".length);
    break;
  }
}

const isMingwTarget = explicitTarget?.endsWith("-windows-gnu") ?? false;

// ── Pre-build espeak-ng and resolve ESPEAK_LIB_DIR ───────────────────────────
let espeakLib;
let platformFlags = []; // extra flags injected before the user's subArgs

if (isMingwTarget) {
  // MinGW cross-compilation — works from Linux, macOS, or MSYS2 on Windows.
  console.log(
    `→ building espeak-ng static library (MinGW) for ${explicitTarget} …`
  );
  execSync("bash scripts/build-espeak-static-mingw.sh", {
    cwd: root,
    stdio: "inherit",
  });
  espeakLib = resolve(root, "src-tauri/espeak-static-mingw/lib");

} else if (isMac) {
  console.log("→ building espeak-ng static library …");
  execSync("bash scripts/build-espeak-static.sh", {
    cwd: root,
    stdio: "inherit",
  });
  espeakLib = resolve(root, "src-tauri/espeak-static/lib");
  // Release builds target Apple Silicon; dev builds use the host triple.
  if (subcommand === "build" && !explicitTarget) {
    platformFlags = ["--target", "aarch64-apple-darwin", "--no-sign"];
  }

} else if (isWin) {
  // Native Windows — MSVC toolchain via PowerShell.
  // Must run from a Developer PowerShell for VS so lib.exe is on PATH.
  console.log("→ building espeak-ng static library (MSVC) …");
  execSync(
    "powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\build-espeak-static.ps1",
    { cwd: root, stdio: "inherit" }
  );
  espeakLib = resolve(root, "src-tauri\\espeak-static\\lib");

} else {
  // Linux native.
  console.log("→ building espeak-ng static library …");
  execSync("bash scripts/build-espeak-static.sh", {
    cwd: root,
    stdio: "inherit",
  });
  espeakLib = resolve(root, "src-tauri/espeak-static/lib");
}

// ── Run Tauri ─────────────────────────────────────────────────────────────────
const cmd = ["npx", "tauri", subcommand, ...platformFlags, ...subArgs]
  .join(" ")
  .trimEnd();

console.log(`→ ${cmd}`);
execSync(cmd, {
  cwd: root,
  stdio: "inherit",
  env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
});
