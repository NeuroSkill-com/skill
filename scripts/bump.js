#!/usr/bin/env node
import { readFileSync, writeFileSync, openSync, readSync, closeSync } from "fs";
import { execSync } from "child_process";
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

function runCheckStep(label, command) {
  let output = "";
  try {
    // Merge stderr into stdout so we capture warnings from both streams
    output = execSync(`${command} 2>&1`, { encoding: "utf8" }) || "";
  } catch (err) {
    // execSync throws on non-zero exit — clear progress line, show output, re-throw
    process.stdout.write("\r\x1b[K");
    output = (err.stdout || "").toString();
    if (output) process.stdout.write(output);
    console.error(`\n[preflight] ✗ ${label} — command failed`);
    throw err;
  }

  // Detect warning lines in combined output — treat any warning as fatal.
  // Exclude:
  //  - "0 warnings" summary lines
  //  - config directives like `deny(warnings)` or `warnings =`
  //  - cargo build-script `warning: <crate>@<ver>:` info messages (cargo:warning= from build.rs)
  //  - "warning: build failed" (cargo's own message when a build error already occurred)
  const warningLines = output
    .split("\n")
    .filter(
      (line) =>
        /\bwarning\b/i.test(line) &&
        !/0 warnings/i.test(line) &&
        !/warnings?\s*=|deny\(warnings\)/i.test(line) &&
        !/^warning: \S+@\S+:/i.test(line.trim()) &&
        !/^warning: build failed/i.test(line.trim())
    );

  if (warningLines.length > 0) {
    process.stdout.write("\r\x1b[K");
    console.error(`[preflight] ✗ ${label} — ${warningLines.length} warning(s) detected:`);
    for (const w of warningLines.slice(0, 10)) {
      console.error(`  ${w.trim()}`);
    }
    if (output) process.stdout.write(output);
    throw new Error(`Bump aborted: warnings found during "${label}"`);
  }
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
    console.warn("\n[preflight] ⚠  Other cargo processes detected:");
    for (const l of lines) console.warn(`  ${l.trim()}`);
    console.warn(
      "\n  Cargo uses a global package-cache lock (~/.cargo/.package-cache)."
    );
    console.warn(
      "  The bump clippy steps will block until these finish.\n"
    );
    const rl = require("readline").createInterface({
      input: process.stdin,
      output: process.stdout,
    });
    const fd = openSync("/dev/tty", "r");
    const buf = Buffer.alloc(1);
    process.stdout.write("  Continue anyway? [y/N] ");
    rl.close();
    let answer = "";
    // Read characters synchronously from the terminal
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

// ── progress bar helpers ─────────────────────────────────────────────────────

function formatDuration(ms) {
  const totalSec = Math.round(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return min > 0 ? `${min}m ${sec}s` : `${sec}s`;
}

function printProgress(current, total, stepLabel, elapsedMs, etaMs) {
  const cols = process.stdout.columns || 80;
  const pct = Math.round((current / total) * 100);
  const barTotal = Math.max(10, Math.min(30, cols - 60));
  const filled = Math.round((current / total) * barTotal);
  const bar = "█".repeat(filled) + "░".repeat(barTotal - filled);

  const elapsed = formatDuration(elapsedMs);
  const eta = etaMs > 0 ? formatDuration(etaMs) : "--";
  const line = `  [${bar}] ${current}/${total} (${pct}%) | elapsed ${elapsed} | ETA ${eta} | ${stepLabel}`;

  // Overwrite current line
  process.stdout.write(`\r\x1b[K${line}`);
}

function runPreflightChecks() {
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
  const startAll = Date.now();
  const stepTimes = []; // elapsed ms per completed step

  for (let i = 0; i < total; i++) {
    const { label, command } = steps[i];

    // Calculate ETA from average step duration so far
    const avgMs = stepTimes.length > 0
      ? stepTimes.reduce((a, b) => a + b, 0) / stepTimes.length
      : 0;
    const remaining = total - i;
    const etaMs = stepTimes.length > 0 ? avgMs * remaining : 0;

    printProgress(i, total, label, Date.now() - startAll, etaMs);

    const stepStart = Date.now();

    if (command === null) {
      // Special: linux tauri deps check (no shell command)
      ensureLinuxTauriDeps();
    } else {
      runCheckStep(label, command);
    }

    stepTimes.push(Date.now() - stepStart);
  }

  // Final 100% progress
  const totalElapsed = Date.now() - startAll;
  printProgress(total, total, "done", totalElapsed, 0);
  console.log(`\n\n[preflight] All ${total} checks passed in ${formatDuration(totalElapsed)}`);
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

// ── resolve new version ───────────────────────────────────────────────────────

const pkg = JSON.parse(readText("package.json"));
const currentVersion = pkg.version;

const arg = process.argv[2];
const newVersion = arg ? validateVersion(arg) : bumpPatch(currentVersion);

console.log(`Bumping  ${currentVersion}  →  ${newVersion}`);

// ── preflight checks (must pass before any file is modified) ────────────────

runPreflightChecks();

// ── package.json ──────────────────────────────────────────────────────────────

pkg.version = newVersion;
writeText("package.json", JSON.stringify(pkg, null, 2) + "\n");
console.log("  ✓  package.json");

// ── src-tauri/tauri.conf.json ─────────────────────────────────────────────────

const tauriConfPath = "src-tauri/tauri.conf.json";
const tauriConf = JSON.parse(readText(tauriConfPath));
tauriConf.version = newVersion;
writeText(tauriConfPath, JSON.stringify(tauriConf, null, 2) + "\n");
console.log("  ✓  src-tauri/tauri.conf.json");

// ── src-tauri/Cargo.toml ──────────────────────────────────────────────────────
// Only the first `version = "..."` line belongs to the package itself.

const cargoPath = "src-tauri/Cargo.toml";
let cargo = readText(cargoPath);

// Replace the first occurrence only (the [package] version)
const versionLine = /^version\s*=\s*"[^"]+"/m;
if (!versionLine.test(cargo)) {
  throw new Error("Could not find package version in Cargo.toml");
}
cargo = cargo.replace(versionLine, `version = "${newVersion}"`);
writeText(cargoPath, cargo);
console.log("  ✓  src-tauri/Cargo.toml");

// ── CHANGELOG.md — compile fragments ─────────────────────────────────────────

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
  // No fragments — fall back to rotating the [Unreleased] header only
  bumpChangelogUnreleased("CHANGELOG.md", newVersion, date);
  console.log("  ✓  CHANGELOG.md (Unreleased → versioned section, no fragments)");
}

// ── regenerate Cargo.lock ─────────────────────────────────────────────────────
// The version change in Cargo.toml invalidates the lockfile.  Regenerate it
// so CI `--locked` builds don't fail.

console.log("\nRegenerating Cargo.lock...");
execSync("cargo generate-lockfile", { stdio: "inherit" });
console.log("  ✓  Cargo.lock");

// ── clean Rust build artifacts ────────────────────────────────────────────────

console.log("\nCleaning Rust build artifacts...");
execSync("npm run clean:rust", { stdio: "inherit" });
console.log("  ✓  clean:rust");

console.log(`\nDone! Version is now ${newVersion}`);
