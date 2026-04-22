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
  meditation: number;
  cognitiveLoad: number;
  drowsiness: number;
}
let { meditation, cognitiveLoad, drowsiness }: Props = $props();
</script>

<CollapsibleSection title={t("dashboard.compositeScores")} dotColor="text-violet-600 dark:text-violet-400">
  <div class="grid grid-cols-3 gap-x-2 gap-y-1.5">
    {#each [
      { k: "meditation",    v: meditation,    c: meditation>60?'#22c55e':meditation>30?'#f59e0b':'#6b7280', grad: 'linear-gradient(90deg,var(--color-violet-400),var(--color-violet-500))' },
      { k: "cognitiveLoad", v: cognitiveLoad, c: cognitiveLoad>70?'#ef4444':cognitiveLoad>40?'#f59e0b':'#22c55e', grad: 'linear-gradient(90deg,#38bdf8,#3b82f6)' },
      { k: "drowsiness",    v: drowsiness,    c: drowsiness>60?'#ef4444':drowsiness>30?'#f59e0b':'#22c55e', grad: 'linear-gradient(90deg,#fbbf24,#f59e0b,#ef4444)' },
    ] as item}
      <MetricTooltip text={t(`tip.${item.k}`)}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t(`dashboard.${item.k}`)}</span>
            <span class="text-ui-sm font-bold tabular-nums" style="color:{item.c}">{item.v.toFixed(0)}</span>
          </div>
          <MetricBar value={item.v} gradient={item.grad} height="h-1.5" />
        </div>
      </MetricTooltip>
    {/each}
  </div>
</CollapsibleSection>
