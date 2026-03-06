#!/usr/bin/env node
import { readFileSync, writeFileSync } from "fs";

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

// ── resolve new version ───────────────────────────────────────────────────────

const pkg = JSON.parse(readText("package.json"));
const currentVersion = pkg.version;

const arg = process.argv[2];
const newVersion = arg ? validateVersion(arg) : bumpPatch(currentVersion);

console.log(`Bumping  ${currentVersion}  →  ${newVersion}`);

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

console.log(`\nDone! Version is now ${newVersion}`);
