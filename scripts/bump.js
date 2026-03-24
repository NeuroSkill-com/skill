#!/usr/bin/env node
import { readFileSync, writeFileSync, openSync, readSync, closeSync } from "fs";
import { execSync, spawn } from "child_process";
import { compileChangelog } from "./compile-changelog.js";

// ── helpers ──────────────────────────────────────────────────────────────────

function readText(path) {
  return readFileSync(path, "utf8");
}

function writeText(path, content) {
  writeFileSync(path, content, "utf8");
}

function bumpPatch(version) {
  const parts = version.split(".").map(Number);
  if (parts.length !== 3 || parts.some(isNaN)) {
    throw new Error(`Invalid version "${version}"`);
  }
  parts[2] += 1;
  return parts.join(".");
}

function validateVersion(v) {
  if (!/^\d+\.\d+\.\d+$/.test(v)) {
    throw new Error(`Version must be in x.x.x format, got "${v}"`);
  }
  return v;
}

function hasPkgConfig(packageName) {
  try {
    execSync(`pkg-config --exists ${packageName}`, { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function ensureLinuxTauriDeps() {
  if (process.platform !== "linux") return;

  const requiredPackages = ["webkit2gtk-4.1", "javascriptcoregtk-4.1", "libsoup-3.0", "libpipewire-0.3"];
  const missingPackages = requiredPackages.filter((name) => !hasPkgConfig(name));

  if (missingPackages.length === 0) return;

  const missingList = missingPackages.join(", ");
  throw new Error(
    [
      `Missing Linux Tauri system dependencies (${missingList}).`,
      "Install required packages before running npm run bump:",
      "  sudo apt install -y libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev libpipewire-0.3-dev",
      "If those are unavailable on your distro image, see LINUX.md for legacy alternatives.",
    ].join("\n")
  );
}

// Workspace crates that CI runs clippy + tests on (mirrors ci.yml).
const WORKSPACE_CRATES = [
  "skill-eeg",
  "skill-data",
  "skill-constants",
  "skill-jobs",
  "skill-tray",
  "skill-autostart",
  "skill-exg",
  "skill-commands",
  "skill-tools",
  "skill-skills",
  "skill-devices",
  "skill-settings",
  "skill-history",
  "skill-label-index",
  "skill-router",
  "skill-tts",
  "skill-headless",
  "skill-vision",
  "skill-health",
  "skill-gpu",
  "skill-screenshots",
  "skill-llm",
];

// Subset of workspace crates that CI runs `cargo test --lib` on.
const TEST_CRATES = [
  "skill-eeg",
  "skill-data",
  "skill-constants",
  "skill-tools",
  "skill-devices",
  "skill-settings",
  "skill-history",
  "skill-health",
  "skill-router",
  "skill-llm",
  "skill-autostart",
  "skill-tts",
  "skill-gpu",
];

function checkForCompetingCargo() {
  try {
    const out = execSync(
      `ps -eo pid,command | grep -E '[c]argo (build|clippy|check|test|install|publish)' || true`,
      { encoding: "utf8" }
    ).trim();
    if (!out) return;
    const lines = out.split("\n").filter(Boolean);
    if (lines.length === 0) return;
    console.warn("\n[preflight] Warning: Other cargo processes detected:");
    for (const l of lines) console.warn(`  ${l.trim()}`);
    console.warn(
      "\n  Cargo uses a global package-cache lock (~/.cargo/.package-cache)."
    );
    console.warn(
      "  The bump clippy steps will block until these finish.\n"
    );
    const fd = openSync("/dev/tty", "r");
    const buf = Buffer.alloc(1);
    process.stdout.write("  Continue anyway? [y/N] ");
    let answer = "";
    while (true) {
      const bytesRead = readSync(fd, buf, 0, 1);
      if (bytesRead === 0) break;
      const ch = buf.toString("utf8", 0, 1);
      if (ch === "\n" || ch === "\r") break;
      answer += ch;
    }
    closeSync(fd);
    if (!/^y(es)?$/i.test(answer.trim())) {
      throw new Error("Bump aborted: competing cargo processes running.");
    }
  } catch (err) {
    if (err.message.includes("Bump aborted")) throw err;
    // ignore ps/grep failures
  }
}

// ── TUI progress helpers ─────────────────────────────────────────────────────

const CSI = "\x1b[";

function formatDuration(ms) {
  const totalSec = Math.round(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return min > 0 ? `${min}m ${sec}s` : `${sec}s`;
}

/**
 * Detect warning lines in command output — treat any warning as fatal.
 * Returns array of warning lines.
 */
function extractWarnings(output) {
  return output
    .split("\n")
    .filter(
      (line) =>
        /\bwarning\b/i.test(line) &&
        !/0 warnings/i.test(line) &&
        !/warnings?\s*=|deny\(warnings\)/i.test(line) &&
        !/^warning: \S+@\S+:/i.test(line.trim()) &&
        !/^warning: build failed/i.test(line.trim())
    );
}

/**
 * TUI class — manages a scrolling log area above a fixed progress bar.
 *
 * Layout:
 *   row 1..rows-2   — scrolling log output
 *   row rows-1      — separator
 *   row rows        — progress bar
 *
 * Uses ANSI scroll region to keep the bottom 2 lines pinned.
 */
class BumpTUI {
  constructor(totalSteps) {
    this.totalSteps = totalSteps;
    this.currentStep = 0;
    this.stepLabel = "";
    this.startTime = Date.now();
    this.stepTimes = [];
    this.rows = process.stdout.rows || 40;
    this.cols = process.stdout.columns || 80;
    this.timer = null;

    // Listen for terminal resize
    process.stdout.on("resize", () => {
      this.rows = process.stdout.rows || 40;
      this.cols = process.stdout.columns || 80;
      this._setupScrollRegion();
      this._renderBar();
    });
  }

  start() {
    // Clear screen and set up scroll region
    process.stdout.write(`${CSI}2J${CSI}H`); // clear + home
    this._setupScrollRegion();
    this._renderBar();

    // Update progress bar every second
    this.timer = setInterval(() => this._renderBar(), 1000);
  }

  stop() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    // Reset scroll region to full screen
    process.stdout.write(`${CSI}r`);
    // Move cursor below the bar
    process.stdout.write(`${CSI}${this.rows};1H\n`);
  }

  _setupScrollRegion() {
    // Scroll region: rows 1..(rows-2) for log output
    const scrollEnd = Math.max(1, this.rows - 2);
    process.stdout.write(`${CSI}1;${scrollEnd}r`);
  }

  /** Write log text into the scroll region (above the bar). */
  log(text) {
    const scrollEnd = Math.max(1, this.rows - 2);
    // Save cursor, move to end of scroll region, print, restore cursor
    process.stdout.write(`${CSI}s`);                      // save
    process.stdout.write(`${CSI}${scrollEnd};1H`);        // move to bottom of scroll area
    process.stdout.write("\n");                            // scroll up if needed
    // Write each line — handle multi-line text
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i++) {
      if (i > 0) {
        process.stdout.write(`${CSI}${scrollEnd};1H\n`);
      }
      process.stdout.write(lines[i]);
    }
    process.stdout.write(`${CSI}u`);                      // restore
    this._renderBar();                                     // redraw bar in case scroll corrupted it
  }

  /** Update current step info and redraw bar. */
  setStep(index, label) {
    if (index > 0 && this.currentStep < index) {
      // record previous step time
      // (stepTimes is used for ETA estimation)
    }
    this.currentStep = index;
    this.stepLabel = label;
    this._renderBar();
  }

  /** Record how long a step took (for ETA). */
  recordStepTime(ms) {
    this.stepTimes.push(ms);
  }

  /** Mark a step as passed — logs a green checkmark. */
  stepPassed(label, durationMs) {
    this.log(`  \x1b[32m✓\x1b[0m  ${label}  \x1b[2m(${formatDuration(durationMs)})\x1b[0m`);
  }

  /** Mark a step as failed — logs a red cross. */
  stepFailed(label) {
    this.log(`  \x1b[31m✗\x1b[0m  ${label}`);
  }

  /** Render the separator + progress bar on the bottom 2 rows. */
  _renderBar() {
    const cols = this.cols;
    const rows = this.rows;
    const current = this.currentStep;
    const total = this.totalSteps;

    const elapsedMs = Date.now() - this.startTime;
    const avgMs = this.stepTimes.length > 0
      ? this.stepTimes.reduce((a, b) => a + b, 0) / this.stepTimes.length
      : 0;
    const remaining = total - current;
    const etaMs = this.stepTimes.length > 0 ? avgMs * remaining : 0;

    const pct = total > 0 ? Math.round((current / total) * 100) : 0;
    const barWidth = Math.max(10, Math.min(30, cols - 60));
    const filled = Math.round((current / total) * barWidth);
    const bar = "\x1b[32m" + "█".repeat(filled) + "\x1b[0m" + "\x1b[2m" + "░".repeat(barWidth - filled) + "\x1b[0m";

    const elapsed = formatDuration(elapsedMs);
    const eta = etaMs > 0 ? formatDuration(etaMs) : "--";

    const sepRow = rows - 1;
    const barRow = rows;

    // Separator line
    const sep = "\x1b[2m" + "─".repeat(cols) + "\x1b[0m";

    // Progress line
    const stepText = this.stepLabel.length > 30
      ? this.stepLabel.slice(0, 29) + "…"
      : this.stepLabel;
    const progressLine =
      `  [${bar}] ${current}/${total} (${pct}%) | ${elapsed} elapsed | ETA ${eta} | ${stepText}`;

    // Draw separator and progress bar (outside scroll region)
    process.stdout.write(`${CSI}${sepRow};1H${CSI}K${sep}`);
    process.stdout.write(`${CSI}${barRow};1H${CSI}K${progressLine}`);
  }
}

// ── run a command with streamed output ───────────────────────────────────────

function runCommandStreaming(label, command, tui) {
  return new Promise((resolve, reject) => {
    const parts = command.split(" ");
    // Use shell to handle pipes, redirects, 2>&1
    const child = spawn("bash", ["-c", `${command} 2>&1`], {
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, FORCE_COLOR: "1" },
    });

    let output = "";

    child.stdout.on("data", (data) => {
      const text = data.toString();
      output += text;
      // Stream each line to the TUI log area
      const lines = text.split("\n");
      for (const line of lines) {
        if (line.length > 0) {
          tui.log(line);
        }
      }
    });

    child.stderr.on("data", (data) => {
      const text = data.toString();
      output += text;
      const lines = text.split("\n");
      for (const line of lines) {
        if (line.length > 0) {
          tui.log(line);
        }
      }
    });

    child.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`Command failed with exit code ${code}: ${label}`));
        return;
      }

      // Check for warnings
      const warningLines = extractWarnings(output);
      if (warningLines.length > 0) {
        tui.log(`\x1b[33m[preflight] ${warningLines.length} warning(s) in ${label}:\x1b[0m`);
        for (const w of warningLines.slice(0, 10)) {
          tui.log(`  \x1b[33m${w.trim()}\x1b[0m`);
        }
        reject(new Error(`Bump aborted: warnings found during "${label}"`));
        return;
      }

      resolve(output);
    });

    child.on("error", (err) => {
      reject(err);
    });
  });
}

// ── preflight checks (async with TUI) ───────────────────────────────────────

async function runPreflightChecks() {
  console.log("Running preflight checks before bump...\n");

  // ── Competing cargo processes ─────────────────────────────────────────────
  checkForCompetingCargo();

  // ── Build the ordered step list ───────────────────────────────────────────
  const steps = [];

  // Frontend checks
  steps.push({ label: "npm run check", command: "npm run check" });
  steps.push({ label: "npm run sync:i18n:check", command: "npm run sync:i18n:check" });
  steps.push({ label: "npm test", command: "npm test" });

  // Linux tauri deps (no command — handled inline)
  steps.push({ label: "linux tauri deps", command: null });

  // Clippy per workspace crate
  for (const crate of WORKSPACE_CRATES) {
    steps.push({
      label: `clippy ${crate}`,
      command: `cargo clippy -p ${crate} -- -D warnings`,
    });
  }

  // Clippy app crate
  steps.push({
    label: "clippy src-tauri",
    command: "cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings",
  });

  // Tests per crate
  for (const crate of TEST_CRATES) {
    steps.push({
      label: `test ${crate}`,
      command: `cargo test -p ${crate} --lib`,
    });
  }

  const total = steps.length;
  const tui = new BumpTUI(total);
  tui.start();

  tui.log("\x1b[1m[preflight] Starting checks...\x1b[0m");

  try {
    for (let i = 0; i < total; i++) {
      const { label, command } = steps[i];
      tui.setStep(i, label);

      const stepStart = Date.now();

      if (command === null) {
        // Special: linux tauri deps check
        ensureLinuxTauriDeps();
        const dt = Date.now() - stepStart;
        tui.recordStepTime(dt);
        tui.stepPassed(label, dt);
      } else {
        try {
          await runCommandStreaming(label, command, tui);
          const dt = Date.now() - stepStart;
          tui.recordStepTime(dt);
          tui.stepPassed(label, dt);
        } catch (err) {
          tui.stepFailed(label);
          tui.stop();
          console.error(`\n[preflight] ✗ ${label} — failed`);
          throw err;
        }
      }
    }

    // Final state
    tui.setStep(total, "done");
    const totalElapsed = Date.now() - tui.startTime;
    tui.log(`\n\x1b[1;32m[preflight] All ${total} checks passed in ${formatDuration(totalElapsed)}\x1b[0m\n`);

    // Brief pause so user can see the final state
    await new Promise((r) => setTimeout(r, 1500));
    tui.stop();
  } catch (err) {
    tui.stop();
    throw err;
  }
}

function todayIsoDate() {
  return new Date().toISOString().slice(0, 10);
}

function bumpChangelogUnreleased(changelogPath, version, date) {
  const changelog = readText(changelogPath);
  const unreleasedHeader = /^## \[Unreleased\]\s*$/m;

  if (!unreleasedHeader.test(changelog)) {
    throw new Error(`Could not find \"## [Unreleased]\" in ${changelogPath}`);
  }

  const replacement = `## [Unreleased]\n\n## [${version}] — ${date}`;
  const updated = changelog.replace(unreleasedHeader, replacement);
  writeText(changelogPath, updated);
}

// ── CLI flags ────────────────────────────────────────────────────────────────

function parseArgs() {
  const args = process.argv.slice(2);
  let dryRun = false;
  let versionArg = null;

  for (const a of args) {
    if (a === "--dry-run") {
      dryRun = true;
    } else if (a === "--help" || a === "-h") {
      console.log(
        [
          "Usage: npm run bump [-- [options] [version]]",
          "",
          "Options:",
          "  --dry-run   Run all preflight checks but skip version bumping",
          "  --help, -h  Show this help message",
          "",
          "If version is omitted the patch component is incremented automatically.",
        ].join("\n")
      );
      process.exit(0);
    } else if (!a.startsWith("-")) {
      versionArg = a;
    } else {
      throw new Error(`Unknown flag: ${a}`);
    }
  }

  return { dryRun, versionArg };
}

// ── main (async) ─────────────────────────────────────────────────────────────

async function main() {
  const { dryRun, versionArg } = parseArgs();

  // ── resolve new version ─────────────────────────────────────────────────────

  const pkg = JSON.parse(readText("package.json"));
  const currentVersion = pkg.version;

  const newVersion = versionArg ? validateVersion(versionArg) : bumpPatch(currentVersion);

  if (dryRun) {
    console.log(`[dry-run] Would bump  ${currentVersion}  ->  ${newVersion}\n`);
  } else {
    console.log(`Bumping  ${currentVersion}  ->  ${newVersion}\n`);
  }

  // ── preflight checks (must pass before any file is modified) ──────────────

  await runPreflightChecks();

  // ── In dry-run mode stop here — no files are modified ─────────────────────

  if (dryRun) {
    console.log(`\n[dry-run] All preflight checks passed. No files were modified.`);
    return;
  }

  // ── package.json ────────────────────────────────────────────────────────────

  // Re-read in case something changed
  const pkgFresh = JSON.parse(readText("package.json"));
  pkgFresh.version = newVersion;
  writeText("package.json", JSON.stringify(pkgFresh, null, 2) + "\n");
  console.log("  ✓  package.json");

  // ── src-tauri/tauri.conf.json ───────────────────────────────────────────────

  const tauriConfPath = "src-tauri/tauri.conf.json";
  const tauriConf = JSON.parse(readText(tauriConfPath));
  tauriConf.version = newVersion;
  writeText(tauriConfPath, JSON.stringify(tauriConf, null, 2) + "\n");
  console.log("  ✓  src-tauri/tauri.conf.json");

  // ── src-tauri/Cargo.toml ────────────────────────────────────────────────────

  const cargoPath = "src-tauri/Cargo.toml";
  let cargo = readText(cargoPath);

  const versionLine = /^version\s*=\s*"[^"]+"/m;
  if (!versionLine.test(cargo)) {
    throw new Error("Could not find package version in Cargo.toml");
  }
  cargo = cargo.replace(versionLine, `version = "${newVersion}"`);
  writeText(cargoPath, cargo);
  console.log("  ✓  src-tauri/Cargo.toml");

  // ── CHANGELOG.md — compile fragments ───────────────────────────────────────

  const date = todayIsoDate();
  const result = compileChangelog(newVersion, date);

  if (result.entryCount > 0) {
    console.log(
      `  ✓  CHANGELOG.md — compiled ${result.entryCount} entries (${result.categories.join(", ")})`
    );
    console.log(
      `  ✓  changes/releases/${newVersion}/ — archived ${result.categories.length} fragments`
    );
  } else {
    bumpChangelogUnreleased("CHANGELOG.md", newVersion, date);
    console.log("  ✓  CHANGELOG.md (Unreleased -> versioned section, no fragments)");
  }

  // ── regenerate Cargo.lock ───────────────────────────────────────────────────

  console.log("\nRegenerating Cargo.lock...");
  execSync("cargo generate-lockfile", { stdio: "inherit" });
  console.log("  ✓  Cargo.lock");

  // ── clean Rust build artifacts ──────────────────────────────────────────────

  console.log("\nCleaning Rust build artifacts...");
  execSync("npm run clean:rust", { stdio: "inherit" });
  console.log("  ✓  clean:rust");

  console.log(`\nDone! Version is now ${newVersion}`);
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
