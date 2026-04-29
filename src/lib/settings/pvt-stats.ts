// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Pure helpers for the Psychomotor Vigilance Task (PVT) — extracted from
 * PvtPanel.svelte so the statistics can be unit-tested without pulling in
 * a Svelte renderer or the daemon HTTP layer.
 *
 * RT inputs are millisecond reaction times.  Lapse threshold follows
 * Dinges & Powell (1985) at 500 ms.  `slowest10_rt_ms` is the mean of the
 * worst 10 % of trials, which is the published anticipation-resistant
 * summary statistic — slightly more robust than overall mean RT to a few
 * outliers.
 */

export interface PvtStats {
  mean_rt_ms: number;
  median_rt_ms: number;
  slowest10_rt_ms: number;
  lapse_count: number;
  response_count: number;
}

export const LAPSE_THRESHOLD_MS = 500;

/** Arithmetic mean.  Returns 0 for an empty input. */
export function mean(xs: readonly number[]): number {
  if (xs.length === 0) return 0;
  let s = 0;
  for (const x of xs) s += x;
  return s / xs.length;
}

/**
 * Median of an unsorted array — picks the middle value (or the upper
 * middle for even-length inputs, matching the Svelte component's
 * implementation).  Returns 0 for an empty input.
 */
export function median(xs: readonly number[]): number {
  if (xs.length === 0) return 0;
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

/**
 * Mean of the slowest 10 % of trials.  When there are fewer than 10
 * trials the slowest-bucket size rounds down to zero, in which case we
 * fall back to the slowest single trial — that's the convention the
 * literature uses for very short PVTs.  Returns 0 for an empty input.
 */
export function slowest10Mean(xs: readonly number[]): number {
  if (xs.length === 0) return 0;
  const sorted = [...xs].sort((a, b) => a - b);
  const cut = Math.floor(sorted.length * 0.9);
  const tail = sorted.slice(cut);
  if (tail.length === 0) return sorted[sorted.length - 1];
  return mean(tail);
}

/** Trials with RT above the lapse threshold (Dinges & Powell 1985 = 500 ms). */
export function lapseCount(xs: readonly number[], threshold: number = LAPSE_THRESHOLD_MS): number {
  let n = 0;
  for (const x of xs) if (x > threshold) n += 1;
  return n;
}

/** Convenience: compute every summary statistic in one pass. */
export function computeStats(rts: readonly number[]): PvtStats {
  return {
    mean_rt_ms: mean(rts),
    median_rt_ms: median(rts),
    slowest10_rt_ms: slowest10Mean(rts),
    lapse_count: lapseCount(rts),
    response_count: rts.length,
  };
}
