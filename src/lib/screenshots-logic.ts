// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * ScreenshotsTab pure logic — extracted from ScreenshotsTab.svelte.
 *
 * Sparkline SVG path generation, metric formatting, and rolling history buffers.
 */

/** Push a value into a fixed-size rolling history array. */
export function pushHistory(arr: number[], val: number, maxLen = 60): number[] {
  const next = [...arr, val];
  return next.length > maxLen ? next.slice(next.length - maxLen) : next;
}

/** Format microseconds as a human-readable string. */
export function fmtUs(us: number): string {
  if (us < 1000) return `${us}\u00B5s`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(1)}ms`;
  return `${(us / 1_000_000).toFixed(2)}s`;
}

/** Format milliseconds as a human-readable string. */
export function fmtMs(ms: number): string {
  if (ms < 1000) return `${ms.toFixed(0)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

/** Format seconds as ETA string (e.g. "~2m 30s"). */
export function fmtEta(secs: number): string {
  if (secs <= 0) return "";
  if (secs < 60) return `~${Math.round(secs)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return s > 0 ? `~${m}m ${s}s` : `~${m}m`;
}

/**
 * Build an SVG polyline `d` path for a sparkline chart.
 *
 * @param data - array of values
 * @param w - SVG viewport width
 * @param h - SVG viewport height
 * @param pad - padding from edges
 */
export function sparklinePath(data: number[], w: number, h: number, pad = 2): string {
  if (data.length < 2) return "";
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const xStep = (w - pad * 2) / (data.length - 1);
  return data.map((v, i) => {
    const x = pad + i * xStep;
    const y = h - pad - ((v - min) / range) * (h - pad * 2);
    return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(" ");
}

/**
 * Build an SVG area path (sparkline + bottom fill) for a filled chart.
 */
export function areaPath(data: number[], w: number, h: number, pad = 2): string {
  const line = sparklinePath(data, w, h, pad);
  if (!line) return "";
  const xEnd = pad + (data.length - 1) * ((w - pad * 2) / (data.length - 1));
  return `${line} L${xEnd.toFixed(1)},${(h - pad).toFixed(1)} L${pad},${(h - pad).toFixed(1)} Z`;
}
