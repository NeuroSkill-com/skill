<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";
import CollapsibleSection from "./CollapsibleSection.svelte";
import MetricTooltip from "./MetricTooltip.svelte";

interface Props {
  faa: number;
}
let { faa }: Props = $props();
</script>

<CollapsibleSection title={t("dashboard.faa")} dotColor="text-violet-500"
                    rootAttrs={{ role: "meter", "aria-label": t("dashboard.faa"),
                                 "aria-valuenow": Math.round(faa * 1000) / 1000, "aria-valuemin": -1, "aria-valuemax": 1 }}>
    <div class="flex items-center justify-end -mt-1 mb-0.5">
      <span class="text-[0.65rem] font-bold tabular-nums shrink-0"
            style="color:{Math.abs(faa) > 0.3 ? 'var(--color-violet-500)' : '#6b7280'}">
        {faa >= 0 ? "+" : ""}{faa.toFixed(3)}
      </span>
    </div>
    <MetricTooltip text={t("tip.faa")}>
      <div class="flex flex-col gap-1">
        <div class="relative h-1.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
          <div class="absolute left-1/2 top-0 w-px h-full bg-muted-foreground/20"></div>
          {#if faa >= 0}
            <div class="absolute top-0 h-full rounded-full transition-all duration-500 ease-out"
                 style="left:50%; width:{Math.min(50, Math.abs(faa) * 50)}%;
                        background: linear-gradient(90deg, var(--color-violet-400), var(--color-violet-500))"></div>
          {:else}
            <div class="absolute top-0 h-full rounded-full transition-all duration-500 ease-out"
                 style="right:50%; width:{Math.min(50, Math.abs(faa) * 50)}%;
                        background: linear-gradient(270deg, var(--color-violet-400), var(--color-violet-500))"></div>
          {/if}
        </div>
        <div class="flex justify-between text-[0.42rem] text-muted-foreground/30">
          <span>{t("dashboard.faaWithdrawal")}</span>
          <span>{t("dashboard.faaFormula")}</span>
          <span>{t("dashboard.faaApproach")}</span>
        </div>
      </div>
    </MetricTooltip>
</CollapsibleSection>
