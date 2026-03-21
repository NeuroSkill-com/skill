#!/usr/bin/env node
/**
 * tauri-flamegraph.js — Profile the Tauri app and produce an interactive
 * flamegraph SVG.
 *
 * Works on Linux (perf), macOS (dtrace), and Windows (dtrace / xperf).
 *
 * Usage (via npm):
 *   npm run tauri:flamegraph              # record until app exits / Ctrl+C
 *   npm run tauri:flamegraph -- 30        # record for 30 seconds
 *   npm run tauri:flamegraph -- --release # profile a release build (default: dev)
 *
 * Output: flamegraph.svg in the project root.
 *
 * The script:
 *   1. Pre-builds espeak-ng (same env setup as tauri-build.js)
 *   2. Builds the Tauri binary via `cargo build` (same profile as `tauri dev`)
 *   3. Starts the Vite dev server in the background
 *   4. Waits for localhost:1420 to be ready
 *   5. Profiles the binary with `flamegraph` (not `cargo flamegraph`)
 *   6. Produces flamegraph.svg in the project root
 *   7. Cleans up the dev server on exit
 *
 * The build and profile steps are separated so that:
 *   - The build runs as the normal user (no permission issues with cargo cache)
 *   - The profiler runs as root (required for dtrace on macOS, perf on some Linux)
 *     and owns the trace file it creates, avoiding permission mismatches
 */

import { execSync, spawn } from "child_process";
import { platform, arch } from "os";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { existsSync, rmSync } from "fs";
import http from "http";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");
const srcTauri = resolve(root, "src-tauri");

const isMac = platform() === "darwin";
const isWin = platform() === "win32";
const isLinux = platform() === "linux";

// ── Parse arguments ──────────────────────────────────────────────────────────

const userArgs = process.argv.slice(2);
let recordSecs = 0;        // 0 = until exit
let useRelease = false;
let extraCargoArgs = [];

for (const arg of userArgs) {
  if (/^\d+$/.test(arg)) {
    recordSecs = parseInt(arg, 10);
  } else if (arg === "--release") {
    useRelease = true;
  } else {
    extraCargoArgs.push(arg);
  }
}

const profile = useRelease ? "release" : "dev";

// ── Helpers ──────────────────────────────────────────────────────────────────

function commandExists(cmd) {
  try {
    const check = isWin ? `where ${cmd}` : `command -v ${cmd}`;
    execSync(check, { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function run(cmd, opts = {}) {
  console.log(`→ ${cmd}`);
  execSync(cmd, { cwd: root, stdio: "inherit", ...opts });
}

function waitForPort(port, timeoutMs = 60_000) {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const tryConnect = () => {
      const req = http.get(`http://localhost:${port}`, (res) => {
        res.resume();
        resolve();
      });
      req.on("error", () => {
        if (Date.now() - start > timeoutMs) {
          reject(new Error(`localhost:${port} did not respond within ${timeoutMs / 1000}s`));
        } else {
          setTimeout(tryConnect, 500);
        }
      });
      req.end();
    };
    tryConnect();
  });
}

function forceRemove(filePath) {
  if (!existsSync(filePath)) return;
  console.log(`→ Removing stale file: ${filePath}`);
  try {
    rmSync(filePath, { recursive: true, force: true });
  } catch {
    try {
      execSync(`sudo rm -rf ${JSON.stringify(filePath)}`, { stdio: "inherit" });
    } catch { /* best effort */ }
  }
}

// ── Preflight: flamegraph tool ───────────────────────────────────────────────

if (!commandExists("flamegraph")) {
  console.log("→ Installing cargo-flamegraph …");
  run("cargo install flamegraph");
}

// Resolve the `flamegraph` binary path (needed for sudo invocation)
let flamegraphBin;
try {
  flamegraphBin = execSync(isWin ? "where flamegraph" : "which flamegraph", {
    encoding: "utf8",
  }).trim().split("\n")[0];
} catch {
  console.error("✖ Could not find `flamegraph` binary after install.");
  process.exit(1);
}
console.log(`→ Using flamegraph binary: ${flamegraphBin}`);

// ── Preflight: platform profiler ─────────────────────────────────────────────

if (isLinux) {
  if (!commandExists("perf")) {
    console.error(
      [
        "✖ 'perf' not found. Install it:",
        "  Ubuntu/Debian: sudo apt install linux-tools-$(uname -r) linux-perf",
        "  Fedora:        sudo dnf install perf",
        "  Arch:          sudo pacman -S perf",
      ].join("\n")
    );
    process.exit(1);
  }

  // Allow perf for non-root
  try {
    const paranoid = parseInt(
      execSync("cat /proc/sys/kernel/perf_event_paranoid", { encoding: "utf8" }).trim(),
      10
    );
    if (paranoid > -1) {
      console.log("→ Setting kernel.perf_event_paranoid=-1 (may require sudo password) …");
      execSync("sudo sysctl -w kernel.perf_event_paranoid=-1", { stdio: "inherit" });
    }
  } catch {
    console.warn("→ Could not check/set perf_event_paranoid — flamegraph may need sudo");
  }
} else if (isMac) {
  // dtrace requires root on macOS. Warm sudo now, before the long build.
  console.log("→ macOS: dtrace requires sudo — authenticating now …");
  try {
    execSync("sudo -v", { stdio: "inherit" });
  } catch {
    console.error("✖ sudo authentication failed — dtrace requires root on macOS.");
    process.exit(1);
  }
} else if (isWin) {
  const hasDtrace = commandExists("dtrace");
  const hasXperf = commandExists("xperf");
  if (!hasDtrace && !hasXperf) {
    console.error(
      [
        "✖ No supported profiler found on Windows.",
        "  Options:",
        "  1. dtrace — built into Windows 10 2004+ (requires admin + BCD flag)",
        "     bcdedit /set dtrace ON   (run as admin, then reboot)",
        "  2. xperf — part of Windows Performance Toolkit (WPT)",
        "     Install via Windows ADK or the Windows SDK",
      ].join("\n")
    );
    process.exit(1);
  }
}

// ── Pre-build espeak-ng ──────────────────────────────────────────────────────

let espeakLib;

if (isMac) {
  console.log("→ Building espeak-ng static library …");
  run("bash scripts/build-espeak-static.sh");
  espeakLib = resolve(root, "src-tauri/espeak-static/lib");
} else if (isWin) {
  console.log("→ Building espeak-ng static library (MSVC) …");
  run("powershell -NoProfile -ExecutionPolicy Bypass -File scripts\\build-espeak-static.ps1");
  espeakLib = resolve(root, "src-tauri\\espeak-static\\lib");
} else {
  console.log("→ Ensuring Vulkan SDK …");
  run("bash scripts/install-vulkan-sdk.sh");
  console.log("→ Building espeak-ng static library …");
  run("bash scripts/build-espeak-static.sh");
  espeakLib = resolve(root, "src-tauri/espeak-static/lib");
}

// ── Wayland workaround (Linux) ───────────────────────────────────────────────

if (isLinux) {
  const sessionType = (process.env.XDG_SESSION_TYPE || "").toLowerCase();
  if (sessionType === "wayland" || (process.env.WAYLAND_DISPLAY || "").trim()) {
    process.env.WEBKIT_DISABLE_DMABUF_RENDERER = process.env.WEBKIT_DISABLE_DMABUF_RENDERER || "1";
  }
}

// ── Determine features ──────────────────────────────────────────────────────

let features = "";
if (!extraCargoArgs.includes("--features") && !extraCargoArgs.includes("--no-default-features")) {
  if (isLinux || isWin) {
    features = "llm-vulkan";
  }
}

// ── Enable debug symbols ─────────────────────────────────────────────────────
// Both dev and release profiles need debuginfo for useful flamegraphs.
// Dev has it by default; release needs it explicitly.

if (useRelease && !process.env.CARGO_PROFILE_RELEASE_DEBUG) {
  process.env.CARGO_PROFILE_RELEASE_DEBUG = "true";
  console.log("→ Enabling debug symbols in release build (CARGO_PROFILE_RELEASE_DEBUG=true)");
}

// ── Step 1: Build the binary (normal user) ───────────────────────────────────

const buildArgs = ["build"];
if (useRelease) buildArgs.push("--release");
if (features) buildArgs.push("--features", features);
buildArgs.push(...extraCargoArgs);

const buildCmd = `cargo ${buildArgs.join(" ")}`;
console.log(`→ Building: ${buildCmd}`);
execSync(buildCmd, {
  cwd: srcTauri,
  stdio: "inherit",
  env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
});

// Locate the built binary
const profileDir = useRelease ? "release" : "debug";
const binaryName = isWin ? "skill.exe" : "skill";
const binaryPath = resolve(srcTauri, "target", profileDir, binaryName);

if (!existsSync(binaryPath)) {
  console.error(`✖ Built binary not found at: ${binaryPath}`);
  process.exit(1);
}
console.log(`→ Binary: ${binaryPath}`);

// ── Step 2: Start Vite dev server ────────────────────────────────────────────

let viteProc = null;

function cleanup() {
  if (viteProc && !viteProc.killed) {
    console.log(`→ Stopping Vite dev server (PID ${viteProc.pid}) …`);
    if (isWin) {
      try { execSync(`taskkill /PID ${viteProc.pid} /T /F`, { stdio: "ignore" }); } catch { /* ignore */ }
    } else {
      try { process.kill(-viteProc.pid, "SIGTERM"); } catch {
        try { viteProc.kill("SIGTERM"); } catch { /* ignore */ }
      }
    }
  }
}

process.on("exit", cleanup);
process.on("SIGINT", () => { cleanup(); process.exit(130); });
process.on("SIGTERM", () => { cleanup(); process.exit(143); });

console.log("→ Starting Vite dev server …");

const npmCmd = isWin ? "npm.cmd" : "npm";
viteProc = spawn(npmCmd, ["run", "dev"], {
  cwd: root,
  stdio: "ignore",
  detached: !isWin,
});

console.log("→ Waiting for Vite on http://localhost:1420 …");
try {
  await waitForPort(1420, 90_000);
  console.log("→ Vite is ready.");
} catch (e) {
  console.error(`✖ ${e.message}`);
  process.exit(1);
}

// ── Step 3: Clean stale trace files ──────────────────────────────────────────

for (const stale of [
  resolve(srcTauri, "cargo-flamegraph.trace"),
  resolve(root, "cargo-flamegraph.trace"),
  resolve(root, "flamegraph.svg"),
]) {
  forceRemove(stale);
}

// ── Step 4: Profile with flamegraph ──────────────────────────────────────────
// We run `flamegraph` (standalone, not `cargo flamegraph`) against the
// pre-built binary.  On macOS/Linux we use sudo so the profiler (dtrace/perf)
// has the permissions it needs AND owns the trace files it creates.

const svgPath = resolve(root, "flamegraph.svg");

// Refresh sudo right before profiling (macOS) so the cache hasn't expired
// during the build step.
if (isMac) {
  try { execSync("sudo -v", { stdio: "inherit" }); } catch { /* ignore */ }
}

// Build the flamegraph invocation
const fgArgs = ["-o", svgPath, "--", binaryPath];

// On macOS and Linux, use sudo for the profiler.
// On Windows, run directly (dtrace/xperf need admin — user should run terminal as admin).
const needsSudo = isMac || isLinux;
const fgCmd = needsSudo ? "sudo" : flamegraphBin;
const fgFullArgs = needsSudo ? [flamegraphBin, ...fgArgs] : fgArgs;

console.log("");
console.log("================================================================");
console.log(`  Profile:    ${profile} (${useRelease ? "optimized + debuginfo" : "debug"})`);
if (recordSecs > 0) {
  console.log(`  Flamegraph: recording for ${recordSecs}s (app will be killed after)`);
} else {
  console.log("  Flamegraph: recording until you close the app or press Ctrl+C");
}
console.log(`  Output:     ${svgPath}`);
console.log("================================================================");
console.log("");

const env = { ...process.env, ESPEAK_LIB_DIR: espeakLib };

try {
  const fg = spawn(fgCmd, fgFullArgs, {
    cwd: srcTauri,
    stdio: "inherit",
    env,
  });

  let timer;
  if (recordSecs > 0) {
    timer = setTimeout(() => {
      console.log(`\n→ ${recordSecs}s elapsed — stopping profiled app …`);
      if (isWin) {
        try { execSync(`taskkill /PID ${fg.pid} /T /F`, { stdio: "ignore" }); } catch { /* ignore */ }
      } else {
        // Send SIGINT to the sudo/flamegraph process group to trigger SVG generation
        fg.kill("SIGINT");
      }
    }, recordSecs * 1000);
  }

  await new Promise((res, rej) => {
    fg.on("close", (code) => {
      if (timer) clearTimeout(timer);
      res(code);
    });
    fg.on("error", rej);
  });
} catch {
  // flamegraph may exit non-zero when the app is killed — that's fine
}

// ── Step 5: Fix ownership (sudo may have created root-owned SVG) ─────────────

if (needsSudo && existsSync(svgPath)) {
  try {
    const whoami = execSync("whoami", { encoding: "utf8" }).trim();
    execSync(`sudo chown ${whoami} ${JSON.stringify(svgPath)}`, { stdio: "ignore" });
  } catch { /* best effort */ }
}

// ── Report ───────────────────────────────────────────────────────────────────

console.log("");
if (existsSync(svgPath)) {
  console.log(`✓ Flamegraph written to: ${svgPath}`);
  console.log("  Open it in a browser for interactive zoom/search.");
} else {
  console.log("⚠ No flamegraph.svg produced — the profiler may not have captured enough samples.");
  console.log("  Try running longer or check profiler permissions.");
}
