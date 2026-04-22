<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";
import CollapsibleSection from "./CollapsibleSection.svelte";
import MetricBar from "./MetricBar.svelte";
import MetricTooltip from "./MetricTooltip.svelte";

interface Props {
  relaxation: number;
  engagement: number;
}
let { relaxation, engagement }: Props = $props();

const items = $derived([
  {
    key: "relaxation",
    score: relaxation,
    color: "#10b981",
    gradFrom: "#6ee7b7",
    gradTo: "#10b981",
    blink: "text-emerald-500",
    formula: "α/(β+θ)",
  },
  {
    key: "engagement",
    score: engagement,
    color: "#f59e0b",
    gradFrom: "#fcd34d",
    gradTo: "#f59e0b",
    blink: "text-amber-500",
    formula: "β/(α+θ)",
  },
]);
</script>

<CollapsibleSection title={t("dashboard.brainState")} dotColor="text-emerald-500"
                    rootAttrs={{ role: "group", "aria-label": "Brain state scores" }}>
  <div class="grid grid-cols-2 gap-1.5 min-w-0">
    {#each items as m}
      <MetricTooltip text={t(`tip.${m.key}`)}>
        <div class="rounded-lg border border-border/60 dark:border-white/[0.04]
                    bg-background/40 dark:bg-white/[0.02] px-2 py-1.5 flex flex-col gap-1"
             role="meter" aria-label={t(`dashboard.${m.key}`)}
             aria-valuenow={Math.round(m.score)} aria-valuemin={0} aria-valuemax={100}>
          <div class="flex items-center gap-1">
            <span class="text-ui-2xs font-semibold tracking-widest uppercase text-muted-foreground truncate">
              {t(`dashboard.${m.key}`)}
            </span>
            <span class="text-[0.45rem] {m.blink} live-blink shrink-0" aria-hidden="true">●</span>
          </div>
          <div class="flex items-end gap-1">
            <span class="text-[1.2rem] font-bold tabular-nums leading-none"
                  style="color:{m.score > 60 ? m.color : m.score > 35 ? '#6b7280' : '#94a3b8'}">
              {Math.round(m.score)}
            </span>
            <span class="text-ui-2xs text-muted-foreground/40 pb-0.5">/100</span>
          </div>
          <MetricBar value={Math.min(100, Math.max(0, m.score))} gradient="linear-gradient(90deg, {m.gradFrom}, {m.gradTo})" />
        </div>
      </MetricTooltip>
    {/each}
  </div>
</CollapsibleSection>
