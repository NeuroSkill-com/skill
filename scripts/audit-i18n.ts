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

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { discoverLocales, extractKeysFromDir, isExempt } from "../src/lib/i18n/i18n-utils";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const I18N_DIR = path.resolve(__dirname, "../src/lib/i18n");

// ── Main ──────────────────────────────────────────────────────────────────────

function main() {
  const args = process.argv.slice(2);
  const doCheck = args.includes("--check");
  const verbose = args.includes("--verbose");
  const localeIdx = args.indexOf("--locale");
  const filterLocale = localeIdx !== -1 ? args[localeIdx + 1] : null;

  const enDir = path.join(I18N_DIR, "en");
  if (!fs.existsSync(enDir)) {
    console.error("[audit-i18n] Missing source locale directory: src/lib/i18n/en");
    process.exit(1);
  }

  const discovered = discoverLocales(I18N_DIR, "en");
  if (discovered.length === 0) {
    console.error("[audit-i18n] No non-source locales found under src/lib/i18n");
    process.exit(1);
  }

  if (filterLocale && !discovered.includes(filterLocale)) {
    console.error(`[audit-i18n] Unknown locale '${filterLocale}'. Available: ${discovered.join(", ")}`);
    process.exit(1);
  }

  const locales = filterLocale ? [filterLocale] : discovered;
  const enKeys = extractKeysFromDir(enDir);

  let totalUntranslated = 0;
  let _totalExempt = 0;

  for (const locale of locales) {
    const locDir = path.join(I18N_DIR, locale);
    if (!fs.existsSync(locDir)) {
      continue;
    }

    const locKeys = extractKeysFromDir(locDir);
    const untranslated: Array<[string, string]> = [];
    let exemptCount = 0;

    for (const [key, enVal] of enKeys) {
      const locVal = locKeys.get(key);
      if (locVal === undefined) continue; // missing keys are handled by sync-i18n
      if (locVal !== enVal) continue; // translated — different value

      // Value is identical to English
      if (isExempt(key, enVal)) {
        exemptCount++;
      } else {
        untranslated.push([key, enVal]);
      }
    }

    _totalExempt += exemptCount;
    totalUntranslated += untranslated.length;

    const status = untranslated.length === 0 ? "✅" : "⚠️ ";
    console.log(`\n${status} ${locale}: ${untranslated.length} untranslated key(s)`);

    if (untranslated.length > 0) {
      const show = verbose ? untranslated : untranslated.slice(0, 15);
      for (const [key, val] of show) {
        const preview = val.length > 72 ? `${val.slice(0, 72)}…` : val;
        console.log(`    ${key}  →  "${preview}"`);
      }
      if (!verbose && untranslated.length > 15) {
        console.log(`    … and ${untranslated.length - 15} more`);
      }
    }
  }

  if (doCheck && totalUntranslated > 0) {
    console.error(`\n[audit-i18n] ${totalUntranslated} untranslated key(s) found. Run without --check for details.`);
    process.exit(1);
  } else if (totalUntranslated === 0) {
    console.log("\n✅ All keys are translated (or exempt).");
  }
}

main();
