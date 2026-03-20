// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Font-size store — manages the global UI font size multiplier.
 * Persists to localStorage as "skill-font-size".
 * Applies a CSS custom property `--font-scale` on <html> and sets
 * `font-size` on the root element so all rem/em units scale accordingly.
 */

const STORAGE_KEY = "skill-font-size";

/** Available presets (percentage of base 16px). */
export const FONT_SIZE_PRESETS = [
  { label: "XS",  value: 75  },
  { label: "S",   value: 87  },
  { label: "M",   value: 100 },
  { label: "L",   value: 112 },
  { label: "XL",  value: 125 },
  { label: "XXL", value: 150 },
] as const;

export type FontSizeValue = (typeof FONT_SIZE_PRESETS)[number]["value"];

let current = $state<number>(load());

function load(): number {
  if (typeof localStorage === "undefined") return 100;
  const v = localStorage.getItem(STORAGE_KEY);
  if (v) {
    const n = parseInt(v, 10);
    if (FONT_SIZE_PRESETS.some(p => p.value === n)) return n;
  }
  return 100;
}

function apply() {
  if (typeof document === "undefined") return;
  const pct = current;
  // Set root font-size so all rem units scale
  document.documentElement.style.fontSize = `${pct}%`;
  // Also expose as a CSS variable for anything that needs it
  document.documentElement.style.setProperty("--font-scale", String(pct / 100));
}

export function getFontSize(): number { return current; }

export function setFontSize(pct: number) {
  current = pct;
  if (typeof localStorage !== "undefined") {
    if (pct === 100) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, String(pct));
  }
  apply();
}

// Apply on load
apply();
