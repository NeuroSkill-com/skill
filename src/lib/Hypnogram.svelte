<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Hypnogram — staircase sleep-stage chart using TimeSeriesChart.

  Props:
    epochs   — array of { utc, stage } from get_sleep_stages
    summary  — { total_epochs, wake_epochs, n1_epochs, n2_epochs, n3_epochs, rem_epochs, epoch_secs }
-->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import TimeSeriesChart from "$lib/dashboard/TimeSeriesChart.svelte";
  import type { SleepEpoch, SleepSummary } from "$lib/types";

  let { epochs, summary, xMin, xMax }: {
    epochs: SleepEpoch[];
    summary: SleepSummary;
    xMin?: number;
    xMax?: number;
  } = $props();

  // ── Stage metadata ─────────────────────────────────────────────────────────
  // Y values (top → bottom visually): Wake=4, REM=3, N1=2, N2=1, N3=0
  const STAGES = [
    { stage: 0, key: "sleep.wake", color: "#f59e0b", yVal: 4 },
    { stage: 5, key: "sleep.rem",  color: "#a855f7", yVal: 3 },
    { stage: 1, key: "sleep.n1",   color: "#38bdf8", yVal: 2 },
    { stage: 2, key: "sleep.n2",   color: "#3b82f6", yVal: 1 },
    { stage: 3, key: "sleep.n3",   color: "#6366f1", yVal: 0 },
  ] as const;

  const stageToY = new Map(STAGES.map(s => [s.stage, s.yVal]));

  // Build series data: one series per stage (colored segments) would be complex.
  // Simpler: single series with the Y value mapped from stage.
  // Use a gradient-colored single series in the dominant stage color... 
  // Actually, simplest: single series, color = a neutral that looks good.
  // The area fill + staircase will show the structure.

  // For multi-colored stages, use one series per stage where data is NaN
  // except when that stage is active. But TimeSeriesChart draws LINE_STRIP
  // which breaks on NaN. Instead, use a single series with the mapped Y value.

  let chartTimestamps = $derived.by(() => {
    return epochs.map(e => e.utc);
  });

  let chartData = $derived.by(() => {
    return epochs.map(e => stageToY.get(e.stage as 0 | 1 | 2 | 3 | 5) ?? 2);
  });

  let chartSeries = $derived.by(() => {
    return [{
      key: "sleep",
      label: "Sleep",
      color: "#818cf8", // indigo-400 — neutral across all stages
      data: chartData,
    }];
  });

  let chartYTicks = $derived.by(() => {
    return STAGES.map(s => ({
      value: s.yVal,
      label: t(s.key),
      color: s.color,
    }));
  });

  // ── Summary helpers ────────────────────────────────────────────────────────
  function stagePct(n: number): string {
    if (summary.total_epochs === 0) return "0";
    return ((n / summary.total_epochs) * 100).toFixed(0);
  }
  function stageDur(n: number): string {
    const secs = n * summary.epoch_secs;
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }
</script>

<div class="flex flex-col gap-2">
  <!-- Hypnogram chart -->
  <TimeSeriesChart
    series={chartSeries}
    timestamps={chartTimestamps}
    height={140}
    yMin={-0.5}
    yMax={4.5}
    {xMin}
    {xMax}
    yTicks={chartYTicks}
    stepped={true}
  />

  <!-- Stage breakdown pills -->
  <div class="flex flex-wrap items-center gap-1.5">
    {#each STAGES as s}
      {@const count = s.stage === 0 ? summary.wake_epochs
                    : s.stage === 5 ? summary.rem_epochs
                    : s.stage === 1 ? summary.n1_epochs
                    : s.stage === 2 ? summary.n2_epochs
                    : summary.n3_epochs}
      <div class="flex items-center gap-1 rounded-full border px-2 py-0.5
                  border-border dark:border-white/[0.08]
                  bg-white dark:bg-[#14141e]">
        <span class="w-1.5 h-1.5 rounded-full shrink-0" style="background:{s.color}"></span>
        <span class="text-[0.52rem] font-semibold" style="color:{s.color}">{t(s.key)}</span>
        <span class="text-[0.48rem] tabular-nums text-muted-foreground/60">
          {stagePct(count)}% · {stageDur(count)}
        </span>
      </div>
    {/each}
  </div>
</div>
