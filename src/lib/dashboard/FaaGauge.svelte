<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import MetricTooltip from "./MetricTooltip.svelte";
  interface Props { faa: number; }
  let { faa }: Props = $props();

  let expanded = $state(true);
</script>

<div class="rounded-xl border border-border dark:border-white/[0.04]
            bg-muted dark:bg-[#1a1a28] px-3 py-2 flex flex-col gap-1.5"
     role="meter" aria-label={t("dashboard.faa")}
     aria-valuenow={Math.round(faa * 1000) / 1000} aria-valuemin={-1} aria-valuemax={1}>
  <button class="flex items-center gap-1.5 w-full group" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
         stroke-linecap="round" stroke-linejoin="round"
         class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                transition-transform duration-150 shrink-0 {expanded ? 'rotate-90' : ''}">
      <path d="M9 18l6-6-6-6"/>
    </svg>
    <div class="flex items-center gap-1 flex-1 min-w-0">
      <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground
                   group-hover:text-foreground transition-colors">{t("dashboard.faa")}</span>
      <span class="text-[0.45rem] text-purple-500 live-blink shrink-0" aria-hidden="true">●</span>
    </div>
    <span class="text-[0.65rem] font-bold tabular-nums shrink-0"
          style="color:{Math.abs(faa) > 0.3 ? '#a855f7' : '#6b7280'}">
      {faa >= 0 ? "+" : ""}{faa.toFixed(3)}
    </span>
  </button>

  {#if expanded}
    <MetricTooltip text={t("tip.faa")}>
      <div class="flex flex-col gap-1">
        <div class="relative h-1.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
          <div class="absolute left-1/2 top-0 w-px h-full bg-muted-foreground/20"></div>
          {#if faa >= 0}
            <div class="absolute top-0 h-full rounded-full transition-all duration-500 ease-out"
                 style="left:50%; width:{Math.min(50, Math.abs(faa) * 50)}%;
                        background: linear-gradient(90deg, #c084fc, #a855f7)"></div>
          {:else}
            <div class="absolute top-0 h-full rounded-full transition-all duration-500 ease-out"
                 style="right:50%; width:{Math.min(50, Math.abs(faa) * 50)}%;
                        background: linear-gradient(270deg, #c084fc, #a855f7)"></div>
          {/if}
        </div>
        <div class="flex justify-between text-[0.42rem] text-muted-foreground/30">
          <span>{t("dashboard.faaWithdrawal")}</span>
          <span>{t("dashboard.faaFormula")}</span>
          <span>{t("dashboard.faaApproach")}</span>
        </div>
      </div>
    </MetricTooltip>
  {/if}
</div>
