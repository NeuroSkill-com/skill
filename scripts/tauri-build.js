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
import { platform, cpus } from "os";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { readFileSync } from "fs";

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

  // The install script sets $env:VULKAN_SDK inside its own child process, but
  // that env var dies when the child exits.  Re-detect the SDK root here and
  // inject it into process.env so cargo / CMake (grandchildren of this process)
  // can find it without a shell restart.
  if (!process.env.VULKAN_SDK) {
    // Mirror the detection order used by install-vulkan-sdk.ps1:
    //   1. Machine-level env var written by the LunarG installer
    //   2. Registry key written by the LunarG installer
    //   3. Newest versioned directory under C:\VulkanSDK\
    const detectPs = `
$p = [System.Environment]::GetEnvironmentVariable('VULKAN_SDK','Machine')
if (-not $p) {
  foreach ($reg in @('HKLM:\\SOFTWARE\\LunarG\\Vulkan SDK','HKLM:\\SOFTWARE\\WOW6432Node\\LunarG\\Vulkan SDK')) {
    if (Test-Path $reg) {
      $ip = (Get-ItemProperty $reg -ErrorAction SilentlyContinue).InstallPath
      if ($ip -and (Test-Path (Join-Path $ip 'Include\\vulkan\\vulkan.h'))) { $p = $ip; break }
    }
  }
}
if (-not $p -and (Test-Path 'C:\\VulkanSDK')) {
  $latest = Get-ChildItem 'C:\\VulkanSDK' -Directory | Sort-Object Name -Descending | Select-Object -First 1
  if ($latest -and (Test-Path (Join-Path $latest.FullName 'Include\\vulkan\\vulkan.h'))) { $p = $latest.FullName }
}
if ($p) { Write-Output $p }
`.trim().replace(/\n/g, " ");

    try {
      const detected = execSync(
        `powershell -NoProfile -Command "${detectPs}"`,
        { cwd: root }
      ).toString().trim();
      if (detected) {
        process.env.VULKAN_SDK = detected;
        console.log(`→ VULKAN_SDK detected and set: ${detected}`);
      }
    } catch {
      // Non-fatal — if detection fails, cargo will surface the missing SDK.
    }
  }

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

  // Ensure the Vulkan SDK (headers + loader + glslc) is present before
  // building.  The script is a no-op when the packages are already installed,
  // so repeated `npm run tauri dev` calls are cheap.
  console.log("→ ensuring Vulkan SDK is installed …");
  execSync("bash scripts/install-vulkan-sdk.sh", {
    cwd: root,
    stdio: "inherit",
  });

  console.log("→ building espeak-ng static library …");
  execSync("bash scripts/build-espeak-static.sh", {
    cwd: root,
    stdio: "inherit",
  });
  espeakLib = resolve(root, "src-tauri/espeak-static/lib");

  // ── Linux: enable Vulkan GPU offloading for LLM inference ────────────────
  //
  // Vulkan is the broadest Linux GPU backend: it covers NVIDIA (via the
  // official driver's Vulkan ICD), AMD (RADV in Mesa or the AMDVLK driver),
  // and Intel Arc (ANV in Mesa) without requiring CUDA or ROCm at build time.
  // llama.cpp falls back to CPU automatically if no Vulkan-capable device is
  // found at runtime, so the binary is safe to ship on headless machines.
  //
  // cmake's FindVulkan module locates headers and the loader via pkg-config
  // on Linux -- no VULKAN_SDK env var needs to be set (unlike Windows).
  //
  // Only inject the flag when the caller hasn't already passed --features.
  if (!subArgs.includes("--features")) {
    platformFlags = [...platformFlags, "--features", "llm-vulkan"];
    console.log(
      "→ Linux: injecting --features llm-vulkan (Vulkan GPU offloading for LLM)"
    );
  }
}

// ── Parallelism cap for Alpine / musl ─────────────────────────────────────────
//
// On Alpine Linux (musl libc), running many Cargo compilation jobs in parallel
// can exhaust container memory.  When a crate compilation is killed by the OOM
// reaper its artifact is never written, and every dependent crate then fails
// with a cascading "E0463: can't find crate for X" error — the actual OOM kill
// is not surfaced in the Rust error output, making the root cause invisible.
//
// Symptoms that point to this: errors like "can't find crate for `yoke`" inside
// zerovec, or similar E0463 cascades for otherwise-pure-Rust crates that compile
// fine in isolation (cargo build -p yoke succeeds; the full build does not).
//
// Capping jobs at the number of logical CPUs (or CARGO_BUILD_JOBS if already set
// by the caller) keeps peak RSS manageable.  On well-resourced machines this has
// no measurable effect; on memory-constrained Alpine CI containers it is the
// difference between a clean build and a mysterious cascade failure.
//
// To override, set CARGO_BUILD_JOBS before calling npm run tauri build:
//   CARGO_BUILD_JOBS=8 npm run tauri build
if (!isWin && !isMac && !process.env.CARGO_BUILD_JOBS) {
  let onAlpine = false;
  try {
    onAlpine = readFileSync("/etc/os-release", "utf8").includes("ID=alpine");
  } catch { /* not on Alpine or /etc/os-release unreadable */ }

  if (onAlpine) {
    process.env.CARGO_BUILD_JOBS = String(cpus().length);
    console.log(
      `→ Alpine Linux detected: capping Cargo parallelism at ${process.env.CARGO_BUILD_JOBS} job(s)` +
      ` to prevent OOM-induced cascade errors (set CARGO_BUILD_JOBS to override)`
    );
  }
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
