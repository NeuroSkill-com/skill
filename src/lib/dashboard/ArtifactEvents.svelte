<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import MetricTooltip from "./MetricTooltip.svelte";
  import CollapsibleSection from "./CollapsibleSection.svelte";
  import MetricBar from "./MetricBar.svelte";
  interface Props { blinkCount: number; blinkRate: number; }
  let { blinkCount, blinkRate }: Props = $props();
</script>

<CollapsibleSection title={t("dashboard.artifacts")} dotColor="text-pink-500">
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
        <MetricBar value={Math.min(100, blinkRate / 30 * 100)} bg="bg-pink-400" />
      </div>
    </MetricTooltip>
  </div>
</CollapsibleSection>
