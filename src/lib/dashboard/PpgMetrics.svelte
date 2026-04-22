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
  hr: number;
  rmssd: number;
  sdnn: number;
  pnn50: number;
  lfHf: number;
  respRate: number;
  spo2: number;
  perfIdx: number;
  stressIdx: number;
}
let { hr, rmssd, sdnn, pnn50, lfHf, respRate, spo2, perfIdx, stressIdx }: Props = $props();
</script>

<CollapsibleSection title={t("dashboard.ppgMetrics")} dotColor="text-red-500">
  <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-x-2 gap-y-1.5">
    {#each [
      { k:"hr",              v:hr.toFixed(0)+' bpm',   c:hr>100?'#ef4444':hr<50?'#f59e0b':'#22c55e', bar:Math.min(100,(hr-40)/120*100), bg:'bg-red-400' },
      { k:"rmssd",           v:rmssd.toFixed(1)+' ms', c:rmssd>50?'#22c55e':rmssd<20?'#ef4444':'#f59e0b', bar:Math.min(100,rmssd/100*100), bg:'bg-emerald-500' },
      { k:"sdnn",            v:sdnn.toFixed(1)+' ms',  c:'#6b7280', bar:Math.min(100,sdnn/100*100), bg:'bg-teal-500' },
      { k:"pnn50",           v:pnn50.toFixed(1)+'%',   c:'#6b7280', bar:pnn50, bg:'bg-indigo-400' },
      { k:"lfHfRatio",       v:lfHf.toFixed(2),        c:lfHf>2?'#ef4444':lfHf<0.5?'#22c55e':'#6b7280', bar:Math.min(100,lfHf/4*100), bg:'bg-amber-500' },
      { k:"respiratoryRate", v:respRate.toFixed(1)+' bpm', c:'#6b7280', bar:Math.min(100,respRate/30*100), bg:'bg-sky-400' },
      { k:"spo2",            v:spo2.toFixed(1)+'%',    c:spo2>95?'#22c55e':spo2>90?'#f59e0b':'#ef4444', bar:Math.min(100,(spo2-70)/30*100), bg:'bg-green-500' },
      { k:"perfusionIndex",  v:perfIdx.toFixed(2)+'%', c:perfIdx>1?'#22c55e':perfIdx>0.3?'#f59e0b':'#ef4444', bar:Math.min(100,perfIdx/5*100), bg:'bg-violet-500/60' },
      { k:"stressIndex",     v:stressIdx.toFixed(0),   c:stressIdx>200?'#ef4444':stressIdx>100?'#f59e0b':'#22c55e', bar:Math.min(100,stressIdx/300*100), bg:'', grad:'linear-gradient(90deg,#22c55e,#f59e0b,#ef4444)' },
    ] as item}
      <MetricTooltip text={t(`tip.${item.k}`)}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider">{t(`dashboard.${item.k}`)}</span>
            <span class="text-ui-sm font-bold tabular-nums" style="color:{item.c}">{item.v}</span>
          </div>
          <MetricBar value={item.bar} bg={item.bg ?? ''} gradient={item.grad ?? ''} />
        </div>
      </MetricTooltip>
    {/each}
  </div>
</CollapsibleSection>
