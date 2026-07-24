// SPDX-License-Identifier: GPL-3.0-only
//
// Shared SemVer-with-RC helpers used by bump.js, release.js, and tests.
//
// Versions in this project are either:
//   - x.y.z         (stable)
//   - x.y.z-rc.N    (release candidate)
//
// Anything else is rejected.
//
// Source of truth: repo-root `VERSION` (one line, no "v" prefix).
// bump.js writes VERSION then syncs package.json / tauri.conf.json /
// src-tauri/Cargo.toml from it.

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const VERSION_RE = /^(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$/;

const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
export const VERSION_FILE = join(REPO_ROOT, "VERSION");

/** @param {string} version */
export function parseVersion(version) {
  const m = String(version).match(VERSION_RE);
  if (!m) throw new Error(`Invalid version "${version}"`);
  return {
    major: Number(m[1]),
    minor: Number(m[2]),
    patch: Number(m[3]),
    rc: m[4] === undefined ? null : Number(m[4]),
  };
}

/** @param {string} v */
export function validateVersion(v) {
  if (!VERSION_RE.test(v)) {
    throw new Error(`Version must be x.y.z or x.y.z-rc.N, got "${v}"`);
  }
  return v;
}

/**
 * Read the product version from the repo-root VERSION file.
 * @param {string} [root]
 */
export function readVersionFile(root = REPO_ROOT) {
  const path = join(root, "VERSION");
  if (!existsSync(path)) {
    throw new Error(`VERSION file missing at ${path}`);
  }
  const raw = readFileSync(path, "utf8").trim();
  const line = raw.split(/\r?\n/)[0]?.trim() ?? "";
  if (!line) throw new Error(`VERSION file is empty (${path})`);
  return validateVersion(line);
}

/**
 * Write the product version to the repo-root VERSION file (single line + newline).
 * @param {string} version
 * @param {string} [root]
 */
export function writeVersionFile(version, root = REPO_ROOT) {
  const v = validateVersion(version);
  writeFileSync(join(root, "VERSION"), `${v}\n`, "utf8");
  return v;
}

/**
 * Compute the next version.
 *
 *   bumpVersion("0.5.0",       { rc: false }) → "0.5.1"
 *   bumpVersion("0.5.0",       { rc: true  }) → "0.5.1-rc.1"
 *   bumpVersion("0.5.1-rc.1",  { rc: true  }) → "0.5.1-rc.2"
 *   bumpVersion("0.5.1-rc.3",  { rc: false }) → "0.5.2"   // start next stable cycle
 *
 * @param {string} version
 * @param {{ rc?: boolean }} [opts]
 */
export function bumpVersion(version, { rc = false } = {}) {
  const { major, minor, patch, rc: rcN } = parseVersion(version);
  const onRc = rcN !== null;
  if (rc) {
    return onRc
      ? `${major}.${minor}.${patch}-rc.${rcN + 1}`
      : `${major}.${minor}.${patch + 1}-rc.1`;
  }
  return `${major}.${minor}.${patch + 1}`;
}

/**
 * Strip an `-rc.N` suffix to get the base version (e.g. for branch names).
 * @param {string} version
 */
export function baseVersion(version) {
  const { major, minor, patch } = parseVersion(version);
  return `${major}.${minor}.${patch}`;
}

/**
 * True if the version string carries an RC pre-release suffix.
 * @param {string} version
 */
export function isRc(version) {
  return parseVersion(version).rc !== null;
}
