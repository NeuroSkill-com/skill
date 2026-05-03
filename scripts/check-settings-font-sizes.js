#!/usr/bin/env node
//
// check-settings-font-sizes.js — guard against font-size drift in
// src/lib/settings/*.svelte.
//
// Settings tabs should size text via the `text-ui-{xs,sm,base,md,lg,xl}` scale
// only. Bare Tailwind sizes (`text-xs`, `text-base`, `text-lg`, `text-2xl`,
// `text-[10px]`, …) cause visual inconsistency between tabs and were the
// motivation for introducing the `text-ui-*` system.
//
// We don't fix the existing 200+ pre-existing violations — that's a separate
// cleanup pass. Instead this script snapshots the current per-file violation
// counts in `check-settings-font-sizes.baseline.json` and fails if any file's
// count grows or a previously-clean file gains a violation. New files must
// start at zero.
//
// Refresh the baseline after an intentional cleanup with:
//   node scripts/check-settings-font-sizes.js --update
//
// Allowed:   text-ui-xs | text-ui-sm | text-ui-base | text-ui-md | text-ui-lg | text-ui-xl
// Violation: text-(xs|sm|base|lg|xl|<digit>xl|[<arbitrary>])

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const SETTINGS_DIR = path.resolve("src/lib/settings");
const BASELINE_PATH = path.resolve("scripts/check-settings-font-sizes.baseline.json");

// `(?!ui-)` skips `text-ui-*`. Trailing `\b` works for word-char endings;
// the alternation includes `\[…\]` for arbitrary values.
const VIOLATION_RE = /text-(?!ui-)((?:\d?xl|xs|sm|base|lg|xl)\b|\[[^\]]+\])/g;

function listSettingsTabs() {
  return readdirSync(SETTINGS_DIR)
    .filter((f) => f.endsWith(".svelte"))
    .sort();
}

function countViolations(filePath) {
  const src = readFileSync(filePath, "utf8");
  return (src.match(VIOLATION_RE) ?? []).length;
}

function currentCounts() {
  const out = {};
  for (const file of listSettingsTabs()) {
    out[file] = countViolations(path.join(SETTINGS_DIR, file));
  }
  return out;
}

function loadBaseline() {
  try {
    return JSON.parse(readFileSync(BASELINE_PATH, "utf8"));
  } catch {
    return null;
  }
}

const update = process.argv.includes("--update");
const counts = currentCounts();

if (update) {
  writeFileSync(BASELINE_PATH, `${JSON.stringify(counts, null, 2)}\n`);
  console.log(`✅ baseline written: ${BASELINE_PATH}`);
  process.exit(0);
}

const baseline = loadBaseline();
if (!baseline) {
  writeFileSync(BASELINE_PATH, `${JSON.stringify(counts, null, 2)}\n`);
  console.log(`📌 baseline initialised: ${BASELINE_PATH}`);
  process.exit(0);
}

const regressions = [];
for (const [file, count] of Object.entries(counts)) {
  const prev = baseline[file];
  if (prev === undefined && count > 0) {
    regressions.push(`  ✗ ${file}: new file with ${count} non-ui- text size(s)`);
  } else if (prev !== undefined && count > prev) {
    regressions.push(`  ✗ ${file}: ${prev} → ${count} non-ui- text size(s)`);
  }
}

if (regressions.length > 0) {
  console.error("❌ settings tab font-size regressions:");
  console.error(regressions.join("\n"));
  console.error(
    "\nUse the `text-ui-{xs,sm,base,md,lg,xl}` scale instead of bare Tailwind sizes.\n" +
      "If the change is intentional (e.g. removed an outlier), refresh the baseline:\n" +
      "  node scripts/check-settings-font-sizes.js --update",
  );
  process.exit(1);
}

// Surface drops too — they're not failures, but worth knowing.
const drops = [];
for (const [file, prev] of Object.entries(baseline)) {
  const cur = counts[file];
  if (cur === undefined) continue;
  if (cur < prev) drops.push(`  ✓ ${file}: ${prev} → ${cur}`);
}
if (drops.length > 0) {
  console.log("ℹ︎ settings tab font-size violations decreased — refresh baseline to lock in:");
  console.log(drops.join("\n"));
  console.log("  node scripts/check-settings-font-sizes.js --update");
}

console.log("✅ no settings tab font-size regressions");
