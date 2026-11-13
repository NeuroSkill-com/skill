// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Pure helpers for the electrodermal-activity card: the rolling trace buffer and
// how an arousal z-score is described. Kept out of the component so they can be
// tested without a DOM.

/** Seconds of raw skin conductance the card keeps and charts. */
export const EDA_TRACE_WINDOW_S = 120;

/** A rolling trace as two parallel arrays: Unix seconds and microsiemens. */
export interface EdaTrace {
  t: number[];
  v: number[];
}

export const EMPTY_TRACE: EdaTrace = { t: [], v: [] };

/**
 * Append `(timestamp, µS)` samples and drop everything older than `windowS`
 * before the newest sample. Returns a new trace (the input is not mutated) so a
 * Svelte `$state` assignment picks the change up. Non-finite samples are skipped.
 */
export function appendTrace(trace: EdaTrace, samples: [number, number][], windowS = EDA_TRACE_WINDOW_S): EdaTrace {
  const t = trace.t.slice();
  const v = trace.v.slice();
  for (const [ts, us] of samples) {
    if (!Number.isFinite(ts) || !Number.isFinite(us)) continue;
    t.push(ts);
    v.push(us);
  }
  if (t.length === 0) return EMPTY_TRACE;
  const cutoff = t[t.length - 1] - windowS;
  let drop = 0;
  while (drop < t.length && t[drop] < cutoff) drop++;
  return drop === 0 ? { t, v } : { t: t.slice(drop), v: v.slice(drop) };
}

export type ArousalLevel = "warmingUp" | "low" | "typical" | "elevated" | "high";

/**
 * Describe an arousal z-score. `null` means the daemon has not yet learned this
 * session's baseline, which is not the same as "typical".
 */
export function arousalLevel(z: number | null | undefined): ArousalLevel {
  if (z == null || !Number.isFinite(z)) return "warmingUp";
  if (z < -1) return "low";
  if (z < 1) return "typical";
  if (z < 2) return "elevated";
  return "high";
}

export const AROUSAL_COLORS: Record<ArousalLevel, string> = {
  warmingUp: "#6b7280",
  low: "#38bdf8",
  typical: "#22c55e",
  elevated: "#f59e0b",
  high: "#ef4444",
};

/** Position of a z-score on a 0..100 bar spanning -3..+3, clamped. */
export function arousalBar(z: number | null | undefined): number {
  if (z == null || !Number.isFinite(z)) return 0;
  return Math.max(0, Math.min(100, ((z + 3) / 6) * 100));
}

/** Percentage of recent samples rejected as artifacts, from a 0..1 valid fraction. */
export function rejectedPercent(validFraction: number): number {
  if (!Number.isFinite(validFraction)) return 0;
  return Math.round((1 - Math.max(0, Math.min(1, validFraction))) * 100);
}

/** Below this valid fraction the card warns about skin contact. */
export const POOR_CONTACT_FRACTION = 0.7;
