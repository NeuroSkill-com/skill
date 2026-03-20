// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Canvas rendering functions for the history page.
 *
 * Extracted from `routes/history/+page.svelte` to reduce file size.
 * All functions are pure — they take data + canvas and produce visual output
 * without referencing any reactive state.
 */

import type { LabelRow } from "$lib/types";
import type { EpochRow }  from "$lib/dashboard/SessionDetail.svelte";
import { setupHiDpiCanvas } from "$lib/format";
import {
  type SessionEntry,
  GRID_COLS, GRID_ROWS, GRID_BIN,
  SESSION_COLORS, assignLabelRainbowColors,
} from "$lib/history-helpers";

// ── Types ────────────────────────────────────────────────────────────────────

export interface DotTimelineData {
  sessions: SessionEntry[];
  dayStart: number;
  labels: LabelRow[];
}

export interface GridData {
  sessions: SessionEntry[];
  dayStart: number;
  labels: LabelRow[];
  screenshotTs: Set<number>;
}

/** Callback to look up timeseries data for a session's CSV path. */
export type TsAccessor = (csvPath: string) => EpochRow[] | null;

// ── Heatmap color utility ────────────────────────────────────────────────────

/** Return a Tailwind class string for a calendar heatmap cell. */
export function heatColor(count: number, maxC: number): string {
  if (count === 0) return "";
  const intensity = Math.min(1, count / Math.max(1, maxC));
  if (intensity < 0.25) return "bg-violet-200/60 dark:bg-violet-900/40";
  if (intensity < 0.5)  return "bg-violet-300/70 dark:bg-violet-800/50";
  if (intensity < 0.75) return "bg-violet-400/80 dark:bg-violet-700/60";
  return "bg-violet-500 dark:bg-violet-600/80";
}

// ── Epoch dot timeline (week / day views) ────────────────────────────────────

/** Render session epoch dots and label markers on a timeline canvas. */
export function renderDayDots(
  canvas: HTMLCanvasElement,
  data: DotTimelineData,
  getTs: TsAccessor,
) {
  const w = canvas.clientWidth, h = canvas.clientHeight;
  if (w === 0 || h === 0) return;
  const ctx = setupHiDpiCanvas(canvas, w, h);

  const { sessions, dayStart, labels } = data;
  const dayEnd = dayStart + 86400;

  // Draw hour grid lines
  ctx.strokeStyle = getComputedStyle(canvas).getPropertyValue("--dot-grid") || "rgba(128,128,128,0.08)";
  ctx.lineWidth = 0.5;
  for (let hr = 0; hr <= 24; hr += 3) {
    const x = (hr / 24) * w;
    ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke();
  }

  // Draw hour labels at the top
  ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("--dot-hour-text") || "rgba(128,128,128,0.35)";
  ctx.font = `${Math.max(7, h * 0.14)}px system-ui, sans-serif`;
  ctx.textAlign = "center";
  for (let hr = 0; hr < 24; hr += 6) {
    const x = (hr / 24) * w;
    ctx.fillText(`${String(hr).padStart(2, "0")}`, x + 2, Math.max(8, h * 0.18));
  }

  // Compute vertical layout: split height into bands per session
  const dotAreaTop = Math.max(10, h * 0.22);
  const dotAreaH   = h - dotAreaTop - 2;
  const nSessions  = sessions.length;
  const bandH      = nSessions > 0 ? dotAreaH / nSessions : dotAreaH;

  // Draw epoch dots for each session
  sessions.forEach((session, sIdx) => {
    const color = SESSION_COLORS[sIdx % SESSION_COLORS.length];
    const ts = getTs(session.csv_path);
    if (!ts || ts.length === 0) return;

    const bandY = dotAreaTop + sIdx * bandH;
    const dotR  = Math.min(2.5, Math.max(1, bandH * 0.3));

    ctx.fillStyle = color;
    ctx.globalAlpha = 0.7;

    for (const row of ts) {
      if (row.t < dayStart || row.t >= dayEnd) continue;
      const x = ((row.t - dayStart) / 86400) * w;
      // Map relaxation (0–1) to Y within the band
      const valNorm = Math.max(0, Math.min(1, row.relaxation));
      const y = bandY + (1 - valNorm) * (bandH - dotR * 2) + dotR;
      ctx.beginPath();
      ctx.arc(x, y, dotR, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1.0;
  });

  // Draw labels as rainbow-colored circles
  if (labels.length > 0) {
    const labelColors = assignLabelRainbowColors(labels);
    ctx.globalAlpha = 0.9;
    const dotR = Math.max(3, Math.min(5, h * 0.06));
    for (const label of labels) {
      const t = label.eeg_start;
      if (t < dayStart || t >= dayEnd) continue;
      const x = ((t - dayStart) / 86400) * w;
      const color = labelColors.get(label.id) ?? "#f59e0b";
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.arc(x, h - dotR - 1, dotR, 0, Math.PI * 2);
      ctx.fill();
      // Subtle white border for visibility
      ctx.strokeStyle = "rgba(255,255,255,0.6)";
      ctx.lineWidth = 0.5;
      ctx.stroke();
    }
    ctx.globalAlpha = 1.0;
  }
}

// ── Day-grid heatmap (24 cols × 720 rows) ────────────────────────────────────

/** Render the 24×720 day-grid heatmap with session epochs, labels, and screenshots. */
export function renderDayGrid(
  canvas: HTMLCanvasElement,
  data: GridData,
  getTs: TsAccessor,
) {
  const w = canvas.clientWidth, h = canvas.clientHeight;
  if (w === 0 || h === 0) return;
  const ctx = setupHiDpiCanvas(canvas, w, h);

  const { sessions, dayStart, labels } = data;
  const colW = w / GRID_COLS;
  const rowH = h / GRID_ROWS;

  // ① Background — detect dark mode via the document class or media query
  const isDark = document.documentElement.classList.contains("dark") ||
                 window.matchMedia("(prefers-color-scheme: dark)").matches;
  ctx.fillStyle = isDark ? "#0a0a14" : "#f8f8fa";
  ctx.fillRect(0, 0, w, h);

  // ② Build a lookup: for each grid cell, store the best epoch data.
  const cellData = new Map<number, { relaxation: number; engagement: number; sIdx: number }>();

  for (let sIdx = 0; sIdx < sessions.length; sIdx++) {
    const ts = getTs(sessions[sIdx].csv_path);
    if (!ts) continue;
    for (const ep of ts) {
      const secOff = ep.t - dayStart;
      if (secOff < 0 || secOff >= 86400) continue;
      const col = Math.floor(secOff / 3600);
      const row = Math.floor((secOff % 3600) / GRID_BIN);
      const key = col * GRID_ROWS + row;
      if (!cellData.has(key)) {
        cellData.set(key, { relaxation: ep.relaxation, engagement: ep.engagement, sIdx });
      }
    }
  }

  // ③ Draw filled cells
  for (const [key, d] of cellData) {
    const col = Math.floor(key / GRID_ROWS);
    const row = key % GRID_ROWS;
    const x = col * colW;
    const y = row * rowH;
    const baseColor = SESSION_COLORS[d.sIdx % SESSION_COLORS.length];
    const intensity = Math.max(0.15, Math.min(1, (d.relaxation + d.engagement) / 2));
    ctx.globalAlpha = intensity;
    ctx.fillStyle = baseColor;
    ctx.fillRect(x, y, Math.ceil(colW), Math.ceil(rowH));
  }
  ctx.globalAlpha = 1.0;

  // ④ Draw hour separator lines
  ctx.strokeStyle = isDark ? "rgba(255,255,255,0.06)" : "rgba(0,0,0,0.06)";
  ctx.lineWidth = 0.5;
  for (let c = 1; c < GRID_COLS; c++) {
    const x = c * colW;
    ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke();
  }

  // ⑤ Draw 15-minute horizontal grid lines (faint)
  ctx.strokeStyle = isDark ? "rgba(255,255,255,0.025)" : "rgba(0,0,0,0.03)";
  for (let min = 15; min < 60; min += 15) {
    const y = (min * 60 / GRID_BIN) * rowH;
    ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
  }

  // ⑥ Draw labels as rainbow circles at their grid position
  if (labels.length > 0) {
    const labelColors = assignLabelRainbowColors(labels);
    const dotR = Math.max(2.5, Math.min(5, colW * 0.15));
    ctx.globalAlpha = 0.95;
    for (const label of labels) {
      const secOff = label.eeg_start - dayStart;
      if (secOff < 0 || secOff >= 86400) continue;
      const col = Math.floor(secOff / 3600);
      const row = Math.floor((secOff % 3600) / GRID_BIN);
      const cx = col * colW + colW / 2;
      const cy = row * rowH + rowH / 2;
      const color = labelColors.get(label.id) ?? "#f59e0b";
      ctx.shadowColor = color;
      ctx.shadowBlur = 4;
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.arc(cx, cy, dotR, 0, Math.PI * 2);
      ctx.fill();
      ctx.shadowBlur = 0;
      ctx.strokeStyle = "rgba(255,255,255,0.7)";
      ctx.lineWidth = 0.8;
      ctx.stroke();
    }
    ctx.globalAlpha = 1.0;
  }

  // ⑦ Draw "now" marker if viewing today
  const nowSec = Date.now() / 1000;
  const nowOff = nowSec - dayStart;
  if (nowOff >= 0 && nowOff < 86400) {
    const nowCol = Math.floor(nowOff / 3600);
    const nowRow = (nowOff % 3600) / GRID_BIN;
    const nx = nowCol * colW;
    const ny = nowRow * rowH;
    ctx.strokeStyle = "rgba(239,68,68,0.7)";
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(nx, ny);
    ctx.lineTo(nx + colW, ny);
    ctx.stroke();
    ctx.fillStyle = "rgba(239,68,68,0.85)";
    ctx.beginPath();
    ctx.moveTo(nx, ny - 3);
    ctx.lineTo(nx + 5, ny);
    ctx.lineTo(nx, ny + 3);
    ctx.closePath();
    ctx.fill();
  }

  // ⑧ Draw screenshot indicators — small diamond markers
  if (data.screenshotTs.size > 0) {
    const accentColor = getComputedStyle(document.documentElement).getPropertyValue("--color-violet-500").trim();
    const iconR = Math.max(2, Math.min(4, colW * 0.12));
    ctx.globalAlpha = 0.85;
    for (const ts of data.screenshotTs) {
      const secOff = ts - dayStart;
      if (secOff < 0 || secOff >= 86400) continue;
      const col = Math.floor(secOff / 3600);
      const row = Math.floor((secOff % 3600) / GRID_BIN);
      const cx = col * colW + colW - iconR - 1;
      const cy = row * rowH + rowH / 2;
      ctx.fillStyle = accentColor || (isDark ? "rgba(96,165,250,0.9)" : "rgba(59,130,246,0.85)");
      ctx.beginPath();
      ctx.moveTo(cx, cy - iconR);
      ctx.lineTo(cx + iconR, cy);
      ctx.lineTo(cx, cy + iconR);
      ctx.lineTo(cx - iconR, cy);
      ctx.closePath();
      ctx.fill();
      ctx.fillStyle = "rgba(255,255,255,0.8)";
      ctx.beginPath();
      ctx.arc(cx, cy, iconR * 0.3, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1.0;
  }
}

// ── Sparkline ────────────────────────────────────────────────────────────────

/** Render a mini sparkline (relaxation + engagement lines) on a canvas. */
export function renderSparkline(canvas: HTMLCanvasElement, ts: EpochRow[]) {
  const dpr = devicePixelRatio || 1;
  const w = canvas.clientWidth, h = canvas.clientHeight;
  canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
  const ctx = canvas.getContext("2d")!;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  if (ts.length < 3) return;
  const n = ts.length, maxPts = Math.min(n, w * 2), step = Math.max(1, Math.floor(n / maxPts));
  const relaxVals: number[] = [], engageVals: number[] = [];
  for (let i = 0; i < n; i += step) { relaxVals.push(ts[i].relaxation); engageVals.push(ts[i].engagement); }
  function drawLine(vals: number[], color: string) {
    const max = Math.max(...vals, 1);
    ctx.beginPath(); ctx.strokeStyle = color; ctx.lineWidth = 1;
    for (let i = 0; i < vals.length; i++) {
      const x = (i / (vals.length - 1)) * w, y = h - (vals[i] / max) * h;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    }
    ctx.stroke();
  }
  drawLine(relaxVals, "#10b981"); drawLine(engageVals, "#f59e0b");
}
