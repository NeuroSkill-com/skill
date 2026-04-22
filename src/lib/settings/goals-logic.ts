// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * GoalsTab pure logic — extracted from GoalsTab.svelte.
 *
 * Progress bar coloring and time formatting for the daily goal tracker.
 */

/** Color for the daily goal progress bar based on minutes recorded. */
export function barColor(mins: number, goalMin: number): string {
  if (mins >= goalMin) return "#22c55e"; // green — goal met
  if (mins >= goalMin * 0.5) return "#3b82f6"; // blue — halfway+
  if (mins === 0) return "transparent";
  return "#6366f1"; // indigo — some progress
}

/** Format minutes as "1h 23m" or "45m" or "—" for zero. */
export function fmtMins(m: number): string {
  if (m === 0) return "\u2014";
  if (m < 60) return `${m}m`;
  const remainder = m % 60;
  return `${Math.floor(m / 60)}h${remainder > 0 ? ` ${remainder}m` : ""}`;
}
