#!/usr/bin/env node
/**
 * Cross-platform Tauri build wrapper.
 *
 * On macOS           → always builds for aarch64-apple-darwin (Apple Silicon).
 *                      Runs build-espeak-static.sh first.
 * On Windows (MSVC)  → builds for the host triple (x86_64-pc-windows-msvc).
 *                      Runs build-espeak-static.ps1 first.
 * On Linux           → builds for the host triple (x86_64-unknown-linux-gnu).
 *                      Runs build-espeak-static.sh first.
 *
 * MinGW cross-compilation (any host):
 *   npm run tauri:build -- --target x86_64-pc-windows-gnu
 *   Runs build-espeak-static-mingw.sh and passes --target to tauri build.
 *   Requires: x86_64-w64-mingw32-gcc (Linux/macOS) or MSYS2 MinGW on Windows.
 *
 * Usage (via npm):
 *   npm run tauri:build
 *   npm run tauri:build -- --target x86_64-pc-windows-gnu
 *
 * Usage (direct):
 *   node scripts/tauri-build.js [extra tauri-cli flags …]
 *   node scripts/tauri-build.js --debug
 *   node scripts/tauri-build.js --target x86_64-pc-windows-gnu
 */

import { execSync } from "child_process";
import { platform } from "os";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

const isMac = platform() === "darwin";
const isWin = platform() === "win32";

// ── Parse --target from the argument list ────────────────────────────────────
// All args (including --target) are forwarded to `npx tauri build` via `extra`.
// We also inspect the target to decide which espeak library to pre-build.
const args = process.argv.slice(2);
const extra = args.join(" ");

let explicitTarget = null;
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--target" && i + 1 < args.length) {
    explicitTarget = args[i + 1];
    break;
  }
  if (args[i].startsWith("--target=")) {
    explicitTarget = args[i].slice("--target=".length);
    break;
  }
}

// Any *-windows-gnu triple triggers the MinGW build path regardless of host.
const isMingwTarget = explicitTarget?.endsWith("-windows-gnu") ?? false;

// ── MinGW cross-compilation ───────────────────────────────────────────────────
// Triggered by --target *-windows-gnu from any host:
//   Linux/macOS: uses x86_64-w64-mingw32-* cross-compiler
//   MSYS2:       uses the native MinGW toolchain (no cross prefix)
if (isMingwTarget) {
  console.log(`→ building espeak-ng MinGW static library for ${explicitTarget} …`);
  execSync("bash scripts/build-espeak-static-mingw.sh", {
    cwd: root,
    stdio: "inherit",
  });

  const espeakLib = resolve(root, "src-tauri/espeak-static-mingw/lib");
  const cmd = `npx tauri build ${extra}`.trimEnd();
  console.log(`→ ${cmd}`);
  execSync(cmd, {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
  });

// ── macOS native ──────────────────────────────────────────────────────────────
} else if (isMac) {
  console.log("→ building espeak-ng static library …");
  execSync("bash scripts/build-espeak-static.sh", {
    cwd: root,
    stdio: "inherit",
  });

  const espeakLib = resolve(root, "src-tauri/espeak-static/lib");
  const cmd =
    `npx tauri build --target aarch64-apple-darwin --no-sign ${extra}`.trimEnd();
  console.log(`→ ${cmd}`);
  execSync(cmd, {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
  });

// ── Windows native (MSVC) ─────────────────────────────────────────────────────
} else if (isWin) {
  // Must run from a Developer PowerShell for VS (lib.exe in PATH) so that
  // the MSVC companion-library merge step works.
  console.log("→ building espeak-ng static library (MSVC / PowerShell) …");
  execSync(
    "powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\build-espeak-static.ps1",
    { cwd: root, stdio: "inherit" }
  );

  const espeakLib = resolve(root, "src-tauri\\espeak-static\\lib");
  const cmd = `npx tauri build ${extra}`.trimEnd();
  console.log(`→ ${cmd}`);
  execSync(cmd, {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
  });

// ── Linux native ──────────────────────────────────────────────────────────────
} else {
  console.log("→ building espeak-ng static library …");
  execSync("bash scripts/build-espeak-static.sh", {
    cwd: root,
    stdio: "inherit",
  });

  const espeakLib = resolve(root, "src-tauri/espeak-static/lib");
  const cmd = `npx tauri build ${extra}`.trimEnd();
  console.log(`→ ${cmd}`);
  execSync(cmd, {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
  });
}
