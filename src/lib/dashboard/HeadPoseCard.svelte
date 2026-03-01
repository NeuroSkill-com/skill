<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import MetricTooltip from "./MetricTooltip.svelte";
  interface Props { pitch: number; roll: number; stillness: number; nodCount: number; shakeCount: number; }
  let { pitch, roll, stillness, nodCount, shakeCount }: Props = $props();

  let expanded = $state(true);
</script>

<div class="rounded-xl border border-border dark:border-white/[0.04]
            bg-muted dark:bg-[#1a1a28] px-3 py-2 flex flex-col gap-1.5">
  <button class="flex items-center gap-1.5 w-full group" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
         stroke-linecap="round" stroke-linejoin="round"
         class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                transition-transform duration-150 shrink-0 {expanded ? 'rotate-90' : ''}">
      <path d="M9 18l6-6-6-6"/>
    </svg>
    <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground
                 group-hover:text-foreground transition-colors">{t("dashboard.headPose")}</span>
    <span class="text-[0.45rem] text-sky-500 live-blink shrink-0" aria-hidden="true">●</span>
  </button>

  {#if expanded}
    <div class="grid grid-cols-2 sm:grid-cols-5 gap-x-2 gap-y-1.5">
      <MetricTooltip text={t("tip.pitch")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.pitch")}</span>
            <span class="text-[0.58rem] font-bold tabular-nums">{pitch >= 0 ? "+" : ""}{pitch.toFixed(1)}°</span>
          </div>
          <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden relative">
            <div class="absolute top-0 h-full rounded-full transition-all duration-300 bg-sky-400"
              style="left:{50 + Math.max(-50, Math.min(50, pitch))}%; width:2px"></div>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.roll")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.roll")}</span>
            <span class="text-[0.58rem] font-bold tabular-nums">{roll >= 0 ? "+" : ""}{roll.toFixed(1)}°</span>
          </div>
          <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden relative">
            <div class="absolute top-0 h-full rounded-full transition-all duration-300 bg-indigo-400"
              style="left:{50 + Math.max(-50, Math.min(50, roll))}%; width:2px"></div>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.stillness")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.stillness")}</span>
            <span class="text-[0.58rem] font-bold tabular-nums" style="color:{stillness > 80 ? '#22c55e' : stillness > 40 ? '#f59e0b' : '#ef4444'}">{stillness.toFixed(0)}</span>
          </div>
          <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500 bg-emerald-400" style="width:{stillness}%"></div>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.nods")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.nods")}</span>
            <span class="text-[0.58rem] font-bold tabular-nums">{nodCount}</span>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.shakes")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.shakes")}</span>
            <span class="text-[0.58rem] font-bold tabular-nums">{shakeCount}</span>
          </div>
        </div>
      </MetricTooltip>
    </div>
  {/if}
</div>
