// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Compare page business logic — pure functions extracted from +page.svelte.
 *
 * Timeline helpers, date/time conversions, and range computations.
 */

import type { EmbeddingSession } from "$lib/compare/compare-types";
import {
  dateToLocalKey,
  fmtDateTime,
  fmtDateTimeLocalInput,
  fmtDayKey,
  fmtDuration as fmtDurationSecs,
  fromUnix,
  parseDateTimeLocalInput,
} from "$lib/format";

// ── Date / time helpers ──────────────────────────────────────────────────────

/** Local date string "YYYY-MM-DD" from a UTC unix-second timestamp. */
export function localDateFromUtc(utc: number): string {
  return dateToLocalKey(fromUnix(utc));
}

/** UTC seconds for local midnight of a "YYYY-MM-DD" date string. */
export function localMidnight(dateStr: string): number {
  const [y, mo, d] = dateStr.split("-").map(Number);
  return Math.floor(new Date(y, mo - 1, d, 0, 0, 0).getTime() / 1000);
}

/** "HH:MM" from a UTC unix-second timestamp (local time). */
export function utcToTimeStr(utc: number): string {
  const d = new Date(utc * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** UTC seconds from a "YYYY-MM-DD" date and "HH:MM" time (local). */
export function timeStrToUtc(dateStr: string, timeStr: string): number {
  const [y, mo, d] = dateStr.split("-").map(Number);
  const [h, mi] = timeStr.split(":").map(Number);
  return Math.floor(new Date(y, mo - 1, d, h, mi, 0).getTime() / 1000);
}

/** Human-readable date label for a "YYYY-MM-DD" string. */
export function dayLabel(dateStr: string): string {
  return fmtDayKey(dateStr);
}

/** "YYYY-MM-DDThh:mm:ss" from a UTC unix-second timestamp (for datetime-local inputs). */
export function utcToDateTimeLocal(utc: number): string {
  return fmtDateTimeLocalInput(utc);
}

/** UTC seconds from a "YYYY-MM-DDThh:mm" datetime-local string. */
export function dateTimeLocalToUtc(dt: string): number {
  return parseDateTimeLocalInput(dt);
}

// ── Session helpers ──────────────────────────────────────────────────────────

/** Sessions that overlap the half-open interval [startUtc, endUtc). */
export function sessionsInRange(sessions: EmbeddingSession[], startUtc: number, endUtc: number): EmbeddingSession[] {
  return sessions.filter((s) => s.end_utc > startUtc && s.start_utc < endUtc);
}

/** Format a session as a human-readable label. */
export function sessionLabel(s: EmbeddingSession): string {
  const dt = fmtDateTime(s.start_utc);
  const dur = fmtDurationSecs(s.end_utc - s.start_utc);
  return `${dt}  (${dur}, ${s.n_epochs} ep)`;
}

/** Sorted unique day strings (newest first) that have recorded sessions. */
export function sortedSessionDays(sessions: EmbeddingSession[]): string[] {
  const s = new Set<string>();
  for (const sess of sessions) {
    const d = new Date(
      fromUnix(sess.start_utc).getFullYear(),
      fromUnix(sess.start_utc).getMonth(),
      fromUnix(sess.start_utc).getDate(),
    );
    const endD = fromUnix(sess.end_utc);
    const endMid = new Date(endD.getFullYear(), endD.getMonth(), endD.getDate());
    while (d <= endMid) {
      s.add(dateToLocalKey(d));
      d.setDate(d.getDate() + 1);
    }
  }
  return [...s].sort().reverse(); // newest first
}

// ── Range selection helpers ──────────────────────────────────────────────────

/** Auto-select the range covering all sessions within the 48h window starting at anchorUtc. */
export function autoSelectRange(sessions: EmbeddingSession[], anchorUtc: number): { start: number; end: number } {
  const windowSess = sessions.filter((s) => s.end_utc > anchorUtc && s.start_utc < anchorUtc + 172800);
  const rangeS = windowSess.length > 0 ? Math.min(...windowSess.map((s) => s.start_utc)) : anchorUtc;
  const rangeE = Math.min(
    windowSess.length > 0 ? Math.max(...windowSess.map((s) => s.end_utc)) : anchorUtc + 86400,
    rangeS + 86400,
  );
  return { start: rangeS, end: rangeE };
}

/** Snap a UTC timestamp to the nearest minute. */
export function snapToMinute(utc: number): number {
  return Math.round(utc / 60) * 60;
}

/** Convert a pointer event's X position within an element to a UTC timestamp in a 48h window. */
export function pointerToUtc(clientX: number, rect: DOMRect, anchorUtc: number): number {
  const pct = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
  return Math.round(anchorUtc + pct * 172800);
}
