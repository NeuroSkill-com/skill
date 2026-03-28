// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Shared i18n utilities for scripts (sync-i18n, audit-i18n).
 */
import fs from "node:fs";
import path from "node:path";

/** Discover locale directories under i18n root (excluding source locale). */
export function discoverLocales(i18nDir: string, sourceLocale = "en"): string[] {
  if (!fs.existsSync(i18nDir)) return [];
  return fs
    .readdirSync(i18nDir, { withFileTypes: true })
    .filter((ent) => ent.isDirectory())
    .map((ent) => ent.name)
    .filter((name) => name !== sourceLocale)
    .filter((name) => fs.existsSync(path.join(i18nDir, name, "index.ts")))
    .sort();
}

/**
 * Extract {key → value} map from a .ts locale file (namespace or flat).
 * Handles both:  "key.name":   "value"
 * and:           "key.name":   `template string`
 */
export function extractKeys(filePath: string): Map<string, string> {
  const src = fs.readFileSync(filePath, "utf8");
  const map = new Map<string, string>();
  // Matches: "key": "value"  or  "key": 'value'  or  "key": `value`
  // The key and value may be on separate lines (multi-line format).
  const re = /^\s*"([^"]+)"\s*:\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'|`((?:[^`\\]|\\.)*)`)/gms;
  // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic RegExp exec loop
  for (let m: RegExpExecArray | null; (m = re.exec(src)) !== null; ) {
    const key = m[1];
    const val = m[2] !== undefined ? m[2] : m[3] !== undefined ? m[3] : m[4];
    map.set(key, val ?? "");
  }
  return map;
}

/** Namespace files in the expected order. */
export const NS_FILES = [
  "common",
  "dashboard",
  "settings",
  "search",
  "calibration",
  "history",
  "hooks",
  "llm",
  "onboarding",
  "screenshots",
  "tts",
  "perm",
  "help",
  "help-ref",
  "ui",
];

/**
 * Extract keys from a locale directory (merges all namespace .ts files).
 */
export function extractKeysFromDir(dirPath: string): Map<string, string> {
  const map = new Map<string, string>();
  if (!fs.existsSync(dirPath) || !fs.statSync(dirPath).isDirectory()) {
    // Fall back to single-file extraction
    return extractKeys(dirPath);
  }
  for (const ns of NS_FILES) {
    const fp = path.join(dirPath, `${ns}.ts`);
    if (fs.existsSync(fp)) {
      const sub = extractKeys(fp);
      for (const [k, v] of sub) map.set(k, v);
    }
  }
  return map;
}

// ── Translation-exemption rules ────────────────────────────────────────────
// Keys matching any of these rules are considered legitimately identical
// across locales and are NOT reported as untranslated.

/** Exact key prefixes whose values are typically language-neutral. */
export const EXEMPT_KEY_PREFIXES = [
  "dashboard.faaFormula", // math formula
  "dashboard.tar", // technical acronyms
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
  "helpRef.authors", // academic citations — not translatable
  "helpRef.journal",
  "helpRef.title",
  "helpRef.metrics",
  "helpRef.doi",
  "helpApi.cmd", // API command names are code identifiers
  "onboarding.models.", // model product names (Qwen3.5, NeuTTS, Kitten TTS)
  "ttsTab.backend", // TTS engine names (KittenTTS, NeuTTS)
  "ttsTab.voice", // voice names (Juliette, Jasper)
  "ttsTab.kittenModel", // model spec string
  "calibration.preset.", // preset names used as identifiers
  "focusTimer.preset.", // preset names
  "sd.delta",
  "sd.theta",
  "sd.alpha",
  "sd.beta",
  "sd.gamma", // Greek letter + band name
  "sd.hjorthMob",
  "sd.permEnt",
  "sd.higuchiFd", // scientific metric abbreviations
  "sd.stress", // loanword used across languages
  "sd.meditation", // loanword used across languages
  "sd.chartFaa",
  "sd.chartHjorth",
  "sd.chartHrv", // chart label + acronym
  "compare.rmssd",
  "compare.sdnn", // HRV metric acronyms
  "perm.bluetooth",
  "perm.whyBluetooth", // technology brand name
  "settings.logBluetooth",
  "settings.logWebsocket", // technology names
  "settings.openbciPreset", // electrode placement names (Frontal, Occipital)
  "settings.gpuLatency", // technical spec string
  "helpPrivacy.ble", // technology full name
  "helpSettings.openbciGanglion", // hardware product name
  "apiStatus.", // generic English labels used in technical context
  "llm.size", // "{gb} GB" template
  "llm.tools.parallel",
  "chat.tools.parallel", // mode label
  "chat.think.", // thinking mode labels (Minimal/Normal)
  "dnd.focusLookbackValue", // "{secs}s" / "{min}m" template
  "settings.currentVersion", // "{app} v{version}" template
  "whatsNew.version", // "Version {version}" template
  "cmdK.section", // command palette section labels
  "onboarding.step.bluetooth", // technology name in step label
  "compare.meditation",
  "compare.heatmap", // loanwords
  "hooks.scenario.emotional", // loanword
  "dashboard.signal",
  "dashboard.meditation", // loanwords
  "appearance.themeSystem", // "System" is universal
  "model.encoder", // technical term
  "calibration.iteration", // Iteration used in German too
  "settings.openbci", // brand name
  "downloads.windowTitle", // "Downloads" is universal
  "llm.mmproj", // "Multimodal" is universal
  "ttsTab.requirementsDesc", // shell commands — language-neutral
  "helpSettings.openbciWifi", // hardware product name
  "dashboard.relaxation",
  "dashboard.engagement",
  "dashboard.migraine", // cognates
  "dashboard.hjorthActivity",
  "dashboard.hjorthComplexity", // scientific labels
  "chartScheme.mono", // "Monochrome" cognate
  "settings.shortcutCalibration",
  "settings.calibration", // "Calibration" cognate
  "calibration.title", // "Calibration" cognate
  "embeddings.dimLegend", // "Dimensions" cognate
  "settings.action1",
  "settings.action2", // "Action" cognate
  "sd.hjorthAct", // scientific abbreviation
  "sd.chartScores",
  "sd.chartSpectral", // chart label cognates
  "umap.sessionA",
  "umap.sessionB", // "Session" cognate
  "search.textViaModel", // "via {model}" technical
  "history.sessions",
  "history.session",
  "history.totalSessions", // "session(s)" cognate
  "helpSettings.calibration", // "Calibration" cognate
  "compare.sessionA",
  "compare.sessionB",
  "compare.scores", // cognates
  "compare.sessions",
  "compare.umapPoints", // cognates
  "settingsTabs.embeddings", // technical term
  "settingsTabs.calibration", // cognate
  "hooks.keywordSuggestions", // "Suggestions" cognate
  "hooks.distance",
  "hooks.logDistance", // "Distance" cognate/technical
  "shortcuts.openCalibration", // "Calibration" cognate
  "onboarding.step.calibration", // "Calibration" cognate
  "focusTimer.sessions", // "sessions" cognate
  "focusTimer.log.cycles",
  "focusTimer.log.cyclesPlural", // "cycle(s)" cognate
  "perm.notifications",
  "perm.matrixNotifications",
  "perm.whyNotifications", // "Notifications" cognate
  "chat.tools.argsLabel", // "Arguments" technical
  "dnd.exitDurationValue", // "{min} min" template
  "dnd.buildingScore", // template with placeholders
  "settings.supportedDevices.company.", // company names (InteraXon, Neurable, OpenBCI, Emotiv)
  "settings.supportedDevices.device.", // device product names (Muse 2, EPOC X, etc.)
  "settings.scanner.bleDesc", // device list "Muse, MW75, Hermes, Ganglion, IDUN"
  "settings.scanner.cortex", // "Emotiv Cortex" product name
  "settings.scanner.ble", // "Bluetooth LE" technology standard
  "settings.deviceApi.emotivTitle", // "Emotiv Cortex" product name
  "settings.deviceApi.idunTitle", // "IDUN Cloud" product name
  "settings.deviceApi.openbciTitle", // "OpenBCI" product name
  "settings.deviceApi.clientId", // "Client ID" technical term
  "settings.deviceApi.clientSecret", // "Client Secret" technical term
  "settings.deviceApi.apiToken", // "API Token" technical term
  "settings.apiTokenLabel", // "Bearer Token" technical term
  "settings.logScanner", // "Scanner" technical term
  "settings.logChatStore", // "Chat Store" technical term
  "screenshots.modelNomic", // model product name
  "screenshots.backendLlmVlm", // "LLM VLM" technical label
  "screenshots.ocrEngineAppleVision", // "Apple Vision" product name
  "screenshots.ocrEngineOcrs", // "ocrs" engine name
  "llm.tools.skillApi", // product name
  "chat.tools.skill_api", // product name
  "about.discord", // brand name
  "helpDash.community", // "Community" cognate
  "chat.ctx.messagesCount", // "messages" cognate (FR)
  "chat.tools.sourcesLabel", // "Sources" cognate (FR)
  "search.modeImages", // "Images" cognate (FR)
  "settingsTabs.lsl", // "LSL" technical acronym
  "lsl.localStreams", // "LSL Streams" technical term
  "lsl.iroh", // "iroh" product name
  "lsl.connect", // "Connect" cognate
  "lsl.scanning", // "Scanning" cognate
  "lsl.scanButton", // "Scan Network" technical
  "lsl.noStreams", // technical LSL context
  "lsl.pair", // "Pair" cognate
  "lsl.unpair", // "Unpair" cognate
  "lsl.paired", // "PAIRED" badge label
  "lsl.autoScanning", // "Auto-scanning" technical
  "lsl.autoConnect", // "Auto-Connect" technical
  "lsl.pairAndConnect", // "Pair & Connect" cognate
  "lsl.streaming", // "STREAMING" badge
  "lsl.sessionActive", // "LSL session active" technical
  "lsl.lastScanJustNow", // "just now" short label
  "lsl.scanningNetwork", // "Scanning local network..." technical
];

/** Exact keys that are always the same across locales. */
export const EXEMPT_KEYS = new Set([
  "lang.dir", // "ltr" / "rtl" — set per locale already
  "dashboard.skill", // "{app}" placeholder
]);

/** Patterns for values that are inherently language-neutral. */
export function isExemptValue(_key: string, value: string): boolean {
  const v = value.trim();

  // Pure placeholder like "{app}", "{gb}", etc.
  if (/^\{[a-zA-Z_]+\}$/.test(v)) return true;

  // Very short technical tokens (≤ 5 chars, no spaces, all ASCII)
  if (v.length <= 5 && /^[A-Za-z0-9_./()\-–]+$/.test(v)) return true;

  // Strings that are only numbers, symbols, units, math
  if (
    /^[A-Za-z0-9\u03B1-\u03C9\u0391-\u03A9_./()\-\u2013+\u00D7\u00F7=<>\u00B2\u00B3\u2082\u2080\u2081 \u00B0%,;:\u00B7\u2026\s]+$/.test(
      v,
    ) &&
    !/[a-z]{4,}/i.test(v)
  )
    return true;

  // URL-only or code-only values
  if (/^https?:\/\//.test(v)) return true;
  if (/^[{}[\]",:0-9.\s]+$/.test(v)) return true;

  return false;
}

/**
 * Check whether a key/value pair is exempt from untranslated-key reporting.
 * Exempt keys are legitimately identical across locales (brand names,
 * math formulas, technical acronyms, etc.).
 */
export function isExempt(key: string, value: string): boolean {
  if (EXEMPT_KEYS.has(key)) return true;
  for (const prefix of EXEMPT_KEY_PREFIXES) {
    if (key.startsWith(prefix)) return true;
  }
  return isExemptValue(key, value);
}
