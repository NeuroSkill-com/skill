<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Electrodermal activity (skin conductance), live.

  Two layers, which arrive on different schedules: the raw trace (about 15 Hz,
  charted) and the metrics the daemon derives from it once per epoch — the
  tonic level, responses per minute and their size, and arousal against this
  session's own baseline.

  Each tile is hidden until its value exists, rather than showing a zero that
  reads as a measurement of zero. Arousal in particular is blank for the first
  half-minute while the baseline is learned, and says so.
-->
<script lang="ts">
import { t } from "#lib/i18n/index.svelte";
import CollapsibleSection from "./CollapsibleSection.svelte";
import {
  AROUSAL_COLORS,
  arousalBar,
  arousalLevel,
  type EdaTrace,
  EMPTY_TRACE,
  POOR_CONTACT_FRACTION,
  rejectedPercent,
} from "./eda-logic";
import MetricBar from "./MetricBar.svelte";
import MetricTooltip from "./MetricTooltip.svelte";
import TimeSeriesChart from "./TimeSeriesChart.svelte";

interface Props {
  /** Tonic level in µS; 0 means "not reported yet". */
  scl: number;
  /** Responses per minute, once there is enough data to say. */
  scrPerMin: number | null;
  /** Mean response amplitude in µS, when any occurred in the last minute. */
  scrAmplitude: number | null;
  /** Arousal z-score, once the baseline is learned. */
  arousalZ: number | null;
  /** Fraction of recent samples that passed the artifact gate, 0..1. */
  validFraction?: number;
  /** Recent raw samples for the chart. */
  trace?: EdaTrace;
  /** Unix seconds of the last metrics epoch; 0 before anything arrives. */
  updatedAt?: number;
}
let { scl, scrPerMin, scrAmplitude, arousalZ, validFraction = 1, trace = EMPTY_TRACE, updatedAt = 0 }: Props = $props();

/** Seconds since the last epoch, recomputed on each new one. */
const staleFor = $derived(updatedAt > 0 ? Math.max(0, Date.now() / 1000 - updatedAt) : 0);
const level = $derived(arousalLevel(arousalZ));
const poorContact = $derived(updatedAt > 0 && validFraction < POOR_CONTACT_FRACTION);

const tiles = $derived(
  [
    scl > 0 && {
      k: "scl",
      v: `${scl.toFixed(2)} µS`,
      c: "#6b7280",
      bar: Math.min(100, (scl / 20) * 100),
      bg: "bg-sky-400",
    },
    scrPerMin != null && {
      k: "scrRate",
      v: scrPerMin.toFixed(1),
      c: "#6b7280",
      bar: Math.min(100, (scrPerMin / 20) * 100),
      bg: "bg-cyan-400",
    },
    scrAmplitude != null && {
      k: "scrAmplitude",
      v: `${scrAmplitude.toFixed(2)} µS`,
      c: "#6b7280",
      bar: Math.min(100, (scrAmplitude / 1) * 100),
      bg: "bg-teal-400",
    },
  ].filter((x): x is { k: string; v: string; c: string; bar: number; bg: string } => x !== false),
);
</script>

<CollapsibleSection title={t("dashboard.eda")} dotColor="text-sky-500">
  {#if scl <= 0 && trace.t.length === 0}
    <p class="text-ui-sm text-muted-foreground/70 leading-relaxed">
      {t("dashboard.edaWaiting")}
    </p>
  {:else}
    {#if trace.t.length > 1}
      <TimeSeriesChart
        series={[{ key: "eda", label: "EDA", color: "#0ea5e9", data: trace.v }]}
        timestamps={trace.t}
        height={90}
        yLabel="µS" />
    {/if}

    <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-x-2 gap-y-1.5 pt-1">
      {#each tiles as item (item.k)}
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

      <!-- Arousal is always shown once data flows, because "still learning" is
           itself the useful answer for the first half-minute. -->
      <MetricTooltip text={t("tip.arousal")}>
        <div class="flex flex-col gap-0.5">
          <div class="flex items-center justify-between">
            <span class="text-ui-2xs font-medium text-muted-foreground uppercase tracking-wider"
              >{t("dashboard.arousal")}</span>
            <span class="text-ui-sm font-bold" style="color:{AROUSAL_COLORS[level]}"
              >{t(`dashboard.arousal${level[0].toUpperCase()}${level.slice(1)}`)}</span>
          </div>
          <MetricBar value={arousalBar(arousalZ)} bg="bg-amber-400" gradient="" />
        </div>
      </MetricTooltip>
    </div>

    {#if poorContact}
      <p class="text-ui-2xs text-amber-500 pt-1">
        {t("dashboard.edaContactPoor").replace("{pct}", String(rejectedPercent(validFraction)))}
      </p>
    {:else if staleFor > 60}
      <p class="text-ui-2xs text-muted-foreground/60 pt-1">
        {t("dashboard.edaStale")}
      </p>
    {/if}
  {/if}
</CollapsibleSection>
