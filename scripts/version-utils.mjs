// SPDX-License-Identifier: GPL-3.0-only
//
// Shared SemVer-with-RC helpers used by bump.js, release.js, and tests.
//
// Versions in this project are either:
//   - x.y.z         (stable)
//   - x.y.z-rc.N    (release candidate)
//
// Anything else is rejected.

export const VERSION_RE = /^(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$/;

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
