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

  // Ensure the Vulkan SDK is present before building (required by llm-vulkan).
  // The script is a no-op when the SDK is already installed.
  console.log("→ ensuring Vulkan SDK is installed …");
  execSync(
    "powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\install-vulkan-sdk.ps1",
    { cwd: root, stdio: "inherit" }
  );

  console.log("→ building espeak-ng static library (MSVC) …");
  execSync(
    "powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\build-espeak-static.ps1",
    { cwd: root, stdio: "inherit" }
  );
  espeakLib = resolve(root, "src-tauri\\espeak-static\\lib");

  // ── Windows: skip Tauri bundling for `build` subcommand ────────────────────
  //
  // The Tauri CLI (≥ 2.10, NAPI-RS native module) crashes with
  // STATUS_ILLEGAL_INSTRUCTION (0xC000_001D) on Windows during the
  // post-compilation bundle/updater-artifact phase.  The crash happens after
  // "Built application at:" is printed and is triggered by the
  // `createUpdaterArtifacts: true` + `"targets": ["app"]` combination:
  //
  //  • "app" is a macOS-only bundle format — Tauri skips it on Windows.
  //  • With no valid Windows bundle produced, the CLI falls through to the
  //    updater-artifact signing / zstd-compression code path.
  //  • That code path in cli.win32-x64-msvc.node uses CPU instructions
  //    (AVX2 or similar) that are not available on all x86-64 processors,
  //    crashing the entire Node.js process.
  //
  // --no-bundle tells Tauri to compile the Rust binary and stop; it skips
  // all installer creation AND the updater-artifact signing step, so the
  // crash never occurs.  The compiled skill.exe is still produced at:
  //   src-tauri\target\release\skill.exe
  //
  // Full Windows packaging (NSIS installer + updater ZIP + signing) is
  // handled separately by release-windows.ps1, which calls
  //   cargo build --release
  //   npx tauri bundle --bundle nsis --no-sign
  // directly — entirely bypassing this code path.
  //
  // Only inject the flag when the caller has not already explicitly passed
  // a --bundle or --no-bundle argument themselves.
  if (
    subcommand === "build" &&
    !subArgs.includes("--bundle") &&
    !subArgs.includes("--no-bundle")
  ) {
    platformFlags = ["--no-bundle"];
    console.log(
      "→ Windows: injecting --no-bundle (skips post-build signing crash; " +
      "use release-windows.ps1 for full NSIS packaging)"
    );
  }

  // ── Windows: enable Vulkan GPU offloading for LLM inference ────────────────
  //
  // Without an explicit GPU feature flag llama-cpp-4 compiles in CPU-only
  // mode.  Vulkan is the broadest Windows GPU backend — it covers NVIDIA,
  // AMD, and Intel Arc GPUs without requiring vendor-specific SDKs (no CUDA
  // toolkit, no ROCm install needed at build time beyond the Vulkan SDK /
  // headers that ship with the Windows SDK and most GPU driver packages).
  //
  // The Vulkan SDK (https://vulkan.lunarg.com) must be installed so that
  // the CMake find-module inside llama.cpp can locate the Vulkan headers and
  // the vulkan-1.lib import library.  At runtime, any Vulkan-capable GPU
  // driver works; llama.cpp falls back to CPU automatically if no Vulkan
  // device is found.
  //
  // Only inject the flag when the caller hasn't already passed --features.
  if (!subArgs.includes("--features")) {
    platformFlags = [...platformFlags, "--features", "llm-vulkan"];
    console.log(
      "→ Windows: injecting --features llm-vulkan (Vulkan GPU offloading for LLM)"
    );
  }

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
