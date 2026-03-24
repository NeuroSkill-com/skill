// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * InteractiveGraph3D pure logic — extracted from InteractiveGraph3D.svelte.
 *
 * 3D vector math, Fibonacci sphere layout, and Turbo colormap.
 */

export type Vec3 = [number, number, number];

const GOLDEN = Math.PI * (3 - Math.sqrt(5)); // golden angle in radians

// ── Vector math ───────────────────────────────────────────────────────────────

export function add3(a: Vec3, b: Vec3): Vec3 {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}

export function scale3(v: Vec3, s: number): Vec3 {
  return [v[0] * s, v[1] * s, v[2] * s];
}

export function normalize3(v: Vec3): Vec3 {
  const len = Math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2) || 1;
  return [v[0] / len, v[1] / len, v[2] / len];
}

export function length3(v: Vec3): number {
  return Math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2);
}

// ── Fibonacci sphere ──────────────────────────────────────────────────────────

/**
 * Compute a point on the Fibonacci sphere for index `i` out of `n` total points.
 * Produces an approximately uniform distribution on the unit sphere.
 */
export function fibSphere(i: number, n: number): Vec3 {
  const y = 1 - (i / Math.max(n - 1, 1)) * 2;
  const r = Math.sqrt(Math.max(0, 1 - y * y));
  const theta = GOLDEN * i;
  return [Math.cos(theta) * r, y, Math.sin(theta) * r];
}

// ── Turbo colormap (simplified) ───────────────────────────────────────────────

/**
 * Attempt a piecewise approximation of the Turbo colormap for t in [0, 1].
 * Returns [r, g, b] in [0, 1].
 */
export function turbo(t: number): Vec3 {
  const tc = Math.max(0, Math.min(1, t));
  const r = Math.max(0, Math.min(1, 1.5 - Math.abs(tc - 0.75) * 4));
  const g = Math.max(0, Math.min(1, 1.5 - Math.abs(tc - 0.5) * 4));
  const b = Math.max(0, Math.min(1, 1.5 - Math.abs(tc - 0.25) * 4));
  return [r, g, b];
}

/** Convert a Turbo colormap value to a CSS hex string. */
export function turboCss(t: number): string {
  const [r, g, b] = turbo(t);
  const hex = (v: number) =>
    Math.round(v * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${hex(r)}${hex(g)}${hex(b)}`;
}

/** Convert a Turbo colormap value to a packed 0xRRGGBB integer. */
export function turboHex(t: number): number {
  const [r, g, b] = turbo(t);
  return (Math.round(r * 255) << 16) | (Math.round(g * 255) << 8) | Math.round(b * 255);
}
