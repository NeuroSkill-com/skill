// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * HooksTab pure logic — extracted from HooksTab.svelte.
 *
 * Timestamp conversion and relative-time formatting for the hooks log.
 */

/**
 * Convert a UTC microsecond timestamp to Unix seconds.
 * Returns 0 for invalid input.
 */
export function tsToUnix(tsUtc: number): number {
  if (!tsUtc || Number.isNaN(tsUtc)) return 0;
  // Detect microsecond vs second timestamps
  return tsUtc > 1e15 ? Math.floor(tsUtc / 1_000_000) : tsUtc > 1e12 ? Math.floor(tsUtc / 1000) : Math.floor(tsUtc);
}

/**
 * Format a relative age string from a UTC timestamp.
 * @param tsUtc - timestamp (microseconds, milliseconds, or seconds)
 * @param nowSecs - current time in Unix seconds
 * @param agoLabel - localized "ago" suffix
 */
export function relativeAge(tsUtc: number, nowSecs: number, agoLabel = "ago"): string {
  const unix = tsToUnix(tsUtc);
  if (!unix || Number.isNaN(unix)) return "";
  const diff = nowSecs - unix;
  if (diff < 0) return "";
  if (diff < 60) return `${diff}s ${agoLabel}`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ${agoLabel}`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ${agoLabel}`;
  return `${Math.floor(diff / 86400)}d ${agoLabel}`;
}
