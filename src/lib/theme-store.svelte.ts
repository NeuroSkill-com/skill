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

const STORAGE_KEY = "skill-theme";
const HC_KEY      = "skill-high-contrast";

let mode         = $state<ThemeMode>(loadMode());
let resolved     = $state<"light" | "dark">(resolve(loadMode()));
let highContrast = $state<boolean>(loadHC());

function loadMode(): ThemeMode {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(STORAGE_KEY);
  if (v === "light" || v === "dark") return v;
  return "system";
}

/** Load persisted theme+language from Tauri settings on startup. */
export async function initFromSettings() {
  try {
    const [theme, _lang] = await invoke<[string, string]>("get_theme_and_language");
    if (theme === "light" || theme === "dark" || theme === "system") {
      setTheme(theme as ThemeMode);
    }
  } catch { /* not available (e.g. dev server without Tauri) */ }
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

export function setTheme(m: ThemeMode) {
  mode = m;
  if (typeof localStorage !== "undefined") {
    if (m === "system") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, m);
  }
  apply();
  // Persist to settings.json via Tauri
  invoke("set_theme", { theme: m }).catch(() => {});
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
