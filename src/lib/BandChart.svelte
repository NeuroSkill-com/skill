<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script module lang="ts">
  // ── Types ──────────────────────────────────────────────────────────────────
  export interface BandPowers {
    channel:        string;
    delta:          number;
    theta:          number;
    alpha:          number;
    beta:           number;
    gamma:          number;
    high_gamma:     number;
    rel_delta:      number;
    rel_theta:      number;
    rel_alpha:      number;
    rel_beta:       number;
    rel_gamma:      number;
    rel_high_gamma: number;
    dominant:       string;
    dominant_symbol:string;
    dominant_color: string;
  }

  export interface BandSnapshot {
    timestamp: number;
    channels:  BandPowers[];
    /** Frontal Alpha Asymmetry: ln(AF8 α) − ln(AF7 α). */
    faa:       number;
    tar?:      number;
    bar?:      number;
    dtr?:      number;
    pse?:      number;
    apf?:      number;
    bps?:      number;
    snr?:      number;
    coherence?: number;
    mu_suppression?: number;
    mood?:     number;
    tbr?:      number;
    sef95?:    number;
    spectral_centroid?: number;
    hjorth_activity?:   number;
    hjorth_mobility?:   number;
    hjorth_complexity?: number;
    permutation_entropy?: number;
    higuchi_fd?:    number;
    dfa_exponent?:  number;
    sample_entropy?: number;
    pac_theta_gamma?: number;
    laterality_index?: number;
    // PPG-derived
    hr?:               number;
    rmssd?:            number;
    sdnn?:             number;
    pnn50?:            number;
    lf_hf_ratio?:      number;
    respiratory_rate?: number;
    spo2_estimate?:    number;
    perfusion_index?:  number;
    stress_index?:     number;
    // Artifact / event detection
    blink_count?:      number;
    blink_rate?:       number;

    // Head pose
    head_pitch?:       number;
    head_roll?:        number;
    stillness?:        number;
    nod_count?:        number;
    shake_count?:      number;
    // Composite scores
    meditation?:       number;
    cognitive_load?:   number;
    drowsiness?:       number;
    // Device telemetry
    temperature_raw?:  number;
    // Headache / migraine EEG correlate indices
    headache_index?:   number;
    migraine_index?:   number;
    // Consciousness metrics
    consciousness_lzc?:          number;
    consciousness_wakefulness?:  number;
    consciousness_integration?:  number;
  }
</script>

<script lang="ts">
  import {
    EEG_CH as CH_NAMES, EEG_COLOR as CH_COLORS,
    BANDS, NUM_BANDS as NBAND,
    BAND_TILE_H   as TILE_H,
    BAND_TILE_GAP as TILE_GAP,
    BAND_CANVAS_H as CANVAS_H,
    BAND_TILE_ML  as ML,
    BAND_TILE_MR  as MR,
    BAND_TAU_MS   as TAU_MS,
  } from "$lib/constants";
  import { animatedCanvas } from "$lib/use-canvas";

  // ── Band metadata + canvas layout ─────────────────────────────────────────
  // Each channel gets one "tile" — a full-width rectangle whose background is
  // the stacked band-power proportions rendered as solid coloured segments.

  // ── Public API ─────────────────────────────────────────────────────────────
  let target = $state<BandSnapshot | null>(null);

  /** Feed in a new snapshot; animation interpolates toward it. */
  export function update(snap: BandSnapshot): void {
    target = snap;
  }

  /** No-op — the animatedCanvas action keeps the RAF loop alive. */
  export function restartRender(): void {
    // Kept for API compat — the action's RAF loop never stops.
  }

  // ── Canvas state ───────────────────────────────────────────────────────────

  // Smoothed display values — [channel][band] relative powers.
  const displayed = Array.from({ length: 4 }, () =>
    new Float64Array(NBAND).fill(1 / NBAND)
  );
  const domIdx = new Int8Array(4).fill(2); // 2 = alpha, initial default

  let lastNow = -1;

  // ── Draw — called every frame by the animatedCanvas action ─────────────────
  function draw(ctx: CanvasRenderingContext2D, W: number, _H: number) {
    const now = performance.now();
    ctx.setTransform(ctx.canvas.width / W, 0, 0, ctx.canvas.height / CANVAS_H, 0, 0);

    const dt    = lastNow < 0 ? 0 : now - lastNow;
    lastNow     = now;
    const alpha = dt > 0 ? 1 - Math.exp(-dt / TAU_MS) : 0;

    // ── Interpolate toward target ─────────────────────────────────────────
    if (target) {
      for (let ci = 0; ci < 4; ci++) {
        const ch = target.channels[ci];
        if (!ch) continue;
        const vals = [
          ch.rel_delta, ch.rel_theta, ch.rel_alpha,
          ch.rel_beta,  ch.rel_gamma, ch.rel_high_gamma,
        ];
        let maxRel = -1, maxIdx = 2;
        for (let b = 0; b < NBAND; b++) {
          displayed[ci][b] += alpha * (vals[b] - displayed[ci][b]);
          if (vals[b] > maxRel) { maxRel = vals[b]; maxIdx = b; }
        }
        domIdx[ci] = maxIdx;
      }
    }

    ctx.clearRect(0, 0, W, CANVAS_H);

    for (let ci = 0; ci < 4; ci++) {
      const ty = ci * (TILE_H + TILE_GAP); // top-y of this tile

      // Normalise so proportions always sum to 1 even during warmup.
      let sum = 0;
      for (let b = 0; b < NBAND; b++) sum += displayed[ci][b];
      if (sum < 1e-6) sum = 1;

      // ── Background: stacked band segments clipped to rounded tile ────────
      ctx.save();
      ctx.beginPath();
      roundRect(ctx, 0, ty, W, TILE_H, 10);
      ctx.clip();

      // Draw each band as a full-height rectangle proportional to its power.
      let xc = 0;
      for (let b = 0; b < NBAND; b++) {
        const segW = (displayed[ci][b] / sum) * W;
        ctx.fillStyle   = BANDS[b].color;
        ctx.globalAlpha = 0.78;
        // +0.5 px overlap prevents hairline gaps between segments.
        ctx.fillRect(xc, ty, segW + 0.5, TILE_H);
        xc += segW;
      }

      // Dark scrim — improves contrast for the white text overlay.
      ctx.globalAlpha = 0.44;
      ctx.fillStyle   = "#000";
      ctx.fillRect(0, ty, W, TILE_H);
      ctx.globalAlpha = 1;

      ctx.restore(); // end clip (rounded corners applied)

      // ── Text overlay ─────────────────────────────────────────────────────
      const dom    = domIdx[ci];
      const domPct = Math.round((displayed[ci][dom] / sum) * 100);

      // Channel label — top-left, coloured with the channel accent.
      ctx.font         = `bold 9px ui-monospace, "JetBrains Mono", monospace`;
      const chColor = CH_COLORS[ci];
      ctx.fillStyle    = chColor;
      ctx.textAlign    = "left";
      ctx.textBaseline = "top";
      ctx.globalAlpha  = 1;
      ctx.fillText(CH_NAMES[ci], ML, ty + 9);

      // Dominant band percentage — top-right, large bold white.
      ctx.font         = `bold 20px ui-sans-serif, system-ui, sans-serif`;
      ctx.fillStyle    = "#ffffff";
      ctx.textAlign    = "right";
      ctx.textBaseline = "top";
      ctx.globalAlpha  = 0.95;
      ctx.fillText(`${domPct}%`, W - MR, ty + 6);

      // Dominant Greek symbol — left-centre, oversized.
      ctx.font         = `bold 24px ui-monospace, "JetBrains Mono", monospace`;
      ctx.fillStyle    = "#ffffff";
      ctx.textAlign    = "left";
      ctx.textBaseline = "middle";
      ctx.globalAlpha  = 0.92;
      ctx.fillText(BANDS[dom].sym, ML, ty + TILE_H / 2 + 1);

      // Dominant band name — right-centre.
      ctx.font         = `bold 10px ui-sans-serif, system-ui, sans-serif`;
      ctx.fillStyle    = "#ffffff";
      ctx.textAlign    = "right";
      ctx.textBaseline = "middle";
      ctx.globalAlpha  = 0.65;
      ctx.fillText(BANDS[dom].name, W - MR, ty + TILE_H / 2 + 1);

      // Bottom strip — compact all-band breakdown.
      ctx.textBaseline = "bottom";
      ctx.textAlign    = "center";
      ctx.font         = `bold 7.5px ui-monospace, "JetBrains Mono", monospace`;
      const stripY  = ty + TILE_H - 6;
      const stripL  = ML + 22;
      const stripR  = W - MR;
      const colW    = (stripR - stripL) / NBAND;

      for (let b = 0; b < NBAND; b++) {
        const bPct = Math.round((displayed[ci][b] / sum) * 100);
        const bx   = stripL + b * colW + colW / 2;
        ctx.fillStyle   = BANDS[b].color;
        ctx.globalAlpha = b === dom ? 1 : 0.72;
        ctx.fillText(`${BANDS[b].sym} ${bPct}`, bx, stripY);
      }

      ctx.globalAlpha = 1;
    }
  }

  // ── Polyfill: roundRect ────────────────────────────────────────────────────
  function roundRect(
    ctx: CanvasRenderingContext2D,
    x: number, y: number, w: number, h: number,
    r: number,
  ) {
    ctx.moveTo(x + r, y);
    ctx.lineTo(x + w - r, y);
    ctx.quadraticCurveTo(x + w, y,     x + w,     y + r);
    ctx.lineTo(x + w, y + h - r);
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
    ctx.lineTo(x + r, y + h);
    ctx.quadraticCurveTo(x,     y + h, x,          y + h - r);
    ctx.lineTo(x, y + r);
    ctx.quadraticCurveTo(x,     y,     x + r,      y);
    ctx.closePath();
  }
</script>

<!--
  Single full-width canvas.  No legend row needed — each tile's bottom strip
  already shows every band's symbol, colour, and percentage.
-->
<div class="w-full overflow-hidden rounded-xl" style="line-height:0">
  <canvas
    use:animatedCanvas={{ draw, heightPx: CANVAS_H }}
    class="block w-full"
    style="height:{CANVAS_H}px"
  ></canvas>
</div>
