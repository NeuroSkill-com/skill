// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Chart color scheme store — manages EEG waveform & chart color palettes.
 *
 * Persists the active scheme name to localStorage as "skill-chart-scheme".
 * Applies CSS custom properties on <html> that override the channel colors
 * and chart accent colors used by EegChart, BandChart, TimeSeriesChart, etc.
 *
 * Channel colors are exposed as:
 *   --ch-color-0  (TP9)
 *   --ch-color-1  (AF7)
 *   --ch-color-2  (AF8)
 *   --ch-color-3  (TP10)
 *
 * Consumers read them via getChannelColors() or getComputedStyle().
 */

const STORAGE_KEY = "skill-chart-scheme";

// ── Scheme definitions ────────────────────────────────────────────────────────

export interface ChartScheme {
  /** Machine-readable ID. */
  id: string;
  /** Human-readable display name (i18n key). */
  labelKey: string;
  /** Short description (i18n key). */
  descKey: string;
  /** 4 channel colors [TP9, AF7, AF8, TP10]. */
  channels: [string, string, string, string];
  /** Band / metric accent colors. */
  delta: string;
  theta: string;
  alpha: string;
  beta:  string;
  gamma: string;
}

/**
 * Default — the original Skill palette.
 * Green / Blue / Purple / Orange channels with vivid band colors.
 */
const DEFAULT: ChartScheme = {
  id:       "default",
  labelKey: "chartScheme.default",
  descKey:  "chartScheme.defaultDesc",
  channels: ["#22c55e", "#60a5fa", "#c084fc", "#fb923c"],
  delta: "#6366f1", theta: "#22c55e", alpha: "#3b82f6", beta: "#f59e0b", gamma: "#ef4444",
};

/**
 * Neon — high-contrast vivid colors on dark backgrounds.
 * Cyan / Magenta / Lime / Gold channels.
 */
const NEON: ChartScheme = {
  id:       "neon",
  labelKey: "chartScheme.neon",
  descKey:  "chartScheme.neonDesc",
  channels: ["#00fff7", "#ff00e5", "#b5ff00", "#ffd000"],
  delta: "#7c3aed", theta: "#00fff7", alpha: "#3b82f6", beta: "#ffd000", gamma: "#ff006a",
};

/**
 * Monochrome — single-hue with brightness variation.
 * White-to-gray channel differentiation for minimal distraction.
 */
const MONO: ChartScheme = {
  id:       "mono",
  labelKey: "chartScheme.mono",
  descKey:  "chartScheme.monoDesc",
  channels: ["#e2e8f0", "#94a3b8", "#cbd5e1", "#64748b"],
  delta: "#94a3b8", theta: "#a1a1aa", alpha: "#d4d4d8", beta: "#71717a", gamma: "#52525b",
};

/**
 * Deuteranopia-safe — avoids red-green confusion.
 * Uses blue / orange / yellow / purple.
 * Based on the Wong (2011) colorblind-safe palette.
 */
const CB_DEUTAN: ChartScheme = {
  id:       "cb-deutan",
  labelKey: "chartScheme.cbDeutan",
  descKey:  "chartScheme.cbDeutanDesc",
  channels: ["#0072b2", "#e69f00", "#f0e442", "#cc79a7"],
  delta: "#0072b2", theta: "#e69f00", alpha: "#56b4e9", beta: "#f0e442", gamma: "#cc79a7",
};

/**
 * Protanopia-safe — avoids red confusion.
 * Uses blue / yellow / teal / pink.
 */
const CB_PROTAN: ChartScheme = {
  id:       "cb-protan",
  labelKey: "chartScheme.cbProtan",
  descKey:  "chartScheme.cbProtanDesc",
  channels: ["#2196f3", "#ffeb3b", "#009688", "#e91e63"],
  delta: "#2196f3", theta: "#009688", alpha: "#03a9f4", beta: "#ffeb3b", gamma: "#e91e63",
};

/**
 * Tritanopia-safe — avoids blue-yellow confusion.
 * Uses red / green / magenta / cyan.
 */
const CB_TRITAN: ChartScheme = {
  id:       "cb-tritan",
  labelKey: "chartScheme.cbTritan",
  descKey:  "chartScheme.cbTritanDesc",
  channels: ["#d32f2f", "#388e3c", "#e040fb", "#00bcd4"],
  delta: "#d32f2f", theta: "#388e3c", alpha: "#e040fb", beta: "#00bcd4", gamma: "#ff7043",
};

// ── All schemes in display order ──────────────────────────────────────────────

export const CHART_SCHEMES: ChartScheme[] = [
  DEFAULT,
  NEON,
  MONO,
  CB_DEUTAN,
  CB_PROTAN,
  CB_TRITAN,
];

// ── Reactive state ────────────────────────────────────────────────────────────

let current = $state<string>(load());

function load(): string {
  if (typeof localStorage === "undefined") return "default";
  return localStorage.getItem(STORAGE_KEY) ?? "default";
}

function findScheme(id: string): ChartScheme {
  return CHART_SCHEMES.find(s => s.id === id) ?? DEFAULT;
}

function apply() {
  if (typeof document === "undefined") return;
  const scheme = findScheme(current);
  const root = document.documentElement;

  // Channel colors
  scheme.channels.forEach((c, i) => {
    root.style.setProperty(`--ch-color-${i}`, c);
  });

  // Band colors
  root.style.setProperty("--band-delta", scheme.delta);
  root.style.setProperty("--band-theta", scheme.theta);
  root.style.setProperty("--band-alpha", scheme.alpha);
  root.style.setProperty("--band-beta", scheme.beta);
  root.style.setProperty("--band-gamma", scheme.gamma);
}

// ── Public API ────────────────────────────────────────────────────────────────

export function getChartScheme(): string { return current; }

export function getActiveScheme(): ChartScheme { return findScheme(current); }

export function setChartScheme(id: string) {
  current = id;
  if (typeof localStorage !== "undefined") {
    if (id === "default") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, id);
  }
  apply();
}

/**
 * Returns the 4 channel colors for the active scheme.
 * Use this instead of importing EEG_COLOR from constants.ts when you want
 * the user's chosen palette.
 */
export function getChannelColors(): readonly [string, string, string, string] {
  return findScheme(current).channels;
}

// Apply on load
apply();
