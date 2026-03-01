<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import MetricTooltip from "./MetricTooltip.svelte";
  interface Props { meditation: number; cognitiveLoad: number; drowsiness: number; }
  let { meditation, cognitiveLoad, drowsiness }: Props = $props();

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
                 group-hover:text-foreground transition-colors">{t("dashboard.compositeScores")}</span>
    <span class="text-[0.45rem] text-violet-500 live-blink shrink-0" aria-hidden="true">●</span>
  </button>

  {#if expanded}
    <div class="grid grid-cols-3 gap-x-2 gap-y-1.5">
      {#each [
        { k: "meditation",    v: meditation,    c: meditation>60?'#22c55e':meditation>30?'#f59e0b':'#6b7280', grad: 'linear-gradient(90deg,#a78bfa,#8b5cf6)' },
        { k: "cognitiveLoad", v: cognitiveLoad, c: cognitiveLoad>70?'#ef4444':cognitiveLoad>40?'#f59e0b':'#22c55e', grad: 'linear-gradient(90deg,#38bdf8,#3b82f6)' },
        { k: "drowsiness",    v: drowsiness,    c: drowsiness>60?'#ef4444':drowsiness>30?'#f59e0b':'#22c55e', grad: 'linear-gradient(90deg,#fbbf24,#f59e0b,#ef4444)' },
      ] as item}
        <MetricTooltip text={t(`tip.${item.k}`)}>
          <div class="flex flex-col gap-0.5">
            <div class="flex items-center justify-between">
              <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t(`dashboard.${item.k}`)}</span>
              <span class="text-[0.58rem] font-bold tabular-nums" style="color:{item.c}">{item.v.toFixed(0)}</span>
            </div>
            <div class="h-1.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
              <div class="h-full rounded-full transition-all duration-500" style="width:{item.v}%; background:{item.grad}"></div>
            </div>
          </div>
        </MetricTooltip>
      {/each}
    </div>
  {/if}
</div>
