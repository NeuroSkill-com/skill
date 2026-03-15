<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts" module>
  /** PPG packet from the Rust backend. */
  export interface PpgPacket {
    channel:   number;   // 0=ambient, 1=infrared, 2=red
    samples:   number[];
    timestamp: number;
  }

  /** Vertical event marker rendered on the PPG chart. */
  export interface PpgMarker {
    timestamp_ms: number;
    label: string;
    color: string;
  }
</script>

<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { animatedCanvas } from "$lib/use-canvas";

  /** Channel labels, colors matching optical wavelengths. */
  const CH = ["Ambient", "IR", "Red"] as const;
  const COLORS = ["#64748b", "#a78bfa", "#ef4444"];   // slate, violet, red

  const CANVAS_W = 400;
  const CANVAS_H = 120;
  const VISIBLE_SAMPLES = 512;    // ~8 s at 64 Hz
  const CH_COUNT = 3;

  // Ring buffers per channel.
  const bufs: Float64Array[] = Array.from({ length: CH_COUNT }, () => new Float64Array(VISIBLE_SAMPLES));
  const heads = [0, 0, 0];
  const filled = [0, 0, 0];

  let needsRedraw = false;
  let latestValues = $state<number[]>([0, 0, 0]);

  // ── Event markers ──────────────────────────────────────────────────────────
  const MAX_MARKERS = 64;
  const PPG_LABEL_MAX = 6;
  interface StoredPpgMarker { headPos: number; label: string; color: string; }
  let ppgMarkers: StoredPpgMarker[] = [];

  // Hit-boxes for click-to-expand
  interface PpgHitBox { x: number; y: number; w: number; h: number; marker: StoredPpgMarker; }
  let ppgHitBoxes: PpgHitBox[] = [];

  // Tooltip state
  let ppgTooltip = $state<{ x: number; y: number; text: string; color: string } | null>(null);
  let ppgTooltipTimer: ReturnType<typeof setTimeout> | undefined;

  /** Add a vertical event marker at the current write position. */
  export function pushMarker(m: PpgMarker): void {
    const pos = Math.min(...heads);
    ppgMarkers.push({ headPos: pos, label: m.label, color: m.color });
    if (ppgMarkers.length > MAX_MARKERS) ppgMarkers.shift();
    needsRedraw = true;
  }

  /** Push samples from one PPG channel. Called externally by the parent. */
  export function pushSamples(ch: number, samples: number[]) {
    if (ch < 0 || ch >= CH_COUNT) return;
    for (const v of samples) {
      bufs[ch][heads[ch] % VISIBLE_SAMPLES] = v;
      heads[ch]++;
      if (filled[ch] < VISIBLE_SAMPLES) filled[ch]++;
    }
    if (samples.length > 0) {
      latestValues[ch] = samples[samples.length - 1];
    }
    needsRedraw = true;
  }

  function draw(ctx: CanvasRenderingContext2D, w: number, h: number) {
    if (!needsRedraw) return;
    needsRedraw = false;

    ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
    ctx.save();
    const dpr = ctx.canvas.width / w;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    // Draw each channel as a separate line, sharing the vertical space.
    const chH = h / CH_COUNT;

    for (let ch = 0; ch < CH_COUNT; ch++) {
      const buf = bufs[ch];
      const head = heads[ch];
      const n = filled[ch];
      if (n < 2) continue;

      // Find min/max for auto-scaling this channel.
      let mn = Infinity, mx = -Infinity;
      for (let i = 0; i < n; i++) {
        const idx = (head - n + i + VISIBLE_SAMPLES * 2) % VISIBLE_SAMPLES;
        const v = buf[idx];
        if (v < mn) mn = v;
        if (v > mx) mx = v;
      }
      const range = mx - mn || 1;
      const yOff = ch * chH;
      const pad = chH * 0.08;

      ctx.beginPath();
      ctx.strokeStyle = COLORS[ch];
      ctx.lineWidth = 1.2;
      for (let i = 0; i < n; i++) {
        const idx = (head - n + i + VISIBLE_SAMPLES * 2) % VISIBLE_SAMPLES;
        const x = (i / (VISIBLE_SAMPLES - 1)) * w;
        const y = yOff + pad + (1 - (buf[idx] - mn) / range) * (chH - 2 * pad);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      // Channel label (left).
      ctx.fillStyle = COLORS[ch];
      ctx.font = `${10}px system-ui, sans-serif`;
      ctx.fillText(CH[ch], 4, yOff + 12);
    }

    // ── Event markers ──────────────────────────────────────────────────────
    const minHead = Math.min(...heads);
    const nVis    = Math.min(minHead, VISIBLE_SAMPLES);
    const oldest  = minHead - nVis;

    // Prune off-screen markers
    while (ppgMarkers.length > 0 && ppgMarkers[0].headPos < oldest) ppgMarkers.shift();

    const frameHits: PpgHitBox[] = [];

    for (const mk of ppgMarkers) {
      const frac = nVis > 0 ? (mk.headPos - oldest) / VISIBLE_SAMPLES : -1;
      if (frac < 0 || frac > 1) continue;
      const mx = frac * w;

      // Dashed vertical line
      ctx.save();
      ctx.setLineDash([4, 3]);
      ctx.strokeStyle = mk.color;
      ctx.lineWidth   = 1.5;
      ctx.globalAlpha = 0.85;
      ctx.beginPath();
      ctx.moveTo(mx, 0);
      ctx.lineTo(mx, h);
      ctx.stroke();
      ctx.setLineDash([]);

      // Top triangle
      ctx.fillStyle   = mk.color;
      ctx.globalAlpha = 0.9;
      const ts = 4;
      ctx.beginPath();
      ctx.moveTo(mx, 0);
      ctx.lineTo(mx - ts, ts * 1.75);
      ctx.lineTo(mx + ts, ts * 1.75);
      ctx.closePath();
      ctx.fill();

      // Truncated label
      if (mk.label) {
        const short = mk.label.length > PPG_LABEL_MAX
          ? mk.label.slice(0, PPG_LABEL_MAX) + "…"
          : mk.label;

        ctx.font         = `bold ${8}px system-ui, sans-serif`;
        ctx.textBaseline = "top";
        const tw  = ctx.measureText(short).width;
        const px  = 3;
        const pillH = 12;
        const pillY = 9;
        let lx  = mx + 5;
        let align: CanvasTextAlign = "left";
        if (lx + tw + px * 2 > w) { align = "right"; lx = mx - 5; }
        ctx.textAlign = align;

        // Background pill
        ctx.globalAlpha = 0.75;
        ctx.fillStyle   = "#1a1a2e";
        const rx = align === "right" ? lx - tw - px : lx - px;
        ctx.fillRect(rx, pillY, tw + px * 2, pillH);

        // Text
        ctx.globalAlpha = 1;
        ctx.fillStyle   = mk.color;
        ctx.fillText(short, lx, pillY + 1);

        // Record hit-box in CSS px for click detection
        frameHits.push({
          x: rx, y: pillY,
          w: tw + px * 2, h: pillH,
          marker: mk,
        });
      }

      ctx.restore();
    }

    ppgHitBoxes = frameHits;
    ctx.restore();
  }

  // ── Marker click → tooltip ───────────────────────────────────────────────
  function onPpgClick(e: MouseEvent) {
    const canvas = e.currentTarget as HTMLCanvasElement;
    const rect = canvas.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;

    for (let i = ppgHitBoxes.length - 1; i >= 0; i--) {
      const hb = ppgHitBoxes[i];
      if (cx >= hb.x && cx <= hb.x + hb.w && cy >= hb.y && cy <= hb.y + hb.h) {
        clearTimeout(ppgTooltipTimer);
        ppgTooltip = { x: hb.x + hb.w / 2, y: hb.y + hb.h + 4, text: hb.marker.label, color: hb.marker.color };
        ppgTooltipTimer = setTimeout(() => { ppgTooltip = null; }, 4000);
        return;
      }
    }
    ppgTooltip = null;
  }
</script>

<div class="flex flex-col gap-1.5">
  <!-- Header with live values -->
  <div class="flex items-center gap-3 flex-wrap">
    {#each CH as label, i}
      <div class="flex items-center gap-1.5">
        <span class="inline-block w-2 h-2 rounded-full" style="background:{COLORS[i]}"></span>
        <span class="text-[0.55rem] font-semibold text-muted-foreground">{label}</span>
        <span class="font-mono text-[0.6rem] text-muted-foreground/70 tabular-nums">
          {latestValues[i] > 0 ? latestValues[i].toFixed(0) : "—"}
        </span>
      </div>
    {/each}
  </div>

  <!-- Canvas + tooltip — lifecycle managed by animatedCanvas action -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="relative">
    <canvas
      use:animatedCanvas={{ draw, heightPx: CANVAS_H, widthPx: CANVAS_W }}
      class="w-full rounded-lg bg-black/[0.03] dark:bg-white/[0.03]"
      style="height:{CANVAS_H}px; image-rendering:pixelated"
      onclick={onPpgClick}
    ></canvas>

    {#if ppgTooltip}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="absolute z-10 pointer-events-auto rounded-md px-2.5 py-1.5
               shadow-lg border border-white/10 backdrop-blur-sm
               text-[0.68rem] font-semibold leading-snug max-w-[220px] break-words
               bg-[#1a1a2e]/90"
        style="left:{Math.min(ppgTooltip.x, CANVAS_W - 120)}px; top:{ppgTooltip.y}px; color:{ppgTooltip.color}"
        onclick={() => { ppgTooltip = null; }}
      >
        {ppgTooltip.text}
      </div>
    {/if}
  </div>
</div>
