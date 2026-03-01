<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import MetricTooltip from "./MetricTooltip.svelte";

  interface Props {
    lzc:         number;   // Lempel-Ziv Complexity proxy (0–100)
    wakefulness: number;   // Wakefulness level (0–100)
    integration: number;   // Information Integration (0–100)
  }
  let { lzc, wakefulness, integration }: Props = $props();

  let expanded = $state(true);

  const items = $derived([
    {
      k: "lzc",
      v: lzc,
      c: lzc > 60 ? "#22c55e" : lzc > 35 ? "#f59e0b" : "#6b7280",
      grad: "linear-gradient(90deg,#67e8f9,#818cf8,#a855f7)",
    },
    {
      k: "wakefulness",
      v: wakefulness,
      c: wakefulness > 60 ? "#3b82f6" : wakefulness > 35 ? "#f59e0b" : "#6b7280",
      grad: "linear-gradient(90deg,#fbbf24,#f59e0b,#3b82f6)",
    },
    {
      k: "integration",
      v: integration,
      c: integration > 60 ? "#10b981" : integration > 35 ? "#f59e0b" : "#6b7280",
      grad: "linear-gradient(90deg,#6ee7b7,#34d399,#10b981)",
    },
  ]);
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
                 group-hover:text-foreground transition-colors">
      {t("dashboard.consciousness")}
    </span>
    <span class="text-[0.45rem] text-purple-400 live-blink shrink-0" aria-hidden="true">●</span>
  </button>

  {#if expanded}
    <div class="grid grid-cols-3 gap-x-2 gap-y-1.5">
      {#each items as item}
        <MetricTooltip text={t(`tip.consciousness.${item.k}`)}>
          <div class="flex flex-col gap-0.5">
            <div class="flex items-center justify-between">
              <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider truncate">
                {t(`dashboard.consciousness.${item.k}`)}
              </span>
              <span class="text-[0.58rem] font-bold tabular-nums ml-1 shrink-0"
                    style="color:{item.c}">
                {item.v.toFixed(0)}
              </span>
            </div>
            <div class="h-1.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
              <div class="h-full rounded-full transition-all duration-500"
                   style="width:{Math.min(100, Math.max(0, item.v))}%;
                          background:{item.grad}">
              </div>
            </div>
            <span class="text-[0.40rem] text-muted-foreground/30">/100</span>
          </div>
        </MetricTooltip>
      {/each}
    </div>
  {/if}
</div>
