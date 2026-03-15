// SPDX-License-Identifier: GPL-3.0-only
// Shared date/time/duration formatting helpers.

/** Zero-pad a number to 2 digits. */
export function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** Format a unix-second UTC timestamp as local "HH:MM:SS" (24h). */
export function fmtTime(utc: number): string {
  return new Date(utc * 1000).toLocaleTimeString([], {
    hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false,
  });
}

/** Format a unix-second UTC timestamp as local "HH:MM" (24h). */
export function fmtTimeShort(utc: number): string {
  return new Date(utc * 1000).toLocaleTimeString(undefined, {
    hour: "2-digit", minute: "2-digit",
  });
}

/** Format a unix-second UTC timestamp as a short local date, e.g. "Mar 12, 2026". */
export function fmtDate(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString(undefined, {
    year: "numeric", month: "short", day: "numeric",
  });
}

/** Format a unix-second UTC timestamp as local "YYYY-MM-DD". */
export function fmtDateIso(unix: number): string {
  const d = new Date(unix * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Format a unix-second UTC timestamp as "YYYY-MM-DD HH:MM". */
export function fmtDateTime(utc: number): string {
  const d = new Date(utc * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Format a unix-second UTC timestamp as "YYYY-MM-DD HH:MM:SS". */
export function fmtDateTimeSecs(utc: number): string {
  const d = new Date(utc * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/**
 * Format a duration in seconds as a human-readable string.
 * "Xs", "Xm Ys", or "Xh Ym".
 */
export function fmtDuration(secs: number): string {
  if (secs <= 0) return "0s";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}h`;
  if (m > 0) return s > 0 ? `${m}m ${s}s` : `${m}m`;
  return `${s}s`;
}

/** Format a pair of unix-second timestamps as a duration string, with "–" fallback. */
export function fmtDurationRange(start: number | null, end: number | null): string {
  if (!start || !end) return "–";
  return fmtDuration(end - start);
}

/** Short seconds formatter: "Xm Ys" or "Xs". */
export function fmtSecs(s: number): string {
  return s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
}

/** Format a YYYYMMDD UTC day string as a localised date. */
export function fmtUtcDay(day: string): string {
  if (day.length === 8)
    return new Date(Date.UTC(+day.slice(0, 4), +day.slice(4, 6) - 1, +day.slice(6, 8), 12))
      .toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  return day;
}

/** Format a LOCAL YYYY-MM-DD key as a human-readable date string. */
export function fmtDayKey(localKey: string): string {
  if (!localKey) return localKey;
  const [y, m, d] = localKey.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    year: "numeric", month: "short", day: "numeric",
  });
}

/** Format a unix-second UTC timestamp as a short locale date+time, e.g. "Mar 12, 2026, 14:30". */
export function fmtDateTimeLocale(utc: number): string {
  return new Date(utc * 1000).toLocaleDateString(undefined, {
    year: "numeric", month: "short", day: "numeric",
    hour: "2-digit", minute: "2-digit",
  });
}

/** Format milliseconds as "Xs" or "Xms". */
export function fmtMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${Math.round(ms)}ms`;
}

/** Format elapsed seconds as "Xm YYs" or "Xs". */
export function fmtElapsed(s: number): string {
  const m = Math.floor(s / 60), ss = s % 60;
  return m > 0 ? `${m}m ${String(ss).padStart(2, "0")}s` : `${s}s`;
}
