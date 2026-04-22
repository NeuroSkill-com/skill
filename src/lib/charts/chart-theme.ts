// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Shared chart theme helper — reads CSS custom properties defined in app.css
 * and provides them as typed values for canvas-based chart rendering.
 *
 * Usage:
 *   const theme = readChartTheme(canvas);
 *   ctx.fillStyle = theme.bg;
 */

export interface ChartTheme {
  bg: string;
  bgStrip: string;
  grid: string;
  baseline: string;
  label: string;
}

/**
 * Read the chart CSS custom properties from the computed style of an element.
 * Falls back to sensible dark-mode defaults if the properties aren't set.
 */
export function readChartTheme(el: HTMLElement): ChartTheme {
  const cs = getComputedStyle(el);
  return {
    bg: cs.getPropertyValue("--chart-bg").trim() || "#0d0d1a",
    bgStrip: cs.getPropertyValue("--chart-bg-strip").trim() || "#111120",
    grid: cs.getPropertyValue("--chart-grid").trim() || "rgba(255,255,255,0.07)",
    baseline: cs.getPropertyValue("--chart-baseline").trim() || "rgba(255,255,255,0.12)",
    label: cs.getPropertyValue("--chart-label").trim() || "rgba(255,255,255,0.4)",
  };
}
