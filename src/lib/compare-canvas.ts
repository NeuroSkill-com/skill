// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Canvas drawing functions for the compare page — spectrum bars, diff chart,
 * radar, heatmaps.
 *
 * Extracted from `routes/compare/+page.svelte`.
 */

import { setupHiDpiCanvas } from "$lib/format";
import { bandKeys, bandMeta, radarMetrics } from "$lib/compare-types";
import type { SessionMetrics, EpochRow } from "$lib/dashboard/SessionDetail.svelte";

// ── Heatmap definitions ──────────────────────────────────────────────────────

export const HM_BANDS_DEF = [
  { key: "rd" as const, sym: "δ", lo: [20, 20, 50]  as [number,number,number], hi: [99,  102, 241] as [number,number,number] },
  { key: "rt" as const, sym: "θ", lo: [25, 15, 55]  as [number,number,number], hi: [139,  92, 246] as [number,number,number] },
  { key: "ra" as const, sym: "α", lo: [10, 40, 20]  as [number,number,number], hi: [34,  197,  94] as [number,number,number] },
  { key: "rb" as const, sym: "β", lo: [10, 30, 60]  as [number,number,number], hi: [59,  130, 246] as [number,number,number] },
  { key: "rg" as const, sym: "γ", lo: [55, 45, 10]  as [number,number,number], hi: [245, 158,  11] as [number,number,number] },
] as const;

export const HM_SCORES_DEF = [
  { key: "relaxation"  as const, sym: "Rlx", lo: [10, 45,  35] as [number,number,number], hi: [16,  185, 129] as [number,number,number] },
  { key: "engagement"  as const, sym: "Eng", lo: [55, 45,  10] as [number,number,number], hi: [245, 158,  11] as [number,number,number] },
  { key: "med"         as const, sym: "Med", lo: [35, 15,  70] as [number,number,number], hi: [139,  92, 246] as [number,number,number] },
  { key: "cog"         as const, sym: "Cog", lo: [10, 35,  55] as [number,number,number], hi: [14,  165, 233] as [number,number,number] },
  { key: "drow"        as const, sym: "Drw", lo: [55, 10,  10] as [number,number,number], hi: [239,  68,  68] as [number,number,number] },
] as const;

export const HEATMAP_ROW_H  = 14;
export const HEATMAP_LABEL_W = 22;

// ── Primitives ───────────────────────────────────────────────────────────────

export function lerpRgba(lo: [number,number,number], hi: [number,number,number], t: number, alpha: number): string {
  const r = Math.round(lo[0] + (hi[0] - lo[0]) * t);
  const g = Math.round(lo[1] + (hi[1] - lo[1]) * t);
  const b = Math.round(lo[2] + (hi[2] - lo[2]) * t);
  return `rgba(${r},${g},${b},${alpha})`;
}

function roundedBar(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  if (h < r * 2) r = h / 2;
  if (r < 0) r = 0;
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h);
  ctx.lineTo(x, y + h);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r);
  ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  ctx.lineTo(x + r, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - r);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}

// ── Spectrum bar ─────────────────────────────────────────────────────────────

export function drawSpectrum(canvas: HTMLCanvasElement, m: SessionMetrics, _label: string) {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const ctx = setupHiDpiCanvas(canvas, w, h);

  const vals = bandKeys.map(k => m[k] ?? 0);
  let sum = vals.reduce((a, b) => a + b, 0);
  if (sum < 1e-6) sum = 1;

  const barY = 0, barH = h, r = 8;
  ctx.save();
  ctx.beginPath();
  roundRect(ctx, 0, barY, w, barH, r);
  ctx.clip();

  let x = 0;
  for (let i = 0; i < 5; i++) {
    const segW = (vals[i] / sum) * w;
    ctx.fillStyle = bandMeta[i].color;
    ctx.globalAlpha = 0.82;
    ctx.fillRect(x, barY, segW + 0.5, barH);
    x += segW;
  }

  ctx.globalAlpha = 0.38;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, barY, w, barH);
  ctx.globalAlpha = 1;

  x = 0;
  ctx.textBaseline = "middle";
  ctx.textAlign = "center";
  for (let i = 0; i < 5; i++) {
    const segW = (vals[i] / sum) * w;
    if (segW > 32) {
      ctx.font = "bold 11px ui-sans-serif, system-ui, sans-serif";
      ctx.fillStyle = "#fff";
      ctx.globalAlpha = 0.95;
      ctx.fillText(`${bandMeta[i].sym} ${Math.round(vals[i] / sum * 100)}%`, x + segW / 2, barH / 2);
    }
    x += segW;
  }
  ctx.restore();
}

// ── Diff chart ───────────────────────────────────────────────────────────────

export function drawDiffChart(canvas: HTMLCanvasElement, a: SessionMetrics, b: SessionMetrics) {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const ctx = setupHiDpiCanvas(canvas, w, h);

  const ml = 4, mr = 4, mt = 14, mb = 16;
  const cw = w - ml - mr;
  const ch = h - mt - mb;

  const valsA = bandKeys.map(k => a[k] ?? 0);
  const valsB = bandKeys.map(k => b[k] ?? 0);
  const maxVal = Math.max(...valsA, ...valsB, 0.01);

  const nBands = 5;
  const groupW = cw / nBands;
  const barW   = groupW * 0.32;
  const gap    = groupW * 0.06;

  for (let i = 0; i < nBands; i++) {
    const gx = ml + i * groupW;

    const hA = (valsA[i] / maxVal) * ch;
    ctx.fillStyle = bandMeta[i].color;
    ctx.globalAlpha = 0.9;
    roundedBar(ctx, gx + groupW / 2 - barW - gap / 2, mt + ch - hA, barW, hA, 3);
    ctx.fill();

    const hB = (valsB[i] / maxVal) * ch;
    ctx.globalAlpha = 0.45;
    roundedBar(ctx, gx + groupW / 2 + gap / 2, mt + ch - hB, barW, hB, 3);
    ctx.fill();

    ctx.globalAlpha = 1;
    ctx.font = "bold 9px ui-monospace, 'JetBrains Mono', monospace";
    ctx.fillStyle = bandMeta[i].color;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    ctx.fillText(bandMeta[i].sym, gx + groupW / 2, mt + ch + 4);
  }

  ctx.font = "bold 8px ui-sans-serif, system-ui, sans-serif";
  ctx.textBaseline = "top";
  ctx.textAlign = "left";
  ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("color") || "#888";
  ctx.globalAlpha = 0.7;
  ctx.fillText("A ■  B ▪", ml + 2, 2);
  ctx.globalAlpha = 1;
}

// ── Radar chart ──────────────────────────────────────────────────────────────

export function drawRadar(canvas: HTMLCanvasElement, a: SessionMetrics, b: SessionMetrics) {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const ctx = setupHiDpiCanvas(canvas, w, h);

  const cx = w / 2, cy = h / 2;
  const radius = Math.min(cx, cy) - 28;
  const n = radarMetrics.length;
  const angleStep = (Math.PI * 2) / n;

  // Grid rings
  for (let ring = 1; ring <= 4; ring++) {
    const r = (ring / 4) * radius;
    ctx.beginPath();
    for (let i = 0; i <= n; i++) {
      const angle = i * angleStep - Math.PI / 2;
      const x = cx + Math.cos(angle) * r;
      const y = cy + Math.sin(angle) * r;
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.strokeStyle = "rgba(128,128,128,0.12)";
    ctx.lineWidth = 0.5;
    ctx.stroke();
  }

  // Axis lines + labels
  ctx.font = "600 9px ui-sans-serif, system-ui, sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  for (let i = 0; i < n; i++) {
    const angle = i * angleStep - Math.PI / 2;
    const ex = cx + Math.cos(angle) * radius;
    const ey = cy + Math.sin(angle) * radius;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(ex, ey);
    ctx.strokeStyle = "rgba(128,128,128,0.15)";
    ctx.lineWidth = 0.5;
    ctx.stroke();

    const lx = cx + Math.cos(angle) * (radius + 16);
    const ly = cy + Math.sin(angle) * (radius + 16);
    ctx.fillStyle = radarMetrics[i].color;
    ctx.globalAlpha = 0.8;
    ctx.fillText(radarMetrics[i].label, lx, ly);
    ctx.globalAlpha = 1;
  }

  function drawPoly(metrics: SessionMetrics, color: string, alpha: number) {
    ctx.beginPath();
    for (let i = 0; i < n; i++) {
      const angle = i * angleStep - Math.PI / 2;
      const val = Number(metrics[radarMetrics[i].key]) || 0;
      const r = (Math.min(val, radarMetrics[i].max) / radarMetrics[i].max) * radius;
      const x = cx + Math.cos(angle) * r;
      const y = cy + Math.sin(angle) * r;
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    }
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.globalAlpha = alpha * 0.2;
    ctx.fill();
    ctx.strokeStyle = color;
    ctx.globalAlpha = alpha;
    ctx.lineWidth = 1.5;
    ctx.stroke();
    ctx.globalAlpha = 1;

    for (let i = 0; i < n; i++) {
      const angle = i * angleStep - Math.PI / 2;
      const val = Number(metrics[radarMetrics[i].key]) || 0;
      const r = (Math.min(val, radarMetrics[i].max) / radarMetrics[i].max) * radius;
      const x = cx + Math.cos(angle) * r;
      const y = cy + Math.sin(angle) * r;
      ctx.beginPath();
      ctx.arc(x, y, 2.5, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.globalAlpha = alpha;
      ctx.fill();
      ctx.globalAlpha = 1;
    }
  }

  drawPoly(a, "#3b82f6", 0.9);
  drawPoly(b, "#f59e0b", 0.65);
}

// ── Heatmaps ─────────────────────────────────────────────────────────────────

export function drawBandHeatmap(canvas: HTMLCanvasElement, ts: EpochRow[], dark: boolean) {
  if (!canvas || ts.length < 2) return;
  const rows = HM_BANDS_DEF;
  const nRows = rows.length;
  const cssH  = HEATMAP_ROW_H * nRows;
  const cssW  = canvas.clientWidth;
  if (cssW <= 0) return;

  const ctx = setupHiDpiCanvas(canvas, cssW, cssH);
  ctx.fillStyle = dark ? "#0e0e1a" : "#f5f5fa";
  ctx.fillRect(0, 0, cssW, cssH);

  const plotW  = cssW - HEATMAP_LABEL_W;
  const nCols  = ts.length;
  const colW   = plotW / nCols;

  for (let ri = 0; ri < nRows; ri++) {
    const { key, sym, lo, hi } = rows[ri];
    const vals = ts.map(r => r[key] as number);
    const vMin = Math.min(...vals);
    const vMax = Math.max(...vals);
    const vRange = vMax - vMin || 1;
    const y0 = ri * HEATMAP_ROW_H;

    ctx.font = "bold 8px ui-monospace, 'JetBrains Mono', monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillStyle = lerpRgba(lo, hi, 0.85, 0.9);
    ctx.fillText(sym, HEATMAP_LABEL_W / 2, y0 + HEATMAP_ROW_H / 2);

    for (let ci = 0; ci < nCols; ci++) {
      const t = Math.max(0, Math.min(1, (vals[ci] - vMin) / vRange));
      ctx.fillStyle = lerpRgba(lo, hi, t, 0.15 + t * 0.85);
      ctx.fillRect(HEATMAP_LABEL_W + ci * colW, y0, colW + 0.5, HEATMAP_ROW_H);
    }

    if (ri < nRows - 1) {
      ctx.fillStyle = "rgba(0,0,0,0.35)";
      ctx.fillRect(HEATMAP_LABEL_W, y0 + HEATMAP_ROW_H - 0.5, plotW, 1);
    }
  }
}

export function drawScoreHeatmap(canvas: HTMLCanvasElement, ts: EpochRow[], dark: boolean) {
  if (!canvas || ts.length < 2) return;
  const rows  = HM_SCORES_DEF;
  const nRows = rows.length;
  const cssH  = HEATMAP_ROW_H * nRows;
  const cssW  = canvas.clientWidth;
  if (cssW <= 0) return;

  const ctx = setupHiDpiCanvas(canvas, cssW, cssH);
  ctx.fillStyle = dark ? "#0e0e1a" : "#f5f5fa";
  ctx.fillRect(0, 0, cssW, cssH);

  const plotW = cssW - HEATMAP_LABEL_W;
  const nCols = ts.length;
  const colW  = plotW / nCols;

  for (let ri = 0; ri < nRows; ri++) {
    const { key, sym, lo, hi } = rows[ri];
    const vals = ts.map(r => r[key] as number);
    const vMin = Math.min(...vals);
    const vMax = Math.max(...vals);
    const vRange = vMax - vMin || 1;
    const y0 = ri * HEATMAP_ROW_H;

    ctx.font = "bold 7px ui-monospace, 'JetBrains Mono', monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillStyle = lerpRgba(lo, hi, 0.85, 0.9);
    ctx.fillText(sym, HEATMAP_LABEL_W / 2, y0 + HEATMAP_ROW_H / 2);

    for (let ci = 0; ci < nCols; ci++) {
      const t = Math.max(0, Math.min(1, (vals[ci] - vMin) / vRange));
      ctx.fillStyle = lerpRgba(lo, hi, t, 0.15 + t * 0.85);
      ctx.fillRect(HEATMAP_LABEL_W + ci * colW, y0, colW + 0.5, HEATMAP_ROW_H);
    }

    if (ri < nRows - 1) {
      ctx.fillStyle = "rgba(0,0,0,0.35)";
      ctx.fillRect(HEATMAP_LABEL_W, y0 + HEATMAP_ROW_H - 0.5, plotW, 1);
    }
  }
}

export function drawBandDiffHeatmap(canvas: HTMLCanvasElement, tsA: EpochRow[], tsB: EpochRow[], dark: boolean) {
  if (!canvas || tsA.length < 2 || tsB.length < 2) return;
  const rows  = HM_BANDS_DEF;
  const nRows = rows.length;
  const cssH  = HEATMAP_ROW_H * nRows + 12;
  const cssW  = canvas.clientWidth;
  if (cssW <= 0) return;

  const ctx = setupHiDpiCanvas(canvas, cssW, cssH);
  ctx.fillStyle = dark ? "#0e0e1a" : "#f5f5fa";
  ctx.fillRect(0, 0, cssW, cssH);

  const nDisplay = Math.min(Math.max(tsA.length, tsB.length), 400);
  const plotW    = cssW - HEATMAP_LABEL_W;
  const colW     = plotW / nDisplay;

  function sampleIdx(tsLen: number, col: number): number {
    return Math.min(Math.round(col / (nDisplay - 1) * (tsLen - 1)), tsLen - 1);
  }

  const BLUE_LO:  [number,number,number] = [10,  50, 200];
  const BLUE_MID: [number,number,number] = [40,  80, 160];
  const NEUTRAL:  [number,number,number] = dark ? [18, 18, 28] : [230, 230, 242];
  const RED_MID:  [number,number,number] = [160, 40,  40];
  const RED_HI:   [number,number,number] = [220, 30,  30];

  function diffColor(d: number): string {
    const absD = Math.abs(d);
    const alpha = 0.12 + absD * 0.88;
    if (d < 0) {
      const t = Math.min(absD, 1);
      return t < 0.5 ? lerpRgba(NEUTRAL, BLUE_MID, t * 2, alpha) : lerpRgba(BLUE_MID, BLUE_LO, (t - 0.5) * 2, alpha);
    } else {
      const t = Math.min(d, 1);
      return t < 0.5 ? lerpRgba(NEUTRAL, RED_MID, t * 2, alpha) : lerpRgba(RED_MID, RED_HI, (t - 0.5) * 2, alpha);
    }
  }

  for (let ri = 0; ri < nRows; ri++) {
    const { key, sym, lo, hi } = rows[ri];
    const y0 = ri * HEATMAP_ROW_H;

    const diffs: number[] = [];
    for (let c = 0; c < nDisplay; c++) {
      const iA = sampleIdx(tsA.length, c);
      const iB = sampleIdx(tsB.length, c);
      diffs.push((tsB[iB][key] as number) - (tsA[iA][key] as number));
    }
    const maxAbsDiff = Math.max(...diffs.map(Math.abs), 0.001);

    ctx.font = "bold 8px ui-monospace, 'JetBrains Mono', monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillStyle = lerpRgba(lo, hi, 0.75, 0.8);
    ctx.fillText(sym, HEATMAP_LABEL_W / 2, y0 + HEATMAP_ROW_H / 2);

    for (let c = 0; c < nDisplay; c++) {
      const normD = diffs[c] / maxAbsDiff;
      ctx.fillStyle = diffColor(normD);
      ctx.fillRect(HEATMAP_LABEL_W + c * colW, y0, colW + 0.5, HEATMAP_ROW_H);
    }

    if (ri < nRows - 1) {
      ctx.fillStyle = "rgba(0,0,0,0.35)";
      ctx.fillRect(HEATMAP_LABEL_W, y0 + HEATMAP_ROW_H - 0.5, plotW, 1);
    }
  }

  // Legend bar
  const legendY = nRows * HEATMAP_ROW_H + 2;
  const legendH = 6;
  const nStops  = 80;
  const sw = plotW / nStops;
  for (let i = 0; i < nStops; i++) {
    const d = (i / (nStops - 1)) * 2 - 1;
    ctx.fillStyle = diffColor(d);
    ctx.globalAlpha = 1;
    ctx.fillRect(HEATMAP_LABEL_W + i * sw, legendY, sw + 0.5, legendH);
  }
  ctx.globalAlpha = 1;

  ctx.font = "7px ui-sans-serif, system-ui, sans-serif";
  ctx.textBaseline = "top";
  ctx.fillStyle = dark ? "rgba(100,120,200,0.9)" : "rgba(40,80,200,0.9)";
  ctx.textAlign = "left";
  ctx.fillText("A>B", HEATMAP_LABEL_W, legendY + legendH + 1);
  ctx.fillStyle = dark ? "rgba(200,70,70,0.9)" : "rgba(180,30,30,0.9)";
  ctx.textAlign = "right";
  ctx.fillText("B>A", cssW, legendY + legendH + 1);
}
