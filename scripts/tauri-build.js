#!/usr/bin/env node
/**
 * Cross-platform Tauri build wrapper.
 *
 * On macOS  → always builds for aarch64-apple-darwin (Apple Silicon only).
 *             Runs build-espeak-static.sh first (required by kittentts / neutts).
 * On Windows → builds for the host triple (x86_64-pc-windows-msvc).
 * On Linux   → builds for the host triple (x86_64-unknown-linux-gnu).
 *
 * Usage (via npm):
 *   npm run tauri:build
 *
 * Usage (direct):
 *   node scripts/tauri-build.js [extra tauri-cli flags …]
 *   node scripts/tauri-build.js --debug
 */

import { execSync } from "child_process";
import { platform } from "os";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

const isMac = platform() === "darwin";
const isWin = platform() === "win32";

const extra = process.argv.slice(2).join(" ");

if (isMac) {
  // Build espeak static lib (no-op if already built)
  console.log("→ building espeak-ng static library …");
  execSync("bash scripts/build-espeak-static.sh", {
    cwd: root,
    stdio: "inherit",
  });

  const espeakLib = resolve(root, "src-tauri/espeak-static/lib");
  const cmd = `npx tauri build --target aarch64-apple-darwin --no-sign ${extra}`.trimEnd();
  console.log(`→ ${cmd}`);
  execSync(cmd, {
    cwd: root,
    stdio: "inherit",
    env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
  });
} else {
  // Windows / Linux — build for the host triple, no espeak static needed
  const cmd = `npx tauri build ${extra}`.trimEnd();
  console.log(`→ ${cmd}`);
  execSync(cmd, { cwd: root, stdio: "inherit" });
}
