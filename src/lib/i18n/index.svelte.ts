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

import { invoke } from "@tauri-apps/api/core";
import { getAppName } from "$lib/stores/app-name.svelte";
import en from "./en/index";
import type { TranslationKey } from "./keys";

// Re-export the key type for call-site usage.
export type { TranslationKey };

// ── Supported locales ──────────────────────────────────────────────────────
export interface LocaleMeta {
  code: string;
  name: string;
  flag: string;
  dir: "ltr" | "rtl";
}

export const SUPPORTED_LOCALES: LocaleMeta[] = [
  { code: "de", name: "Deutsch", flag: "🇩🇪", dir: "ltr" },
  { code: "en", name: "English", flag: "🇺🇸", dir: "ltr" },
  { code: "es", name: "Español", flag: "🇪🇸", dir: "ltr" },
  { code: "fr", name: "Français", flag: "🇫🇷", dir: "ltr" },
  { code: "uk", name: "Українська", flag: "🇺🇦", dir: "ltr" },
  { code: "he", name: "עברית", flag: "🇮🇱", dir: "rtl" },
];

const STORAGE_KEY = "skill-lang";
const USER_SELECTED_KEY = "skill-lang-user-selected";

function isSupportedLocale(code: string | null | undefined): code is string {
  return !!code && SUPPORTED_LOCALES.some((l) => l.code === code);
}

function hasUserSelectedLocale(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(USER_SELECTED_KEY) === "1";
}

// ── Detect system locale ───────────────────────────────────────────────────
function detectLocale(): string {
  // Use persisted locale only when the user explicitly chose one.
  if (typeof localStorage !== "undefined") {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (hasUserSelectedLocale() && isSupportedLocale(saved)) return saved;
  }
  // Otherwise start from system/browser locale.
  if (typeof navigator !== "undefined") {
    const langs = navigator.languages ?? [navigator.language];
    for (const lang of langs) {
      const code = lang.split("-")[0].toLowerCase();
      if (SUPPORTED_LOCALES.some((l) => l.code === code)) return code;
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
  es: () => import("./es/index"),
  he: () => import("./he/index"),
  uk: () => import("./uk/index"),
};

// ── Reactive state ─────────────────────────────────────────────────────────
let _locale = $state(detectLocale());
let _translations = $state<Record<string, string>>(en);
let _dir = $state<"ltr" | "rtl">("ltr");
let _loading = $state(false);
let _userSelectedLocale = $state(hasUserSelectedLocale());

// Load initial locale if not English
{
  const initLocale = detectLocale();
  if (initLocale !== "en") {
    _loading = true;
    loaders[initLocale]?.().then((mod) => {
      _translations = mod.default;
      _dir = SUPPORTED_LOCALES.find((l) => l.code === initLocale)?.dir ?? "ltr";
      applyDir();
      _loading = false;
    });
  }
}

function applyDir() {
  if (typeof document !== "undefined") {
    document.documentElement.dir = _dir;
    document.documentElement.lang = _locale;
  }
}
applyDir();

// ── Public API ─────────────────────────────────────────────────────────────

/** Current locale code (reactive). */
export function getLocale(): string {
  return _locale;
}

/** Current text direction (reactive). */
export function getDir(): "ltr" | "rtl" {
  return _dir;
}

/** Whether a language file is currently loading (reactive). */
export function isLoading(): boolean {
  return _loading;
}

/** True when locale follows system/browser language (no user override saved). */
export function isUsingSystemLocale(): boolean {
  return !_userSelectedLocale;
}

async function applyLocale(code: string, opts: { markUserSelected: boolean; persistBackend: boolean }): Promise<void> {
  if (!isSupportedLocale(code)) return;
  _loading = true;
  try {
    const mod = await loaders[code]?.();
    _translations = mod.default;
    _locale = code;
    _dir = SUPPORTED_LOCALES.find((l) => l.code === code)?.dir ?? "ltr";
    applyDir();

    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, code);
      if (opts.markUserSelected) {
        localStorage.setItem(USER_SELECTED_KEY, "1");
      }
    }

    if (opts.markUserSelected) {
      _userSelectedLocale = true;
    }

    if (opts.persistBackend) {
      invoke("set_language", { language: code }).catch((_e) => {});
    }
  } finally {
    _loading = false;
  }
}

/**
 * Switch locale from user action.
 * Persists the preference so future launches keep the user-selected language.
 */
export async function setLocale(code: string): Promise<void> {
  await applyLocale(code, { markUserSelected: true, persistBackend: true });
}

/**
 * Clear user override and switch back to system/browser locale.
 */
export async function useSystemLocale(): Promise<void> {
  if (typeof localStorage !== "undefined") {
    localStorage.removeItem(USER_SELECTED_KEY);
    localStorage.removeItem(STORAGE_KEY);
  }
  _userSelectedLocale = false;
  await applyLocale(detectLocale(), { markUserSelected: false, persistBackend: true });
}

/**
 * Initialize locale from backend settings only when user has explicitly chosen
 * a language before. Otherwise keep the system locale default.
 */
export async function initLocaleFromSettings(): Promise<void> {
  try {
    const [_theme, lang] = await invoke<[string, string]>("get_theme_and_language");

    if (hasUserSelectedLocale() && isSupportedLocale(lang)) {
      await applyLocale(lang, { markUserSelected: true, persistBackend: false });
      return;
    }

    _userSelectedLocale = false;
    await applyLocale(detectLocale(), { markUserSelected: false, persistBackend: true });
  } catch (_e) {}
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
