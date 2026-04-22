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
  pitch: number;
  roll: number;
  stillness: number;
  nodCount: number;
  shakeCount: number;
}
let { pitch, roll, stillness, nodCount, shakeCount }: Props = $props();
</script>

<CollapsibleSection title={t("dashboard.headPose")} dotColor="text-sky-500">
    <div class="grid grid-cols-2 sm:grid-cols-5 gap-x-2 gap-y-1.5">
      <MetricTooltip text={t("tip.pitch")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.pitch")}</span>
            <span class="text-ui-sm font-bold tabular-nums">{pitch >= 0 ? "+" : ""}{pitch.toFixed(1)}°</span>
          </div>
          <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden relative">
            <div class="absolute top-0 h-full rounded-full transition-all duration-300 bg-sky-400"
              style="left:{50 + Math.max(-50, Math.min(50, pitch))}%; width:2px"></div>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.roll")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.roll")}</span>
            <span class="text-ui-sm font-bold tabular-nums">{roll >= 0 ? "+" : ""}{roll.toFixed(1)}°</span>
          </div>
          <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden relative">
            <div class="absolute top-0 h-full rounded-full transition-all duration-300 bg-indigo-400"
              style="left:{50 + Math.max(-50, Math.min(50, roll))}%; width:2px"></div>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.stillness")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.stillness")}</span>
            <span class="text-ui-sm font-bold tabular-nums" style="color:{stillness > 80 ? '#22c55e' : stillness > 40 ? '#f59e0b' : '#ef4444'}">{stillness.toFixed(0)}</span>
          </div>
          <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500 bg-emerald-400" style="width:{stillness}%"></div>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.nods")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.nods")}</span>
            <span class="text-ui-sm font-bold tabular-nums">{nodCount}</span>
          </div>
        </div>
      </MetricTooltip>
      <MetricTooltip text={t("tip.shakes")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t("dashboard.shakes")}</span>
            <span class="text-ui-sm font-bold tabular-nums">{shakeCount}</span>
          </div>
        </div>
      </MetricTooltip>
    </div>
</CollapsibleSection>
