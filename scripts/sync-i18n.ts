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
 * The script diffs every locale directory against en/ (the source of truth).
 * Missing keys are reported per namespace file; with --fix they are appended
 * using the English value as a fallback (clearly marked with a TODO comment
 * so translators know what to update).
 */

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";
import { extractKeys, extractKeysFromDir, NS_FILES } from "../src/lib/i18n/i18n-utils";

const __filename = fileURLToPath(import.meta.url);
const __dirname  = path.dirname(__filename);

const I18N_DIR = path.resolve(__dirname, "../src/lib/i18n");
const LOCALES  = ["de", "fr", "he", "uk"];

// ── Fix: append missing keys to a namespace file ──────────────────────────────

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
    `  // ── Auto-synced from en/ (${new Date().toISOString().slice(0, 10)}) ──`,
    `  // TODO: translate the following ${missing.size} key(s) into ${langCode}`,
  ];

  for (const [key, val] of missing) {
    lines.push(`  "${key}": "${val}",`);
  }

  const varName = path.basename(filePath, ".ts").replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const newSrc =
    src.slice(0, closingIdx) +
    lines.join("\n") +
    "\n};\n\nexport default " + varName + ";\n";

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

  const enDir = path.join(I18N_DIR, "en");
  if (!fs.existsSync(enDir)) {
    console.error("❌  Could not find en/ at", enDir);
    process.exit(1);
  }

  const enKeys = extractKeysFromDir(enDir);
  console.log(`\n📖  en/: ${enKeys.size} keys (source of truth)\n`);

  let totalMissing = 0;
  let exitCode     = 0;

  for (const locale of LOCALES) {
    const locDir = path.join(I18N_DIR, locale);
    if (!fs.existsSync(locDir)) {
      console.warn(`⚠️   ${locale}/ not found — skipping`);
      continue;
    }

    const locKeys   = extractKeysFromDir(locDir);
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
      `${status}  ${locale}/ — ${locKeys.size} keys` +
      (missing.size > 0 ? `  ⚠ missing ${missing.size}` : "") +
      (extra.size   > 0 ? `  ℹ extra ${extra.size}`    : ""),
    );

    if (missing.size > 0) {
      totalMissing += missing.size;
      exitCode = 1;
      if (doFix) {
        // Group missing keys by namespace and append to the right file
        for (const ns of NS_FILES) {
          const enNsKeys = extractKeys(path.join(enDir, `${ns}.ts`));
          const nsMissing = new Map<string, string>();
          for (const [key, val] of missing) {
            if (enNsKeys.has(key)) nsMissing.set(key, val);
          }
          if (nsMissing.size > 0) {
            appendMissingKeys(path.join(locDir, `${ns}.ts`), nsMissing, locale);
          }
        }
      } else {
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
      console.log(`     info: ${extra.size} key(s) in ${locale}/ not in en/ (may be extras)`);
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
