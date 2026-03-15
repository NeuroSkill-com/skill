<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  GPU utilisation chart — macOS only.
  Visible whenever GPU stats are available (i.e. on supported hardware).

  Smoothness strategy:
  • GPU stats polled every 500 ms (separate from the 6 s model-status check).
  • A requestAnimationFrame loop runs at 60 fps regardless of poll cadence.
  • X-axis is real time: each rAF frame redraws with Date.now() as the right
    edge, so the chart scrolls continuously and the last segment always extends
    to the current moment — no jumpy updates.
  • The horizontal LinearGradient is rebuilt each frame from per-point colours,
    giving smooth colour transitions with zero patching.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { colorForLoad, C_NEUTRAL, rgba as toRgba } from "$lib/theme";
  import { t } from "$lib/i18n/index.svelte";
  import { getResolved } from "$lib/theme-store.svelte";
  import { animatedCanvas } from "$lib/use-canvas";

  interface GpuStats    { render: number; tiler: number; overall: number; }
  interface ModelStatus { encoder_loaded: boolean; }
  interface Point       { overall: number; tiler: number; ts: number; }

  const CANVAS_H    = 52;          // logical px

  let collapsed = $state(false);  // user can hide the canvas while keeping the header
  const WIN_MS      = 120_000;     // 2-minute visible window
  const GPU_POLL_MS = 500;         // GPU sampling rate
  const MDL_POLL_MS = 6_000;       // model-status check (cheap)

  // ── Reactive state (Svelte) ────────────────────────────────────────────────
  let gpuStats    = $state<GpuStats | null>(null);
  let modelActive = $state(false);

  let container: HTMLDivElement | undefined = $state();

  const visible = $derived(gpuStats !== null);
  const col     = $derived(gpuStats ? colorForLoad(gpuStats.overall) : C_NEUTRAL);

  // ── Non-reactive (mutated inside rAF — no Svelte overhead) ────────────────
  let history:   Point[]           = [];
  let gpuTimer:  ReturnType<typeof setInterval> | undefined;
  let mdlTimer:  ReturnType<typeof setInterval> | undefined;

  // ── Draw — called every rAF frame by the animatedCanvas action ─────────────
  function draw(ctx: CanvasRenderingContext2D, W: number, H: number) {
    ctx.save();
    ctx.setTransform(ctx.canvas.width / W, 0, 0, ctx.canvas.height / H, 0, 0);

    const now    = Date.now();
    const wStart = now - WIN_MS;

    // Background
    const dark = getResolved() === "dark";
    ctx.fillStyle = dark ? "#0f0f18" : "#f8f9fb";
    ctx.fillRect(0, 0, W, H);

    // Need at least one real sample
    if (history.length < 1) { ctx.restore(); return; }

    const PAD = 3;
    // x: time-based — right edge = now, left edge = now − WIN_MS
    const px = (ts: number) => ((ts - wStart) / WIN_MS) * W;
    const py = (v: number)  => H - PAD - v * (H - PAD * 2);

    // Visible samples + a virtual "now" point extending the last value to present
    const last = history[history.length - 1];
    const samples: Point[] = [
      ...history.filter(p => p.ts >= wStart),
      { ...last, ts: now },   // live extension to current time
    ];
    if (samples.length < 2) { ctx.restore(); return; }

    const pts  = samples.map(s => [px(s.ts), py(s.overall), s.overall] as [number, number, number]);
    const tPts = samples.map(s => [px(s.ts), py(s.tiler)]              as [number, number]);
    const m    = pts.length;

    // ── Grid lines ───────────────────────────────────────────────────────────
    ctx.lineWidth = 1;
    for (const [frac, alpha] of [[0.25, 0.04], [0.5, 0.07], [0.75, 0.04]] as [number, number][]) {
      ctx.strokeStyle = dark ? `rgba(255,255,255,${alpha})` : `rgba(0,0,0,${alpha * 1.5})`;
      const y = Math.round(py(frac)) + 0.5;
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(W, y); ctx.stroke();
    }

    // ── Horizontal gradient — one stop per sample, colour by load ────────────
    const x0 = Math.max(pts[0][0], 0);
    const x1 = W;
    const grad = ctx.createLinearGradient(x0, 0, x1, 0);
    for (let i = 0; i < m; i++) {
      const stop = Math.max(0, Math.min(1, (pts[i][0] - x0) / (x1 - x0)));
      grad.addColorStop(stop, colorForLoad(pts[i][2]));
    }

    // ── Area fill (clipped path + vertical fade) ─────────────────────────────
    ctx.save();
    ctx.beginPath();
    ctx.moveTo(x0, H);
    ctx.lineTo(x0, pts[0][1]);
    for (const [x, y] of pts.slice(1)) ctx.lineTo(x, y);
    ctx.lineTo(x1, H);
    ctx.closePath();
    ctx.clip();

    ctx.globalAlpha = dark ? 0.22 : 0.15;
    ctx.fillStyle   = grad;
    ctx.fillRect(0, 0, W, H);

    ctx.globalCompositeOperation = "destination-out";
    const vFade = ctx.createLinearGradient(0, H * 0.3, 0, H);
    vFade.addColorStop(0, "rgba(0,0,0,0)");
    vFade.addColorStop(1, dark ? "rgba(0,0,0,0.72)" : "rgba(0,0,0,0.55)");
    ctx.globalAlpha = 1;
    ctx.fillStyle   = vFade;
    ctx.fillRect(0, 0, W, H);
    ctx.restore();

    // ── Tiler line ───────────────────────────────────────────────────────────
    ctx.beginPath();
    ctx.moveTo(tPts[0][0], tPts[0][1]);
    for (const [x, y] of tPts.slice(1)) ctx.lineTo(x, y);
    ctx.strokeStyle = dark ? "rgba(148,163,184,0.28)" : "rgba(100,116,139,0.35)";
    ctx.lineWidth   = 0.8;
    ctx.lineJoin    = "round";
    ctx.setLineDash([2, 5]);
    ctx.stroke();
    ctx.setLineDash([]);

    // ── Main line ─────────────────────────────────────────────────────────────
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (const [x, y] of pts.slice(1)) ctx.lineTo(x, y);
    ctx.strokeStyle = grad;
    ctx.lineWidth   = 1.75;
    ctx.lineJoin    = "round";
    ctx.lineCap     = "round";
    ctx.stroke();

    // ── Tip dot ───────────────────────────────────────────────────────────────
    const tipColor = colorForLoad(pts[m - 1][2]);
    const [tx, ty] = pts[m - 1];
    ctx.beginPath();
    ctx.arc(tx, ty, 2.5, 0, Math.PI * 2);
    ctx.fillStyle = tipColor;
    ctx.fill();
    ctx.beginPath();
    ctx.arc(tx, ty, 4.5, 0, Math.PI * 2);
    ctx.fillStyle = toRgba(tipColor, 0.22);
    ctx.fill();

    ctx.restore();
  }

  // ── Polling ─────────────────────────────────────────────────────────────────
  let gpuErrCount = 0;
  async function pollGpu() {
    try {
      const gpu = await invoke<GpuStats | null>("get_gpu_stats");
      gpuErrCount = 0;
      gpuStats = gpu;
      if (gpu !== null) {
        const ts = Date.now();
        // Keep only samples within the window + a small buffer
        history = [...history.filter(p => p.ts >= ts - WIN_MS - 2_000),
                   { overall: gpu.overall, tiler: gpu.tiler, ts }];
      }
    } catch (err) {
      gpuErrCount++;
      // Stop polling only after 5 consecutive failures (GPU monitor unavailable).
      if (gpuErrCount >= 5) {
        gpuStats = null;
        clearInterval(gpuTimer);
        gpuTimer = undefined;
      }
    }
  }

  async function pollModel() {
    try {
      const m = await invoke<ModelStatus>("get_eeg_model_status");
      modelActive = m.encoder_loaded;
    } catch {
      modelActive = false;
    }
  }

  onMount(() => {
    pollGpu();
    pollModel();
    gpuTimer  = setInterval(pollGpu,  GPU_POLL_MS);
    mdlTimer  = setInterval(pollModel, MDL_POLL_MS);
  });
  onDestroy(() => {
    clearInterval(gpuTimer);
    clearInterval(mdlTimer);
  });
</script>

{#if visible}
  <div class="w-full rounded-xl border border-border dark:border-white/[0.06] overflow-hidden mb-2">

    <div class="flex items-center gap-2 px-3.5 pt-2 pb-1.5 bg-white dark:bg-[#14141e]">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">{t("gpu.title")}</span>
      <span class="gpu-live w-1.5 h-1.5 rounded-full shrink-0" style="background:{col}"></span>
      <span class="ml-auto text-[0.56rem] text-muted-foreground/50 tabular-nums shrink-0">
        {t("gpu.render")}&nbsp;{(gpuStats!.render * 100).toFixed(0)}%&nbsp;·&nbsp;{t("gpu.tiler")}&nbsp;{(gpuStats!.tiler * 100).toFixed(0)}%
      </span>
      <span class="text-[0.72rem] font-bold tabular-nums leading-none shrink-0" style="color:{col}">
        {(gpuStats!.overall * 100).toFixed(0)}%
      </span>
      <!-- collapse / expand toggle -->
      <button
        onclick={() => collapsed = !collapsed}
        title={collapsed ? t("common.expand") : t("common.minimise")}
        class="ml-1 flex items-center justify-center w-5 h-5 rounded
               text-muted-foreground/50 hover:text-muted-foreground transition-colors shrink-0">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 transition-transform duration-200 {collapsed ? '' : 'rotate-180'}">
          <path d="M6 9l6 6 6-6"/>
        </svg>
      </button>
    </div>

    {#if !collapsed}
      <div bind:this={container} class="w-full">
        <canvas
          use:animatedCanvas={{ draw, heightPx: CANVAS_H, container }}
          class="block w-full"
          style="height:{CANVAS_H}px"
        ></canvas>
      </div>
    {/if}

  </div>
{/if}

<style>
  .gpu-live { animation: gpu-blink 1s step-start infinite; }
  @keyframes gpu-blink { 0%,100% { opacity:1 } 50% { opacity:0 } }
</style>
