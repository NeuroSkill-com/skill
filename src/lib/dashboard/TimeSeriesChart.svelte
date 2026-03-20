<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  GPU-accelerated time-series chart using WebGL.
  Renders multiple series as line/area plots with smooth scrolling.
  Falls back to Canvas 2D if WebGL is unavailable.

  Props:
    series: Array of { key, label, color, data: number[] }
    timestamps: number[] (Unix seconds)
    height: number (logical px, default 120)
    yLabel: string (optional axis label)
    yMin / yMax: optional fixed Y range (auto-scale if omitted)
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { getResolved } from "$lib/stores/theme.svelte";
  import { getDpr, setupHiDpiCanvas } from "$lib/format";

  export interface Series {
    key:   string;
    label: string;
    color: string;
    data:  number[];
  }

  interface Props {
    series:      Series[];
    timestamps:  number[];
    height?:     number;
    yLabel?:     string;
    yMin?:       number;
    yMax?:       number;
    /** Fixed X (time) range — use to synchronise multiple charts. */
    xMin?:       number;
    xMax?:       number;
    /** Custom Y-axis tick labels (bottom→top). When provided, replaces numeric labels.
     *  Each entry: { value: number, label: string, color?: string }. */
    yTicks?:     { value: number; label: string; color?: string }[];
    /** If true, draw horizontal steps (staircase) instead of smooth lines. */
    stepped?:    boolean;
  }

  let { series, timestamps, height = 120, yLabel, yMin, yMax, xMin, xMax, yTicks, stepped }: Props = $props();

  // ── Downsampling ────────────────────────────────────────────────────────
  // When there are too many samples for the chart width, downsample using
  // bucket averaging (similar to LTTB but simpler).  Keeps the chart fast
  // and visually clean for long sessions.
  const MAX_POINTS_PER_PX = 2; // at most 2 data points per horizontal pixel

  interface Downsampled {
    timestamps: number[];
    seriesData: number[][]; // one array per series, same order
  }

  function downsample(ts: number[], allSeries: Series[], maxPts: number): Downsampled | null {
    const n = ts.length;
    if (n <= maxPts || maxPts < 4) return null; // no downsampling needed

    const buckets = maxPts;
    const bucketSize = n / buckets;
    const outTs: number[] = new Array(buckets);
    const outData: number[][] = allSeries.map(() => new Array(buckets));

    for (let b = 0; b < buckets; b++) {
      const start = Math.floor(b * bucketSize);
      const end = Math.min(Math.floor((b + 1) * bucketSize), n);
      const count = end - start;

      // Average timestamps
      let tSum = 0;
      for (let i = start; i < end; i++) tSum += ts[i];
      outTs[b] = tSum / count;

      // Average each series
      for (let si = 0; si < allSeries.length; si++) {
        const data = allSeries[si].data;
        let sum = 0, valid = 0;
        for (let i = start; i < end; i++) {
          if (isFinite(data[i])) { sum += data[i]; valid++; }
        }
        outData[si][b] = valid > 0 ? sum / valid : 0;
      }
    }
    return { timestamps: outTs, seriesData: outData };
  }

  let canvas: HTMLCanvasElement | undefined = $state();
  let overlayCanvas: HTMLCanvasElement | undefined = $state();
  let container: HTMLDivElement | undefined = $state();
  let animFrame: number | undefined;
  let ro: ResizeObserver | undefined;
  let gl: WebGLRenderingContext | null = null;
  let glProgram: WebGLProgram | null = null;
  let useWebGL = $state(false);

  // ── Zoom state ──────────────────────────────────────────────────────────
  // When set, these override the visible range (local zoom).
  let zoomXMin: number | undefined = $state();
  let zoomXMax: number | undefined = $state();
  let zoomYMin: number | undefined = $state();
  let zoomYMax: number | undefined = $state();

  // Drag selection state
  let dragStart: { px: number; py: number } | null = null;
  let dragCurrent: { px: number; py: number } | null = $state(null);
  let isDragging = $state(false);

  /** Get chart area margins (px) for the current canvas size. */
  function chartMargins(W: number, H: number) {
    if (useWebGL) {
      // Must match WebGL clip space: x [-0.88,0.96] → ml=0.06W mr=0.02W, y [-0.72,0.92] → mt=0.04H mb=0.14H
      return { ml: W * 0.06, mr: W * 0.02, mt: H * 0.04, mb: H * 0.14 };
    } else {
      return { ml: Math.round(W * 0.06), mr: Math.round(W * 0.02), mt: 4, mb: Math.max(18, Math.round(H * 0.14)) };
    }
  }

  /** Convert pixel position on the overlay canvas → data coordinates. */
  function pxToData(px: number, py: number, W: number, H: number, tMin: number, tRange: number, yLo: number, yRange: number) {
    const { ml, mr, mt, mb } = chartMargins(W, H);
    const cw = W - ml - mr, ch = H - mt - mb;
    const xFrac = Math.max(0, Math.min(1, (px - ml) / cw));
    const yFrac = Math.max(0, Math.min(1, 1 - (py - mt) / ch));
    return { t: tMin + xFrac * tRange, v: yLo + yFrac * yRange };
  }

  /** Current effective data ranges (respecting zoom). */
  function effectiveRanges() {
    const n = timestamps.length;
    const tMin0 = xMin ?? timestamps[0];
    const tMax0 = xMax ?? timestamps[n - 1];

    let lo = yMin ?? Infinity, hi = yMax ?? -Infinity;
    if (yMin === undefined || yMax === undefined) {
      for (const s of series) {
        for (const v of s.data) {
          if (isFinite(v)) {
            if (yMin === undefined && v < lo) lo = v;
            if (yMax === undefined && v > hi) hi = v;
          }
        }
      }
    }
    if (!isFinite(lo)) lo = 0; if (!isFinite(hi)) hi = 1;
    const pad = (hi - lo) * 0.08 || 0.5;
    if (yMin === undefined) lo -= pad; if (yMax === undefined) hi += pad;

    return {
      tMin: zoomXMin ?? tMin0,
      tMax: zoomXMax ?? tMax0,
      yLo: zoomYMin ?? lo,
      yHi: zoomYMax ?? hi,
    };
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return; // left button only
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    dragStart = { px: e.clientX - rect.left, py: e.clientY - rect.top };
    dragCurrent = null;
    isDragging = false;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragStart) return;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    // Only start drag after 4px movement to avoid accidental selections
    if (!isDragging && (Math.abs(px - dragStart.px) > 4 || Math.abs(py - dragStart.py) > 4)) {
      isDragging = true;
    }
    if (isDragging) {
      dragCurrent = { px, py };
      drawSelectionRect();
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragStart || !isDragging || !dragCurrent || !overlayCanvas) {
      dragStart = null; dragCurrent = null; isDragging = false;
      return;
    }
    const W = overlayCanvas.clientWidth;
    const H = overlayCanvas.clientHeight;
    const { tMin, tMax, yLo, yHi } = effectiveRanges();
    const tRange = tMax - tMin || 1;
    const yRange = yHi - yLo || 1;
    const a = pxToData(dragStart.px, dragStart.py, W, H, tMin, tRange, yLo, yRange);
    const b = pxToData(dragCurrent.px, dragCurrent.py, W, H, tMin, tRange, yLo, yRange);

    const newXMin = Math.min(a.t, b.t), newXMax = Math.max(a.t, b.t);
    const newYMin = Math.min(a.v, b.v), newYMax = Math.max(a.v, b.v);

    // Require minimum selection size (at least 1% of range in each axis)
    if ((newXMax - newXMin) > tRange * 0.01 && (newYMax - newYMin) > yRange * 0.01) {
      zoomXMin = newXMin; zoomXMax = newXMax;
      zoomYMin = newYMin; zoomYMax = newYMax;
      draw();
    }

    dragStart = null; dragCurrent = null; isDragging = false;
    drawSelectionRect();
  }

  function onDblClick() {
    zoomXMin = undefined; zoomXMax = undefined;
    zoomYMin = undefined; zoomYMax = undefined;
    draw();
  }

  /** Draw the rubber-band selection rectangle on the overlay canvas. */
  function drawSelectionOverlay() {
    if (!overlayCanvas) return;
    const W = overlayCanvas.clientWidth;
    const H = overlayCanvas.clientHeight;
    const ctx = setupHiDpiCanvas(overlayCanvas, W, H);
    ctx.clearRect(0, 0, W, H);

    // Re-draw labels (WebGL path uses overlay for labels)
    if (useWebGL) {
      const { tMin, tMax, yLo, yHi } = effectiveRanges();
      drawLabelsOverlay(tMin, tMax, tMax - tMin || 1, yLo, yHi - yLo || 1, W, H);
    }

    // Draw selection rect
    if (dragStart && dragCurrent) {
      const dark = getResolved() === "dark";
      const x = Math.min(dragStart.px, dragCurrent.px);
      const y = Math.min(dragStart.py, dragCurrent.py);
      const w = Math.abs(dragCurrent.px - dragStart.px);
      const h = Math.abs(dragCurrent.py - dragStart.py);
      ctx.fillStyle = dark ? "rgba(59,130,246,0.15)" : "rgba(59,130,246,0.12)";
      ctx.fillRect(x, y, w, h);
      ctx.strokeStyle = dark ? "rgba(59,130,246,0.6)" : "rgba(59,130,246,0.5)";
      ctx.lineWidth = 1;
      ctx.strokeRect(x, y, w, h);
    }
  }

  function drawSelectionRect() {
    drawSelectionOverlay();
  }

  // ── WebGL shaders ─────────────────────────────────────────────────────────
  const VERT_SRC = `
    attribute vec2 a_pos;
    uniform vec2 u_scale;
    uniform vec2 u_offset;
    void main() {
      gl_Position = vec4(a_pos * u_scale + u_offset, 0.0, 1.0);
    }
  `;
  const FRAG_SRC = `
    precision mediump float;
    uniform vec4 u_color;
    void main() { gl_FragColor = u_color; }
  `;

  function hexToVec4(hex: string, alpha = 1.0): [number, number, number, number] {
    const r = parseInt(hex.slice(1, 3), 16) / 255;
    const g = parseInt(hex.slice(3, 5), 16) / 255;
    const b = parseInt(hex.slice(5, 7), 16) / 255;
    return [r, g, b, alpha];
  }

  function initWebGL(): boolean {
    if (!canvas) return false;
    gl = canvas.getContext("webgl", { antialias: true, alpha: true, preserveDrawingBuffer: false });
    if (!gl) return false;

    const vs = gl.createShader(gl.VERTEX_SHADER)!;
    gl.shaderSource(vs, VERT_SRC);
    gl.compileShader(vs);
    if (!gl.getShaderParameter(vs, gl.COMPILE_STATUS)) { gl = null; return false; }

    const fs = gl.createShader(gl.FRAGMENT_SHADER)!;
    gl.shaderSource(fs, FRAG_SRC);
    gl.compileShader(fs);
    if (!gl.getShaderParameter(fs, gl.COMPILE_STATUS)) { gl = null; return false; }

    glProgram = gl.createProgram()!;
    gl.attachShader(glProgram, vs);
    gl.attachShader(glProgram, fs);
    gl.linkProgram(glProgram);
    if (!gl.getProgramParameter(glProgram, gl.LINK_STATUS)) { gl = null; return false; }

    gl.useProgram(glProgram);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    return true;
  }

  function drawWebGL() {
    if (!gl || !glProgram || !canvas || series.length === 0 || timestamps.length < 2) return;

    const dpr = getDpr();
    const W = canvas.clientWidth;
    const H = canvas.clientHeight;
    if (W < 2 || H < 2) return; // not laid out yet
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    gl.viewport(0, 0, canvas.width, canvas.height);

    const dark = getResolved() === "dark";
    if (dark) gl.clearColor(0.059, 0.059, 0.094, 1);
    else gl.clearColor(0.973, 0.976, 0.984, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);

    const { tMin, tMax, yLo: lo, yHi: hi } = effectiveRanges();
    const tRange = tMax - tMin || 1;
    const yRange = hi - lo || 1;

    // Downsample if too many points for the pixel width
    const maxPts = Math.round(W * MAX_POINTS_PER_PX);
    const ds = downsample(timestamps, series, maxPts);
    const useTs = ds ? ds.timestamps : timestamps;
    const n = useTs.length;

    const uScale = gl.getUniformLocation(glProgram, "u_scale");
    const uOffset = gl.getUniformLocation(glProgram, "u_offset");
    const uColor = gl.getUniformLocation(glProgram, "u_color");
    const aPos = gl.getAttribLocation(glProgram, "a_pos");

    gl.useProgram(glProgram);

    // Map data → clip space: x: [0,1] → [-0.88, 0.96], y: [0,1] → [-0.72, 0.92]
    // Bottom 14% reserved for time labels, left 6% for Y labels
    const xL = -0.88, xR = 0.96, yB = -0.72, yT = 0.92;
    gl.uniform2f(uScale, xR - xL, yT - yB);
    gl.uniform2f(uOffset, xL, yB);

    // Draw grid lines
    const gridBuf = gl.createBuffer()!;
    const [gr, gg, gb, ga] = dark ? [1, 1, 1, 0.06] : [0, 0, 0, 0.08];
    gl.uniform4f(uColor, gr, gg, gb, ga);
    const gridVerts: number[] = [];
    for (let i = 0; i <= 4; i++) {
      const y = i / 4;
      gridVerts.push(0, y, 1, y);
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, gridBuf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(gridVerts), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
    gl.drawArrays(gl.LINES, 0, gridVerts.length / 2);

    // Clip series to chart area (scissor test) — prevents overflow when zoomed
    const dprS = dpr;
    const sX = Math.round(W * 0.06 * dprS);
    const sY = Math.round(H * 0.04 * dprS);       // top margin in WebGL bottom-up coords
    const sW = Math.round((W - W * 0.06 - W * 0.02) * dprS);
    const sH = Math.round((H - H * 0.04 - H * 0.14) * dprS);
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(sX, Math.round(H * 0.14 * dprS), sW, sH);

    // Draw each series
    const lineBuf = gl.createBuffer()!;
    for (let si = 0; si < series.length; si++) {
      const s = series[si];
      const sData = ds ? ds.seriesData[si] : s.data;
      if (sData.length !== n) continue;

      // Build line vertices — stepped mode inserts horizontal+vertical segments
      let verts: Float32Array;
      let vertCount: number;
      if (stepped && n > 1) {
        // Each point (except first) becomes 2 vertices: (newX, prevY) + (newX, newY)
        const arr: number[] = [];
        arr.push((useTs[0] - tMin) / tRange, (sData[0] - lo) / yRange);
        for (let i = 1; i < n; i++) {
          const x = (useTs[i] - tMin) / tRange;
          const prevY = (sData[i - 1] - lo) / yRange;
          const curY = (sData[i] - lo) / yRange;
          arr.push(x, prevY, x, curY);
        }
        verts = new Float32Array(arr);
        vertCount = arr.length / 2;
      } else {
        verts = new Float32Array(n * 2);
        for (let i = 0; i < n; i++) {
          verts[i * 2] = (useTs[i] - tMin) / tRange;
          verts[i * 2 + 1] = (sData[i] - lo) / yRange;
        }
        vertCount = n;
      }

      gl.bindBuffer(gl.ARRAY_BUFFER, lineBuf);
      gl.bufferData(gl.ARRAY_BUFFER, verts, gl.DYNAMIC_DRAW);
      gl.enableVertexAttribArray(aPos);
      gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

      // Area fill (triangle strip from line to bottom)
      const areaVerts = new Float32Array(vertCount * 4);
      for (let i = 0; i < vertCount; i++) {
        areaVerts[i * 4] = verts[i * 2];
        areaVerts[i * 4 + 1] = verts[i * 2 + 1];
        areaVerts[i * 4 + 2] = verts[i * 2];
        areaVerts[i * 4 + 3] = 0;
      }
      const areaBuf = gl.createBuffer()!;
      gl.bindBuffer(gl.ARRAY_BUFFER, areaBuf);
      gl.bufferData(gl.ARRAY_BUFFER, areaVerts, gl.DYNAMIC_DRAW);
      gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
      const [cr, cg, cb] = hexToVec4(s.color);
      gl.uniform4f(uColor, cr, cg, cb, 0.12);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, vertCount * 2);

      // Line
      gl.bindBuffer(gl.ARRAY_BUFFER, lineBuf);
      gl.bufferData(gl.ARRAY_BUFFER, verts, gl.DYNAMIC_DRAW);
      gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
      gl.uniform4f(uColor, cr, cg, cb, 0.9);
      gl.lineWidth(1.5);
      gl.drawArrays(gl.LINE_STRIP, 0, vertCount);

      gl.deleteBuffer(areaBuf);
    }
    gl.deleteBuffer(lineBuf);
    gl.deleteBuffer(gridBuf);
    gl.disable(gl.SCISSOR_TEST);

    // ── Overlay: time labels + Y labels via 2D canvas ──────────────────
    drawLabelsOverlay(tMin, tMax, tRange, lo, yRange, W, H);
  }

  /** Draw time-axis and Y-axis labels on the overlay canvas (shared by WebGL + Canvas2D). */
  function drawLabelsOverlay(tMin: number, tMax: number, tRange: number, yLo: number, yRange: number, W: number, H: number) {
    if (!overlayCanvas) return;
    const ctx = setupHiDpiCanvas(overlayCanvas, W, H);
    ctx.clearRect(0, 0, W, H);

    const dark = getResolved() === "dark";
    ctx.fillStyle = dark ? "rgba(255,255,255,0.3)" : "rgba(0,0,0,0.35)";
    ctx.font = "9px ui-monospace, monospace";

    // Chart area margins matching WebGL clip space mapping:
    // x: [-0.88, 0.96] → pixel left=0.06W, right=0.98W
    // y: [-0.72, 0.92]  → pixel top=0.04H, bottom=0.86H
    const ml = W * 0.06, mr = W * 0.02;
    const mt = H * 0.04, mb = H * 0.14;
    const cw = W - ml - mr, ch = H - mt - mb;

    // Time labels (bottom)
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    const nLabels = Math.min(6, Math.max(2, Math.floor(cw / 60)));
    for (let i = 0; i < nLabels; i++) {
      const frac = i / (nLabels - 1);
      const t = tMin + frac * tRange;
      const x = ml + frac * cw;
      const d = new Date(t * 1000);
      const span = tRange;
      let label: string;
      if (span > 86400 * 2) {
        label = d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
      } else if (span > 3600) {
        label = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
      } else {
        label = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
      }
      // Clamp edge labels
      if (i === 0) ctx.textAlign = "left";
      else if (i === nLabels - 1) ctx.textAlign = "right";
      else ctx.textAlign = "center";
      ctx.fillText(label, x, mt + ch + 2);
    }

    // Y labels (flush left)
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    if (yTicks && yTicks.length > 0) {
      ctx.font = "bold 8px ui-sans-serif, system-ui, sans-serif";
      for (const tick of yTicks) {
        const frac = (tick.value - yLo) / yRange;
        const y = mt + ch * (1 - frac);
        if (tick.color) ctx.fillStyle = tick.color;
        ctx.fillText(tick.label, 2, y);
      }
    } else {
      for (let i = 0; i <= 4; i++) {
        const y = mt + ch * (1 - i / 4);
        const val = yLo + (i / 4) * yRange;
        ctx.fillText(val.toFixed(val >= 10 ? 0 : 1), 2, y);
      }
    }
  }

  function drawCanvas2D() {
    if (!canvas || series.length === 0 || timestamps.length < 2) return;
    const W = canvas.clientWidth;
    const H = canvas.clientHeight;
    if (W < 2 || H < 2) return; // not laid out yet
    const ctx = setupHiDpiCanvas(canvas, W, H);

    const dark = getResolved() === "dark";
    ctx.fillStyle = dark ? "#0f0f18" : "#f8f9fb";
    ctx.fillRect(0, 0, W, H);

    const { tMin, tMax, yLo: lo, yHi: hi } = effectiveRanges();
    const tRange = tMax - tMin || 1;
    const yRange = hi - lo || 1;

    // Downsample if too many points for the pixel width
    const maxPts = Math.round(W * MAX_POINTS_PER_PX);
    const ds = downsample(timestamps, series, maxPts);
    const useTs = ds ? ds.timestamps : timestamps;
    const n = useTs.length;

    const ml = Math.round(W * 0.06), mr = Math.round(W * 0.02);
    const mt = 4, mb = Math.max(18, Math.round(H * 0.14));
    const cw = W - ml - mr, ch = H - mt - mb;

    // Grid
    ctx.strokeStyle = dark ? "rgba(255,255,255,0.06)" : "rgba(0,0,0,0.08)";
    ctx.lineWidth = 0.5;
    for (let i = 0; i <= 4; i++) {
      const y = mt + ch * (1 - i / 4);
      ctx.beginPath(); ctx.moveTo(ml, y); ctx.lineTo(ml + cw, y); ctx.stroke();
    }

    // Y labels (flush left)
    ctx.fillStyle = dark ? "rgba(255,255,255,0.25)" : "rgba(0,0,0,0.3)";
    ctx.font = "9px ui-monospace, monospace";
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    if (yTicks && yTicks.length > 0) {
      ctx.font = "bold 8px ui-sans-serif, system-ui, sans-serif";
      for (const tick of yTicks) {
        const frac = (tick.value - lo) / yRange;
        const y = mt + ch * (1 - frac);
        if (tick.color) ctx.fillStyle = tick.color;
        else ctx.fillStyle = dark ? "rgba(255,255,255,0.25)" : "rgba(0,0,0,0.3)";
        ctx.fillText(tick.label, 2, y);
      }
    } else {
      for (let i = 0; i <= 4; i++) {
        const y = mt + ch * (1 - i / 4);
        const val = lo + (i / 4) * yRange;
        ctx.fillText(val.toFixed(val >= 10 ? 0 : 1), 2, y);
      }
    }

    // Time labels — local timezone
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    const nLabels2 = Math.min(6, Math.max(2, Math.floor(cw / 60)));
    for (let i = 0; i < nLabels2; i++) {
      const frac = i / (nLabels2 - 1);
      const t = tMin + frac * tRange;
      const x = ml + frac * cw;
      const d = new Date(t * 1000);
      let label: string;
      if (tRange > 86400 * 2) {
        label = d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
      } else if (tRange > 3600) {
        label = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
      } else {
        label = d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
      }
      if (i === 0) ctx.textAlign = "left";
      else if (i === nLabels2 - 1) ctx.textAlign = "right";
      else ctx.textAlign = "center";
      ctx.fillText(label, x, mt + ch + 2);
    }

    // Series — clip to chart area so zoomed data doesn't overflow
    ctx.save();
    ctx.beginPath();
    ctx.rect(ml, mt, cw, ch);
    ctx.clip();
    for (let si = 0; si < series.length; si++) {
      const s = series[si];
      const sData = ds ? ds.seriesData[si] : s.data;
      if (sData.length !== n) continue;

      /** Build path points — stepped inserts horizontal-then-vertical segments. */
      function tracePath(beginPath = true) {
        if (beginPath) ctx.beginPath();
        for (let i = 0; i < n; i++) {
          const x = ml + (useTs[i] - tMin) / tRange * cw;
          const y = mt + ch * (1 - (sData[i] - lo) / yRange);
          if (i === 0) {
            ctx.moveTo(x, y);
          } else if (stepped) {
            const prevY = mt + ch * (1 - (sData[i - 1] - lo) / yRange);
            ctx.lineTo(x, prevY);
            ctx.lineTo(x, y);
          } else {
            ctx.lineTo(x, y);
          }
        }
      }

      // Area
      ctx.beginPath();
      tracePath(false);
      // Close area to bottom
      const lastX = ml + (useTs[n - 1] - tMin) / tRange * cw;
      const firstX = ml + (useTs[0] - tMin) / tRange * cw;
      ctx.lineTo(lastX, mt + ch);
      ctx.lineTo(firstX, mt + ch);
      ctx.closePath();
      ctx.fillStyle = s.color + "1a";
      ctx.fill();

      // Line
      tracePath();
      ctx.strokeStyle = s.color;
      ctx.lineWidth = 1.5;
      ctx.stroke();
    }
    ctx.restore();
  }

  function draw() {
    if (useWebGL) drawWebGL();
    else {
      drawCanvas2D();
      // Clear overlay when using Canvas2D (it draws its own labels)
      if (overlayCanvas) {
        const ctx = overlayCanvas.getContext("2d");
        if (ctx) ctx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
      }
    }
  }

  function resize() {
    draw();
  }

  // Reset zoom when source data or external range props change
  $effect(() => {
    const _xmin = xMin; const _xmax = xMax;
    const _ymin = yMin; const _ymax = yMax;
    const _tlen = timestamps.length;
    zoomXMin = undefined; zoomXMax = undefined;
    zoomYMin = undefined; zoomYMax = undefined;
  });

  $effect(() => {
    // Track all reactive inputs — $effect re-runs when any change
    const _s = series;
    const _t = timestamps;
    const _c = canvas;
    const _h = height;
    const _xmin = xMin;
    const _xmax = xMax;
    const _ymin = yMin;
    const _ymax = yMax;
    const _zxmin = zoomXMin; const _zxmax = zoomXMax;
    const _zymin = zoomYMin; const _zymax = zoomYMax;
    if (_c && _s.length > 0 && _t.length >= 2) {
      // Initialise WebGL on first canvas-ready pass
      if (!gl && !useWebGL) {
        useWebGL = initWebGL();
      }
      draw();
    }
  });

  onMount(() => {
    if (container) {
      ro = new ResizeObserver(resize);
      ro.observe(container);
    }
  });

  onDestroy(() => {
    if (animFrame) cancelAnimationFrame(animFrame);
    ro?.disconnect();
  });
</script>

<div class="flex flex-col gap-0.5" bind:this={container}>
  {#if yLabel}
    <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">{yLabel}</span>
  {/if}
  <div class="relative rounded-lg border border-border dark:border-white/[0.04] overflow-hidden"
       style="height:{height}px">
    <canvas bind:this={canvas} class="w-full h-full block"></canvas>
    <canvas bind:this={overlayCanvas}
            class="absolute inset-0 w-full h-full block"
            style="cursor:crosshair"
            onpointerdown={onPointerDown}
            onpointermove={onPointerMove}
            onpointerup={onPointerUp}
            ondblclick={onDblClick}></canvas>
    {#if zoomXMin !== undefined}
      <button
        class="absolute top-1 right-1 text-[8px] px-1 py-0.5 rounded bg-blue-500/20 text-blue-400 hover:bg-blue-500/30 transition-colors"
        onclick={onDblClick}
        title={t("common.resetZoom")}
      >⟲ reset</button>
    {/if}
  </div>
  <!-- Legend -->
  {#if series.length > 1}
    <div class="flex items-center gap-3 flex-wrap px-0.5">
      {#each series as s}
        <div class="flex items-center gap-1">
          <span class="w-2 h-2 rounded-full shrink-0" style="background:{s.color}"></span>
          <span class="text-[0.48rem] text-muted-foreground">{s.label}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>
