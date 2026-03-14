// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Module-level reactive state shared between the history page and the custom
// titlebar so that the history window's navigation controls live inside the
// 30 px titlebar instead of a separate in-page header row.

/** Calendar heatmap view granularity. */
export type HistoryViewMode = "year" | "month" | "week" | "day";

/** Display data written by the history page, read by CustomTitleBar. */
export const hBar = $state({
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
export const hCbs = {
  prev:          () => {},
  next:          () => {},
  toggleCompare: () => {},
  openCompare:   () => {},
  toggleLabels:  () => {},
  reload:        () => {},
  setViewMode:   (_m: HistoryViewMode) => {},
  calendarPrev:  () => {},
  calendarNext:  () => {},
} as {
  prev:          () => void;
  next:          () => void;
  toggleCompare: () => void;
  openCompare:   () => void;
  toggleLabels:  () => void;
  reload:        () => void;
  setViewMode:   (m: HistoryViewMode) => void;
  calendarPrev:  () => void;
  calendarNext:  () => void;
};
