#!/usr/bin/env node
import { execSync, spawn } from "node:child_process";
import { closeSync, existsSync, openSync, readFileSync, readSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { compileChangelog, validateUnreleasedFragments } from "./compile-changelog.js";

// ── helpers ──────────────────────────────────────────────────────────────────

function readText(path) {
  return readFileSync(path, "utf8");
}

function writeText(path, content) {
  writeFileSync(path, content, "utf8");
}

function bumpPatch(version) {
  const parts = version.split(".").map(Number);
  if (parts.length !== 3 || parts.some(Number.isNaN)) {
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
    ].join("\n"),
  );
}

/**
 * Check if the current version has been tagged and pushed to remote.
 * This prevents accidental multiple bumps before the previous version is tagged.
 */
function checkVersionTagged(version) {
  const tagName = `v${version}`;

  // Check if tag exists locally
  try {
    execSync(`git rev-parse ${tagName}`, { stdio: "ignore" });
  } catch {
    throw new Error(
      `Bump aborted: Current version ${version} is not tagged locally.` +
        `\nRun 'npm run tag' to create and push the tag before bumping to a new version.`,
    );
  }

  // Check if tag exists on at least one remote
  try {
    const remotes = execSync("git remote", { encoding: "utf8" })
      .split("\n")
      .map((name) => name.trim())
      .filter(Boolean);

    let foundOnRemote = false;

    for (const remote of remotes) {
      try {
        // Check if the tag exists on this remote
        execSync(`git ls-remote --tags --exit-code ${remote} refs/tags/${tagName}`, { stdio: "ignore" });
        foundOnRemote = true;
        break;
      } catch {
        // Tag not on this remote, try next one
      }
    }

    if (!foundOnRemote) {
      throw new Error(
        `Bump aborted: Tag ${tagName} exists locally but not on any remote.` +
          `\nRun 'git push --tags' or 'npm run tag' to push the tag before bumping to a new version.`,
      );
    }
  } catch (err) {
    if (err.message.includes("Bump aborted")) {
      throw err;
    }
    throw new Error(`Bump aborted: Could not verify if tag ${tagName} exists on remote: ${err.message}`);
  }
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
// Keep in sync with scripts/test-fast.sh tiers and ci.yml INT_CRATES.
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
  "skill-jobs",
  "skill-exg",
  "skill-label-index",
  "skill-skills",
  "skill-commands",
  "skill-screenshots",
];

function checkForCompetingCargo() {
  try {
    const out = execSync(`ps -eo pid,command | grep -E '[c]argo (build|clippy|check|test|install|publish)' || true`, {
      encoding: "utf8",
    }).trim();
    if (!out) return;
    const lines = out.split("\n").filter(Boolean);
    if (lines.length === 0) return;
    for (const l of lines) {
      console.log(`    ${l}`);
    }
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
        !/^warning: build failed/i.test(line.trim()) &&
        !/DeprecationWarning|--trace-deprecation/i.test(line),
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
    process.stdout.write(`${CSI}s`); // save
    process.stdout.write(`${CSI}${scrollEnd};1H`); // move to bottom of scroll area
    process.stdout.write("\n"); // scroll up if needed
    // Write each line — handle multi-line text
    const lines = text.split("\n");
    for (let i = 0; i < lines.length; i++) {
      if (i > 0) {
        process.stdout.write(`${CSI}${scrollEnd};1H\n`);
      }
      process.stdout.write(lines[i]);
    }
    process.stdout.write(`${CSI}u`); // restore
    this._renderBar(); // redraw bar in case scroll corrupted it
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
    const avgMs = this.stepTimes.length > 0 ? this.stepTimes.reduce((a, b) => a + b, 0) / this.stepTimes.length : 0;
    const remaining = total - current;
    const etaMs = this.stepTimes.length > 0 ? avgMs * remaining : 0;

    const pct = total > 0 ? Math.round((current / total) * 100) : 0;
    const barWidth = Math.max(10, Math.min(30, cols - 60));
    const filled = Math.round((current / total) * barWidth);
    const bar = `\x1b[32m${"█".repeat(filled)}\x1b[0m\x1b[2m${"░".repeat(barWidth - filled)}\x1b[0m`;

    const elapsed = formatDuration(elapsedMs);
    const eta = etaMs > 0 ? formatDuration(etaMs) : "--";

    const sepRow = rows - 1;
    const barRow = rows;

    // Separator line
    const sep = `\x1b[2m${"─".repeat(cols)}\x1b[0m`;

    // Progress line
    const stepText = this.stepLabel.length > 30 ? `${this.stepLabel.slice(0, 29)}…` : this.stepLabel;
    const progressLine = `  [${bar}] ${current}/${total} (${pct}%) | ${elapsed} elapsed | ETA ${eta} | ${stepText}`;

    // Draw separator and progress bar (outside scroll region)
    process.stdout.write(`${CSI}${sepRow};1H${CSI}K${sep}`);
    process.stdout.write(`${CSI}${barRow};1H${CSI}K${progressLine}`);
  }
}

// ── run a command with streamed output ───────────────────────────────────────

function runCommandStreaming(label, command, tui) {
  return new Promise((resolve, reject) => {
    const _parts = command.split(" ");
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

  // Clippy all workspace crates in a single invocation
  {
    const pkgFlags = WORKSPACE_CRATES.map((c) => `-p ${c}`).join(" ");
    steps.push({
      label: `clippy workspace (${WORKSPACE_CRATES.length} crates)`,
      command: `cargo clippy ${pkgFlags} -- -D warnings`,
    });
  }

  // Clippy app crate (depends on workspace artifacts already built above)
  steps.push({
    label: "clippy src-tauri",
    command: "cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings",
  });

  // Test all test crates (unit + integration tests)
  {
    const pkgFlags = TEST_CRATES.map((c) => `-p ${c}`).join(" ");
    steps.push({
      label: `test workspace (${TEST_CRATES.length} crates)`,
      command: `cargo test ${pkgFlags}`,
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

function releaseFilePath(version) {
  return `changes/releases/${version}.md`;
}

// ── CLI flags ────────────────────────────────────────────────────────────────

function parseArgs() {
  const args = process.argv.slice(2);
  let dryRun = false;
  let clean = false;
  let versionArg = null;
  let force = false;

  for (const a of args) {
    if (a === "--dry-run") {
      dryRun = true;
    } else if (a === "--clean") {
      clean = true;
    } else if (a === "--force") {
      force = true;
    } else if (a === "--help" || a === "-h") {
      console.log(`
Usage: npm run bump [version] [--dry-run] [--clean] [--force]

Arguments:
  version       Optional specific version (e.g., 1.2.3). If not provided, patches current version.

Flags:
  --dry-run     Run all checks without making any changes
  --clean       Clean Rust build artifacts after successful bump
  --force       Bypass version tag check (use with caution)

Note: By default, bump will refuse to run if the current version is not tagged
      and pushed to the remote. This prevents accidental multiple bumps. Use --force
      to override this safety check.
`);
      process.exit(0);
    } else if (!a.startsWith("-")) {
      versionArg = a;
    } else {
      throw new Error(`Unknown flag: ${a}`);
    }
  }

  return { dryRun, clean, versionArg, force };
}

// ── auto-generate changelog fragment from git log ────────────────────────────

function generateFragmentFromGitLog(currentVersion) {
  const UNRELEASED_DIR = "changes/unreleased";

  // Always generate from commit history since last version bump
  // Find the last version bump commit (message exactly matches version string)
  let range = "";
  try {
    // Find the most recent commit whose message is exactly the current version
    const sha = execSync(`git log --all --format=%H --grep="^${currentVersion}$" --fixed-strings -1`, {
      encoding: "utf8",
    }).trim();
    if (sha) {
      range = `${sha}..HEAD`;
    }
  } catch {
    // ignore
  }

  // If we couldn't find a version bump commit, use the last version tag
  if (!range) {
    try {
      // Try to find the last version tag
      const lastTag = execSync("git describe --tags --abbrev=0 2>/dev/null", { encoding: "utf8" }).trim();
      if (lastTag?.startsWith("v")) {
        range = `${lastTag}..HEAD`;
      }
    } catch {
      // ignore
    }
  }

  // Collect commit subjects (skip merge commits)
  let logCmd = "git log --no-merges --format=%B";
  if (range) {
    logCmd += ` ${range}`;
  }

  let commits;
  try {
    commits = execSync(logCmd, { encoding: "utf8" }).trim();
  } catch {
    commits = "";
  }

  // Parse commit messages and create bullet points
  const commitMessages = commits.split("\n\n").filter(Boolean);
  const bullets = [];
  const seen = new Set();

  for (const commit of commitMessages) {
    const lines = commit.split("\n").filter(Boolean);
    if (lines.length === 0) continue;

    const subject = lines[0];

    // Skip version-only commits
    if (/^\d+\.\d+\.\d+$/.test(subject)) continue;

    // Skip if we've already seen this subject
    if (seen.has(subject)) continue;
    seen.add(subject);

    // Use the full commit message if it's a conventional commit
    // Otherwise just use the subject line
    if (/^(feat|fix|chore|docs|style|refactor|perf|test|build|ci|revert|WIP):/.test(subject)) {
      bullets.push(`- ${commit.replace(/\n/g, " ").replace(/\s+/g, " ").trim()}`);
    } else {
      bullets.push(`- ${subject}`);
    }
  }

  if (bullets.length === 0) {
    bullets.push("- Minor updates and improvements");
  }

  const fragment = `### Features\n\n${bullets.join("\n")}\n`;

  // Write the auto-generated fragment
  if (!existsSync(UNRELEASED_DIR)) {
    execSync(`mkdir -p ${UNRELEASED_DIR}`);
  }
  writeText(join(UNRELEASED_DIR, "auto-git-log.md"), fragment);
  console.log(`[bump] Generated changelog fragment with ${bullets.length} entries from git log`);
}

// ── main (async) ─────────────────────────────────────────────────────────────

async function main() {
  const { dryRun, clean, versionArg, force } = parseArgs();

  // ── resolve new version ─────────────────────────────────────────────────────

  const pkg = JSON.parse(readText("package.json"));
  const currentVersion = pkg.version;

  // ── Check if current version is already tagged and pushed ─────────────────
  if (!force) {
    checkVersionTagged(currentVersion);
  } else {
    console.log("[bump] --force flag used: skipping version tag check");
  }

  const newVersion = versionArg ? validateVersion(versionArg) : bumpPatch(currentVersion);

  // ── Prevent accidental overwrite of an existing archived release ───────────

  const existingReleasePath = releaseFilePath(newVersion);
  if (existsSync(existingReleasePath)) {
    throw new Error(`Bump aborted: ${existingReleasePath} already exists.`);
  }

  // ── Validate changelog fragments early (fast fail) ─────────────────────────

  const validated = validateUnreleasedFragments();
  if (validated.files.length === 0) {
    console.log("[bump] No changelog fragments found \u2014 generating from git commit history\u2026");
    generateFragmentFromGitLog(currentVersion);
  } else {
    console.log(
      `[bump] Found ${validated.files.length} changelog fragment${validated.files.length === 1 ? "" : "s"} — will use these instead of git log`,
    );
  }

  // ── preflight checks (must pass before any file is modified) ──────────────

  await runPreflightChecks();

  // ── In dry-run mode stop here — no files are modified ─────────────────────

  if (dryRun) {
    return;
  }

  // ── Snapshot git state so we can revert on failure ─────────────────────────
  //
  // Stash any uncommitted changes (there shouldn't be any blocking ones, but
  // be safe), then record HEAD so we know where to reset if the bump fails
  // after files have been mutated.

  const headBefore = execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();

  // Track whether we've started mutating files
  let mutationStarted = false;

  try {
    mutationStarted = true;

    // ── package.json ──────────────────────────────────────────────────────────

    // Re-read in case something changed
    const pkgFresh = JSON.parse(readText("package.json"));
    pkgFresh.version = newVersion;
    writeText("package.json", `${JSON.stringify(pkgFresh, null, 2)}\n`);

    // ── src-tauri/tauri.conf.json ─────────────────────────────────────────────

    const tauriConfPath = "src-tauri/tauri.conf.json";
    const tauriConf = JSON.parse(readText(tauriConfPath));
    tauriConf.version = newVersion;
    writeText(tauriConfPath, `${JSON.stringify(tauriConf, null, 2)}\n`);

    // ── src-tauri/Cargo.toml ──────────────────────────────────────────────────

    const cargoPath = "src-tauri/Cargo.toml";
    let cargo = readText(cargoPath);

    const versionLine = /^version\s*=\s*"[^"]+"/m;
    if (!versionLine.test(cargo)) {
      throw new Error("Could not find package version in Cargo.toml");
    }
    cargo = cargo.replace(versionLine, `version = "${newVersion}"`);
    writeText(cargoPath, cargo);

    // ── CHANGELOG.md — compile fragments ─────────────────────────────────────

    const date = todayIsoDate();
    const result = compileChangelog(newVersion, date);

    if (result.entryCount === 0) {
      throw new Error(
        "Bump aborted: no changelog entries were compiled. Ensure changes/unreleased contains valid markdown fragments.",
      );
    }

    console.log(
      `\n[bump] Compiled ${result.entryCount} changelog entr${result.entryCount === 1 ? "y" : "ies"} from ${result.consumedFiles.length} fragment${result.consumedFiles.length === 1 ? "" : "s"}:`,
    );
    for (const file of result.consumedFiles) {
      console.log(`[bump]   - ${file}`);
    }
    console.log("[bump] Category counts:");
    for (const [category, count] of Object.entries(result.categoryCounts)) {
      console.log(`[bump]   - ${category}: ${count}`);
    }
    execSync("cargo generate-lockfile", { stdio: "inherit" });

    // ── optionally clean Rust build artifacts to free disk space ──────────────

    if (clean) {
      execSync("npm run clean:rust", { stdio: "inherit" });
    }

    // ── regenerate derived indexes ─────────────────────────────────────────────

    execSync("npm run build:settings-index", { stdio: "inherit" });

    // ── create bump commit ────────────────────────────────────────────────────

    execSync("git add -A", { stdio: "inherit" });
    execSync(`git commit -m "${newVersion}"`, { stdio: "inherit" });

    console.log(`\n\x1b[1;32m[bump] ✓ ${newVersion} committed successfully\x1b[0m`);
  } catch (err) {
    // ── Revert all file mutations on failure ──────────────────────────────────
    if (mutationStarted) {
      console.error(`\n\x1b[1;31m[bump] Failed — reverting all changes…\x1b[0m`);
      try {
        // Reset tracked files to pre-bump state
        execSync(`git checkout -- .`, { stdio: "inherit" });
        // Remove any untracked files created during bump (e.g. release archive)
        execSync(`git clean -fd changes/releases`, { stdio: "ignore" });
      } catch (revertErr) {
        console.error(`\x1b[31m[bump] Revert failed: ${revertErr.message}. Manual cleanup may be needed.\x1b[0m`);
        console.error(`[bump] To restore manually: git reset --hard ${headBefore}`);
      }
    }
    throw err;
  }
}

main().catch((err) => {
  if (err?.message) {
    console.error(`\x1b[31m${err.message}\x1b[0m`);
  }
  process.exit(1);
});
