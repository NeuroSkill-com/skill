#!/usr/bin/env node
// Lint rule: forbid arbitrary Tailwind text-size classes below the
// design-system minimum (`text-ui-2xs` = 11px / 0.6875rem).
//
// Background: a visual-layout audit (see `src/tests/visual-layout.spec.ts`)
// flagged 1680 instances of computed font-size < 9px across 270 viewport
// combinations. The root cause was a mix of (a) too-small design tokens
// (since fixed) and (b) ad-hoc `text-[0.42rem]` style overrides bypassing
// the system. This script catches future (b)-class regressions.
//
// What we forbid:
//   • `text-[N rem]` where N < 0.6875       (≈ 11px floor)
//   • `text-[Npx]`   where N < 11
// What we allow:
//   • `text-ui-2xs` … `text-ui-xl`          (design-system tokens)
//   • `text-[≥0.6875rem]` and `text-[≥11px]` (arbitrary but readable)
//   • `text-xs` / `text-sm` / `text-base` …  (Tailwind defaults — sized
//     at 0.75rem+ which already meets the floor)
//
// Run via:   node scripts/lint-tiny-text.mjs
// CI hook:   wired into scripts/test-all.sh as the `tiny-text` suite.

import * as fs from "node:fs";
import * as path from "node:path";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const SRC = path.join(ROOT, "src");

// Minimum readable size in pixels. Matches `--text-ui-2xs` in app.css.
// Don't lower without re-running the visual-layout audit.
const MIN_PX = 11;

// File extensions that can carry Tailwind class strings.
const EXTS = new Set([".svelte", ".ts", ".tsx", ".js", ".jsx", ".html", ".astro"]);

const SKIP_DIRS = new Set(["node_modules", ".svelte-kit", "build", "dist", "test-results"]);

/** rem-or-px arbitrary text size. Captures the inner value verbatim. */
const TEXT_ARBITRARY = /\btext-\[\s*([0-9]+(?:\.[0-9]+)?)\s*(rem|px|em)\s*\]/g;

function pixelsFor(value, unit) {
  switch (unit) {
    case "px":
      return value;
    case "rem":
    case "em":
      return value * 16; // 1rem = 1em (in this context) = 16px at default html font-size
    default:
      return Number.NaN;
  }
}

function* walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith(".")) continue;
    if (SKIP_DIRS.has(entry.name)) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(full);
    else if (EXTS.has(path.extname(entry.name))) yield full;
  }
}

const hits = [];
let scanned = 0;

for (const file of walk(SRC)) {
  scanned++;
  const text = fs.readFileSync(file, "utf8");
  // Cheap pre-filter: skip files with no `text-[` at all.
  if (!text.includes("text-[")) continue;

  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    let m;
    const re = new RegExp(TEXT_ARBITRARY.source, "g");
    while ((m = re.exec(line)) !== null) {
      const value = Number.parseFloat(m[1]);
      const unit = m[2];
      const px = pixelsFor(value, unit);
      if (Number.isNaN(px)) continue;
      if (px < MIN_PX) {
        hits.push({
          file: path.relative(ROOT, file),
          line: i + 1,
          col: m.index + 1,
          className: m[0],
          pixels: px,
        });
      }
    }
  }
}

const banner = "── tiny-text lint ──";
if (hits.length === 0) {
  console.log(`${banner} ${scanned} files scanned, 0 violations`);
  process.exit(0);
}

console.error(`${banner} ${hits.length} violation${hits.length === 1 ? "" : "s"} (minimum ${MIN_PX}px):`);
console.error("");
for (const h of hits) {
  console.error(`  ${h.file}:${h.line}:${h.col}  ${h.className}  → ${h.pixels.toFixed(2)}px`);
}
console.error("");
console.error("Use a design-system token instead:");
console.error("  text-ui-2xs (11px)  text-ui-xs (12px)  text-ui-sm (13px)");
console.error("  text-ui-base (14px) text-ui-md (15px)  text-ui-lg (16px)  text-ui-xl (18px)");
console.error("");
console.error(`If a smaller size is genuinely required, raise the floor in scripts/lint-tiny-text.mjs (currently ${MIN_PX}px)`);
console.error("after running src/tests/visual-layout.spec.ts to confirm readability across viewports.");
process.exit(1);
