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

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname  = path.dirname(__filename);

const I18N_DIR = path.resolve(__dirname, "../src/lib/i18n");
const LOCALES  = ["de", "fr", "he", "uk"];

// ── Key extraction (shared with sync-i18n.ts) ────────────────────────────────

function extractKeys(filePath: string): Map<string, string> {
  const src = fs.readFileSync(filePath, "utf8");
  const map = new Map<string, string>();
  const re = /^\s+"([^"]+)":\s+(?:"((?:[^"\\]|\\.)*)"|`((?:[^`\\]|\\.)*)`)/gm;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const key = m[1];
    const val = m[2] !== undefined ? m[2] : m[3];
    map.set(key, val);
  }
  return map;
}

// ── Exemption rules ──────────────────────────────────────────────────────────
// Keys matching any of these rules are considered legitimately identical
// across locales and are NOT reported as untranslated.

/** Exact key prefixes whose values are typically language-neutral. */
const EXEMPT_KEY_PREFIXES = [
  "dashboard.faaFormula",       // math formula
  "dashboard.tar",              // technical acronyms
  "dashboard.bar",
  "dashboard.dtr",
  "dashboard.pse",
  "dashboard.apf",
  "dashboard.bps",
  "dashboard.snr",
  "dashboard.tbr",
  "dashboard.sef95",
  "dashboard.hjorthMobility",
  "dashboard.higuchiFd",
  "dashboard.dfaExponent",
  "dashboard.pacThetaGamma",
  "dashboard.rmssd",
  "dashboard.sdnn",
  "dashboard.pnn50",
  "dashboard.lfHfRatio",
  "dashboard.spo2",
  "dashboard.imu",
  "dashboard.gyro",
  "dashboard.consciousness.",
  "helpRef.authors",            // academic citations — not translatable
  "helpRef.journal",
  "helpRef.title",
  "helpRef.metrics",
  "helpRef.doi",
];

/** Exact keys that are always the same across locales. */
const EXEMPT_KEYS = new Set([
  "lang.dir",                   // "ltr" / "rtl" — set per locale already
  "dashboard.skill",            // "{app}" placeholder
]);

/** Patterns for values that are inherently language-neutral. */
function isExemptValue(key: string, value: string): boolean {
  const v = value.trim();

  // Pure placeholder like "{app}", "{gb}", etc.
  if (/^\{[a-zA-Z_]+\}$/.test(v)) return true;

  // Very short technical tokens (≤ 5 chars, no spaces, all ASCII)
  // e.g. "UMAP", "GPU", "LLM", "TTS", "FAA", "CSV", "EEG"
  if (v.length <= 5 && /^[A-Za-z0-9_./()\-–]+$/.test(v)) return true;

  // Strings that are only numbers, symbols, units, math
  // e.g. "ln(AF8 α) − ln(AF7 α)", "SpO₂", "θ–γ"
  if (/^[A-Za-z0-9α-ωΑ-Ω_./()\-–+×÷=<>²³₂₀₁ °%,;:·…\s]+$/.test(v) && !/[a-z]{4,}/i.test(v)) return true;

  // URL-only or code-only values
  if (/^https?:\/\//.test(v)) return true;
  if (/^[{}\[\]",:0-9.\s]+$/.test(v)) return true;

  return false;
}

function isExempt(key: string, value: string): boolean {
  if (EXEMPT_KEYS.has(key)) return true;
  for (const prefix of EXEMPT_KEY_PREFIXES) {
    if (key.startsWith(prefix)) return true;
  }
  return isExemptValue(key, value);
}

// ── Main ──────────────────────────────────────────────────────────────────────

function main() {
  const args      = process.argv.slice(2);
  const doCheck   = args.includes("--check");
  const verbose   = args.includes("--verbose");
  const localeIdx = args.indexOf("--locale");
  const filterLocale = localeIdx !== -1 ? args[localeIdx + 1] : null;
  const locales   = filterLocale ? [filterLocale] : LOCALES;

  const enPath = path.join(I18N_DIR, "en.ts");
  if (!fs.existsSync(enPath)) {
    console.error("❌  Could not find en.ts at", enPath);
    process.exit(1);
  }

  const enKeys = extractKeys(enPath);
  console.log(`\n📖  en.ts: ${enKeys.size} keys (source of truth)\n`);

  let totalUntranslated = 0;
  let totalExempt       = 0;

  for (const locale of locales) {
    const locPath = path.join(I18N_DIR, `${locale}.ts`);
    if (!fs.existsSync(locPath)) {
      console.warn(`⚠️   ${locale}.ts not found — skipping`);
      continue;
    }

    const locKeys     = extractKeys(locPath);
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
      `${status} ${locale}.ts — ${untranslated.length} untranslated` +
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
