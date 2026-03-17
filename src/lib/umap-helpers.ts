// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Pure math, color, and geometry helpers for the UMAP 3D viewer.
 *
 * Extracted from `UmapViewer3D.svelte` to reduce file size
 * and enable independent testing.
 */

import type { UmapPoint } from "$lib/types";

// ── Easing ───────────────────────────────────────────────────────────────────

export function easeOut(t: number): number { return 1 - (1 - t) ** 3; }

export function gauss(): number {
  return Math.sqrt(-2 * Math.log(Math.random() || 1e-10)) * Math.cos(Math.PI * 2 * Math.random());
}

// ── Color helpers ────────────────────────────────────────────────────────────

export function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => { const k = (n + h / 30) % 12; return l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1)); };
  return [f(0), f(8), f(4)];
}

export function labelHex(hue: number): string {
  const [r, g, b] = hslToRgb(hue, 0.85, 0.55);
  return `#${[r, g, b].map(v => Math.round(v * 255).toString(16).padStart(2, "0")).join("")}`;
}

/** Turbo colormap — returns raw [R, G, B] in 0–1 range. */
export function turboRaw(t: number): [number, number, number] {
  const c = Math.max(0, Math.min(1, t));
  const r = Math.max(0, Math.min(1, 0.13572138 + c*(4.61539260 + c*(-42.66032258 + c*(132.13108234 + c*(-152.54893924 + c* 59.28637943))))));
  const g = Math.max(0, Math.min(1, 0.09140261 + c*(2.19418839 + c*(  4.84296658 + c*(-14.18503333 + c*(   4.27729857 + c*  2.82956604))))));
  const b = Math.max(0, Math.min(1, 0.10667330 + c*(12.64194608 + c*(-60.58204836 + c*(110.36276771 + c*( -89.90310912 + c* 27.34824973))))));
  return [r, g, b];
}

/** Jet colormap — returns raw [R, G, B] in 0–1 range. */
export function jet(t: number): [number, number, number] {
  const c = Math.max(0, Math.min(1, t));
  return [
    Math.min(1, Math.max(0, 1.5 - Math.abs(4 * c - 3))),
    Math.min(1, Math.max(0, 1.5 - Math.abs(4 * c - 2))),
    Math.min(1, Math.max(0, 1.5 - Math.abs(4 * c - 1))),
  ];
}

/** Jet colormap → CSS hex string. */
export function jetHex(t: number): string {
  const [r, g, b] = jet(t);
  return `#${[r, g, b].map(v => Math.round(v * 255).toString(16).padStart(2, "0")).join("")}`;
}

// ── Timestamp formatting ─────────────────────────────────────────────────────

/** Format a UTC timestamp for the gradient legend, adapting precision to span. */
export function fmtGradientTs(utc: number, span: number): string {
  if (utc <= 0) return "";
  const d = new Date(utc * 1000);
  if (span >= 172800) return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  if (span >= 3600)   return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/** Format a UTC timestamp as "HH:MM" local time. */
export function fmtUtcTime(utc: number): string {
  const d = new Date(utc * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** Convert UTC seconds → local "YYYY-MM-DD" string. */
export function utcToLocalDate(utc: number): string {
  const d = new Date(utc * 1000);
  return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,"0")}-${String(d.getDate()).padStart(2,"0")}`;
}

// ── Geometry ─────────────────────────────────────────────────────────────────

/** Normalise UMAP points into a [-1, 1]³ cube centered at the centroid. */
export function normalise(pts: UmapPoint[]): Float32Array {
  const n = pts.length;
  const pos = new Float32Array(n * 3);
  if (n === 0) return pos;
  let mx = 0, my = 0, mz = 0;
  for (const p of pts) { mx += p.x; my += p.y; mz += p.z; }
  mx /= n; my /= n; mz /= n;
  let maxR = 0;
  for (const p of pts) {
    const dx = p.x - mx, dy = p.y - my, dz = p.z - mz;
    maxR = Math.max(maxR, Math.sqrt(dx*dx + dy*dy + dz*dz));
  }
  if (maxR < 1e-8) maxR = 1;
  for (let i = 0; i < n; i++) {
    pos[i*3]   = (pts[i].x - mx) / maxR;
    pos[i*3+1] = (pts[i].y - my) / maxR;
    pos[i*3+2] = (pts[i].z - mz) / maxR;
  }
  return pos;
}

/** Generate random initial positions for animation. */
export function randomPositions(pts: UmapPoint[]): Float32Array {
  const n = pts.length;
  const pos = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) {
    pos[i*3]   = gauss() * 0.15;
    pos[i*3+1] = gauss() * 0.15;
    pos[i*3+2] = gauss() * 0.15;
  }
  return pos;
}

/**
 * Build tick positions for the trace gradient legend.
 * Returns array of { t: number (0-1), label: string }.
 */
export function buildTraceTimeTicks(sorted: number[]): { t: number; label: string }[] {
  if (sorted.length < 2) return [];
  const tMin = sorted[0];
  const tMax = sorted[sorted.length - 1];
  const span = tMax - tMin;
  if (span <= 0) return [];

  // Choose a "nice" tick interval
  const TARGET_TICKS = 5;
  const raw = span / TARGET_TICKS;
  const niceIntervals = [60, 120, 300, 600, 900, 1800, 3600, 7200, 14400, 28800, 43200, 86400];
  let interval = niceIntervals.find(i => i >= raw) ?? 86400;

  // Generate ticks at whole multiples of interval
  const firstTick = Math.ceil(tMin / interval) * interval;
  const ticks: { t: number; label: string }[] = [];
  for (let ts = firstTick; ts <= tMax; ts += interval) {
    const normT = (ts - tMin) / span;
    if (normT < 0.02 || normT > 0.98) continue; // skip ticks too close to edges
    ticks.push({ t: normT, label: fmtGradientTs(ts, span) });
  }

  // Always include endpoints if we have room
  if (ticks.length === 0 || ticks[0].t > 0.1) {
    ticks.unshift({ t: 0, label: fmtGradientTs(tMin, span) });
  }
  if (ticks.length === 0 || ticks[ticks.length - 1].t < 0.9) {
    ticks.push({ t: 1, label: fmtGradientTs(tMax, span) });
  }
  return ticks;
}

/**
 * Build a date → color palette for multi-day UMAP coloring.
 * Returns a Map from local date string to [r, g, b] tuple.
 */
export function buildDatePaletteRaw(pts: UmapPoint[]): Map<string, [number, number, number]> {
  const daySet = new Set<string>();
  for (const p of pts) { if (p.utc > 0) daySet.add(utcToLocalDate(p.utc)); }
  const days = [...daySet].sort();
  const palette = new Map<string, [number, number, number]>();
  for (let i = 0; i < days.length; i++) {
    const hue = (i / Math.max(days.length, 1)) * 360;
    palette.set(days[i], hslToRgb(hue, 0.8, 0.55));
  }
  return palette;
}
