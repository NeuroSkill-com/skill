// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Dashboard page business logic — extracted from routes/+page.svelte.
 *
 * Pure functions for EEG score computation, display formatting, and
 * device classification. No Svelte reactivity — unit-testable independently.
 */

// ── Score computation ─────────────────────────────────────────────────────────

/**
 * Sigmoid mapping (0, ∞) → (0, 100) with midpoint at x=1.
 * Used to compress raw band-power ratios into a 0–100 score.
 */
export function sigmoid100(x: number): number {
  return 100 / (1 + Math.exp(-2.5 * (x - 1)));
}

/** Band-power channel shape from the backend. */
export interface BandChannel {
  rel_alpha?: number;
  rel_beta?: number;
  rel_theta?: number;
}

/** Intermediate raw scores before EMA smoothing. */
export interface RawScores {
  focus: number;
  relax: number;
  engagement: number;
}

/**
 * Compute raw focus/relax/engagement scores from a band-power snapshot.
 * Returns `null` if the snapshot has no valid channels.
 */
export function computeRawScores(channels: BandChannel[]): RawScores | null {
  if (!channels || channels.length === 0) return null;
  let sumFocus = 0;
  let sumRelax = 0;
  let sumEngage = 0;
  let n = 0;
  for (const ch of channels) {
    const a = ch.rel_alpha || 0;
    const b = ch.rel_beta || 0;
    const t = ch.rel_theta || 0;
    const denom1 = a + t;
    const denom2 = b + t;
    const denom3 = a + t;
    if (denom1 > 0.001) sumFocus += b / denom1;
    if (denom2 > 0.001) sumRelax += a / denom2;
    if (denom3 > 0.001) sumEngage += b / denom3;
    n++;
  }
  if (n === 0) return null;
  return {
    focus: sigmoid100(sumFocus / n),
    relax: sigmoid100(sumRelax / n),
    engagement: 100 / (1 + Math.exp(-2 * (sumEngage / n - 0.8))),
  };
}

/**
 * Apply exponential moving average (EMA) smoothing to a score.
 */
export function emaSmooth(prev: number, raw: number, tau: number): number {
  return prev + tau * (raw - prev);
}

// ── Display formatting ────────────────────────────────────────────────────────

/** Format seconds as HH:MM:SS. */
export function fmtUptime(s: number): string {
  return [Math.floor(s / 3600), Math.floor((s % 3600) / 60), s % 60]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

/** Format EEG microvolt value with sign and unit. */
export function fmtEeg(v: number | null | undefined): string {
  return v != null && Number.isFinite(v) ? `${(v >= 0 ? "+" : "") + v.toFixed(1)} \u00B5V` : "\u2014";
}

/** Redact a serial number / MAC, showing only the last segment. */
export function redact(v: string): string {
  const parts = v.split("-");
  return [...parts.slice(0, -1).map((p) => "*".repeat(p.length)), parts.at(-1)].join("-");
}

/** Goal progress percentage (clamped 0-100). */
export function goalProgress(totalSecs: number, goalMinutes: number): number {
  if (goalMinutes <= 0) return 0;
  return Math.min(100, (totalSecs / 60 / goalMinutes) * 100);
}

// ── Device classification ─────────────────────────────────────────────────────

export type DeviceKind = "muse" | "ganglion" | "mw75" | "hermes" | "emotiv" | "idun" | "openbci" | "unknown";

export function isMuseDevice(kind: DeviceKind): boolean {
  return kind === "muse" || kind === "unknown";
}

export function hasBattery(kind: DeviceKind): boolean {
  return isMuseDevice(kind) || kind === "mw75" || kind === "emotiv" || kind === "idun";
}
