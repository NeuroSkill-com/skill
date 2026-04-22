// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Types, constants, and pure helper functions for the history page.
 *
 * Extracted from `routes/history/+page.svelte`.
 */

import type { EpochRow } from "$lib/dashboard/SessionDetail.svelte";
import { fmtTimeShort, fromUnix, pad } from "$lib/format";
import type { LabelRow } from "$lib/types";

// ── Types ────────────────────────────────────────────────────────────────────

export interface SessionEntry {
  csv_file: string;
  csv_path: string;
  session_start_utc: number | null;
  session_end_utc: number | null;
  device_name: string | null;
  serial_number: string | null;
  battery_pct: number | null;
  total_samples: number | null;
  sample_rate_hz: number | null;
  labels: LabelRow[];
  file_size_bytes: number;
  /** Average signal-to-noise ratio (dB) for the session. `null` for very old sessions. */
  avg_snr_db: number | null;
}

export interface HistoryStatsData {
  total_sessions: number;
  total_secs: number;
  this_week_secs: number;
  last_week_secs: number;
}

// ── Grid constants ───────────────────────────────────────────────────────────

export const GRID_COLS = 24;
export const GRID_ROWS = 720;
export const GRID_BIN = 5;

// ── Session colors ───────────────────────────────────────────────────────────

export { SESSION_COLORS, sessionColor } from "$lib/constants";

// ── Pure helpers ─────────────────────────────────────────────────────────────

/** Format a compact duration from total seconds (e.g. "2h 15m"). */
export function fmtDurCompact(secs: number): string {
  if (secs <= 0) return "";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h > 0 && m > 0) return `${h}h ${m}m`;
  if (h > 0) return `${h}h`;
  return `${m}m`;
}

/** Total recording seconds for a list of sessions. */
export function totalDurationSecs(sessionList: SessionEntry[]): number {
  let total = 0;
  for (const s of sessionList) {
    if (s.session_start_utc && s.session_end_utc) total += s.session_end_utc - s.session_start_utc;
  }
  return total;
}

/** Compute day-level aggregate metrics from loaded timeseries. */
export function dayAggregateMetrics(
  sessionList: SessionEntry[],
  getTs: (csvPath: string) => EpochRow[] | null,
): { avgRelax: number; avgEngage: number; totalEpochs: number } | null {
  let sumR = 0,
    sumE = 0,
    n = 0;
  for (const s of sessionList) {
    const ts = getTs(s.csv_path);
    if (!ts) continue;
    for (const ep of ts) {
      sumR += ep.relaxation;
      sumE += ep.engagement;
      n++;
    }
  }
  if (n === 0) return null;
  return { avgRelax: sumR / n, avgEngage: sumE / n, totalEpochs: n };
}

/** Collect all labels for a day from sessions. */
export function labelsForDay(_dayKey: string, sessionsForDay: SessionEntry[]): LabelRow[] {
  const all: LabelRow[] = [];
  for (const s of sessionsForDay) all.push(...s.labels);
  return all;
}

/** Proximity threshold in seconds — labels within this window are "close". */
export const LABEL_PROXIMITY_SECS = 15;

/** Assign rainbow HSL colors to labels based on their temporal proximity. */
export function assignLabelRainbowColors(labels: LabelRow[]): Map<number, string> {
  if (labels.length === 0) return new Map();
  const sorted = [...labels].sort((a, b) => a.eeg_start - b.eeg_start);
  const colorMap = new Map<number, string>();
  const hueStep = 360 / Math.max(sorted.length, 1);
  for (let i = 0; i < sorted.length; i++) {
    colorMap.set(sorted[i].id, `hsl(${Math.round(i * hueStep)}, 85%, 60%)`);
  }
  return colorMap;
}

/** Given a hovered label and all labels in context, returns sets of
 *  exact-match and close-proximity label IDs.  */
export function labelRelations(hovered: LabelRow, all: LabelRow[]): { exactIds: Set<number>; closeIds: Set<number> } {
  const exactIds = new Set<number>();
  const closeIds = new Set<number>();
  const hovText = hovered.text.toLowerCase().trim();
  for (const l of all) {
    if (l.id === hovered.id) continue;
    if (l.text.toLowerCase().trim() === hovText) exactIds.add(l.id);
    if (Math.abs(l.eeg_start - hovered.eeg_start) <= LABEL_PROXIMITY_SECS) closeIds.add(l.id);
  }
  return { exactIds, closeIds };
}

// ── Date helpers ─────────────────────────────────────────────────────────────

export function dateKey(utc: number): string {
  const d = fromUnix(utc);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function dateLabel(key: string): string {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
  });
}

export function fmtTime(utc: number | null): string {
  return utc ? fmtTimeShort(utc) : "—";
}

export function fmtDuration(start: number | null, end: number | null): string {
  if (!start || !end || end <= start) return "—";
  const secs = end - start;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  return h > 0 ? `${h}h ${m}m ${s}s` : m > 0 ? `${m}m ${s}s` : `${s}s`;
}

export { fmtBytes as fmtSize } from "$lib/format";

export function fmtSamples(n: number | null): string {
  if (!n) return "—";
  return n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}K` : String(n);
}

export function dayPct(utc: number, dayStart: number): number {
  return Math.max(0, Math.min(100, ((utc - dayStart) / 86400) * 100));
}

// ── Local-day helpers ────────────────────────────────────────────────────────

// NOTE: secToUtcDir, localDayBounds, and buildLocalDays have been moved to
// the Rust `skill-history` crate (local_days.rs) as the single source of
// truth.  The frontend now calls `list_local_session_days` and
// `list_sessions_for_local_day` IPC commands instead.
//
// See crates/skill-history/src/local_days.rs for the canonical implementation
// and its comprehensive test suite.
