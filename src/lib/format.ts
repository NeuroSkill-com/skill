// SPDX-License-Identifier: GPL-3.0-only
// Shared date/time/duration formatting helpers.

// ── Primitives ────────────────────────────────────────────────────────────────

/** Zero-pad a number to 2 digits. */
export function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** Convert a unix-second UTC timestamp to a JS Date. */
export function fromUnix(utc: number): Date {
  return new Date(utc * 1000);
}

/** Convert a JS Date to unix seconds (integer). */
export function toUnix(d: Date): number {
  return Math.floor(d.getTime() / 1000);
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

// ── Metric color helpers ──────────────────────────────────────────────────────

/**
 * Pick a color based on a value crossing thresholds.
 *
 * `thresholds` is an array of `[cutoff, color]` pairs sorted **descending**.
 * The first entry whose cutoff the value exceeds wins; the last color is the fallback.
 *
 * Example: `thresholdColor(score, [[60, "#22c55e"], [30, "#f59e0b"]], "#6b7280")`
 */
export function thresholdColor(
  value: number,
  thresholds: [number, string][],
  fallback: string,
): string {
  for (const [cutoff, color] of thresholds) {
    if (value > cutoff) return color;
  }
  return fallback;
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

// ── Date-key helpers ──────────────────────────────────────────────────────────

/**
 * Format a JS Date as a local "YYYY-MM-DD" date key string.
 * Useful for calendar/history day keys derived from local time.
 */
export function dateToLocalKey(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/**
 * Format a JS Date as a compact local "YYYYMMDD" string (no dashes).
 * Useful for localStorage keys and compact day identifiers.
 */
export function dateToCompactKey(d: Date): string {
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}`;
}

/**
 * Format a unix-second UTC timestamp as a local "YYYY-MM-DD" date key.
 * Shorthand for `dateToLocalKey(fromUnix(utc))`.
 */
export function unixToLocalKey(utc: number): string {
  return dateToLocalKey(fromUnix(utc));
}

/**
 * Format a unix-second UTC timestamp as a compact local "YYYYMMDD" string.
 * Shorthand for `dateToCompactKey(fromUnix(utc))`.
 */
export function unixToCompactKey(utc: number): string {
  return dateToCompactKey(fromUnix(utc));
}

/**
 * UTC midnight of a local "YYYY-MM-DD" key, returned as unix seconds.
 * The timestamp corresponds to local midnight (not UTC midnight).
 */
export function localKeyToUnix(key: string): number {
  const [y, m, d] = key.split("-").map(Number);
  return Math.floor(new Date(y, m - 1, d).getTime() / 1000);
}

// ── Additional locale-aware formatters ────────────────────────────────────────

/** Format a unix-second UTC timestamp as locale short date+time, e.g. "3/12/26, 2:30 PM". */
export function fmtLocaleShort(utc: number): string {
  return fromUnix(utc).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
}

/**
 * Format a unix-second timestamp for a datetime-local input value.
 * Returns "YYYY-MM-DDThh:mm:ss" in local time.
 */
export function fmtDateTimeLocalInput(utc: number): string {
  const d = fromUnix(utc);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/**
 * Parse a datetime-local input value "YYYY-MM-DDThh:mm..." to unix seconds.
 */
export function parseDateTimeLocalInput(s: string): number {
  return Math.floor(new Date(s).getTime() / 1000);
}

/** Format a countdown timer as "MM:SS". */
export function fmtCountdown(totalSecs: number): string {
  const m = Math.floor(totalSecs / 60);
  const s = totalSecs % 60;
  return `${pad(m)}:${pad(s)}`;
}

// ── Canvas helpers ────────────────────────────────────────────────────────────

/**
 * Set up a canvas for hi-DPI (Retina) rendering.
 *
 * Sets canvas.width/height to the CSS size × devicePixelRatio, applies the
 * DPR scale transform, and returns the 2D context. All subsequent drawing
 * coordinates are in CSS pixels.
 *
 * @param canvas  The canvas element.
 * @param cssW    Desired CSS width (defaults to `canvas.clientWidth`).
 * @param cssH    Desired CSS height (defaults to `canvas.clientHeight`).
 * @returns       The 2D rendering context with DPR transform applied.
 */
export function setupHiDpiCanvas(
  canvas: HTMLCanvasElement,
  cssW?: number,
  cssH?: number,
): CanvasRenderingContext2D {
  const dpr = devicePixelRatio || 1;
  const w = cssW ?? canvas.clientWidth;
  const h = cssH ?? canvas.clientHeight;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  const ctx = canvas.getContext("2d")!;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return ctx;
}

/**
 * Returns the current devicePixelRatio (for canvas-only sizing without setTransform).
 */
export function getDpr(): number {
  return devicePixelRatio || 1;
}

// ── Size formatting ──────────────────────────────────────────────────────────

/** Format a byte count as a human-readable string (e.g. "1.5 MB"). */
export function fmtBytes(bytes: number): string {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + " GB";
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(1) + " MB";
  if (bytes >= 1e3) return (bytes / 1e3).toFixed(1) + " KB";
  return bytes + " B";
}

/** Format a size given in gigabytes as a human-readable string (e.g. "3.2 GB"). */
export function fmtGB(gb: number): string {
  if (gb >= 1) return gb.toFixed(1) + " GB";
  return (gb * 1024).toFixed(0) + " MB";
}
