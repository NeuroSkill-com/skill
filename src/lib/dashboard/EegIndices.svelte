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
  tar: number;
  bar: number;
  dtr: number;
  pse: number;
  apf: number;
  mood: number;
  bps: number;
  snr: number;
  coherence: number;
  mu: number;
  tbr: number;
  sef95: number;
  sc: number;
  ha: number;
  hm: number;
  hc: number;
  pe: number;
  hfd: number;
  dfa: number;
  se: number;
  pac: number;
  lat: number;
  headache: number;
  migraine: number;
  /**
   * Show the Mu Suppression metric.
   * Set to `false` for devices without central electrodes (e.g. Muse),
   * where mu-rhythm measurement is not meaningful.
   * Defaults to `true` for unknown / future devices.
   */
  showMu?: boolean;
}
let {
  tar,
  bar,
  dtr,
  pse,
  apf,
  mood,
  bps,
  snr,
  coherence,
  mu,
  tbr,
  sef95,
  sc,
  ha,
  hm,
  hc,
  pe,
  hfd,
  dfa,
  se,
  pac,
  lat,
  headache,
  migraine,
  showMu = true,
}: Props = $props();
</script>

<CollapsibleSection title={t("dashboard.indices")} dotColor="text-cyan-500">
    <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-x-3 gap-y-1.5">
      {#each [
        { k: "tar", v: tar.toFixed(2), c: tar > 1.5 ? '#f59e0b':'#6b7280', bar: Math.min(100,tar*33), bg:'bg-amber-400' },
        { k: "bar", v: bar.toFixed(2), c: bar > 1.5 ? '#3b82f6':'#6b7280', bar: Math.min(100,bar*33), bg:'bg-blue-500' },
        { k: "dtr", v: dtr.toFixed(2), c: dtr > 2 ? '#8b5cf6':'#6b7280', bar: Math.min(100,dtr*25), bg:'bg-violet-500' },
        { k: "pse", v: pse.toFixed(3), c: '#6b7280', bar: pse*100, bg:'bg-teal-500' },
        { k: "apf", v: apf.toFixed(1)+' Hz', c: '#6b7280', bar: Math.min(100,(apf-7)*20), bg:'bg-green-500' },
        { k: "mood", v: mood.toFixed(0), c: mood>60?'#22c55e':mood<40?'#ef4444':'#6b7280', bar: mood, bg:'', grad:'linear-gradient(90deg,#ef4444,#f59e0b,#22c55e)' },
        { k: "bps", v: bps.toFixed(2), c: '#6b7280' },
        { k: "snr", v: snr.toFixed(1)+' dB', c: snr>10?'#22c55e':snr<3?'#ef4444':'#f59e0b' },
        { k: "coherence", v: coherence.toFixed(3), c: '#6b7280' },
        ...(showMu ? [{ k: "muSuppression", v: mu.toFixed(3), c: mu<0.8?'var(--color-violet-500)':'#6b7280' }] : []),
        { k: "tbr", v: tbr.toFixed(2), c: tbr>3?'#ef4444':tbr>2?'#f59e0b':'#6b7280', bar: Math.min(100,tbr*20), bg:'bg-rose-400' },
        { k: "sef95", v: sef95.toFixed(1)+' Hz', c: '#6b7280', bar: Math.min(100,sef95/128*100), bg:'bg-sky-400' },
        { k: "spectralCentroid", v: sc.toFixed(1)+' Hz', c: '#6b7280', bar: Math.min(100,sc/60*100), bg:'bg-cyan-500' },
        { k: "hjorthActivity", v: ha.toFixed(1), c: '#6b7280' },
        { k: "hjorthMobility", v: hm.toFixed(3), c: '#6b7280' },
        { k: "hjorthComplexity", v: hc.toFixed(3), c: '#6b7280' },
        { k: "permEntropy", v: pe.toFixed(3), c: '#6b7280', bar: pe*100, bg:'bg-pink-500' },
        { k: "higuchiFd", v: hfd.toFixed(3), c: '#6b7280', bar: Math.min(100,hfd/2*100), bg:'bg-orange-400' },
        { k: "dfaExponent", v: dfa.toFixed(3), c: dfa>0.5&&dfa<1.0?'#22c55e':'#f59e0b', bar: Math.min(100,dfa/1.5*100), bg:'bg-lime-500' },
        { k: "sampleEntropy", v: se.toFixed(3), c: '#6b7280' },
        { k: "pacThetaGamma", v: pac.toFixed(3), c: pac>0.5?'var(--color-violet-500)':'#6b7280', bar: pac*100, bg:'bg-violet-500' },
        { k: "lateralityIndex", v: (lat>=0?'+':'')+lat.toFixed(3), c: '#6b7280' },
        { k: "headache", v: headache.toFixed(0), c: headache>60?'#f43f5e':headache>30?'#f59e0b':'#22c55e', bar: Math.min(100,headache), grad:'linear-gradient(90deg,#f87171,#ef4444)' },
        { k: "migraine", v: migraine.toFixed(0), c: migraine>60?'#f43f5e':migraine>30?'#f59e0b':'#22c55e', bar: Math.min(100,migraine), grad:'linear-gradient(90deg,#fb7185,#f43f5e)' },
      ] as item}
        <MetricTooltip text={t(`tip.${item.k}`)}>
          <div class="flex flex-col gap-0.5 min-w-0">
            <div class="flex items-center justify-between gap-1 min-w-0">
              <span class="text-[0.42rem] font-medium text-muted-foreground uppercase tracking-wider truncate min-w-0">{t(`dashboard.${item.k}`)}</span>
              <span class="text-ui-sm font-bold tabular-nums shrink-0" style="color:{item.c}">{item.v}</span>
            </div>
            {#if item.bar !== undefined}
              <div class="h-1 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
                <div class="h-full rounded-full transition-all duration-500 {item.bg ?? ''}"
                     style="width:{item.bar}%; {item.grad ? `background:${item.grad}` : ''}"></div>
              </div>
            {/if}
          </div>
        </MetricTooltip>
      {/each}
    </div>
</CollapsibleSection>
