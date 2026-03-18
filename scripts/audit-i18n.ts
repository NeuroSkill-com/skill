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
import { extractKeysFromDir } from "../src/lib/i18n/i18n-utils";

const __filename = fileURLToPath(import.meta.url);
const __dirname  = path.dirname(__filename);

const I18N_DIR = path.resolve(__dirname, "../src/lib/i18n");
const LOCALES  = ["de", "fr", "he", "uk"];

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
  "helpApi.cmd",                // API command names are code identifiers
  "onboarding.models.",         // model product names (Qwen3.5, NeuTTS, Kitten TTS)
  "ttsTab.backend",             // TTS engine names (KittenTTS, NeuTTS)
  "ttsTab.voice",               // voice names (Juliette, Jasper)
  "ttsTab.kittenModel",         // model spec string
  "calibration.preset.",        // preset names used as identifiers
  "focusTimer.preset.",         // preset names
  "sd.delta", "sd.theta", "sd.alpha", "sd.beta", "sd.gamma", // Greek letter + band name
  "sd.hjorthMob", "sd.permEnt", "sd.higuchiFd",               // scientific metric abbreviations
  "sd.stress",                  // loanword used across languages
  "sd.meditation",              // loanword used across languages
  "sd.chartFaa", "sd.chartHjorth", "sd.chartHrv",             // chart label + acronym
  "compare.rmssd", "compare.sdnn",                             // HRV metric acronyms
  "perm.bluetooth", "perm.whyBluetooth",                       // technology brand name
  "settings.logBluetooth", "settings.logWebsocket",            // technology names
  "settings.openbciPreset",     // electrode placement names (Frontal, Occipital)
  "settings.gpuLatency",        // technical spec string
  "helpPrivacy.ble",            // technology full name
  "helpSettings.openbciGanglion", // hardware product name
  "apiStatus.",                 // generic English labels used in technical context
  "llm.size",                   // "{gb} GB" template
  "llm.tools.parallel", "chat.tools.parallel", // mode label
  "chat.think.",                // thinking mode labels (Minimal/Normal)
  "dnd.focusLookbackValue",     // "{secs}s" / "{min}m" template
  "settings.currentVersion",    // "{app} v{version}" template
  "whatsNew.version",           // "Version {version}" template
  "cmdK.section",               // command palette section labels
  "onboarding.step.bluetooth",  // technology name in step label
  "compare.meditation", "compare.heatmap", // loanwords
  "hooks.scenario.emotional",   // loanword
  "dashboard.signal", "dashboard.meditation", // loanwords
  "appearance.themeSystem",     // "System" is universal
  "model.encoder",              // technical term
  "calibration.iteration",      // Iteration used in German too
  "settings.openbci",           // brand name
  "downloads.windowTitle",      // "Downloads" is universal
  "llm.mmproj",                 // "Multimodal" is universal
  "ttsTab.requirementsDesc",    // shell commands — language-neutral
  "helpSettings.openbciWifi",   // hardware product name
  "dashboard.relaxation", "dashboard.engagement", "dashboard.migraine", // cognates
  "dashboard.hjorthActivity", "dashboard.hjorthComplexity",   // scientific labels
  "chartScheme.mono",           // "Monochrome" cognate
  "settings.shortcutCalibration", "settings.calibration",     // "Calibration" cognate
  "calibration.title",          // "Calibration" cognate
  "embeddings.dimLegend",       // "Dimensions" cognate
  "settings.action1", "settings.action2",                     // "Action" cognate
  "sd.hjorthAct",               // scientific abbreviation
  "sd.chartScores", "sd.chartSpectral",                       // chart label cognates
  "umap.sessionA", "umap.sessionB",                           // "Session" cognate
  "search.textViaModel",        // "via {model}" technical
  "history.sessions", "history.session", "history.totalSessions", // "session(s)" cognate
  "helpSettings.calibration",   // "Calibration" cognate
  "compare.sessionA", "compare.sessionB", "compare.scores",  // cognates
  "compare.sessions", "compare.umapPoints",                   // cognates
  "settingsTabs.embeddings",    // technical term
  "settingsTabs.calibration",   // cognate
  "hooks.keywordSuggestions",   // "Suggestions" cognate
  "hooks.distance", "hooks.logDistance",                       // "Distance" cognate/technical
  "shortcuts.openCalibration",  // "Calibration" cognate
  "onboarding.step.calibration",// "Calibration" cognate
  "focusTimer.sessions",        // "sessions" cognate
  "focusTimer.log.cycles", "focusTimer.log.cyclesPlural",     // "cycle(s)" cognate
  "perm.notifications", "perm.matrixNotifications", "perm.whyNotifications", // "Notifications" cognate
  "chat.tools.argsLabel",       // "Arguments" technical
  "dnd.exitDurationValue",      // "{min} min" template
  "dnd.buildingScore",          // template with placeholders
  "calibration.iteration",      // "Iteration" cognate
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
  if (v.length <= 5 && /^[A-Za-z0-9_./()\-–]+$/.test(v)) return true;

  // Strings that are only numbers, symbols, units, math
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
