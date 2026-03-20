// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Internationalisation store — Svelte 5 reactive state.
 *
 * Usage:
 *   import { t, locale, setLocale, SUPPORTED_LOCALES } from "$lib/i18n/index.svelte";
 *   t("search.pageOf", { page: 1, total: 5 }) → "Page 1 of 5"
 *
 * The `{app}` placeholder is automatically injected into every string
 * using the canonical app name fetched from the Rust backend.
 */

import en from "./en/index";
import type { TranslationKey } from "./keys";
import { getAppName } from "$lib/stores/app-name.svelte";
import { invoke } from "@tauri-apps/api/core";

// Re-export the key type for call-site usage.
export type { TranslationKey };

// ── Supported locales ──────────────────────────────────────────────────────
export interface LocaleMeta {
  code: string;
  name: string;
  flag: string;
  dir:  "ltr" | "rtl";
}

export const SUPPORTED_LOCALES: LocaleMeta[] = [
  { code: "de", name: "Deutsch",    flag: "🇩🇪", dir: "ltr" },
  { code: "en", name: "English",    flag: "🇺🇸", dir: "ltr" },
  { code: "fr", name: "Français",   flag: "🇫🇷", dir: "ltr" },
  { code: "uk", name: "Українська", flag: "🇺🇦", dir: "ltr" },
  { code: "he", name: "עברית",      flag: "🇮🇱", dir: "rtl" },
];

const STORAGE_KEY = "skill-lang";

// ── Detect system locale ───────────────────────────────────────────────────
function detectLocale(): string {
  // Check saved preference first
  if (typeof localStorage !== "undefined") {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && SUPPORTED_LOCALES.some(l => l.code === saved)) return saved;
  }
  // Fall back to browser locale
  if (typeof navigator !== "undefined") {
    const langs = navigator.languages ?? [navigator.language];
    for (const lang of langs) {
      const code = lang.split("-")[0].toLowerCase();
      if (SUPPORTED_LOCALES.some(l => l.code === code)) return code;
    }
  }
  return "en";
}

// ── Lazy-load translation files ────────────────────────────────────────────
// `en` is already statically imported above, so we wrap it in a resolved
// promise instead of a dynamic import — this avoids Vite's
// "dynamically imported but also statically imported" chunk-split warning.
const loaders: Record<string, () => Promise<{ default: Record<string, string> }>> = {
  en: async () => ({ default: en }),
  fr: () => import("./fr/index"),
  de: () => import("./de/index"),
  he: () => import("./he/index"),
  uk: () => import("./uk/index"),
};

// ── Reactive state ─────────────────────────────────────────────────────────
let _locale       = $state(detectLocale());
let _translations = $state<Record<string, string>>(en);
let _dir          = $state<"ltr" | "rtl">("ltr");
let _loading      = $state(false);

// Load initial locale if not English
{
  const initLocale = detectLocale();
  if (initLocale !== "en") {
    _loading = true;
    loaders[initLocale]?.().then(mod => {
      _translations = mod.default;
      _dir = SUPPORTED_LOCALES.find(l => l.code === initLocale)?.dir ?? "ltr";
      applyDir();
      _loading = false;
    });
  }
}

function applyDir() {
  if (typeof document !== "undefined") {
    document.documentElement.dir  = _dir;
    document.documentElement.lang = _locale;
  }
}
applyDir();

// ── Public API ─────────────────────────────────────────────────────────────

/** Current locale code (reactive). */
export function getLocale(): string { return _locale; }

/** Current text direction (reactive). */
export function getDir(): "ltr" | "rtl" { return _dir; }

/** Whether a language file is currently loading (reactive). */
export function isLoading(): boolean { return _loading; }

/**
 * Switch locale. Loads the translation file asynchronously,
 * persists to localStorage, and updates the document dir attribute.
 */
export async function setLocale(code: string): Promise<void> {
  if (!SUPPORTED_LOCALES.some(l => l.code === code)) return;
  _loading = true;
  try {
    const mod = await loaders[code]!();
    _translations = mod.default;
    _locale = code;
    _dir = SUPPORTED_LOCALES.find(l => l.code === code)?.dir ?? "ltr";
    applyDir();
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, code);
    }
    // Persist to settings.json via Tauri
    invoke("set_language", { language: code }).catch(e => console.warn("[i18n] set_language failed:", e));
  } finally {
    _loading = false;
  }
}

/** Load persisted language from Tauri settings on startup. */
export async function initLocaleFromSettings(): Promise<void> {
  try {
    const [_theme, lang] = await invoke<[string, string]>("get_theme_and_language");
    if (lang && SUPPORTED_LOCALES.some(l => l.code === lang)) {
      await setLocale(lang);
    }
  } catch (e) { console.warn("[i18n] initLocaleFromSettings failed:", e); }
}

/**
 * Translate a key, optionally interpolating `{param}` placeholders.
 *
 * Falls back to the English string, then to the raw key.
 *
 * Accepts both literal `TranslationKey` (for compile-time safety on static
 * keys) and plain `string` (for dynamic/computed keys like template literals).
 */
export function t(key: TranslationKey, params?: Record<string, string | number>): string;
export function t(key: string, params?: Record<string, string | number>): string;
export function t(key: string, params?: Record<string, string | number>): string {
  let str = _translations[key] ?? en[key] ?? key;
  // Always inject the canonical app name so any string can use {app}.
  str = str.replaceAll("{app}", getAppName());
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      str = str.replaceAll(`{${k}}`, String(v));
    }
  }
  return str;
}
