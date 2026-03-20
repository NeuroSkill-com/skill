// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Titlebar reactive state — shared between page components (writers) and
 * CustomTitleBar.svelte (reader).
 *
 * Consolidated from the former per-page titlebar files:
 *   titlebar-state.svelte.ts, chat-titlebar.svelte.ts,
 *   history-titlebar.svelte.ts, label-titlebar.svelte.ts,
 *   help-search-state.svelte.ts
 */

// ── Factory helpers ───────────────────────────────────────────────────────────

/**
 * Create a module-level `$state` store with a typed initial value.
 *
 * ```ts
 * export const chatTitlebar = createTitlebarState({ modelName: "", status: "stopped" as const });
 * // page writes:   chatTitlebar.modelName = "phi-3";
 * // titlebar reads: {chatTitlebar.modelName}
 * ```
 */
export function createTitlebarState<T extends Record<string, unknown>>(initial: T): T {
  let s: T = $state(initial) as T;
  return s;
}

/**
 * Create a callback bag initialised with no-op functions.
 *
 * ```ts
 * export const hCbs = createTitlebarCallbacks({ prev: () => {}, next: () => {} });
 * // page sets:      hCbs.prev = () => navigatePrev();
 * // titlebar calls: hCbs.prev();
 * ```
 */
export function createTitlebarCallbacks<T extends Record<string, (...args: any[]) => void>>(
  defaults: T,
): T {
  // Plain object — intentionally not reactive.  The titlebar calls these
  // synchronously; Svelte reactivity is not needed on the callback bag itself.
  return { ...defaults };
}

// ── Chat titlebar ─────────────────────────────────────────────────────────────

export type LlmStatus = "stopped" | "loading" | "running";

export const chatTitlebarState = createTitlebarState<{
  modelName: string;
  status: LlmStatus;
}>({ modelName: "", status: "stopped" });

// ── History titlebar ──────────────────────────────────────────────────────────

/** Calendar heatmap view granularity. */
export type HistoryViewMode = "year" | "month" | "week" | "day";

/** Display data written by the history page, read by CustomTitleBar. */
export const hBar = createTitlebarState({
  /** True once the history page has mounted and set up callbacks. */
  active:          false,
  /** Mirrors history page `daysLoading`. */
  daysLoading:     true,
  /** Total number of local calendar days with recordings. */
  dayCount:        0,
  /** Index of the currently displayed day (0 = newest). */
  currentDayIdx:   0,
  /** Pre-formatted label for the current day (e.g. "Mar 12, 2026"). */
  currentDayLabel: "",
  /** Whether compare mode is active. */
  compareMode:     false,
  /** Number of sessions selected for comparison. */
  compareCount:    0,
  /** Whether the labels panel is open. */
  showLabels:      false,
  /** Current history view mode (calendar granularity). */
  viewMode:        "month" as HistoryViewMode,
  /** Navigation label for calendar views (e.g. "March 2026"). */
  calendarLabel:   "",
});

/** Callbacks set by the history page on mount. */
export const hCbs = createTitlebarCallbacks({
  prev:          () => {},
  next:          () => {},
  toggleCompare: () => {},
  openCompare:   () => {},
  toggleLabels:  () => {},
  reload:        () => {},
  setViewMode:   (_m: HistoryViewMode) => {},
  calendarPrev:  () => {},
  calendarNext:  () => {},
});

// ── Label titlebar ────────────────────────────────────────────────────────────

export const labelTitlebarState = createTitlebarState({ active: false, elapsed: "0s" });

// ── Help titlebar ─────────────────────────────────────────────────────────────

export const helpTitlebarState = $state({ query: "", version: "…" });
