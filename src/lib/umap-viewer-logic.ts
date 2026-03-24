// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * UmapViewer3D pure logic — extracted from UmapViewer3D.svelte.
 *
 * Geometry normalization, color mapping, and point cloud generation
 * for the 3D UMAP scatter plot. No Three.js or Svelte dependencies.
 */

import type { UmapPoint } from "$lib/types";

// ── Turbo colormap ────────────────────────────────────────────────────────────

/**
 * Attempt to apply a Turbo-like RGB mapping for a parameter t in [0, 1].
 * Falls back to a simple blue→cyan→green→yellow→red gradient.
 */
export function turboApprox(t: number): [number, number, number] {
  const tc = Math.max(0, Math.min(1, t));
  // Attempt a piecewise approximation of the Turbo colormap
  const r = Math.max(0, Math.min(1, 1.5 - Math.abs(tc - 0.75) * 4));
  const g = Math.max(0, Math.min(1, 1.5 - Math.abs(tc - 0.5) * 4));
  const b = Math.max(0, Math.min(1, 1.5 - Math.abs(tc - 0.25) * 4));
  return [r, g, b];
}

/** Convert RGB [0-1] triple to a hex color string. */
export function rgbToHex(r: number, g: number, b: number): string {
  const hex = (v: number) =>
    Math.round(v * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${hex(r)}${hex(g)}${hex(b)}`;
}

// ── Point cloud geometry ──────────────────────────────────────────────────────

/**
 * Normalize UMAP points into a [-1, 1]^3 cube centered at origin.
 * Returns a Float32Array of xyz triples (length = points.length * 3).
 */
export function normalisePoints(pts: UmapPoint[]): Float32Array {
  const n = pts.length;
  const out = new Float32Array(n * 3);
  let mnX = Infinity,
    mxX = -Infinity,
    mnY = Infinity,
    mxY = -Infinity,
    mnZ = Infinity,
    mxZ = -Infinity;

  for (const p of pts) {
    const z = p.z ?? 0;
    if (p.x < mnX) mnX = p.x;
    if (p.x > mxX) mxX = p.x;
    if (p.y < mnY) mnY = p.y;
    if (p.y > mxY) mxY = p.y;
    if (z < mnZ) mnZ = z;
    if (z > mxZ) mxZ = z;
  }

  const rX = mxX - mnX || 1;
  const rY = mxY - mnY || 1;
  const rZ = mxZ - mnZ || 1;
  const scale = Math.max(rX, rY, rZ);

  for (let i = 0; i < n; i++) {
    const p = pts[i];
    out[i * 3] = ((p.x - mnX) / scale) * 2 - 1;
    out[i * 3 + 1] = ((p.y - mnY) / scale) * 2 - 1;
    out[i * 3 + 2] = (((p.z ?? 0) - mnZ) / scale) * 2 - 1;
  }
  return out;
}

/**
 * Generate random xyz positions in [-1, 1]^3 for initial point-cloud animation.
 */
export function randomPositions(count: number, seed = 42): Float32Array {
  const out = new Float32Array(count * 3);
  // Simple deterministic pseudo-random (mulberry32)
  let s = seed | 0;
  const rand = () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  for (let i = 0; i < out.length; i++) {
    out[i] = rand() * 2 - 1;
  }
  return out;
}

/**
 * Compute per-point color array based on a normalized parameter (0-1 per point).
 * Returns a Float32Array of rgb triples.
 */
export function buildColorArray(values: number[], isDark: boolean, dimFactor = 0.75): Float32Array {
  const out = new Float32Array(values.length * 3);
  for (let i = 0; i < values.length; i++) {
    let [r, g, b] = turboApprox(values[i]);
    if (!isDark) {
      r *= dimFactor;
      g *= dimFactor;
      b *= dimFactor;
    }
    out[i * 3] = r;
    out[i * 3 + 1] = g;
    out[i * 3 + 2] = b;
  }
  return out;
}
