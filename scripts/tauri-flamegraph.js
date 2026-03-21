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
import { existsSync, rmSync, statSync } from "fs";
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
    } catch { /* ignore — checked below */ }
  }
  if (existsSync(filePath)) {
    console.error(`✖ Could not remove ${filePath} — the profiler would run a stale binary.`);
    console.error("  Fix: sudo rm -f " + filePath);
    process.exit(1);
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

// ── Build cache + fast linker detection ──────────────────────────────────────
// Mirror the sccache / mold detection from tauri-build.js so the flamegraph
// build uses the same compilation environment and cargo fingerprints match.
// Without this, switching between `tauri dev` and `tauri:flamegraph` causes
// full rebuilds because the env (RUSTC_WRAPPER, linker flags) differs.

function detectSccache() {
  if (process.env.SKILL_NO_SCCACHE === "1") return false;
  if (process.env.RUSTC_WRAPPER) return false;
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
  console.log("→ sccache detected — enabling compilation cache (RUSTC_WRAPPER=sccache)");
}

if (hasMold) {
  const hostArchMap = { x64: "x86_64", arm64: "aarch64" };
  const hostArch = hostArchMap[arch()] || arch();
  const target = `${hostArch}-unknown-linux-gnu`;
  const envKey = target.toUpperCase().replace(/-/g, "_");
  if (!process.env[`CARGO_TARGET_${envKey}_LINKER`]) {
    process.env[`CARGO_TARGET_${envKey}_LINKER`] = "clang";
    process.env[`CARGO_TARGET_${envKey}_RUSTFLAGS`] =
      (process.env[`CARGO_TARGET_${envKey}_RUSTFLAGS`] || "") +
      " -C link-arg=-fuse-ld=mold";
  }
  console.log("→ mold + clang detected — enabling fast linker (-fuse-ld=mold)");
}

// ── Step 0: Clean build artifacts ────────────────────────────────────────────
// Full cargo clean + SvelteKit clean so the flamegraph captures a fresh build.

console.log("→ Cleaning cargo build artifacts …");
try {
  execSync("cargo clean", { cwd: srcTauri, stdio: "inherit" });
} catch {
  console.warn("→ cargo clean failed — continuing anyway");
}

console.log("→ Cleaning SvelteKit build artifacts …");
for (const cache of [
  resolve(root, ".svelte-kit"),
  resolve(root, "node_modules", ".vite"),
  resolve(root, "build"),
]) {
  forceRemove(cache);
}

// ── Step 1: Build the binary (normal user) ───────────────────────────────────

// Locate the expected binary path
const profileDir = useRelease ? "release" : "debug";
const binaryName = isWin ? "skill.exe" : "skill";
const binaryPath = resolve(srcTauri, "target", profileDir, binaryName);

// Record the pre-build timestamp so we can verify the binary is truly fresh.
const preBuildTs = Date.now();

const buildArgs = ["build", "-p", "skill"];
if (useRelease) buildArgs.push("--release");
if (features) buildArgs.push("--features", features);
buildArgs.push(...extraCargoArgs);

const buildCmd = `cargo ${buildArgs.join(" ")}`;
console.log(`→ Building: ${buildCmd}`);
execSync(buildCmd, {
  cwd: root,
  stdio: "inherit",
  env: { ...process.env, ESPEAK_LIB_DIR: espeakLib },
});

if (!existsSync(binaryPath)) {
  console.error(`✖ Built binary not found at: ${binaryPath}`);
  process.exit(1);
}

// Verify the binary was actually rebuilt (not a stale leftover).
const binaryMtime = statSync(binaryPath).mtimeMs;
if (binaryMtime < preBuildTs) {
  console.error(
    `✖ Binary at ${binaryPath} is older than the build start time.` +
    "\n  cargo may have skipped the build (stale fingerprints)." +
    "\n  Try: cargo clean -p skill   (then re-run)"
  );
  process.exit(1);
}
console.log(`→ Binary: ${binaryPath} (freshly built)`);

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

// ── Step 3b: Clear WebView cache ─────────────────────────────────────────────
// WebKitGTK (Linux) and WebKit (macOS) aggressively cache HTML/CSS/JS/icons.
// Without clearing this, the profiled binary loads stale frontend assets from
// a previous session instead of the fresh content served by the Vite dev server.
{
  const home = process.env.HOME || "";
  const appId = "com.neuroskill.skill";

  const webviewCaches = [];

  if (isLinux) {
    // WebKitGTK stores per-app data under XDG_DATA_HOME/<id>/
    const dataHome = process.env.XDG_DATA_HOME || resolve(home, ".local", "share");
    const cacheHome = process.env.XDG_CACHE_HOME || resolve(home, ".cache");
    webviewCaches.push(
      resolve(dataHome, appId),              // CacheStorage, WebKitCache, localstorage, etc.
      resolve(cacheHome, appId),             // additional cache
    );
  } else if (isMac) {
    // macOS WebKit caches per-app under ~/Library/
    webviewCaches.push(
      resolve(home, "Library", "WebKit", appId),
      resolve(home, "Library", "Caches", appId),
    );
  }

  for (const dir of webviewCaches) {
    if (existsSync(dir)) {
      console.log(`→ Clearing WebView cache: ${dir}`);
      forceRemove(dir);
    }
  }
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
//
// We pass `--preserve-env=HOME,USER,ESPEAK_LIB_DIR,PATH,WEBKIT_DISABLE_DMABUF_RENDERER`
// so the profiled binary runs with the real user's HOME.  Without this, sudo
// sets HOME=/var/root (macOS) or /root (Linux) and the app reads stale or
// empty config/data from root's home instead of the user's ~/.skill, and
// WebKit loads cached pages from root's ~/Library/WebKit/.
const needsSudo = isMac || isLinux;
const sudoEnvVars = [
  "HOME", "USER", "LOGNAME",
  "ESPEAK_LIB_DIR", "PATH",
  "WEBKIT_DISABLE_DMABUF_RENDERER",
  // XDG dirs — ensures the app reads/writes the real user's config, data, and cache
  "XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR",
  // Cargo / Rust toolchain — prevent sudo from using /root/.cargo
  "CARGO_HOME", "RUSTUP_HOME",
  // Display — needed so the GUI opens on the current user's session
  "DISPLAY", "WAYLAND_DISPLAY",
  // D-Bus — needed for desktop integration (tray, notifications, etc.)
  "DBUS_SESSION_BUS_ADDRESS",
].join(",");
const fgCmd = needsSudo ? "sudo" : flamegraphBin;
const fgFullArgs = needsSudo
  ? [`--preserve-env=${sudoEnvVars}`, flamegraphBin, ...fgArgs]
  : fgArgs;

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
    cwd: root,
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

if (needsSudo) {
  // Use the real invoking user (not "root") to fix ownership of files
  // created by the sudo-elevated profiler.
  const realUser = process.env.USER || execSync("id -un", { encoding: "utf8" }).trim();
  const filesToChown = [
    svgPath,
    resolve(srcTauri, "cargo-flamegraph.trace"),
    resolve(root, "cargo-flamegraph.trace"),
  ].filter(existsSync);
  for (const f of filesToChown) {
    try {
      execSync(`sudo chown ${realUser} ${JSON.stringify(f)}`, { stdio: "ignore" });
    } catch { /* best effort */ }
  }
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
