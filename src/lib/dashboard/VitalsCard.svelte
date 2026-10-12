<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Vitals a device reported itself, as opposed to the PPG card next door, which
  shows values computed here from an optical waveform.

  A wearable like the Oura ring does its own beat detection, so there is no
  waveform to chart — and it reports one vital at a time, which is why each
  tile is hidden until a value for it has actually arrived rather than showing
  a placeholder zero that reads as a measurement of zero.
-->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";
import CollapsibleSection from "./CollapsibleSection.svelte";
import MetricBar from "./MetricBar.svelte";
import MetricTooltip from "./MetricTooltip.svelte";

interface Props {
  /** Heart rate in bpm; 0 means "not reported yet". */
  hr: number;
  /** Inter-beat interval in ms behind that heart rate. */
  ibi: number;
  /** Blood-oxygen saturation in percent. */
  spo2: number;
  /** Skin temperature in degrees Celsius. */
  temperature: number;
  /** Unix seconds of the last reading; 0 before anything arrives. */
  updatedAt?: number;
}
let { hr, ibi, spo2, temperature, updatedAt = 0 }: Props = $props();

/** Seconds since the last reading, recomputed on each new one. */
const staleFor = $derived(updatedAt > 0 ? Math.max(0, Date.now() / 1000 - updatedAt) : 0);

const items = $derived(
  [
    hr > 0 && {
      k: "hr",
      v: `${hr.toFixed(0)} bpm`,
      c: hr > 100 ? "#ef4444" : hr < 50 ? "#f59e0b" : "#22c55e",
      bar: Math.min(100, ((hr - 40) / 120) * 100),
      bg: "bg-red-400",
    },
    ibi > 0 && {
      k: "ibi",
      v: `${ibi.toFixed(0)} ms`,
      c: "#6b7280",
      bar: Math.min(100, (ibi / 1500) * 100),
      bg: "bg-rose-400",
    },
    spo2 > 0 && {
      k: "spo2",
      v: `${spo2.toFixed(0)}%`,
      c: spo2 > 95 ? "#22c55e" : spo2 > 90 ? "#f59e0b" : "#ef4444",
      bar: Math.min(100, ((spo2 - 70) / 30) * 100),
      bg: "bg-green-500",
    },
    temperature > 0 && {
      k: "skinTemperature",
      v: `${temperature.toFixed(2)} °C`,
      c: "#6b7280",
      bar: Math.min(100, ((temperature - 28) / 12) * 100),
      bg: "bg-orange-400",
    },
  ].filter((x): x is { k: string; v: string; c: string; bar: number; bg: string } => x !== false),
);
</script>

<CollapsibleSection title={t("dashboard.vitals")} dotColor="text-rose-500">
  {#if items.length === 0}
    <!-- A worn ring measures on its own schedule, so an empty card is the
         normal state for the first minute or so, not a fault. -->
    <p class="text-ui-sm text-muted-foreground/70 leading-relaxed">
      {t("dashboard.vitalsWaiting")}
    </p>
  {:else}
    <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-x-2 gap-y-1.5">
      {#each items as item (item.k)}
        <MetricTooltip text={t(`tip.${item.k}`)}>
          <div class="flex flex-col gap-0.5">
            <div class="flex items-center justify-between">
              <span class="text-ui-2xs font-medium text-muted-foreground uppercase tracking-wider"
                >{t(`dashboard.${item.k}`)}</span>
              <span class="text-ui-sm font-bold tabular-nums" style="color:{item.c}">{item.v}</span>
            </div>
            <MetricBar value={item.bar} bg={item.bg} gradient="" />
          </div>
        </MetricTooltip>
      {/each}
    </div>
    {#if staleFor > 120}
      <p class="text-ui-2xs text-muted-foreground/60 pt-1">
        {t("dashboard.vitalsStale")}
      </p>
    {/if}
  {/if}
</CollapsibleSection>
