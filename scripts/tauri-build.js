#!/usr/bin/env node
/**
 * Tauri wrapper — handles platform-specific setup (Vulkan SDK, linker
 * flags, macOS target) before delegating to the Tauri CLI.
 *
 * Handles: dev, build (and passes everything else straight through).
 *
 * Usage (via npm — all standard Tauri flags work as normal):
 *   npm run tauri dev
 *   npm run tauri build
 *   npm run tauri build -- --debug
 *   npm run tauri build -- --target x86_64-pc-windows-gnu
 *   npm run tauri info
 */

import { execSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { createConnection } from "node:net";
import { arch, cpus, platform, tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

function runMarkdownRendererGuard() {
  execSync("node scripts/check-markdown-renderer.js", {
    cwd: root,
    stdio: "inherit",
  });
}

const isMac = platform() === "darwin";
const isWin = platform() === "win32";
const isLinux = platform() === "linux";

function commandExists(cmd) {
  try {
    const check = isWin ? `where ${cmd}` : `command -v ${cmd}`;
    execSync(check, { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function linuxTrayRuntimeLooksPresent() {
  if (!commandExists("ldconfig")) return true;
  try {
    const out = execSync("ldconfig -p", { encoding: "utf8" });
    const hasAyatana = /libayatana-appindicator3\.so(?:\.1)?\b/.test(out);
    const hasLegacy = /libappindicator3\.so(?:\.1)?\b/.test(out);
    return hasAyatana || hasLegacy;
  } catch {
    return true;
  }
}

function _linuxInstallHintForTrayRuntime() {
  if (commandExists("apt-get")) {
    return [
      "  sudo apt update",
      "  sudo apt install -y libayatana-appindicator3-1",
      "  # fallback package on some distros:",
      "  # sudo apt install -y libappindicator3-1",
    ].join("\n");
  }
  if (commandExists("dnf")) {
    return [
      "  sudo dnf install -y libappindicator-gtk3",
      "  # fallback on some distros:",
      "  # sudo dnf install -y libayatana-appindicator-gtk3",
    ].join("\n");
  }
  if (commandExists("pacman")) {
    return "  sudo pacman -S --needed libayatana-appindicator";
  }
  if (commandExists("zypper")) {
    return "  sudo zypper install -y libayatana-appindicator3-1";
  }
  return "  Install a package that provides libayatana-appindicator3.so.1 (or libappindicator3.so.1)";
}

function ensureLinuxTrayRuntimeForDev() {
  if (!isLinux || subcommand !== "dev") return;
  if (linuxTrayRuntimeLooksPresent()) return;
  process.exit(1);
}

function parseExplicitBundleArg(args) {
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if ((arg === "--bundle" || arg === "--bundles") && i + 1 < args.length) {
      return {
        index: i,
        consumesNext: true,
        value: args[i + 1],
      };
    }
    if (arg.startsWith("--bundle=") || arg.startsWith("--bundles=")) {
      return {
        index: i,
        consumesNext: false,
        value: arg.slice(arg.indexOf("=") + 1),
      };
    }
  }
  return null;
}

function splitBundleTargets(rawValue) {
  if (!rawValue) return [];
  return rawValue
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

function removeBundleArg(args, parsedBundleArg) {
  if (!parsedBundleArg) return [...args];
  if (parsedBundleArg.consumesNext) {
    return args.filter((_, idx) => idx !== parsedBundleArg.index && idx !== parsedBundleArg.index + 1);
  }
  return args.filter((_, idx) => idx !== parsedBundleArg.index);
}

// ── Parse arguments ───────────────────────────────────────────────────────────
// argv: ["node", "tauri-build.js", subcommand?, ...rest]
const [subcommand = "", ...subArgs] = process.argv.slice(2);

// Subcommands that need platform setup before Tauri runs.
const needsSetup = subcommand === "dev" || subcommand === "build";

// ── Pass-through for subcommands that don't need setup ───────────────────────
if (!needsSetup) {
  const passCmd =
    process.env.TAURI_USE_NPX !== "1" && commandExists("cargo-tauri") ? ["cargo", "tauri"] : ["npx", "tauri"];
  const cmd = [...passCmd, subcommand, ...subArgs].filter(Boolean).join(" ");
  execSync(cmd, { cwd: root, stdio: "inherit" });
  process.exit(0);
}

runMarkdownRendererGuard();

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

// Detect whether caller explicitly requested bundling behavior.
// Tauri accepts both:
//   --bundle <targets>
//   --bundles <targets>
// and their --flag=value forms.
const hasExplicitBundleArg = subArgs.some(
  (arg) =>
    arg === "--bundle" ||
    arg === "--bundles" ||
    arg === "--no-bundle" ||
    arg.startsWith("--bundle=") ||
    arg.startsWith("--bundles="),
);

// ── Linux preflight: prevent accidental cross-target trap ───────────────────
//
// On Linux, forcing an x86_64 target from an ARM host (or vice versa) triggers
// pkg-config based sys crates (glib-sys, gobject-sys, etc.) to fail unless a
// full cross sysroot/toolchain is configured. Most local builds intend native
// host output, so fail fast with an actionable message.
if (isLinux && explicitTarget?.endsWith("-unknown-linux-gnu") && process.env.ALLOW_LINUX_CROSS !== "1") {
  const hostArchMap = {
    x64: "x86_64",
    arm64: "aarch64",
  };
  const rustHostArch = hostArchMap[arch()];

  if (rustHostArch) {
    const nativeTarget = `${rustHostArch}-unknown-linux-gnu`;
    if (explicitTarget !== nativeTarget) {
      process.exit(1);
    }
  }
}

// ── Platform-specific setup ──────────────────────────────────────────────────
//
// espeak-ng is now a pure Rust dependency (espeak-ng crate) — no C build
// scripts needed.  This section handles platform flags, Vulkan SDK, and
// macOS/Windows/Linux quirks.

let platformFlags = []; // extra flags injected before the user's subArgs

if (isMingwTarget) {
  // MinGW cross-compilation — no special setup needed.
} else if (isMac) {
  // Release builds target Apple Silicon; dev builds use the host triple.
  if (subcommand === "build" && !explicitTarget) {
    platformFlags = ["--target", "aarch64-apple-darwin", "--no-sign"];
  }

  // ── macOS: skip Tauri bundling for default local builds ──────────────────
  //
  // On recent macOS runners/hosts, the Tauri CLI can crash in the
  // post-compilation bundle/updater-artifact phase even when Rust compilation
  // succeeds. `--no-bundle` keeps local builds reliable by stopping after the
  // release binary is produced.
  //
  // Callers can still opt into explicit bundling by passing --bundle/
  // --bundles (or their own --no-bundle) themselves.
  if (subcommand === "build" && !hasExplicitBundleArg) {
    platformFlags = [...platformFlags, "--no-bundle"];
  }
} else if (isWin) {
  execSync("powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\install-vulkan-sdk.ps1", {
    cwd: root,
    stdio: "inherit",
  });

  // The install script sets $env:VULKAN_SDK inside its own child process, but
  // that env var dies when the child exits.  Re-detect the SDK root here and
  // inject it into process.env so cargo / CMake (grandchildren of this process)
  // can find it without a shell restart.
  //
  // We write a temporary .ps1 file rather than using PowerShell's -Command
  // flag because inline -Command quoting is fragile when passed through
  // Node's execSync → Windows shell (registry paths, spaces, brackets all
  // cause problems).  A file-based script has none of those issues.
  if (!process.env.VULKAN_SDK) {
    // Mirror the detection order used by install-vulkan-sdk.ps1:
    //   1. Machine-level env var written by the LunarG installer
    //   2. Registry key written by the LunarG installer (both 64- and 32-bit hives)
    //   3. Newest versioned directory under C:\VulkanSDK\
    const tmpScript = resolve(tmpdir(), `detect-vulkan-${Date.now()}.ps1`);
    writeFileSync(
      tmpScript,
      `$p = [System.Environment]::GetEnvironmentVariable('VULKAN_SDK', 'Machine')\r\n` +
        `if (-not $p) {\r\n` +
        `  foreach ($reg in @('HKLM:\\SOFTWARE\\LunarG\\Vulkan SDK', 'HKLM:\\SOFTWARE\\WOW6432Node\\LunarG\\Vulkan SDK')) {\r\n` +
        `    if (Test-Path $reg) {\r\n` +
        `      $ip = (Get-ItemProperty $reg -ErrorAction SilentlyContinue).InstallPath\r\n` +
        `      if ($ip -and (Test-Path (Join-Path $ip 'Include\\vulkan\\vulkan.h'))) { $p = $ip; break }\r\n` +
        `    }\r\n` +
        `  }\r\n` +
        `}\r\n` +
        `if (-not $p -and (Test-Path 'C:\\VulkanSDK')) {\r\n` +
        `  $latest = Get-ChildItem 'C:\\VulkanSDK' -Directory | Sort-Object Name -Descending | Select-Object -First 1\r\n` +
        `  if ($latest -and (Test-Path (Join-Path $latest.FullName 'Include\\vulkan\\vulkan.h'))) { $p = $latest.FullName }\r\n` +
        `}\r\n` +
        `if ($p) { Write-Output $p }\r\n`,
    );
    try {
      const detected = execSync(`powershell -NoProfile -ExecutionPolicy Bypass -File "${tmpScript}"`, { cwd: root })
        .toString()
        .trim();
      if (detected) {
        process.env.VULKAN_SDK = detected;
      } else {
      }
    } catch (_e) {
    } finally {
      try {
        unlinkSync(tmpScript);
      } catch {
        /* ignore */
      }
    }
  } else {
  }

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
  if (subcommand === "build" && !hasExplicitBundleArg) {
    platformFlags = ["--no-bundle"];
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
  }
} else {
  // Linux native.

  ensureLinuxTrayRuntimeForDev();

  if (subcommand === "dev" && !process.env.WEBKIT_DISABLE_DMABUF_RENDERER) {
    const inWayland =
      (process.env.XDG_SESSION_TYPE || "").toLowerCase() === "wayland" || !!(process.env.WAYLAND_DISPLAY || "").trim();
    if (inWayland) {
      process.env.WEBKIT_DISABLE_DMABUF_RENDERER = "1";
    }
  }
  execSync("bash scripts/install-vulkan-sdk.sh", {
    cwd: root,
    stdio: "inherit",
  });

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
  }

  // ── Linux: skip Tauri bundling for default local builds ───────────────────
  //
  // This project ships with `bundle.targets: ["app"]`, which is macOS-only.
  // On Linux, `tauri build` still compiles successfully, then enters the
  // post-build bundle/updater-artifact path and can crash the Tauri CLI with a
  // native segmentation fault right after printing "Built application at:".
  //
  // `--no-bundle` keeps local Linux builds stable by stopping after the Rust
  // binary is produced. Callers can still opt into explicit bundling by passing
  // `--bundle ...` (or their own `--no-bundle`) themselves.
  if (subcommand === "build" && !hasExplicitBundleArg) {
    platformFlags = [...platformFlags, "--no-bundle"];
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
  } catch {
    /* not on Alpine or /etc/os-release unreadable */
  }

  if (onAlpine) {
    process.env.CARGO_BUILD_JOBS = String(cpus().length);
  }
}

// ── Build cache + fast linker detection ────────────────────────────────────────
//
// sccache caches both rustc and C/C++ (-sys crate) compilation outputs,
// reducing clean-rebuild time by ~50%.  mold is a fast linker for Linux.
// Both are auto-detected; neither is required.
//
// To disable: SKILL_NO_SCCACHE=1 or SKILL_NO_MOLD=1

function detectSccache() {
  if (process.env.SKILL_NO_SCCACHE === "1") return false;
  if (process.env.RUSTC_WRAPPER) return false; // already set by caller
  return commandExists("sccache");
}

function detectMold() {
  if (!isLinux) return false;
  if (process.env.SKILL_NO_MOLD === "1") return false;
  return commandExists("mold") && commandExists("clang");
}

const hasSccache = detectSccache();
const hasMold = detectMold();

if (hasSccache) {
  process.env.RUSTC_WRAPPER = "sccache";
} else if (!process.env.RUSTC_WRAPPER) {
  const _sccacheHint = isMac
    ? "brew install sccache"
    : isWin
      ? "scoop install sccache  (or: cargo install sccache)"
      : "cargo install sccache  (or: sudo apt install sccache)";
}

if (hasMold) {
  // Cargo env-var form of [target.<triple>.linker] and [target.<triple>.rustflags]
  // Uses the target from explicit --target arg or auto-detected host triple.
  const hostArchMap = { x64: "x86_64", arm64: "aarch64" };
  const hostArch = hostArchMap[arch()] || arch();
  const targets = explicitTarget ? [explicitTarget] : [`${hostArch}-unknown-linux-gnu`];

  for (const target of targets) {
    const envKey = target.toUpperCase().replace(/-/g, "_");
    if (!process.env[`CARGO_TARGET_${envKey}_LINKER`]) {
      process.env[`CARGO_TARGET_${envKey}_LINKER`] = "clang";
      process.env[`CARGO_TARGET_${envKey}_RUSTFLAGS`] =
        `${process.env[`CARGO_TARGET_${envKey}_RUSTFLAGS`] || ""} -C link-arg=-fuse-ld=mold`;
    }
  }
} else if (isLinux) {
}

// ── Windows: fast linker (lld-link) ──────────────────────────────────────────
//
// lld-link is LLVM's drop-in replacement for MSVC's link.exe and is
// significantly faster for large Rust projects.  It ships with LLVM and
// is auto-detected here when available.
//
// To disable: SKILL_NO_LLD=1

function detectLldLink() {
  if (!isWin) return false;
  if (process.env.SKILL_NO_LLD === "1") return false;
  // Don't override if the caller already set a linker.
  if (process.env.CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER) return false;
  return commandExists("lld-link");
}

const hasLldLink = detectLldLink();

if (hasLldLink) {
  const target = explicitTarget || "x86_64-pc-windows-msvc";
  const envKey = target.toUpperCase().replace(/-/g, "_");
  if (!process.env[`CARGO_TARGET_${envKey}_LINKER`]) {
    process.env[`CARGO_TARGET_${envKey}_LINKER`] = "lld-link";
  }
} else if (isWin) {
}

// ── Run Tauri ─────────────────────────────────────────────────────────────────
// ── Tauri CLI binary selection ─────────────────────────────────────────────
// Prefer `cargo tauri` (Rust binary compiled on this machine) over
// `npx tauri` (pre-built NAPI-RS native Node module).  The NAPI-RS
// binaries shipped by @tauri-apps/cli ≥ 2.10 contain SIMD instructions
// (AVX2 / advanced NEON) that cause SIGILL ("illegal hardware instruction")
// on some machines — especially during the post-build bundling /
// updater-artifact zstd-compression phase.  `cargo tauri` is compiled
// locally so it always matches the host CPU.
//
// Set TAURI_USE_NPX=1 to force the old npx path.
const useCargo = process.env.TAURI_USE_NPX !== "1" && commandExists("cargo-tauri");
const tauriCmd = useCargo ? ["cargo", "tauri"] : ["npx", "tauri"];

function runTauriWithArgs(args) {
  const cmd = [...tauriCmd, subcommand, ...args].join(" ").trimEnd();
  execSync(cmd, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
  });
}

function runTauriSubcommand(command, args) {
  const cmd = [...tauriCmd, command, ...args].join(" ").trimEnd();
  execSync(cmd, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
  });
}

function tryLinuxBundleSubcommandFallback(baseArgs, bundleTarget) {
  runTauriSubcommand("bundle", [...baseArgs, "--bundles", bundleTarget]);
}

function hasBundleArtifacts(targetTriple, bundleTarget) {
  const normalized = bundleTarget.trim().toLowerCase();
  const bundleRoot = explicitTarget
    ? resolve(root, "src-tauri", "target", targetTriple, "release", "bundle")
    : resolve(root, "src-tauri", "target", "release", "bundle");

  const targetDirName = normalized;
  const targetDir = resolve(bundleRoot, targetDirName);
  if (!existsSync(targetDir)) return false;

  const expectedPatternsByTarget = {
    deb: [".deb"],
    appimage: [".appimage", ".appimage.tar.gz"],
    rpm: [".rpm"],
    msi: [".msi"],
    nsis: [".exe"],
    app: [".app"],
    dmg: [".dmg"],
  };

  const expectedSuffixes = expectedPatternsByTarget[normalized];
  if (!expectedSuffixes) {
    return readdirSync(targetDir).length > 0;
  }

  const entries = readdirSync(targetDir, { withFileTypes: true });
  return entries.some((entry) => {
    if (!entry.isFile()) return false;
    const name = entry.name.toLowerCase();
    return expectedSuffixes.some((suffix) => name.endsWith(suffix));
  });
}

function hasBuiltReleaseBinary(targetTriple) {
  const binaryPath = targetTriple
    ? resolve(root, "src-tauri", "target", targetTriple, "release", "skill")
    : resolve(root, "src-tauri", "target", "release", "skill");
  return existsSync(binaryPath);
}

function maybeTreatLinuxCrashAsCompileOnlySuccess(error, _reason) {
  const crashExitCode = Number(error?.status);
  const isCrashExit = crashExitCode === 139 || crashExitCode === 134;
  const targetLooksArm64 = (explicitTarget ?? "").startsWith("aarch64-unknown-linux-gnu") || arch() === "arm64";

  if (
    !isLinux ||
    subcommand !== "build" ||
    !hasExplicitBundleArg ||
    !targetLooksArm64 ||
    !isCrashExit ||
    process.env.DISABLE_LINUX_CRASH_COMPILE_FALLBACK === "1"
  ) {
    return false;
  }

  if (!hasBuiltReleaseBinary(explicitTarget)) {
    return false;
  }
  process.exit(0);
}

function runBundleTargetWithLinuxSegfaultFallback(baseArgs, bundleTarget) {
  try {
    runTauriWithArgs([...baseArgs, "--bundles", bundleTarget]);
  } catch (error) {
    const hasSegfaultExitCode = Number(error?.status) === 139;
    const hasArtifacts = hasBundleArtifacts(explicitTarget, bundleTarget);

    if (isLinux && hasSegfaultExitCode && hasArtifacts) {
      return;
    }

    if (isLinux && hasSegfaultExitCode) {
      try {
        tryLinuxBundleSubcommandFallback(baseArgs, bundleTarget);
      } catch (bundleError) {
        const bundleSegfault = Number(bundleError?.status) === 139;
        const recoveredAfterBundleSegfault = hasBundleArtifacts(explicitTarget, bundleTarget);

        if (bundleSegfault && recoveredAfterBundleSegfault) {
          return;
        }

        maybeTreatLinuxCrashAsCompileOnlySuccess(bundleError, `fallback 'tauri bundle --bundles ${bundleTarget}'`);

        throw bundleError;
      }

      if (hasBundleArtifacts(explicitTarget, bundleTarget)) {
        return;
      }
    }

    maybeTreatLinuxCrashAsCompileOnlySuccess(error, `'tauri build --bundles ${bundleTarget}'`);

    throw error;
  }
}

const finalArgs = [...platformFlags, ...subArgs];
const bundleArg = parseExplicitBundleArg(finalArgs);
const bundleTargets = splitBundleTargets(bundleArg?.value);
const hasSingleBundleTarget = isLinux && subcommand === "build" && bundleArg && bundleTargets.length === 1;
const canRetryBundlesSequentially = isLinux && subcommand === "build" && bundleArg && bundleTargets.length > 1;

// ── macOS .app assembly fallback ───────────────────────────────────────────
// When the Tauri CLI bundler stack-overflows (exit 134 / SIGABRT) on macOS,
// assemble the .app bundle manually from the already-built release binary,
// Info.plist, icons, entitlements, and resources.
function assembleMacOsApp() {
  const triple = explicitTarget || "aarch64-apple-darwin";
  const binaryPath = resolve(root, "src-tauri/target", triple, "release/skill");
  if (!existsSync(binaryPath)) {
    return false;
  }

  // Read product name and bundle config from tauri.conf.json
  const tauriConf = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf-8"));
  const productName = tauriConf.productName || "NeuroSkill";
  const bundleId = tauriConf.identifier || "com.neuroskill.skill";
  const version = tauriConf.version || "0.0.0";
  const resources = tauriConf.bundle?.resources || {};
  const macConf = tauriConf.bundle?.macOS || {};

  const bundleDir = resolve(root, "src-tauri/target", triple, "release/bundle/macos");
  const appDir = resolve(bundleDir, `${productName}.app`);
  const contentsDir = resolve(appDir, "Contents");
  const macOSDir = resolve(contentsDir, "MacOS");
  const resDir = resolve(contentsDir, "Resources");

  // Clean and create structure
  execSync(`rm -rf ${JSON.stringify(appDir)}`, { cwd: root });
  for (const d of [macOSDir, resDir]) {
    execSync(`mkdir -p ${JSON.stringify(d)}`, { cwd: root });
  }

  // Copy binary
  execSync(`cp ${JSON.stringify(binaryPath)} ${JSON.stringify(resolve(macOSDir, productName))}`, { cwd: root });
  execSync(`chmod +x ${JSON.stringify(resolve(macOSDir, productName))}`, { cwd: root });

  // Copy Info.plist (prefer custom, then generate minimal)
  const infoPlistSrc = macConf.infoPlist ? resolve(root, "src-tauri", macConf.infoPlist) : null;
  if (infoPlistSrc && existsSync(infoPlistSrc)) {
    // Read the custom plist and inject required keys if missing
    let plistContent = readFileSync(infoPlistSrc, "utf-8");
    // Inject CFBundle keys before closing </dict> if not present
    const injections = [
      [`CFBundleExecutable`, `<key>CFBundleExecutable</key>\n  <string>${productName}</string>`],
      [`CFBundleIdentifier`, `<key>CFBundleIdentifier</key>\n  <string>${bundleId}</string>`],
      [`CFBundleVersion`, `<key>CFBundleVersion</key>\n  <string>${version}</string>`],
      [`CFBundleShortVersionString`, `<key>CFBundleShortVersionString</key>\n  <string>${version}</string>`],
      [`CFBundlePackageType`, `<key>CFBundlePackageType</key>\n  <string>APPL</string>`],
      [`CFBundleIconFile`, `<key>CFBundleIconFile</key>\n  <string>icon</string>`],
      [`NSHighResolutionCapable`, `<key>NSHighResolutionCapable</key>\n  <true/>`],
    ];
    for (const [key, xml] of injections) {
      if (!plistContent.includes(key)) {
        plistContent = plistContent.replace("</dict>", `  ${xml}\n</dict>`);
      }
    }
    writeFileSync(resolve(contentsDir, "Info.plist"), plistContent);
  } else {
    // Generate minimal Info.plist
    const plist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>${productName}</string>
  <key>CFBundleIdentifier</key>
  <string>${bundleId}</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>icon</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>`;
    writeFileSync(resolve(contentsDir, "Info.plist"), plist);
  }

  // Copy icon
  const icons = tauriConf.bundle?.icon || [];
  const icns = icons.find((i) => i.endsWith(".icns"));
  if (icns) {
    const icnsSrc = resolve(root, "src-tauri", icns);
    if (existsSync(icnsSrc)) {
      execSync(`cp ${JSON.stringify(icnsSrc)} ${JSON.stringify(resolve(resDir, "icon.icns"))}`, { cwd: root });
    }
  }

  // Copy resources (e.g. neutts-samples)
  for (const [src, dst] of Object.entries(resources)) {
    const srcPath = resolve(root, "src-tauri", src);
    const dstPath = resolve(resDir, dst);
    if (existsSync(srcPath)) {
      execSync(`mkdir -p ${JSON.stringify(dirname(dstPath))}`, { cwd: root });
      execSync(`ditto ${JSON.stringify(srcPath)} ${JSON.stringify(dstPath)}`, { cwd: root });
    } else {
    }
  }

  // Copy entitlements (for ad-hoc signing)
  const entitlements = macConf.entitlements ? resolve(root, "src-tauri", macConf.entitlements) : null;

  // Ad-hoc codesign
  try {
    const signArgs = entitlements && existsSync(entitlements) ? `--entitlements ${JSON.stringify(entitlements)}` : "";
    execSync(`codesign --force --deep --sign - ${signArgs} ${JSON.stringify(appDir)}`, { cwd: root, stdio: "inherit" });
  } catch (_e) {}
  return true;
}

// ── Daemon: build for release / build+spawn for dev ────────────────────────────
if (subcommand === "build") {
  console.log("\n🔧 Building daemon sidecar for release…");
  try {
    execSync("bash scripts/prepare-daemon-sidecar.sh", { cwd: root, stdio: "inherit", env: process.env });
  } catch (e) {
    console.warn(`⚠ Daemon sidecar build failed: ${e.message}`);
  }
}
let daemonChild = null;
if (subcommand === "dev") {
  console.log("\n🔧 Building skill-daemon…");
  try {
    const daemonBuildArgs = ["build", "-p", "skill-daemon"];
    if (explicitTarget) daemonBuildArgs.push("--target", explicitTarget);
    execSync(["cargo", ...daemonBuildArgs].join(" "), { cwd: root, stdio: "inherit", env: process.env });

    // Find the built binary (target-dir = src-tauri/target per .cargo/config.toml)
    const targetDir = resolve(root, "src-tauri", "target");
    const triple = explicitTarget || "";
    const candidates = [
      resolve(targetDir, triple, "debug", "skill-daemon"),
      resolve(targetDir, "debug", "skill-daemon"),
      resolve(targetDir, triple, "debug", "skill-daemon.exe"),
      resolve(targetDir, "debug", "skill-daemon.exe"),
    ];
    const daemonBin = candidates.find((c) => existsSync(c));

    if (daemonBin) {
      console.log(`\n🚀 Starting daemon: ${daemonBin}`);
      const { spawn } = await import("node:child_process");
      daemonChild = spawn(daemonBin, [], {
        env: { ...process.env, SKILL_DAEMON_ADDR: "127.0.0.1:18444", RUST_LOG: "skill_daemon=info,info" },
        stdio: ["ignore", "inherit", "inherit"],
        detached: false,
      });
      daemonChild.on("error", (e) => console.error(`[daemon] spawn error: ${e.message}`));
      daemonChild.on("exit", (code) => console.log(`[daemon] exited with code ${code}`));
      // Poll the TCP port until the daemon is ready (up to 10 s).
      // A fixed sleep is unreliable: on first run or slow machines the daemon
      // can take longer than 1 s to bind, causing Tauri's ensure_daemon_running()
      // to detect the port as closed and spawn a second instance — resulting in
      // two daemons fighting each other and a 1000% CPU spike at startup.
      const daemonAddrEnv = process.env.SKILL_DAEMON_ADDR || "127.0.0.1:18444";
      const lastColon = daemonAddrEnv.lastIndexOf(":");
      const daemonHost = daemonAddrEnv.slice(0, lastColon) || "127.0.0.1";
      const daemonPort = parseInt(daemonAddrEnv.slice(lastColon + 1), 10) || 18444;
      let daemonReady = false;
      for (let i = 0; i < 100; i++) {
        await new Promise((r) => setTimeout(r, 100));
        daemonReady = await new Promise((resolve) => {
          const sock = createConnection({ host: daemonHost, port: daemonPort });
          sock.once("connect", () => {
            sock.destroy();
            resolve(true);
          });
          sock.once("error", () => {
            sock.destroy();
            resolve(false);
          });
        });
        if (daemonReady) break;
      }
      if (daemonReady) {
        console.log("[daemon] ready, launching Tauri…\n");
      } else {
        console.warn("[daemon] not ready after 10 s — launching Tauri anyway (ensure_daemon_running will retry)\n");
      }
    } else {
      console.warn("⚠ skill-daemon binary not found after build — Tauri will attempt auto-launch");
    }
  } catch (e) {
    console.warn(`⚠ Failed to build/start daemon: ${e.message}`);
  }
}

// Clean up daemon on exit
process.on("exit", () => {
  if (daemonChild) {
    daemonChild.kill();
  }
});
process.on("SIGINT", () => {
  if (daemonChild) {
    daemonChild.kill();
  }
  process.exit(0);
});
process.on("SIGTERM", () => {
  if (daemonChild) {
    daemonChild.kill();
  }
  process.exit(0);
});

try {
  runTauriWithArgs(finalArgs);
} catch (error) {
  const hasSegfaultExitCode = Number(error?.status) === 139;
  const hasCrashExitCode = Number(error?.status) === 134; // SIGABRT (stack overflow)
  const baseArgs = bundleArg ? removeBundleArg(finalArgs, bundleArg) : [...finalArgs];

  // ── macOS: Tauri CLI bundler stack-overflow recovery ────────────────────
  // The Tauri CLI itself (not our app) can stack-overflow during the
  // .app bundling phase.  When this happens the release binary is already
  // built.  Fall back to manual .app assembly.
  if (isMac && (hasCrashExitCode || hasSegfaultExitCode) && subcommand === "build") {
    const hasBinary = hasBuiltReleaseBinary(explicitTarget);
    if (hasBinary) {
      // First, try compile-only to ensure binary is ready (may be a no-op)
      try {
        runTauriWithArgs([...removeBundleArg(finalArgs, bundleArg), "--no-bundle"]);
      } catch (_) {
        /* binary already exists, ignore */
      }

      if (assembleMacOsApp()) {
        process.exit(0);
      }
    }
  }

  if (hasSingleBundleTarget && hasSegfaultExitCode) {
    const target = bundleTargets[0];
    const hasArtifacts = hasBundleArtifacts(explicitTarget, target);
    if (hasArtifacts) {
      process.exit(0);
    }

    try {
      tryLinuxBundleSubcommandFallback(baseArgs, target);
      if (hasBundleArtifacts(explicitTarget, target)) {
        process.exit(0);
      }
    } catch (bundleError) {
      const bundleSegfault = Number(bundleError?.status) === 139;
      const recoveredAfterBundleSegfault = hasBundleArtifacts(explicitTarget, target);
      if (bundleSegfault && recoveredAfterBundleSegfault) {
        process.exit(0);
      }

      maybeTreatLinuxCrashAsCompileOnlySuccess(
        bundleError,
        `single-target fallback 'tauri bundle --bundles ${target}'`,
      );
      throw bundleError;
    }
  }

  maybeTreatLinuxCrashAsCompileOnlySuccess(error, "initial tauri build");

  if (!canRetryBundlesSequentially || !hasSegfaultExitCode) {
    throw error;
  }

  for (const target of bundleTargets) {
    runBundleTargetWithLinuxSegfaultFallback(baseArgs, target);
  }
}
