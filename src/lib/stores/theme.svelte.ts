// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Theme store — manages light / dark / system / high-contrast preference.
 * Persists to settings.json via Tauri IPC (with localStorage fallback).
 * Applies the "dark" and "high-contrast" classes on <html> reactively.
 */

import { invoke } from "@tauri-apps/api/core";

export type ThemeMode = "system" | "light" | "dark";

// ── Accent colour presets ─────────────────────────────────────────────────────
//
// Each preset carries a full `--color-violet-*` palette. At runtime we
// override Tailwind CSS custom properties on <html> so accent-like utility
// classes across the app (`violet-*`, `blue-*`, `indigo-*`, `sky-*`) all
// adopt the selected hue — no template-wide refactor required.
//
// Values are taken directly from Tailwind v4's built-in color palette so
// each preset is a perfectly balanced, perceptually-uniform ramp.

export interface AccentPreset {
  id:      string;
  label:   string;
  /** oklch of the 600-shade — shown as the swatch colour in the picker. */
  swatch:  string;
  palette: Record<string, string>;
}

export const ACCENT_PRESETS: AccentPreset[] = [
  {
    id: "violet", label: "Violet", swatch: "oklch(0.558 0.288 302.3)",
    palette: {
      "--color-violet-50":  "oklch(0.969 0.016 272.3)",
      "--color-violet-100": "oklch(0.943 0.029 294.6)",
      "--color-violet-200": "oklch(0.895 0.058 296.8)",
      "--color-violet-300": "oklch(0.842 0.117 293.1)",
      "--color-violet-400": "oklch(0.749 0.194 293.2)",
      "--color-violet-500": "oklch(0.627 0.265 292.7)",
      "--color-violet-600": "oklch(0.558 0.288 302.3)",
      "--color-violet-700": "oklch(0.491 0.270 292.0)",
      "--color-violet-800": "oklch(0.432 0.232 292.0)",
      "--color-violet-900": "oklch(0.380 0.189 293.0)",
      "--color-violet-950": "oklch(0.283 0.141 291.1)",
    },
  },
  {
    id: "indigo", label: "Indigo", swatch: "oklch(0.511 0.262 276.9)",
    palette: {
      "--color-violet-50":  "oklch(0.962 0.018 272.3)",
      "--color-violet-100": "oklch(0.930 0.034 272.7)",
      "--color-violet-200": "oklch(0.870 0.065 274.0)",
      "--color-violet-300": "oklch(0.785 0.115 274.7)",
      "--color-violet-400": "oklch(0.673 0.182 276.9)",
      "--color-violet-500": "oklch(0.585 0.233 277.1)",
      "--color-violet-600": "oklch(0.511 0.262 276.9)",
      "--color-violet-700": "oklch(0.457 0.240 277.0)",
      "--color-violet-800": "oklch(0.398 0.195 277.7)",
      "--color-violet-900": "oklch(0.359 0.144 278.7)",
      "--color-violet-950": "oklch(0.257 0.090 281.3)",
    },
  },
  {
    id: "blue", label: "Blue", swatch: "oklch(0.546 0.245 262.9)",
    palette: {
      "--color-violet-50":  "oklch(0.970 0.014 254.6)",
      "--color-violet-100": "oklch(0.932 0.032 255.6)",
      "--color-violet-200": "oklch(0.882 0.059 254.1)",
      "--color-violet-300": "oklch(0.809 0.105 251.8)",
      "--color-violet-400": "oklch(0.707 0.165 254.6)",
      "--color-violet-500": "oklch(0.623 0.214 259.1)",
      "--color-violet-600": "oklch(0.546 0.245 262.9)",
      "--color-violet-700": "oklch(0.488 0.243 264.4)",
      "--color-violet-800": "oklch(0.424 0.199 265.6)",
      "--color-violet-900": "oklch(0.379 0.146 265.1)",
      "--color-violet-950": "oklch(0.282 0.091 267.9)",
    },
  },
  {
    id: "sky", label: "Sky", swatch: "oklch(0.588 0.158 241.9)",
    palette: {
      "--color-violet-50":  "oklch(0.977 0.013 236.6)",
      "--color-violet-100": "oklch(0.951 0.026 236.6)",
      "--color-violet-200": "oklch(0.901 0.058 230.9)",
      "--color-violet-300": "oklch(0.828 0.111 230.3)",
      "--color-violet-400": "oklch(0.746 0.161 225.9)",
      "--color-violet-500": "oklch(0.685 0.169 237.3)",
      "--color-violet-600": "oklch(0.588 0.158 241.9)",
      "--color-violet-700": "oklch(0.500 0.134 242.7)",
      "--color-violet-800": "oklch(0.443 0.110 240.8)",
      "--color-violet-900": "oklch(0.391 0.090 240.4)",
      "--color-violet-950": "oklch(0.293 0.066 243.2)",
    },
  },
  {
    id: "teal", label: "Teal", swatch: "oklch(0.600 0.145 184.7)",
    palette: {
      "--color-violet-50":  "oklch(0.984 0.014 180.7)",
      "--color-violet-100": "oklch(0.963 0.023 180.8)",
      "--color-violet-200": "oklch(0.910 0.048 180.4)",
      "--color-violet-300": "oklch(0.855 0.099 180.4)",
      "--color-violet-400": "oklch(0.777 0.152 180.6)",
      "--color-violet-500": "oklch(0.705 0.157 187.3)",
      "--color-violet-600": "oklch(0.600 0.145 184.7)",
      "--color-violet-700": "oklch(0.510 0.122 184.8)",
      "--color-violet-800": "oklch(0.442 0.095 185.9)",
      "--color-violet-900": "oklch(0.391 0.077 188.7)",
      "--color-violet-950": "oklch(0.277 0.056 192.6)",
    },
  },
  {
    id: "emerald", label: "Emerald", swatch: "oklch(0.596 0.145 163.0)",
    palette: {
      "--color-violet-50":  "oklch(0.979 0.021 166.1)",
      "--color-violet-100": "oklch(0.950 0.052 163.1)",
      "--color-violet-200": "oklch(0.905 0.093 164.1)",
      "--color-violet-300": "oklch(0.845 0.143 164.8)",
      "--color-violet-400": "oklch(0.765 0.177 163.2)",
      "--color-violet-500": "oklch(0.696 0.170 162.5)",
      "--color-violet-600": "oklch(0.596 0.145 163.0)",
      "--color-violet-700": "oklch(0.508 0.118 165.6)",
      "--color-violet-800": "oklch(0.432 0.095 166.7)",
      "--color-violet-900": "oklch(0.378 0.077 168.1)",
      "--color-violet-950": "oklch(0.262 0.051 172.6)",
    },
  },
  {
    id: "rose", label: "Rose", swatch: "oklch(0.586 0.253 17.6)",
    palette: {
      "--color-violet-50":  "oklch(0.969 0.015 12.4)",
      "--color-violet-100": "oklch(0.941 0.030 12.6)",
      "--color-violet-200": "oklch(0.892 0.058 10.0)",
      "--color-violet-300": "oklch(0.811 0.111 8.6)",
      "--color-violet-400": "oklch(0.712 0.194 13.4)",
      "--color-violet-500": "oklch(0.645 0.246 16.4)",
      "--color-violet-600": "oklch(0.586 0.253 17.6)",
      "--color-violet-700": "oklch(0.514 0.222 16.9)",
      "--color-violet-800": "oklch(0.455 0.188 13.1)",
      "--color-violet-900": "oklch(0.408 0.153 2.4)",
      "--color-violet-950": "oklch(0.271 0.105 12.8)",
    },
  },
  {
    id: "pink", label: "Pink", swatch: "oklch(0.592 0.249 0.6)",
    palette: {
      "--color-violet-50":  "oklch(0.971 0.014 343.2)",
      "--color-violet-100": "oklch(0.948 0.028 342.3)",
      "--color-violet-200": "oklch(0.899 0.061 343.2)",
      "--color-violet-300": "oklch(0.823 0.120 343.4)",
      "--color-violet-400": "oklch(0.718 0.202 349.8)",
      "--color-violet-500": "oklch(0.656 0.241 355.0)",
      "--color-violet-600": "oklch(0.592 0.249 0.6)",
      "--color-violet-700": "oklch(0.525 0.223 3.9)",
      "--color-violet-800": "oklch(0.459 0.187 3.8)",
      "--color-violet-900": "oklch(0.408 0.153 2.4)",
      "--color-violet-950": "oklch(0.271 0.105 12.8)",
    },
  },
];

const STORAGE_KEY    = "skill-theme";
const HC_KEY         = "skill-high-contrast";
const ACCENT_KEY     = "skill-accent";

let mode         = $state<ThemeMode>(loadMode());
let resolved     = $state<"light" | "dark">(resolve(loadMode()));
let highContrast = $state<boolean>(loadHC());
let accentId     = $state<string>(loadAccentId());

function loadMode(): ThemeMode {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(STORAGE_KEY);
  if (v === "light" || v === "dark") return v;
  return "system";
}

function loadAccentId(): string {
  if (typeof localStorage === "undefined") return "violet";
  return localStorage.getItem(ACCENT_KEY) ?? "violet";
}

/** Load persisted theme + accent from Tauri settings on startup. */
export async function initFromSettings() {
  try {
    const [theme, _lang] = await invoke<[string, string]>("get_theme_and_language");
    if (theme === "light" || theme === "dark" || theme === "system") {
      setTheme(theme as ThemeMode);
    }
  } catch { /* not available (e.g. dev server without Tauri) */ }
  try {
    const id = await invoke<string>("get_accent_color");
    applyAccent(id);
  } catch { /* degrade gracefully */ }
}

function loadHC(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(HC_KEY) === "true";
}

function resolve(m: ThemeMode): "light" | "dark" {
  if (m !== "system") return m;
  if (typeof window === "undefined") return "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function apply() {
  if (typeof document === "undefined") return;
  resolved = resolve(mode);
  document.documentElement.classList.toggle("dark", resolved === "dark");
  document.documentElement.classList.toggle("high-contrast", highContrast);
}

/** Listen for OS theme changes (only matters in "system" mode). */
if (typeof window !== "undefined") {
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => {
      if (mode === "system") apply();
    });
  // Also respond to OS high-contrast / forced-colors preference.
  window
    .matchMedia("(prefers-contrast: more)")
    .addEventListener("change", (e) => {
      if (!localStorage.getItem(HC_KEY)) {
        highContrast = e.matches;
        apply();
      }
    });
}

export function getTheme(): ThemeMode { return mode; }
export function getResolved(): "light" | "dark" { return resolved; }
export function getHighContrast(): boolean { return highContrast; }
export function getAccentId(): string { return accentId; }

// ── Accent application ────────────────────────────────────────────────────────

/**
 * Override the Tailwind `--color-violet-*` CSS custom properties on <html>
 * so every `violet-*` utility class adopts the new accent hue instantly.
 * Falls back to the "violet" preset if `id` is unknown.
 */
export function applyAccent(id: string) {
  const preset = ACCENT_PRESETS.find(p => p.id === id) ?? ACCENT_PRESETS[0];
  accentId = preset.id;
  if (typeof localStorage !== "undefined") {
    if (preset.id === "violet") localStorage.removeItem(ACCENT_KEY);
    else localStorage.setItem(ACCENT_KEY, preset.id);
  }
  if (typeof document === "undefined") return;
  const style = document.documentElement.style;
  const accentFamilies = ["violet", "blue", "indigo", "sky"] as const;
  for (const [prop, val] of Object.entries(preset.palette)) {
    style.setProperty(prop, val);
    for (const family of accentFamilies) {
      if (family === "violet") continue;
      style.setProperty(prop.replace("--color-violet-", `--color-${family}-`), val);
    }
  }
}

export function setAccent(id: string) {
  applyAccent(id);
  invoke("set_accent_color", { accent: id }).catch(e => console.warn("[theme] set_accent_color failed:", e));
}

export function setTheme(m: ThemeMode) {
  mode = m;
  if (typeof localStorage !== "undefined") {
    if (m === "system") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, m);
  }
  apply();
  // Persist to settings.json via Tauri
  invoke("set_theme", { theme: m }).catch(e => console.warn("[theme] set_theme failed:", e));
}

export function setHighContrast(on: boolean) {
  highContrast = on;
  if (typeof localStorage !== "undefined") {
    if (on) localStorage.setItem(HC_KEY, "true");
    else localStorage.removeItem(HC_KEY);
  }
  apply();
}

export function toggleHighContrast() {
  setHighContrast(!highContrast);
}

/** Toggle between light and dark (ignores system — just flips the resolved value). */
export function toggleTheme() {
  setTheme(resolved === "dark" ? "light" : "dark");
}

/** Cycle: system → light → dark → system */
export function cycleTheme() {
  const next: Record<ThemeMode, ThemeMode> = {
    system: "light",
    light: "dark",
    dark: "system",
  };
  setTheme(next[mode]);
}

// Apply on load
apply();
applyAccent(loadAccentId());
