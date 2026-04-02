<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- EXG tab — signal processing filters, EEG embedding config, model backend. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Card, CardContent } from "$lib/components/ui/card";
import { DEFAULT_FILTER_CONFIG, EMBEDDING_EPOCH_SECS, EMBEDDING_OVERLAP_SECS } from "$lib/constants";
import EegModelTab from "$lib/EegModelTab.svelte";
import { t } from "$lib/i18n/index.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
type PowerlineFreq = "Hz60" | "Hz50";
interface FilterConfig {
  sample_rate: number;
  low_pass_hz: number | null;
  high_pass_hz: number | null;
  notch: PowerlineFreq | null;
  notch_bandwidth_hz: number;
}

// ── State ──────────────────────────────────────────────────────────────────
let exgInferenceDevice = $state<"gpu" | "cpu">("gpu");
let exgInferenceDeviceSaving = $state(false);
let filter = $state<FilterConfig>({ ...DEFAULT_FILTER_CONFIG });
let filterSaving = $state(false);
let overlapSecs = $state(EMBEDDING_OVERLAP_SECS);
let overlapSaving = $state(false);

// ── Filter ─────────────────────────────────────────────────────────────────
async function applyFilter(patch: Partial<FilterConfig>) {
  filter = { ...filter, ...patch };
  filterSaving = true;
  try {
    await invoke("set_filter_config", { config: filter });
  } finally {
    filterSaving = false;
  }
}
const setNotch = (v: PowerlineFreq | null) => applyFilter({ notch: v });
const setHighPass = (hz: number | null) => applyFilter({ high_pass_hz: hz });
const setLowPass = (hz: number | null) => applyFilter({ low_pass_hz: hz });

// ── Overlap ────────────────────────────────────────────────────────────────
const OVERLAP_PRESETS: [string, number][] = [
  ["0 s — none", 0],
  ["1.25 s — 25%", 1.25],
  ["2.5 s — 50%", 2.5],
  ["3.75 s — 75%", 3.75],
  ["4.5 s — 90%", 4.5],
];

async function setOverlap(secs: number) {
  overlapSecs = secs;
  overlapSaving = true;
  try {
    await invoke("set_embedding_overlap", { overlapSecs: secs });
  } finally {
    overlapSaving = false;
  }
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
async function setExgInferenceDevice(dev: "gpu" | "cpu") {
  if (exgInferenceDevice === dev || exgInferenceDeviceSaving) return;
  exgInferenceDeviceSaving = true;
  exgInferenceDevice = dev;
  await invoke("set_exg_inference_device", { device: dev }).catch(() => {});
  exgInferenceDeviceSaving = false;
}

onMount(async () => {
  filter = await invoke<FilterConfig>("get_filter_config");
  overlapSecs = await invoke<number>("get_embedding_overlap");
  exgInferenceDevice = (await invoke<string>("get_exg_inference_device").catch(() => "gpu")) as "gpu" | "cpu";
});
</script>

<section class="flex flex-col gap-4">

  <!-- ── Inference Device ──────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("settings.inferenceDevice")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
        <p class="px-4 py-2.5 text-[0.62rem] text-muted-foreground leading-relaxed">
          {t("settings.inferenceDeviceDesc")}
        </p>
        <div class="flex items-stretch divide-x divide-border dark:divide-white/[0.05]">
          {#each (["gpu", "cpu"] as ("gpu" | "cpu")[]) as dev}
            {@const isActive = exgInferenceDevice === dev}
            <button
              onclick={() => setExgInferenceDevice(dev)}
              class="flex-1 flex flex-col gap-0.5 items-start px-4 py-3 text-left transition-colors cursor-pointer
                     {isActive
                       ? 'bg-violet-50 dark:bg-violet-500/[0.08]'
                       : 'hover:bg-slate-50 dark:hover:bg-white/[0.02]'}">
              <div class="flex items-center gap-2">
                <span
                  class="text-[0.72rem] font-semibold
                         {isActive ? 'text-violet-600 dark:text-violet-400' : 'text-foreground'}">
                  {dev === "gpu" ? t("settings.inferenceDeviceGpu") : t("settings.inferenceDeviceCpu")}
                </span>
                {#if isActive}
                  <span class="text-[0.52rem] font-bold tracking-widest uppercase text-violet-500">Active</span>
                {/if}
                {#if exgInferenceDeviceSaving && isActive}
                  <span class="text-[0.52rem] text-muted-foreground">saving…</span>
                {/if}
              </div>
              <span class="text-[0.6rem] text-muted-foreground leading-snug">
                {dev === "gpu" ? t("settings.inferenceDeviceGpuDesc") : t("settings.inferenceDeviceCpuDesc")}
              </span>
            </button>
          {/each}
        </div>
        <p class="px-4 py-2 text-[0.58rem] text-amber-500/80 dark:text-amber-400/70">
          ⚠️ {t("settings.inferenceDeviceExgRestartHint")}
        </p>
      </CardContent>
    </Card>
  </section>

  <!-- ── EXG Model (encoder, backend, HNSW, re-embed) ──────────────────────── -->
  <EegModelTab />

  <!-- ── Signal Processing ──────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("settings.signalProcessing")}
      </span>
      {#if filterSaving}
        <span class="text-[0.56rem] text-muted-foreground">{t("common.saving")}</span>
      {/if}
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

        <!-- Powerline notch -->
        <div class="flex flex-col gap-2.5 px-4 py-4">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.powerlineNotch")}</span>
          <div class="flex gap-2">
            {#each ([["Hz60","🇺🇸",t("settings.us60Hz"),t("settings.us60HzSub")],["Hz50","🇪🇺",t("settings.eu50Hz"),t("settings.eu50HzSub")]] as const) as [val, flag, label, sub]}
              <button onclick={() => setNotch(val)}
                class="flex flex-col items-center gap-1 rounded-xl border px-3 py-2.5 flex-1
                       transition-all cursor-pointer select-none
                       {filter.notch === val
                         ? 'border-primary/50 bg-primary/10'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                <span class="text-[1rem]">{flag}</span>
                <span class="text-[0.7rem] font-semibold leading-tight
                             {filter.notch === val ? 'text-primary' : 'text-foreground'}">
                  {label}
                </span>
                <span class="text-[0.58rem] text-muted-foreground">{sub}</span>
                {#if filter.notch === val}
                  <span class="text-[0.52rem] font-bold tracking-widest uppercase text-primary mt-0.5">{t("common.active")}</span>
                {/if}
              </button>
            {/each}

            <button onclick={() => setNotch(null)}
              class="flex flex-col items-center gap-1 rounded-xl border px-3 py-2.5 flex-1
                     transition-all cursor-pointer select-none
                     {filter.notch === null
                       ? 'border-slate-400/40 bg-slate-100 dark:bg-white/[0.05]'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              <span class="text-[1rem]">🔕</span>
              <span class="text-[0.7rem] font-semibold text-muted-foreground leading-tight">{t("common.off")}</span>
              <span class="text-[0.58rem] text-muted-foreground">{t("settings.noNotch")}</span>
              {#if filter.notch === null}
                <span class="text-[0.52rem] font-bold tracking-widest uppercase text-slate-500 mt-0.5">{t("common.active")}</span>
              {/if}
            </button>
          </div>
        </div>

        <!-- High-pass -->
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.highPassCutoff")}</span>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each ([null, 0.5, 1, 4, 8] as const) as hz}
              <button onclick={() => setHighPass(hz)}
                class="rounded-lg border px-3 py-1.5 text-[0.68rem] font-semibold
                       transition-all cursor-pointer select-none
                       {filter.high_pass_hz === hz
                         ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                {hz === null ? t("common.off") : `${hz} Hz`}
              </button>
            {/each}
          </div>
        </div>

        <!-- Low-pass -->
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.lowPassCutoff")}</span>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each ([null, 30, 50, 100] as const) as hz}
              <button onclick={() => setLowPass(hz)}
                class="rounded-lg border px-3 py-1.5 text-[0.68rem] font-semibold
                       transition-all cursor-pointer select-none
                       {filter.low_pass_hz === hz
                         ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                {hz === null ? t("common.off") : `${hz} Hz`}
              </button>
            {/each}
          </div>
        </div>

        <!-- Pipeline summary -->
        <div class="flex items-center gap-2 flex-wrap px-4 py-3 bg-slate-50 dark:bg-[#111118]">
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
            {t("settings.pipeline")}
          </span>
          {#if filter.high_pass_hz !== null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
              HP {filter.high_pass_hz} Hz
            </Badge>
          {/if}
          {#if filter.low_pass_hz !== null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
              LP {filter.low_pass_hz} Hz
            </Badge>
          {/if}
          {#if filter.notch !== null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
              Notch {filter.notch === "Hz60" ? "60+120 Hz" : "50+100 Hz"}
            </Badge>
          {/if}
          {#if filter.high_pass_hz === null && filter.low_pass_hz === null && filter.notch === null}
            <Badge variant="outline"
              class="text-[0.56rem] py-0 px-1.5 bg-slate-500/10 text-slate-500 border-slate-500/20">
              {t("settings.passthrough")}
            </Badge>
          {/if}
          <span class="ml-auto text-[0.56rem] text-muted-foreground/60 shrink-0">{t("settings.gpuLatency")}</span>
        </div>

      </CardContent>
    </Card>
  </div>

  <!-- ── EEG Embedding ───────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("settings.eegEmbedding")}
      </span>
      {#if overlapSaving}
        <span class="text-[0.56rem] text-muted-foreground">saving…</span>
      {/if}
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

        <div class="flex flex-col gap-2 px-4 py-3.5">
          <div class="flex items-baseline justify-between">
            <span class="text-[0.78rem] font-semibold text-foreground">{t("settings.epochOverlap")}</span>
            <span class="text-[0.68rem] text-muted-foreground">
              {t("settings.everyNSecs", { n: (EMBEDDING_EPOCH_SECS - overlapSecs).toFixed(2).replace(/\.?0+$/, "") })}
            </span>
          </div>
          <p class="text-[0.68rem] text-muted-foreground leading-relaxed -mt-0.5">
            {t("settings.overlapDescription")}
          </p>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each OVERLAP_PRESETS as [label, val]}
              <button
                onclick={() => setOverlap(val)}
                class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                       transition-all cursor-pointer select-none
                       {overlapSecs === val
                         ? 'border-primary/50 bg-primary/10 text-primary'
                         : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                {label}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex items-center gap-2 flex-wrap px-4 py-3 bg-slate-50 dark:bg-[#111118]">
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
            Pipeline
          </span>
          <Badge variant="outline"
            class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
            {EMBEDDING_EPOCH_SECS} s window
          </Badge>
          <Badge variant="outline"
            class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
            {overlapSecs} s overlap
          </Badge>
          <Badge variant="outline"
            class="text-[0.56rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
            {Math.round(overlapSecs / EMBEDDING_EPOCH_SECS * 100)}% shared
          </Badge>
          <span class="ml-auto text-[0.56rem] text-muted-foreground/60 shrink-0">wgpu</span>
        </div>

      </CardContent>
    </Card>
  </div>


</section>
