<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  GPU utilisation — macOS only (IOKit via `get_gpu_stats` Tauri command).
  Renders nothing on other platforms or if the command is unavailable.

  Shows:
    • a rolling SVG sparkline (last 60 polls = 2 min at 2 s interval)
    • a thin stacked bar (render ↔ tiler)
    • current percentage

  All colours via theme.ts — no hex literals here.
-->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import { getGpuStats } from "$lib/daemon/client";
import { colorForLoad } from "$lib/theme";

interface GpuStats {
  render: number;
  tiler: number;
  overall: number;
}

const MAX_PTS = 60; // 2 min at 2 s polling
const SW = 54; // sparkline SVG width (px)
const SH = 14; // sparkline SVG height (px)

let gpuStats = $state<GpuStats | null>(null);
let history = $state<number[]>([]);
let timer: ReturnType<typeof setInterval> | undefined;

// ── Derived sparkline geometry ──────────────────────────────────────────────
const spark = $derived.by(() => {
  if (history.length < 2) return null;
  const n = history.length;
  const off = MAX_PTS - n; // align newest point to right edge

  const pts = history.map((v, i): [number, number] => [
    ((off + i) / (MAX_PTS - 1)) * SW,
    SH - v * SH * 0.88, // 0.88 keeps the peak slightly below top edge
  ]);

  const line = pts.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" ");

  const [x0, y0] = pts[0];
  const [xE] = pts[n - 1];
  const area =
    `M ${x0.toFixed(1)},${SH} L ${x0.toFixed(1)},${y0.toFixed(1)} ` +
    pts
      .slice(1)
      .map(([x, y]) => `L ${x.toFixed(1)},${y.toFixed(1)}`)
      .join(" ") +
    ` L ${xE.toFixed(1)},${SH} Z`;

  return { line, area };
});

const col = $derived(gpuStats ? colorForLoad(gpuStats.overall) : "#94a3b8");

// ── Polling ─────────────────────────────────────────────────────────────────
async function poll() {
  try {
    const s = await getGpuStats();
    gpuStats = s;
    if (s !== null) {
      history = [...history.slice(-(MAX_PTS - 1)), s.overall];
    }
  } catch {
    gpuStats = null;
    clearInterval(timer); // command absent on this platform — stop silently
  }
}

onMount(() => {
  poll();
  timer = setInterval(poll, 2000);
});
onDestroy(() => clearInterval(timer));
</script>

{#if gpuStats !== null}
  <div
    class="flex items-center gap-1.5 shrink-0"
    title="GPU  render {(gpuStats.render * 100).toFixed(0)}%  tiler {(gpuStats.tiler * 100).toFixed(0)}%"
  >
    <!-- Label -->
    <span class="text-[0.52rem] font-semibold tracking-widest uppercase text-muted-foreground/70 leading-none">
      GPU
    </span>

    <!-- Sparkline -->
    {#if spark}
      <svg width={SW} height={SH} viewBox="0 0 {SW} {SH}" class="shrink-0 overflow-visible">
        <path d={spark.area} fill={col} opacity="0.18" />
        <polyline
          points={spark.line}
          fill="none"
          stroke={col}
          stroke-width="1.3"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    {/if}

    <!-- Stacked render + tiler bar -->
    <div class="relative w-10 h-1.5 rounded-full bg-muted overflow-hidden shrink-0">
      <div
        class="absolute inset-y-0 left-0 rounded-full transition-all duration-700"
        style="width:{(gpuStats.render * 100).toFixed(1)}%; background:{col}; opacity:0.9"
      ></div>
      <div
        class="absolute inset-y-0 left-0 rounded-full transition-all duration-700"
        style="width:{(gpuStats.tiler * 100).toFixed(1)}%; background:{col}; opacity:0.45"
      ></div>
    </div>

    <!-- Percentage -->
    <span class="text-[0.6rem] tabular-nums leading-none" style="color:{col}">
      {(gpuStats.overall * 100).toFixed(0)}%
    </span>
  </div>
{/if}
