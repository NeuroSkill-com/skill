#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Check that the local Rust toolchain meets the minimum version.
// Called by dev:guard and test-all.sh to catch version mismatches early.

import { execSync } from "child_process";

const MIN_MAJOR = 1;
const MIN_MINOR = 95;
const MIN_VERSION = `${MIN_MAJOR}.${MIN_MINOR}`;

try {
  const output = execSync("rustc --version", { encoding: "utf8" }).trim();
  const match = output.match(/rustc (\d+)\.(\d+)/);
  if (!match) {
    console.warn(`\x1b[33m⚠ Could not parse Rust version from: ${output}\x1b[0m`);
    process.exit(0);
  }

  const [, major, minor] = match.map(Number);
  if (major < MIN_MAJOR || (major === MIN_MAJOR && minor < MIN_MINOR)) {
    console.error(`\x1b[31m✗ Rust ${major}.${minor} is below minimum ${MIN_VERSION}\x1b[0m`);
    console.error(`  Run: rustup update stable`);
    console.error(`  CI uses the latest stable (currently ${MIN_VERSION}+) — clippy lints may differ.`);
    process.exit(1);
  }

  // Silent on success for dev:guard speed
  if (process.argv.includes("--verbose")) {
    console.log(`✓ Rust ${major}.${minor} (>= ${MIN_VERSION})`);
  }
} catch {
  console.warn("\x1b[33m⚠ rustc not found — install Rust: https://rustup.rs\x1b[0m");
  process.exit(0); // Don't block frontend-only dev
}
