<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import MetricTooltip from "./MetricTooltip.svelte";

  interface Props { relaxation: number; engagement: number; }
  let { relaxation, engagement }: Props = $props();

  let expanded = $state(true);

  const items = $derived([
    { key: "relaxation", score: relaxation,  color: "#10b981", gradFrom: "#6ee7b7", gradTo: "#10b981", blink: "text-emerald-500",  formula: "α/(β+θ)" },
    { key: "engagement", score: engagement, color: "#f59e0b", gradFrom: "#fcd34d", gradTo: "#f59e0b", blink: "text-amber-500",    formula: "β/(α+θ)" },
  ]);
</script>

<div class="rounded-xl border border-border dark:border-white/[0.04]
            bg-muted dark:bg-[#1a1a28] px-3 py-2 flex flex-col gap-1.5"
     role="group" aria-label="Brain state scores">
  <button class="flex items-center gap-1.5 w-full group" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
         stroke-linecap="round" stroke-linejoin="round"
         class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                transition-transform duration-150 shrink-0 {expanded ? 'rotate-90' : ''}">
      <path d="M9 18l6-6-6-6"/>
    </svg>
    <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground
                 group-hover:text-foreground transition-colors">{t("dashboard.brainState")}</span>
    <span class="text-[0.45rem] text-emerald-500 live-blink shrink-0" aria-hidden="true">●</span>
  </button>

  {#if expanded}
    <div class="grid grid-cols-2 gap-1.5 min-w-0">
      {#each items as m}
        <MetricTooltip text={t(`tip.${m.key}`)}>
          <div class="rounded-lg border border-border/60 dark:border-white/[0.03]
                      bg-background/40 dark:bg-white/[0.02] px-2 py-1.5 flex flex-col gap-1"
               role="meter" aria-label={t(`dashboard.${m.key}`)}
               aria-valuenow={Math.round(m.score)} aria-valuemin={0} aria-valuemax={100}>
            <div class="flex items-center gap-1">
              <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground truncate">
                {t(`dashboard.${m.key}`)}
              </span>
              <span class="text-[0.45rem] {m.blink} live-blink shrink-0" aria-hidden="true">●</span>
            </div>
            <div class="flex items-end gap-1">
              <span class="text-[1.2rem] font-bold tabular-nums leading-none"
                    style="color:{m.score > 60 ? m.color : m.score > 35 ? '#6b7280' : '#94a3b8'}">
                {Math.round(m.score)}
              </span>
              <span class="text-[0.48rem] text-muted-foreground/40 pb-0.5">/100</span>
            </div>
            <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
              <div class="h-full rounded-full transition-all duration-500 ease-out"
                   style="width:{Math.min(100, Math.max(0, m.score))}%;
                          background: linear-gradient(90deg, {m.gradFrom}, {m.gradTo})">
              </div>
            </div>
            <span class="text-[0.42rem] text-muted-foreground/30">{m.formula}</span>
          </div>
        </MetricTooltip>
      {/each}
    </div>
  {/if}
</div>
