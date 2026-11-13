#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Shrink src-tauri/target without losing anything that matters.
//
//   npm run clean:rust:debug [-- --dry-run] [-- --rebuild]
//
// target/debug grows with every profile or feature variant that is built, and a
// clippy run next to a test build roughly doubles it (about 7 GB -> 17 GB). Cargo
// never prunes the stale generations. Wiping the debug directory and rebuilding
// leaves only the current set; with kache warm that rebuild takes a few minutes.
// Release output, the DuckDB sources and the cargo registry are left alone.
//
//   --dry-run  print what would be removed, change nothing
//   --rebuild  after the wipe, run `cargo test --workspace --no-run` to refill the cache

import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, rmSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TARGET = join(ROOT, "src-tauri", "target");
const REMOVE = ["debug", "tmp"].map((name) => join(TARGET, name));

function dirSize(dir) {
  let total = 0;
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const entry of entries) {
    const path = join(dir, entry.name);
    try {
      if (entry.isSymbolicLink()) continue;
      total += entry.isDirectory() ? dirSize(path) : statSync(path).size;
    } catch {}
  }
  return total;
}

const fmt = (bytes) =>
  bytes >= 1024 ** 3 ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : `${(bytes / 1024 ** 2).toFixed(0)} MB`;

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const rebuild = args.includes("--rebuild");
for (const a of args) {
  if (!["--dry-run", "--rebuild"].includes(a)) {
    console.error(`Unknown flag: ${a}\nUsage: npm run clean:rust:debug [-- --dry-run] [-- --rebuild]`);
    process.exit(1);
  }
}

const present = REMOVE.filter((dir) => existsSync(dir));
const before = present.reduce((sum, dir) => sum + dirSize(dir), 0);
if (present.length === 0) {
  console.log("[slim-target] nothing to remove: no debug or tmp directory under src-tauri/target.");
} else {
  for (const dir of present)
    console.log(`[slim-target] ${dryRun ? "would remove" : "removing"} ${dir} (${fmt(dirSize(dir))})`);
  if (!dryRun) for (const dir of present) rmSync(dir, { recursive: true, force: true });
}

if (dryRun) {
  console.log(`[slim-target] dry run: ${fmt(before)} would be reclaimed.`);
  process.exit(0);
}
console.log(`[slim-target] reclaimed ${fmt(before)}.`);

if (rebuild) {
  console.log("[slim-target] rebuilding the test targets: cargo test --workspace --no-run");
  const result = spawnSync("cargo", ["test", "--workspace", "--no-run"], { cwd: ROOT, stdio: "inherit" });
  if (result.status !== 0) process.exit(result.status ?? 1);
  console.log(`[slim-target] target/debug is now ${fmt(dirSize(join(TARGET, "debug")))}.`);
}
