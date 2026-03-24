#!/usr/bin/env tsx
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * audit-i18n.ts — find untranslated keys in non-English locale files
 *
 * A key is "untranslated" when its value is identical to the English source
 * string AND the key is not in the exempt list (technical tokens, brand
 * names, formulas, etc. that are legitimately the same across locales).
 *
 * Usage:
 *   npx tsx scripts/audit-i18n.ts                # full report
 *   npx tsx scripts/audit-i18n.ts --check        # exit 1 if untranslated keys exist (CI)
 *   npx tsx scripts/audit-i18n.ts --locale de    # audit only German
 *   npx tsx scripts/audit-i18n.ts --verbose       # show English value next to each key
 */

import path from "path";
import fs from "fs";
import { fileURLToPath } from "url";
import { extractKeysFromDir, isExempt } from "../src/lib/i18n/i18n-utils";

const __filename = fileURLToPath(import.meta.url);
const __dirname  = path.dirname(__filename);

const I18N_DIR = path.resolve(__dirname, "../src/lib/i18n");
const LOCALES  = ["de", "fr", "he", "uk"];

// ── Main ──────────────────────────────────────────────────────────────────────

function main() {
  const args      = process.argv.slice(2);
  const doCheck   = args.includes("--check");
  const verbose   = args.includes("--verbose");
  const localeIdx = args.indexOf("--locale");
  const filterLocale = localeIdx !== -1 ? args[localeIdx + 1] : null;
  const locales   = filterLocale ? [filterLocale] : LOCALES;

  const enDir = path.join(I18N_DIR, "en");
  if (!fs.existsSync(enDir)) {
    console.error("❌  Could not find en/ at", enDir);
    process.exit(1);
  }

  const enKeys = extractKeysFromDir(enDir);
  console.log(`\n📖  en/: ${enKeys.size} keys (source of truth)\n`);

  let totalUntranslated = 0;
  let totalExempt       = 0;

  for (const locale of locales) {
    const locDir = path.join(I18N_DIR, locale);
    if (!fs.existsSync(locDir)) {
      console.warn(`⚠️   ${locale}/ not found — skipping`);
      continue;
    }

    const locKeys     = extractKeysFromDir(locDir);
    const untranslated: Array<[string, string]> = [];
    let exemptCount   = 0;

    for (const [key, enVal] of enKeys) {
      const locVal = locKeys.get(key);
      if (locVal === undefined) continue;       // missing keys are handled by sync-i18n
      if (locVal !== enVal) continue;            // translated — different value

      // Value is identical to English
      if (isExempt(key, enVal)) {
        exemptCount++;
      } else {
        untranslated.push([key, enVal]);
      }
    }

    totalExempt += exemptCount;
    totalUntranslated += untranslated.length;

    const status = untranslated.length === 0 ? "✅" : "⚠️ ";
    console.log(
      `${status} ${locale}/ — ${untranslated.length} untranslated` +
      `  (${exemptCount} exempt)`
    );

    if (untranslated.length > 0) {
      const show = verbose ? untranslated : untranslated.slice(0, 15);
      for (const [key, val] of show) {
        const preview = val.length > 72 ? val.slice(0, 72) + "…" : val;
        console.log(`     ${key}${verbose ? ` → ${preview}` : ""}`);
      }
      if (!verbose && untranslated.length > 15) {
        console.log(`     … and ${untranslated.length - 15} more (use --verbose to see all)`);
      }
    }
  }

  console.log(`\n📊  Total untranslated: ${totalUntranslated} across ${locales.length} locale(s)`);
  console.log(`📋  Total exempt (legitimately identical): ${totalExempt}`);

  if (doCheck && totalUntranslated > 0) {
    console.log("\n❌  Untranslated keys found. Translate them or add to the exempt list.");
    process.exit(1);
  } else if (totalUntranslated === 0) {
    console.log("\n✅  All keys are translated (or legitimately exempt).");
  }
}

main();
