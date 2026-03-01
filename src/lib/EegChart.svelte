<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script module lang="ts">
  // ── Types ───────────────────────────────────────────────────────────────────
  export interface SpectrogramColumn {
    timestamp_ms: number;
    /** power[channel][freq_bin], freq_bin 0–50 = 0 Hz … 50 Hz */
    power: number[][];
  }

  /** Vertical event marker rendered on the waveform/spectrogram overlay. */
  export interface EventMarker {
    /** When the event occurred (Date.now()-style ms timestamp). */
    timestamp_ms: number;
    /** Short label rendered next to the line (e.g. "Eyes Open", "🏷"). */
    label: string;
    /** CSS colour for the vertical line. */
    color: string;
  }
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { cn } from "$lib/utils";
  import {
    CHART_H, TIME_H, WAVE_H, ROW_PAD as PAD,
    EEG_CH, EEG_COLOR, EEG_CHANNELS as N_CH,
    N_EPOCHS, EPOCH_S, SAMPLE_RATE, EPOCH_SAMP, BUF_SIZE, EEG_RANGE_UV as EEG_RANGE,
    SPEC_N_FREQ, FILTER_HOP as HOP, SPEC_COLS,
    SPEC_CMAP_STOPS_DARK, SPEC_CMAP_STOPS_LIGHT,
    SPEC_LOG_INIT, SPEC_LOG_DECAY as LOG_DECAY,
    SPEC_LOG_RANGE as LOG_RANGE, SPEC_LOG_FLOOR as LOG_FLOOR,
    DC_BETA, SMOOTH_K, WP_TAU_MS as WP_TAU,
  } from "$lib/constants";
  import { getResolved } from "$lib/theme-store.svelte";

  // ── Spectrogram colormap LUT ─────────────────────────────────────────────────
  // 256-entry RGBA lookup table; index = Math.round(normalised_power × 255).
  function buildLut(stops: readonly (readonly [number, number, number, number, number])[]) {
    const lut = new Uint8ClampedArray(256 * 4);
    for (let i = 0; i < 256; i++) {
      const t = i / 255;
      let ri = 0, gi = 0, bi = 0, ai = 0;
      for (let s = 1; s < stops.length; s++) {
        const [t0, r0, g0, b0, a0] = stops[s - 1];
        const [t1, r1, g1, b1, a1] = stops[s];
        if (t <= t1) {
          const f = (t - t0) / (t1 - t0);
          ri = r0 + f * (r1 - r0);
          gi = g0 + f * (g1 - g0);
          bi = b0 + f * (b1 - b0);
          ai = a0 + f * (a1 - a0);
          break;
        }
        [, ri, gi, bi, ai] = stops[stops.length - 1];
      }
      lut[i * 4 + 0] = ri;
      lut[i * 4 + 1] = gi;
      lut[i * 4 + 2] = bi;
      lut[i * 4 + 3] = ai;
    }
    return lut;
  }
  const CMAP_LUT_DARK  = buildLut(SPEC_CMAP_STOPS_DARK);
  const CMAP_LUT_LIGHT = buildLut(SPEC_CMAP_STOPS_LIGHT);

  // Active LUT — switches with theme
  let CMAP_LUT = CMAP_LUT_DARK;

  // ── Ring buffers ────────────────────────────────────────────────────────────
  const buffers  = Array.from({ length: N_CH }, () => new Float64Array(BUF_SIZE));
  const writePos = new Int32Array(N_CH);

  // Per-channel one-pole high-pass (DC blocker, τ ≈ 780 ms @ 256 Hz).
  // DC_BETA is imported from constants.ts.
  const dcEma = new Float64Array(N_CH);

  // ── Spectrogram state ────────────────────────────────────────────────────────
  // Four off-screen tape canvases: each is SPEC_COLS × SPEC_N_FREQ px.
  // Columns are written left-to-right in a ring (specWriteCol mod SPEC_COLS).
  // Created lazily in onMount because document is unavailable on the server.
  let specTapes: HTMLCanvasElement[]              = [];
  let specCtxs:  CanvasRenderingContext2D[]       = [];
  let specWriteCol = 0;

  // Per-channel adaptive log-scale normalisation.
  // logMaxPwr[ch]: running soft-maximum of log₁₀(PSD) values seen so far.
  // Decays slowly so short bursts don't permanently crush the colormap.
  // LOG_DECAY / LOG_RANGE / LOG_FLOOR / SPEC_LOG_INIT imported from constants.ts.
  const logMaxPwr = new Float64Array(N_CH).fill(SPEC_LOG_INIT);

  function initSpecTapes() {
    specTapes = [];
    specCtxs  = [];
    for (let ch = 0; ch < N_CH; ch++) {
      const c = document.createElement("canvas");
      c.width  = SPEC_COLS;
      c.height = SPEC_N_FREQ;
      const ctx = c.getContext("2d")!;
      // Fill with the zero-power color so untouched columns look like background.
      ctx.fillStyle = `rgba(10,10,25,0)`;
      ctx.fillRect(0, 0, SPEC_COLS, SPEC_N_FREQ);
      specTapes.push(c);
      specCtxs.push(ctx);
    }
    specWriteCol = 0;
    logMaxPwr.fill(SPEC_LOG_INIT);
  }

  // ── Event markers ────────────────────────────────────────────────────────────
  // Each marker stores the writePos at the time it was added, so we can compute
  // its X position relative to the current display head.
  const MAX_MARKERS = 64;
  const MARKER_LABEL_MAX = 6;  // max visible chars on canvas
  interface StoredMarker { samplePos: number; label: string; color: string; }
  let markers: StoredMarker[] = [];

  // Hit-boxes recalculated every frame for click-to-expand.
  interface MarkerHitBox { x: number; y: number; w: number; h: number; marker: StoredMarker; }
  let markerHitBoxes: MarkerHitBox[] = [];

  // Tooltip state (managed via Svelte $state).
  let tooltip = $state<{ x: number; y: number; text: string; color: string } | null>(null);
  let tooltipTimer: ReturnType<typeof setTimeout> | undefined;

  /** Add a vertical event marker at the current write position. */
  export function pushMarker(m: EventMarker): void {
    const pos = Math.min(...Array.from(writePos)); // use the slowest channel
    markers.push({ samplePos: pos, label: m.label, color: m.color });
    if (markers.length > MAX_MARKERS) markers.shift();
  }

  // ── Public API ──────────────────────────────────────────────────────────────
  /**
   * Receive one spectrogram column from the Rust backend and paint it into
   * the corresponding off-screen tape canvas for all 4 channels.
   *
   * This is called ~8× per second (every filter hop = 32 samples @ 256 Hz).
   * The actual canvas drawing is just `putImageData` of a 1-px-wide strip.
   */
  export function pushSpecColumn(col: SpectrogramColumn): void {
    if (specTapes.length === 0) return; // not yet mounted
    CMAP_LUT = getResolved() === "dark" ? CMAP_LUT_DARK : CMAP_LUT_LIGHT;

    const wc = specWriteCol % SPEC_COLS;

    for (let ch = 0; ch < N_CH; ch++) {
      const powers = col.power[ch];
      if (!powers || powers.length < SPEC_N_FREQ) continue;

      // Update running soft-max for this channel (slow decay).
      logMaxPwr[ch] *= LOG_DECAY;
      for (let f = 0; f < SPEC_N_FREQ; f++) {
        const p = powers[f];
        if (p > 0) {
          const lp = Math.log10(p);
          if (lp > logMaxPwr[ch]) logMaxPwr[ch] = lp;
        }
      }

      // Build a 1-px × SPEC_N_FREQ ImageData for this column.
      const img = specCtxs[ch].createImageData(1, SPEC_N_FREQ);
      const lo  = logMaxPwr[ch] - LOG_RANGE;

      for (let f = 0; f < SPEC_N_FREQ; f++) {
        const p    = powers[f];
        const logP = p > 0 ? Math.log10(p) : LOG_FLOOR;
        const denom = logMaxPwr[ch] - lo;
        const t    = denom > 1e-6 ? (logP - lo) / denom : 0;
        const norm = Math.max(0, Math.min(1, t));
        const idx  = Math.round(norm * 255) * 4;

        // Flip Y: freq bin 0 (DC, 0 Hz) goes at the BOTTOM of the row.
        // Canvas Y=0 is the TOP, so bin 0 → row pixel (SPEC_N_FREQ - 1).
        const yInv = (SPEC_N_FREQ - 1 - f) * 4;
        img.data[yInv + 0] = CMAP_LUT[idx + 0];
        img.data[yInv + 1] = CMAP_LUT[idx + 1];
        img.data[yInv + 2] = CMAP_LUT[idx + 2];
        img.data[yInv + 3] = CMAP_LUT[idx + 3];
      }

      specCtxs[ch].putImageData(img, wc, 0);
    }

    specWriteCol++;
  }

  export function pushSamples(ch: number, samples: number[]): void {
    if (ch < 0 || ch >= N_CH) return;
    for (const v of samples) {
      if (isFinite(v)) dcEma[ch] += DC_BETA * (v - dcEma[ch]);
      buffers[ch][writePos[ch] % BUF_SIZE] = isFinite(v) ? v - dcEma[ch] : 0;
      writePos[ch]++;
    }
  }

  // ── Buffer read (chronological, oldest → newest) ────────────────────────────
  function readBufferAt(ch: number, endPos: number): Float64Array {
    const buf = buffers[ch];
    const end = Math.floor(endPos);
    const out = new Float64Array(BUF_SIZE);
    for (let i = 0; i < BUF_SIZE; i++) {
      const p = end - BUF_SIZE + 1 + i;
      out[i] = buf[((p % BUF_SIZE) + BUF_SIZE) % BUF_SIZE];
    }
    return out;
  }

  // ── Centred moving average (fixed: odd K, correct window init) ──────────────
  // SMOOTH_K = 9 → half = 4 → window [i-4 .. i+4]; imported from constants.ts.
  function smooth(buf: Float64Array): Float64Array {
    const out  = new Float64Array(buf.length);
    const half = (SMOOTH_K - 1) >> 1;   // 4
    let   sum  = 0;
    for (let j = -half; j <= half; j++) sum += buf[Math.max(0, j)];
    for (let i = 0; i < buf.length; i++) {
      out[i] = sum / SMOOTH_K;
      sum   += buf[Math.min(buf.length - 1, i + half + 1)]
             - buf[Math.max(0, i - half)];
    }
    return out;
  }

  // ── EWMA write-head tracking ────────────────────────────────────────────────
  // Single-force EWMA: low-pass the write head, pin displayPos to it.
  // WP_TAU (ms) imported from constants.ts — smoothing τ ≫ 48 ms Muse bursts.

  // ── Canvas + ResizeObserver ─────────────────────────────────────────────────
  let canvasEl!: HTMLCanvasElement;
  let cssW      = $state(880);     // updated each resize
  let animFrame: number | undefined;
  let rendering = false;
  let ro: ResizeObserver | undefined;

  function startRender() {
    if (rendering) return;
    rendering = true;

    let ewmaWp       = Math.min(...Array.from(writePos));
    let lastFrameNow = -1;

    function frame(now: DOMHighResTimeStamp) {
      if (!rendering) return;

      const ctx = canvasEl.getContext("2d");
      if (!ctx) { animFrame = requestAnimationFrame(frame); return; }

      // Scale context so all coordinates are in CSS pixels (DPR-transparent).
      const dpr = devicePixelRatio || 1;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      const W = cssW;
      const H = CHART_H;
      const ROW_H = WAVE_H / N_CH;   // ≈ 38.5 px

      // ── EWMA write head ──────────────────────────────────────────────────
      const dt    = lastFrameNow < 0 ? 0 : now - lastFrameNow;
      lastFrameNow = now;
      const minWp = Math.min(...Array.from(writePos));
      ewmaWp     += (minWp - ewmaWp) * (1 - Math.exp(-dt / WP_TAU));
      let displayPos = ewmaWp;
      if (displayPos < minWp - BUF_SIZE) displayPos = minWp - BUF_SIZE;

      // ── Theme tokens (read from CSS each frame — switches instantly on OS change)
      const isDark   = getResolved() === "dark";
      const cs       = getComputedStyle(canvasEl);
      const cBg      = cs.getPropertyValue("--chart-bg").trim();
      const cBgStrip = cs.getPropertyValue("--chart-bg-strip").trim();
      const cGrid    = cs.getPropertyValue("--chart-grid").trim();
      const cBase    = cs.getPropertyValue("--chart-baseline").trim();
      const cLabel   = cs.getPropertyValue("--chart-label").trim();

      // Per-channel colors (overridable via chart color scheme)
      const chColors = [
        cs.getPropertyValue("--ch-color-0").trim() || EEG_COLOR[0],
        cs.getPropertyValue("--ch-color-1").trim() || EEG_COLOR[1],
        cs.getPropertyValue("--ch-color-2").trim() || EEG_COLOR[2],
        cs.getPropertyValue("--ch-color-3").trim() || EEG_COLOR[3],
      ];

      // ── Background ───────────────────────────────────────────────────────
      ctx.fillStyle = cBg;
      ctx.fillRect(0, 0, W, H);
      ctx.fillStyle = cBgStrip;
      ctx.fillRect(0, WAVE_H, W, TIME_H);

      // ── Spectrogram background ────────────────────────────────────────────
      //
      // Each channel row is filled with its spectrogram tape, stretched to
      // fit the row dimensions.  The tape is a SPEC_COLS × SPEC_N_FREQ px
      // offscreen canvas rendered as a circular buffer; we draw it in two
      // halves to unroll the ring: oldest on the left, newest on the right.
      //
      // Alignment with the waveform:
      //   - BUF_SIZE   = 3840 samples  = 15 s of waveform history
      //   - SPEC_COLS  = 120 columns   = 15 s of spectrogram (1 col per HOP=32)
      //   - Both scroll at the same wall-clock rate, so they stay in sync.
      //
      // `imageSmoothingEnabled = true` lets the GPU bilinearly interpolate the
      // tiny tape (120×51 px) when it is stretched to fill the row (~880×38 px).
      if (specTapes.length === N_CH && specWriteCol > 0) {
        ctx.imageSmoothingEnabled = true;
        ctx.imageSmoothingQuality = "low";
        ctx.save();

        const ROW_H_F = WAVE_H / N_CH;
        const filled  = Math.min(specWriteCol, SPEC_COLS);
        const tapeX   = specWriteCol % SPEC_COLS; // oldest column in the ring

        for (let ch = 0; ch < N_CH; ch++) {
          const rowY = ch * ROW_H_F;
          const tape = specTapes[ch];

          if (specWriteCol < SPEC_COLS) {
            // Tape not yet full: draw the filled columns left-aligned.
            const dstW = (filled / SPEC_COLS) * W;
            ctx.drawImage(tape,
              0, 0, filled, SPEC_N_FREQ,   // src
              0, rowY, dstW, ROW_H_F       // dst
            );
          } else {
            // Full ring: two-part draw to unroll the circular buffer.
            // Part A: oldest half [tapeX … SPEC_COLS) → left side of the row.
            const partA     = SPEC_COLS - tapeX;
            const partA_dstW = (partA / SPEC_COLS) * W;
            if (partA > 0) {
              ctx.drawImage(tape,
                tapeX, 0, partA, SPEC_N_FREQ,    // src: oldest
                0, rowY, partA_dstW, ROW_H_F      // dst: left
              );
            }
            // Part B: newest half [0 … tapeX) → right side of the row.
            if (tapeX > 0) {
              ctx.drawImage(tape,
                0, 0, tapeX, SPEC_N_FREQ,                    // src: newest
                partA_dstW, rowY, W - partA_dstW, ROW_H_F   // dst: right
              );
            }
          }
        }

        ctx.restore();
        ctx.imageSmoothingEnabled = false;
      }

      // ── Epoch separators (dashed verticals) ──────────────────────────────
      ctx.setLineDash([3, 6]);
      ctx.strokeStyle = cGrid;
      ctx.lineWidth = 1;
      for (let e = 1; e < N_EPOCHS; e++) {
        const x = (e / N_EPOCHS) * W;
        ctx.beginPath();
        ctx.moveTo(x, 0);
        ctx.lineTo(x, WAVE_H);
        ctx.stroke();
      }
      ctx.setLineDash([]);

      // Baseline between waveforms and time strip
      ctx.strokeStyle = cGrid;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, WAVE_H);
      ctx.lineTo(W, WAVE_H);
      ctx.stroke();

      // ── Time labels ───────────────────────────────────────────────────────
      ctx.font      = `9px ui-monospace, "JetBrains Mono", monospace`;
      ctx.fillStyle = cLabel;
      const ticks: [number, string, CanvasTextAlign][] = [
        [3,         `−${N_EPOCHS * EPOCH_S}s`,         "left"  ],
        [W / 3,     `−${(N_EPOCHS - 1) * EPOCH_S}s`,  "center"],
        [2 * W / 3, `−${(N_EPOCHS - 2) * EPOCH_S}s`,  "center"],
        [W - 3,     "now",                             "right" ],
      ];
      for (const [x, label, align] of ticks) {
        ctx.textAlign = align;
        ctx.fillText(label, x, H - 4);
      }

      // ── Channel rows ─────────────────────────────────────────────────────
      for (let ch = 0; ch < N_CH; ch++) {
        const y0  = ch * ROW_H;
        const mid = y0 + ROW_H / 2;

        // Row divider (skip the first row — no line above it)
        if (ch > 0) {
          ctx.setLineDash([]);
          ctx.strokeStyle = cGrid;
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(0, y0);
          ctx.lineTo(W, y0);
          ctx.stroke();
        }

        // Dashed zero-baseline
        ctx.setLineDash([4, 8]);
        ctx.strokeStyle = cBase;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(0, mid);
        ctx.lineTo(W, mid);
        ctx.stroke();
        ctx.setLineDash([]);

        // Channel label (top-left of the row)
        ctx.font      = `bold 10px ui-monospace, "JetBrains Mono", monospace`;
        ctx.textAlign = "left";
        if (!isDark) {
          // Dark outline behind label for readability on bright spectrogram
          ctx.lineWidth   = 3;
          ctx.strokeStyle = "rgba(255,255,255,0.85)";
          ctx.lineJoin    = "round";
          ctx.strokeText(EEG_CH[ch], 6, y0 + 13);
        }
        ctx.fillStyle = chColors[ch];
        ctx.fillText(EEG_CH[ch], 6, y0 + 13);

        // ── Waveform path, clipped to this row ───────────────────────────
        const buf = smooth(readBufferAt(ch, displayPos));

        ctx.save();
        ctx.beginPath();
        ctx.rect(0, y0, W, ROW_H);
        ctx.clip();

        ctx.lineJoin = "round";
        ctx.lineCap  = "round";

        // Build the waveform path once — store endpoints for live dot
        let lastX = 0, lastY = mid;
        const path = new Path2D();
        for (let i = 0; i < BUF_SIZE; i++) {
          const x = (i / (BUF_SIZE - 1)) * W;
          const y = mid - (buf[i] / EEG_RANGE) * (ROW_H / 2 - PAD);
          i === 0 ? path.moveTo(x, y) : path.lineTo(x, y);
          lastX = x; lastY = y;
        }

        // ── Gradient fill under curve (only in dark mode — too heavy on light spec) ──
        if (isDark) {
          const fillGrad = ctx.createLinearGradient(0, y0, 0, y0 + ROW_H);
          const chColor = chColors[ch] || "#3b82f6";
          // Convert chColor to rgba for gradient stops — parse hex or use fallback
          fillGrad.addColorStop(0, chColor.replace(/^#/, '') .length === 6
            ? `rgba(${parseInt(chColor.slice(1,3),16)},${parseInt(chColor.slice(3,5),16)},${parseInt(chColor.slice(5,7),16)},0.08)`
            : `rgba(100,130,220,0.08)`);
          fillGrad.addColorStop(0.5, "rgba(0,0,0,0)");
          fillGrad.addColorStop(1, chColor.replace(/^#/, '') .length === 6
            ? `rgba(${parseInt(chColor.slice(1,3),16)},${parseInt(chColor.slice(3,5),16)},${parseInt(chColor.slice(5,7),16)},0.06)`
            : `rgba(100,130,220,0.06)`);
          // Close the fill area
          const fillPath = new Path2D(path);
          fillPath.lineTo(W, mid);
          fillPath.lineTo(0, mid);
          fillPath.closePath();
          ctx.fillStyle = fillGrad;
          ctx.globalAlpha = 0.7;
          ctx.fill(fillPath);
          ctx.globalAlpha = 1;
        }

        // ── Glow layer (wider semi-transparent stroke, dark mode only) ─────
        if (isDark) {
          ctx.shadowBlur   = 6;
          ctx.shadowColor  = chColors[ch] || "#3b82f6";
          ctx.strokeStyle  = chColors[ch] || "#3b82f6";
          ctx.lineWidth    = 3.5;
          ctx.globalAlpha  = 0.25;
          ctx.stroke(path);
          ctx.shadowBlur   = 0;
          ctx.globalAlpha  = 1;
        }

        // ── Light-mode contrast outline ────────────────────────────────────
        if (!isDark) {
          ctx.strokeStyle = "rgba(0,0,0,0.45)";
          ctx.lineWidth   = 3.5;
          ctx.stroke(path);
        }

        // ── Main waveform stroke ────────────────────────────────────────────
        ctx.strokeStyle = chColors[ch];
        ctx.lineWidth   = isDark ? 1.5 : 1.8;
        ctx.stroke(path);

        // ── Live-edge pulse dot (bright circle at the rightmost sample) ─────
        if (isDark) {
          ctx.beginPath();
          ctx.arc(lastX, lastY, 2.5, 0, Math.PI * 2);
          ctx.fillStyle   = chColors[ch] || "#3b82f6";
          ctx.shadowBlur  = 8;
          ctx.shadowColor = chColors[ch] || "#3b82f6";
          ctx.globalAlpha = 0.9;
          ctx.fill();
          ctx.shadowBlur  = 0;
          ctx.globalAlpha = 1;
        }

        ctx.restore();
      }

      // ── Event markers (vertical lines + labels) ──────────────────────────
      // Each marker has a samplePos recorded at creation time.
      // X = ((samplePos - oldest) / BUF_SIZE) * W, where oldest = displayPos - BUF_SIZE.
      {
        const oldest = displayPos - BUF_SIZE;
        // Prune markers that have scrolled off the left edge.
        while (markers.length > 0 && markers[0].samplePos < oldest) markers.shift();

        const frameHits: MarkerHitBox[] = [];

        for (const mk of markers) {
          const frac = (mk.samplePos - oldest) / BUF_SIZE;
          if (frac < 0 || frac > 1) continue;
          const mx = frac * W;

          // Vertical dashed line across the waveform area
          ctx.save();
          ctx.setLineDash([4, 3]);
          ctx.strokeStyle = mk.color;
          ctx.lineWidth   = 1.5;
          ctx.globalAlpha = 0.85;
          ctx.beginPath();
          ctx.moveTo(mx, 0);
          ctx.lineTo(mx, WAVE_H);
          ctx.stroke();
          ctx.setLineDash([]);

          // Small triangle notch at the top
          ctx.fillStyle   = mk.color;
          ctx.globalAlpha = 0.9;
          ctx.beginPath();
          ctx.moveTo(mx, 0);
          ctx.lineTo(mx - 4, 7);
          ctx.lineTo(mx + 4, 7);
          ctx.closePath();
          ctx.fill();

          // Truncated label text (with background pill for readability)
          if (mk.label) {
            const short = mk.label.length > MARKER_LABEL_MAX
              ? mk.label.slice(0, MARKER_LABEL_MAX) + "…"
              : mk.label;

            ctx.font         = `bold 8px ui-sans-serif, system-ui, sans-serif`;
            ctx.textAlign    = "left";
            ctx.textBaseline = "top";
            const tw  = ctx.measureText(short).width;
            const px  = 3;
            const pillH = 12;
            const pillY = 9;
            let lx  = mx + 5;
            let align: CanvasTextAlign = "left";
            // Flip to the left side if too close to the right edge
            if (lx + tw + px * 2 > W) { align = "right"; lx = mx - 5; }
            ctx.textAlign = align;

            // Background pill
            ctx.globalAlpha = isDark ? 0.75 : 0.85;
            ctx.fillStyle   = isDark ? "#1a1a2e" : "#fff";
            const rectX = align === "right" ? lx - tw - px : lx - px;
            ctx.fillRect(rectX, pillY, tw + px * 2, pillH);

            // Label text
            ctx.globalAlpha = 1;
            ctx.fillStyle   = mk.color;
            ctx.fillText(short, lx, pillY + 1);

            // Record hit-box (in CSS px) for click detection
            frameHits.push({
              x: rectX, y: pillY,
              w: tw + px * 2, h: pillH,
              marker: mk,
            });
          }

          ctx.restore();
        }

        markerHitBoxes = frameHits;
      }

      animFrame = requestAnimationFrame(frame);
    }

    animFrame = requestAnimationFrame(frame);
  }

  function stopRender() {
    rendering = false;
    if (animFrame !== undefined) {
      cancelAnimationFrame(animFrame);
      animFrame = undefined;
    }
  }

  // ── Marker click → tooltip ───────────────────────────────────────────────
  function onCanvasClick(e: MouseEvent) {
    const rect = canvasEl.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;

    // Check hit-boxes (newest on top → reverse)
    for (let i = markerHitBoxes.length - 1; i >= 0; i--) {
      const hb = markerHitBoxes[i];
      if (cx >= hb.x && cx <= hb.x + hb.w && cy >= hb.y && cy <= hb.y + hb.h) {
        showTooltip(hb.x + hb.w / 2, hb.y + hb.h + 4, hb.marker.label, hb.marker.color);
        return;
      }
    }
    // Clicked elsewhere → dismiss
    tooltip = null;
  }

  function showTooltip(x: number, y: number, text: string, color: string) {
    clearTimeout(tooltipTimer);
    tooltip = { x, y, text, color };
    // Auto-dismiss after 4 s
    tooltipTimer = setTimeout(() => { tooltip = null; }, 4000);
  }

  // ── Lifecycle ───────────────────────────────────────────────────────────────
  onMount(() => {
    initSpecTapes();

    const resize = () => {
      const dpr = devicePixelRatio || 1;
      cssW = canvasEl.clientWidth;
      canvasEl.width  = Math.round(cssW    * dpr);
      canvasEl.height = Math.round(CHART_H * dpr);
    };

    ro = new ResizeObserver(resize);
    ro.observe(canvasEl);
    resize();
    startRender();
  });

  onDestroy(() => {
    stopRender();
    ro?.disconnect();
    clearTimeout(tooltipTimer);
  });
</script>

<!--
  Canvas is always in the DOM — never inside {#if} — so bind:this resolves
  before onMount and the RAF loop always has a live element to draw into.
  Flat baselines render while disconnected; live waveforms appear on data.
-->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class={cn("w-full overflow-hidden rounded-xl border border-white/[0.06] relative")}
  style="line-height:0">
  <canvas
    bind:this={canvasEl}
    class="block w-full"
    style="height:{CHART_H}px"
    onclick={onCanvasClick}
  ></canvas>

  {#if tooltip}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute z-10 pointer-events-auto rounded-md px-2.5 py-1.5
             shadow-lg border border-white/10 backdrop-blur-sm
             text-[0.68rem] font-semibold leading-snug max-w-[220px] break-words
             bg-[#1a1a2e]/90 dark:bg-[#1a1a2e]/90
             light:bg-white/95 light:border-black/10"
      style="left:{Math.min(tooltip.x, cssW - 120)}px; top:{tooltip.y}px; color:{tooltip.color}"
      onclick={() => { tooltip = null; }}
    >
      {tooltip.text}
    </div>
  {/if}
</div>
