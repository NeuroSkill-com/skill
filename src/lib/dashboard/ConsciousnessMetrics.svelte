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
  lzc: number; // Lempel-Ziv Complexity proxy (0–100)
  wakefulness: number; // Wakefulness level (0–100)
  integration: number; // Information Integration (0–100)
}
let { lzc, wakefulness, integration }: Props = $props();

const items = $derived([
  {
    k: "lzc",
    v: lzc,
    c: lzc > 60 ? "#22c55e" : lzc > 35 ? "#f59e0b" : "#6b7280",
    grad: "linear-gradient(90deg,#67e8f9,var(--color-violet-400),var(--color-violet-500))",
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

<CollapsibleSection title={t("dashboard.consciousness")} dotColor="text-violet-400">
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
          <MetricBar value={Math.min(100, Math.max(0, item.v))} gradient={item.grad} height="h-1.5" />
          <span class="text-[0.40rem] text-muted-foreground/30">/100</span>
        </div>
      </MetricTooltip>
    {/each}
  </div>
</CollapsibleSection>
