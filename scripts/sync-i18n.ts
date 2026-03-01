#!/usr/bin/env tsx
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * sync-i18n.ts — i18n key synchronisation utility
 *
 * Usage:
 *   npx tsx scripts/sync-i18n.ts            # show report
 *   npx tsx scripts/sync-i18n.ts --fix      # write English fallbacks for missing keys
 *   npx tsx scripts/sync-i18n.ts --check    # exit 1 if any keys are missing (for CI)
 *
 * The script diffs every locale file against en.ts (the source of truth).
 * Missing keys are reported in a table; with --fix they are appended to the
 * locale file using the English value as a fallback (clearly marked with a
 * TODO comment so translators know what to update).
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname  = path.dirname(__filename);

const I18N_DIR = path.resolve(__dirname, "../src/lib/i18n");
const LOCALES  = ["de", "fr", "he", "uk"];

// ── Key extraction ────────────────────────────────────────────────────────────

/** Extracts {key → value} map from a .ts locale file.
 *  Handles both:  "key.name":   "value"
 *  and:           "key.name":   `template string`
 */
function extractKeys(filePath: string): Map<string, string> {
  const src = fs.readFileSync(filePath, "utf8");
  const map = new Map<string, string>();
  // Match "key":  "value" or "key": `value`
  const re = /^\s+"([^"]+)":\s+(?:"((?:[^"\\]|\\.)*)"|`((?:[^`\\]|\\.)*)`)/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const key = m[1];
    const val = m[2] !== undefined ? m[2] : m[3];
    map.set(key, val);
  }
  return map;
}

// ── Fix: append missing keys to a locale file ─────────────────────────────────

function appendMissingKeys(
  filePath:   string,
  missing:    Map<string, string>,
  langCode:   string,
): void {
  let src = fs.readFileSync(filePath, "utf8");

  // Remove the closing `};` to allow appending.
  const closingIdx = src.lastIndexOf("};");
  if (closingIdx === -1) {
    console.error(`  Could not find closing "};" in ${path.basename(filePath)}`);
    return;
  }

  const lines: string[] = [
    "",
    `  // ── Auto-synced from en.ts (${new Date().toISOString().slice(0, 10)}) ──`,
    `  // TODO: translate the following ${missing.size} key(s) into ${langCode}`,
  ];

  for (const [key, val] of missing) {
    // val is captured raw from a double-quoted TS string literal, so it already
    // contains the correct escape sequences (e.g. \" for an embedded quote, \\
    // for a literal backslash).  Re-applying escaping would double-encode those
    // sequences (\" → \\\" ) and corrupt every string that contains them.
    lines.push(`  "${key}": "${val}",`);
  }

  const newSrc =
    src.slice(0, closingIdx) +
    lines.join("\n") +
    "\n};\n\nexport default " +
    path.basename(filePath, ".ts") +
    ";\n";

  // Only write if changed.
  if (newSrc !== src) {
    fs.writeFileSync(filePath, newSrc, "utf8");
    console.log(`  ✅ Wrote ${missing.size} fallback key(s) to ${path.basename(filePath)}`);
  }
}

// ── Main ──────────────────────────────────────────────────────────────────────

function main() {
  const args   = process.argv.slice(2);
  const doFix  = args.includes("--fix");
  const doCheck= args.includes("--check");

  const enPath = path.join(I18N_DIR, "en.ts");
  if (!fs.existsSync(enPath)) {
    console.error("❌  Could not find en.ts at", enPath);
    process.exit(1);
  }

  const enKeys = extractKeys(enPath);
  console.log(`\n📖  en.ts: ${enKeys.size} keys (source of truth)\n`);

  let totalMissing = 0;
  let exitCode     = 0;

  for (const locale of LOCALES) {
    const locPath = path.join(I18N_DIR, `${locale}.ts`);
    if (!fs.existsSync(locPath)) {
      console.warn(`⚠️   ${locale}.ts not found — skipping`);
      continue;
    }

    const locKeys   = extractKeys(locPath);
    const missing   = new Map<string, string>();
    const extra     = new Map<string, string>();

    for (const [key, val] of enKeys) {
      if (!locKeys.has(key)) missing.set(key, val);
    }
    for (const key of locKeys.keys()) {
      if (!enKeys.has(key)) extra.set(key, locKeys.get(key)!);
    }

    const status = missing.size === 0 ? "✅" : "❌";
    console.log(
      `${status}  ${locale}.ts — ${locKeys.size} keys` +
      (missing.size > 0 ? `  ⚠ missing ${missing.size}` : "") +
      (extra.size   > 0 ? `  ℹ extra ${extra.size}`    : ""),
    );

    if (missing.size > 0) {
      totalMissing += missing.size;
      exitCode = 1;
      if (doFix) {
        appendMissingKeys(locPath, missing, locale);
      } else {
        // Print the first 10 missing keys as a sample.
        let i = 0;
        for (const key of missing.keys()) {
          console.log(`     missing: "${key}"`);
          if (++i >= 10 && missing.size > 10) {
            console.log(`     … and ${missing.size - 10} more`);
            break;
          }
        }
      }
    }

    if (extra.size > 0 && !doCheck) {
      console.log(`     info: ${extra.size} key(s) in ${locale}.ts not in en.ts (may be extras)`);
    }
  }

  console.log(`\n📊  Total missing: ${totalMissing} keys across ${LOCALES.length} locales`);

  if (doFix && totalMissing > 0) {
    console.log("🔧  --fix applied: English fallbacks written. Search for TODO translate to prioritise.");
  } else if (totalMissing > 0 && !doFix) {
    console.log("💡  Run with --fix to automatically add English fallbacks.");
  }

  if (doCheck) {
    process.exit(exitCode);
  }
}

main();
