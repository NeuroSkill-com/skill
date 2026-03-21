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
 *   npm run tauri:flamegraph -- --release # profile a release build
 *
 * Output: flamegraph.svg in the project root.
 *
 * The script:
 *   1. Pre-builds espeak-ng (same env setup as tauri-build.js)
 *   2. Starts the Vite dev server in the background
 *   3. Waits for localhost:1420 to be ready
 *   4. Runs `cargo flamegraph` in src-tauri/
 *   5. Moves the SVG to the project root
 *   6. Cleans up the dev server on exit
 */

import { execSync, spawn } from "child_process";
import { platform, arch } from "os";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { existsSync, renameSync } from "fs";
import http from "http";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

const isMac = platform() === "darwin";
const isWin = platform() === "win32";
const isLinux = platform() === "linux";

// ── Parse arguments ──────────────────────────────────────────────────────────

const userArgs = process.argv.slice(2);
let recordSecs = 0; // 0 = until exit
let extraCargoArgs = [];

for (const arg of userArgs) {
  if (/^\d+$/.test(arg)) {
    recordSecs = parseInt(arg, 10);
  } else {
    extraCargoArgs.push(arg);
  }
}

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

// ── Preflight: cargo-flamegraph ──────────────────────────────────────────────

if (!commandExists("cargo-flamegraph")) {
  console.log("→ Installing cargo-flamegraph …");
  run("cargo install flamegraph");
}

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
  // dtrace is built into macOS; cargo flamegraph uses it automatically.
  // It requires root/sudo — cargo flamegraph handles this via --root.
  console.log("→ macOS: cargo flamegraph will use dtrace (may prompt for sudo)");
} else if (isWin) {
  // On Windows, cargo flamegraph can use dtrace (Windows 10+) or xperf.
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
  // Linux
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
  if (isLinux) {
    features = "llm-vulkan";
  } else if (isWin) {
    features = "llm-vulkan";
  }
  // macOS: Metal is the default via target-specific deps, no extra feature needed
}

// ── Start Vite dev server ────────────────────────────────────────────────────

let viteProc = null;

function cleanup() {
  if (viteProc && !viteProc.killed) {
    console.log(`→ Stopping Vite dev server (PID ${viteProc.pid}) …`);
    if (isWin) {
      // On Windows, spawn creates a process tree; kill the tree
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
  detached: !isWin, // process group for clean kill on Unix
});

console.log("→ Waiting for Vite on http://localhost:1420 …");
try {
  await waitForPort(1420, 90_000);
  console.log("→ Vite is ready.");
} catch (e) {
  console.error(`✖ ${e.message}`);
  process.exit(1);
}

// ── Build flamegraph command ─────────────────────────────────────────────────

const flamegraphArgs = [
  "flamegraph",
  "-o", resolve(root, "flamegraph.svg"),
  "--root",  // sudo for perf/dtrace
];

if (features) {
  flamegraphArgs.push("--features", features);
}

// Pass through any extra cargo args (e.g. --release)
flamegraphArgs.push(...extraCargoArgs);

// Trailing args for the binary itself
flamegraphArgs.push("--");

const cmd = ["cargo", ...flamegraphArgs].join(" ");

console.log("");
console.log("================================================================");
if (recordSecs > 0) {
  console.log(`  Flamegraph: recording for ${recordSecs}s (app will be killed after)`);
} else {
  console.log("  Flamegraph: recording until you close the app or press Ctrl+C");
}
console.log(`  Output:     ${resolve(root, "flamegraph.svg")}`);
console.log("================================================================");
console.log("");

// ── Run cargo flamegraph ─────────────────────────────────────────────────────

const env = { ...process.env, ESPEAK_LIB_DIR: espeakLib };

try {
  if (recordSecs > 0) {
    // Run with a timeout — spawn so we can kill after N seconds
    const fg = spawn("cargo", flamegraphArgs, {
      cwd: resolve(root, "src-tauri"),
      stdio: "inherit",
      env,
    });

    const timer = setTimeout(() => {
      console.log(`\n→ ${recordSecs}s elapsed — stopping profiled app …`);
      if (isWin) {
        try { execSync(`taskkill /PID ${fg.pid} /T /F`, { stdio: "ignore" }); } catch { /* ignore */ }
      } else {
        fg.kill("SIGINT"); // triggers flamegraph SVG generation
      }
    }, recordSecs * 1000);

    await new Promise((resolve, reject) => {
      fg.on("close", (code) => {
        clearTimeout(timer);
        // cargo flamegraph exits non-zero when killed by signal — that's expected
        resolve(code);
      });
      fg.on("error", reject);
    });
  } else {
    execSync(cmd, {
      cwd: resolve(root, "src-tauri"),
      stdio: "inherit",
      env,
    });
  }
} catch {
  // cargo flamegraph may exit non-zero when the app is killed — that's fine
}

// ── Report ───────────────────────────────────────────────────────────────────

const svgPath = resolve(root, "flamegraph.svg");
// Also check src-tauri in case -o didn't land in root
const svgAlt = resolve(root, "src-tauri", "flamegraph.svg");
if (!existsSync(svgPath) && existsSync(svgAlt)) {
  renameSync(svgAlt, svgPath);
}

console.log("");
if (existsSync(svgPath)) {
  console.log(`✓ Flamegraph written to: ${svgPath}`);
  console.log("  Open it in a browser for interactive zoom/search.");
} else {
  console.log("⚠ No flamegraph.svg produced — the profiler may not have captured enough samples.");
  console.log("  Try running longer or check profiler permissions.");
}
