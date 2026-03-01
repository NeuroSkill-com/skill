<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import MetricTooltip from "./MetricTooltip.svelte";
  interface Props { blinkCount: number; blinkRate: number; }
  let { blinkCount, blinkRate }: Props = $props();

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
                 group-hover:text-foreground transition-colors">{t("dashboard.artifacts")}</span>
    <span class="text-[0.45rem] text-pink-500 live-blink shrink-0" aria-hidden="true">●</span>
  </button>

  {#if expanded}
    <div class="grid grid-cols-2 gap-x-2 gap-y-1.5">
      <MetricTooltip text={t("tip.blinks")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.blinks")}</span>
            <span class="text-[0.58rem] font-bold tabular-nums">{blinkCount}</span>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.blinkRate")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.blinkRate")}</span>
            <span class="text-[0.58rem] font-bold tabular-nums" style="color:{blinkRate > 25 ? '#f59e0b' : '#6b7280'}">{blinkRate.toFixed(1)}/min</span>
          </div>
          <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500 bg-pink-400" style="width:{Math.min(100, blinkRate / 30 * 100)}%"></div>
          </div>
        </div>
      </MetricTooltip>
    </div>
  {/if}
</div>
