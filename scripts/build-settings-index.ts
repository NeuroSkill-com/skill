#!/usr/bin/env npx tsx
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Builds per-locale search indexes for the Cmd-K command palette from:
//   1. i18n translation files  (all user-facing strings, per locale)
//   2. *Tab.svelte components  (which keys each tab uses)
//
// EN is the reference locale — it determines which keys qualify as settings.
// Other locales reuse the same key set, substituting translated labels/descs.
//
// Output: src/lib/generated/settings-search-index.{locale}.json
//
// Run:  npx tsx scripts/build-settings-index.ts
// Also runs automatically via the Vite plugin in vite.config.js.

import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dirname, "..");
const I18N_BASE = path.join(ROOT, "src/lib/i18n");
const LIB_DIR = path.join(ROOT, "src/lib");
const OUT_DIR = path.join(ROOT, "src/lib/generated");

// ── Tab file → tab ID mapping ───────────────────────────────────────────────

const TAB_FILE_TO_ID: Record<string, string> = {
  AppearanceTab: "appearance",
  CalibrationTab: "calibration",
  ClientsTab: "clients",
  DevicesTab: "devices",
  EegModelTab: "exg",
  EmbeddingsTab: "embeddings",
  ExgTab: "exg",
  GoalsTab: "goals",
  HooksTab: "hooks",
  LlmTab: "llm",
  LslTab: "lsl",
  PermissionsTab: "permissions",
  ScreenshotsTab: "screenshots",
  SettingsTab: "settings",
  ShortcutsTab: "shortcuts",
  SleepTab: "sleep",
  TokensTab: "tokens",
  ToolsTab: "tools",
  TtsTab: "tts",
  UmapTab: "umap",
  UpdatesTab: "updates",
  VirtualEegTab: "exg",
};

// ── i18n key prefix → tab ID (for keys not found via Tab file scanning) ─────

const KEY_PREFIX_TO_TAB: Record<string, string> = {
  appearance: "appearance",
  calibration: "calibration",
  dnd: "goals",
  goals: "goals",
  embeddings: "embeddings",
  hooks: "hooks",
  llm: "llm",
  lsl: "lsl",
  perm: "permissions",
  screenshots: "screenshots",
  settings: "settings",
  shortcuts: "shortcuts",
  sleep: "sleep",
  sleepSettings: "sleep",
  tokens: "tokens",
  tools: "tools",
  tts: "tts",
  umap: "umap",
  updates: "updates",
  veeg: "exg",
};

// ── Filtering: only keep keys that represent configurable settings ───────────

const SKIP_PREFIXES = [
  "settingsTabs.",
  "cmdK.",
  "common.",
  "toast.",
  "dashboard.",
  "search.",
  "history.",
  "help.",
  "helpRef.",
  "onboarding.",
  "focusTimer.",
  "labels.",
  "downloads.",
  "shortcuts.",
  "nav.",
];

const SKIP_SUFFIXES = [
  "Placeholder",
  "Error",
  "Toast",
  "Confirm",
  "Success",
  "Warning",
  "StatusMsg",
  "Loading",
  "Btn",
  "Button",
  "Title",
  "Subtitle",
  "Header",
  "Footer",
  "Hint",
  "Info",
  "Note",
  "Help",
  "Empty",
  "None",
  "NoData",
  "Unavailable",
  "Unsupported",
  "Required",
  "Active",
  "Inactive",
  "Status",
  "Count",
  "Badge",
  "Label",
];

const isDescKey = (key: string) => key.endsWith("Desc");

const DISPLAY_ONLY = [
  /\.status[A-Z]/,
  /\.legend/,
  /\.chart/,
  /\.how/,
  /\.info\d/,
  /\.preset\d/,
  /Preset\d/,
  /\.value/,
  /Value/,
  /\.current/,
  /\.last[A-Z]/,
  /\.no[A-Z]/,
  /\.error/,
  /\.toast/,
  /\.confirm/,
  /\.until/,
  /\.building/,
  /\.activating/,
  /\.exiting/,
  /\.force/,
  /\.requiresMac/,
  /\.section$/,
  /Desc$/,
  /\.action\d+$/,
  /\.window[A-Z]/,
  /\.path$/,
  /\.since$/,
  /\.today$/,
];

function isSettingKey(key: string, value: string, allKeys: Set<string>): boolean {
  if (SKIP_PREFIXES.some((p) => key.startsWith(p))) return false;
  if (isDescKey(key)) return false;
  if (DISPLAY_ONLY.some((p) => p.test(key))) return false;
  if (value.length < 5) return false;
  if ((value.match(/\{/g) || []).length >= 2) return false;
  const lastPart = key.split(".").pop() || "";
  if (SKIP_SUFFIXES.some((s) => lastPart.endsWith(s) || lastPart === s.toLowerCase())) return false;

  if (allKeys.has(`${key}Desc`)) return true;

  const settingWords =
    /\b(enabled?|toggle|mode|provider|model|interval|timeout|threshold|format|size|speed|rate|layers?|batch|cutoff|overlap|host|port|directory|dir|endpoint|section|autostart|auto[A-Z])\b/i;
  if (settingWords.test(lastPart)) return true;

  return false;
}

// ── Load translations for a locale ──────────────────────────────────────────

function loadTranslations(locale: string): Record<string, string> {
  const dir = path.join(I18N_BASE, locale);
  if (!fs.existsSync(dir)) return {};

  const all: Record<string, string> = {};
  const files = fs.readdirSync(dir).filter((f) => f.endsWith(".ts") && f !== "index.ts");

  for (const file of files) {
    const content = fs.readFileSync(path.join(dir, file), "utf-8");
    const re = /"([^"]+)":\s*(?:"([^"]*(?:\\.[^"]*)*)"|`([^`]*)`|"([^"]*)")/g;
    let m: RegExpExecArray | null = re.exec(content);
    while (m) {
      const key = m[1];
      const value = m[2] ?? m[3] ?? m[4] ?? "";
      all[key] = value.replace(/\\"/g, '"').replace(/\\n/g, " ").trim();
      m = re.exec(content);
    }
  }

  // Multi-line string concatenation
  for (const file of files) {
    const content = fs.readFileSync(path.join(dir, file), "utf-8");
    const multiLine = /"([^"]+)":\s*\n\s*"([^"]*)"/g;
    let m: RegExpExecArray | null = multiLine.exec(content);
    while (m) {
      if (!all[m[1]]) all[m[1]] = m[2].trim();
      m = multiLine.exec(content);
    }
  }

  return all;
}

// ── Scan Tab files for t("key") usage ───────────────────────────────────────

function scanTabKeys(): Map<string, Set<string>> {
  const tabKeys = new Map<string, Set<string>>();
  const tabFiles = fs.readdirSync(LIB_DIR).filter((f) => f.endsWith("Tab.svelte"));

  for (const file of tabFiles) {
    const name = file.replace(".svelte", "");
    const tabId = TAB_FILE_TO_ID[name];
    if (!tabId) continue;

    const content = fs.readFileSync(path.join(LIB_DIR, file), "utf-8");
    const re = /t\("([^"]+)"\)/g;
    let m: RegExpExecArray | null = re.exec(content);
    const keys = new Set<string>();
    while (m) {
      keys.add(m[1]);
      m = re.exec(content);
    }

    if (!tabKeys.has(tabId)) tabKeys.set(tabId, new Set());
    const existing = tabKeys.get(tabId) as Set<string>;
    for (const k of keys) existing.add(k);
  }

  return tabKeys;
}

// ── Build the key skeleton from EN ──────────────────────────────────────────

export interface SearchEntry {
  tab: string;
  key: string;
  label: string;
  desc?: string;
}

interface SettingsSkeleton {
  tab: string;
  key: string;
  descKey: string | null;
}

function buildSkeleton(): SettingsSkeleton[] {
  const enTranslations = loadTranslations("en");
  const allKeys = new Set(Object.keys(enTranslations));
  const tabKeys = scanTabKeys();
  const skeleton: SettingsSkeleton[] = [];
  const seen = new Set<string>();

  // First pass: entries from Tab file scanning
  for (const [tabId, keys] of tabKeys) {
    for (const key of keys) {
      const value = enTranslations[key];
      if (!value || !isSettingKey(key, value, allKeys)) continue;
      if (seen.has(key)) continue;
      const descKey = allKeys.has(`${key}Desc`) ? `${key}Desc` : null;
      skeleton.push({ tab: tabId, key, descKey });
      seen.add(key);
    }
  }

  // Second pass: prefix-based keys not in Tab files
  for (const [key, value] of Object.entries(enTranslations)) {
    if (seen.has(key) || !isSettingKey(key, value, allKeys)) continue;
    const prefix = key.split(".")[0];
    const tabId = KEY_PREFIX_TO_TAB[prefix];
    if (!tabId) continue;
    const descKey = allKeys.has(`${key}Desc`) ? `${key}Desc` : null;
    skeleton.push({ tab: tabId, key, descKey });
    seen.add(key);
  }

  skeleton.sort((a, b) => a.tab.localeCompare(b.tab) || a.key.localeCompare(b.key));
  return skeleton;
}

// ── Resolve skeleton into a locale's entries ────────────────────────────────

function resolveLocale(
  skeleton: SettingsSkeleton[],
  translations: Record<string, string>,
  enTranslations: Record<string, string>,
): SearchEntry[] {
  return skeleton.map(({ tab, key, descKey }) => {
    // Use locale translation, fall back to EN
    const label = translations[key] || enTranslations[key] || key;
    const desc = descKey ? translations[descKey] || enTranslations[descKey] : undefined;
    return { tab, key, label, ...(desc && { desc }) };
  });
}

// ── Write outputs ───────────────────────────────────────────────────────────

function writeIfChanged(filePath: string, json: string): boolean {
  const existing = fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf-8") : "";
  if (json !== existing) {
    fs.writeFileSync(filePath, json);
    return true;
  }
  return false;
}

export function generate() {
  if (!fs.existsSync(OUT_DIR)) {
    fs.mkdirSync(OUT_DIR, { recursive: true });
  }

  // Discover all locales
  const locales = fs.readdirSync(I18N_BASE).filter((d) => {
    const full = path.join(I18N_BASE, d);
    return fs.statSync(full).isDirectory() && fs.existsSync(path.join(full, "index.ts"));
  });

  // Build skeleton from EN
  const skeleton = buildSkeleton();
  const enTranslations = loadTranslations("en");

  let totalWritten = 0;

  for (const locale of locales) {
    const translations = loadTranslations(locale);
    const entries = resolveLocale(skeleton, translations, enTranslations);
    const outFile = path.join(OUT_DIR, `settings-search-index.${locale}.json`);
    const json = JSON.stringify(entries, null, 2);
    const written = writeIfChanged(outFile, json);
    const tag = written ? "generated" : "unchanged";
    console.log(`[settings-index] ${locale}: ${entries.length} entries (${tag})`);
    if (written) totalWritten++;
  }

  // Also write a manifest so the runtime knows which locales are available
  const manifest = { locales, entriesPerLocale: skeleton.length };
  writeIfChanged(path.join(OUT_DIR, "settings-search-manifest.json"), JSON.stringify(manifest, null, 2));

  console.log(
    `[settings-index] ${locales.length} locales, ${skeleton.length} entries each, ${totalWritten} files written`,
  );
}

// Run directly
if (process.argv[1]?.includes("build-settings-index")) {
  generate();
}
