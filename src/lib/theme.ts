// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Single source of truth for every semantic colour used in inline `style=`
 * attributes across the app.
 *
 * Tailwind *class* names (e.g. `text-emerald-500`, `bg-muted`) stay in the
 * templates as-is — only colours that must be dynamic JS values live here.
 *
 * Naming convention:  C_<ROLE>
 */

// ── Base palette ──────────────────────────────────────────────────────────────

/** green-500  — success / connected / good signal / low load */
export const C_GOOD = "#22c55e";
/** yellow-500 — warning / scanning / fair signal / mid load */
export const C_WARN = "#eab308";
/** red-500    — error / bt_off / poor signal / high load */
export const C_BAD = "#ef4444";
/** slate-400  — neutral / disconnected ring / no-signal / absent reading */
export const C_NEUTRAL = "#94a3b8";
/** slate-500  — muted text in disconnected / inactive state */
export const C_MUTED = "#64748b";
/** slate-300  — border / ring colour in disconnected state */
export const C_BORDER = "#cbd5e1";

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Expand a 6-digit hex to `rgba(r,g,b,alpha)`. */
export function rgba(hex: string, alpha: number): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

/**
 * Colour for a **percentage level** where higher is better (battery, signal %).
 * @param pct    0–100
 * @param warnAt threshold below which the value is considered a warning (default 30)
 * @param goodAt threshold above which the value is considered good (default 60)
 */
export function colorForLevel(pct: number, warnAt = 30, goodAt = 60): string {
  return pct >= goodAt ? C_GOOD : pct >= warnAt ? C_WARN : C_BAD;
}

/**
 * Colour for a **load ratio** where lower is better (GPU %, CPU %, etc.).
 * @param ratio  0.0–1.0
 * @param warnAt ratio above which the value is a warning (default 0.50)
 * @param badAt  ratio above which the value is bad     (default 0.80)
 */
export function colorForLoad(ratio: number, warnAt = 0.5, badAt = 0.8): string {
  return ratio >= badAt ? C_BAD : ratio >= warnAt ? C_WARN : C_GOOD;
}

/**
 * Colour for an **RSSI value** in dBm (less negative = stronger = better).
 * @param dBm   0 means absent (returns C_NEUTRAL)
 */
export function colorForRssi(dBm: number): string {
  return dBm === 0 ? C_NEUTRAL : dBm > -65 ? C_GOOD : dBm > -80 ? C_WARN : C_BAD;
}

// ── Semantic maps ─────────────────────────────────────────────────────────────

/** Colour for each EEG signal-quality label. */
export const QUALITY_COLORS: Record<string, string> = {
  good: C_GOOD,
  fair: C_WARN,
  poor: C_BAD,
  no_signal: C_NEUTRAL,
};

/** Per-state ring / badge / text / border colours for the Muse connection ring. */
export const STATE_COLORS = {
  connected: { ring: C_GOOD, badge: rgba(C_GOOD, 0.15), text: C_GOOD, border: rgba(C_GOOD, 0.35) },
  scanning: { ring: C_WARN, badge: rgba(C_WARN, 0.13), text: C_WARN, border: rgba(C_WARN, 0.32) },
  bt_off: { ring: C_BAD, badge: rgba(C_BAD, 0.13), text: C_BAD, border: rgba(C_BAD, 0.32) },
  disconnected: { ring: C_NEUTRAL, badge: "transparent", text: C_MUTED, border: C_BORDER },
} as const;
