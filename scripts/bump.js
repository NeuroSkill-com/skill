#!/usr/bin/env node
import { readFileSync, writeFileSync } from "fs";
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
  console.log(`\n[preflight] ${label}`);
  execSync(command, { stdio: "inherit" });
  console.log(`[preflight] ✓ ${label}`);
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

  const requiredPackages = ["webkit2gtk-4.1", "javascriptcoregtk-4.1", "libsoup-3.0"];
  const missingPackages = requiredPackages.filter((name) => !hasPkgConfig(name));

  if (missingPackages.length === 0) return;

  const missingList = missingPackages.join(", ");
  throw new Error(
    [
      `Missing Linux Tauri system dependencies (${missingList}).`,
      "Install required packages before running npm run bump:",
      "  sudo apt install -y libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev",
      "If those are unavailable on your distro image, see LINUX.md for legacy alternatives.",
    ].join("\n")
  );
}

function runPreflightChecks() {
  console.log("Running preflight checks before bump...");
  runCheckStep("npm run check", "npm run check");
  console.log("\n[preflight] linux tauri deps (pkg-config)");
  ensureLinuxTauriDeps();
  console.log("[preflight] ✓ linux tauri deps (pkg-config)");
  runCheckStep("cargo clippy (src-tauri)", "cargo clippy --manifest-path src-tauri/Cargo.toml");
  runCheckStep("npm run sync:i18n:check", "npm run sync:i18n:check");
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

console.log(`\nDone! Version is now ${newVersion}`);
