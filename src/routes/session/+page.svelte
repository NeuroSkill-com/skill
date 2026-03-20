<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Standalone Session Detail view — opened from search results or history. -->
<script lang="ts">
  import { onMount }       from "svelte";
  import { invoke }        from "@tauri-apps/api/core";
  import { t }             from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/stores/window-title.svelte";
  import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
  import { SessionDetail } from "$lib/dashboard";
  import { Spinner }       from "$lib/components/ui/spinner";
  import Hypnogram         from "$lib/Hypnogram.svelte";
  import type { SessionMetrics, EpochRow, CsvMetricsResult } from "$lib/dashboard/SessionDetail.svelte";
  import type { SleepStages } from "$lib/types";
  import { analyzeSleep, type SleepAnalysis } from "$lib/sleep-analysis";
  import { fmtTime, fmtDateIso as fmtDate, fmtDuration } from "$lib/format";

  // ── Parse query params ────────────────────────────────────────────────────
  let csvPath = $state("");
  let metrics = $state<SessionMetrics | null>(null);
  let timeseries = $state<EpochRow[] | null>(null);
  let loading = $state(true);
  let error = $state("");
  let sessionMeta = $state<Record<string, any> | null>(null);
  let sleepData = $state<SleepStages | null>(null);
  let sleepAnalysisResult = $state<SleepAnalysis | null>(null);

  onMount(async () => {
    const params = new URLSearchParams(window.location.search);
    csvPath = params.get("csv_path") || "";

    if (!csvPath) {
      error = "No csv_path provided.";
      loading = false;
      return;
    }

    // Try to load session metadata (JSON sidecar)
    try {
      const jsonPath = csvPath.replace(/\.csv$/, ".json");
      // Read the sidecar file via a simple fetch or invoke
      // Since this is a Tauri app, we can try reading it
      // The get_csv_metrics command will give us the data
    } catch (e) { console.warn("[session] read sidecar failed:", e); }

    // Load metrics from CSV
    try {
      const result = await invoke<CsvMetricsResult>("get_csv_metrics", { csvPath });
      if (result && result.n_rows > 0) {
        metrics = result.summary;
        timeseries = result.timeseries;
      }
    } catch (e1) {
      console.warn("[session] CSV metrics failed:", e1);
      // Try SQLite fallback — need start/end UTC from the path
      // Extract date from the csv path to make a rough query
    }

    // Try loading session metadata
    try {
      const sessions = await invoke<Array<Record<string, unknown>>>("list_sessions");
      const match = sessions.find((s) => s.csv_path === csvPath);
      if (match) sessionMeta = match;
    } catch (e) { console.warn("[session] list_sessions failed:", e); }

    loading = false;

    // Load sleep data (non-blocking) if session is long enough (>30min)
    if (sessionMeta?.session_start_utc && sessionMeta?.session_end_utc) {
      const dur = sessionMeta.session_end_utc - sessionMeta.session_start_utc;
      if (dur >= 1800) {
        try {
          const sleep = await invoke<SleepStages>("get_sleep_stages", {
            startUtc: sessionMeta.session_start_utc,
            endUtc: sessionMeta.session_end_utc,
          });
          if (sleep && sleep.epochs.length > 0) {
            sleepData = sleep;
            sleepAnalysisResult = analyzeSleep(sleep);
          }
        } catch { /* no sleep data available */ }
      }
    }
  });

  useWindowTitle("window.title.session");
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden">

  <!-- ── Content ──────────────────────────────────────────────────────────── -->
  <div class="flex-1 overflow-y-auto min-h-0 p-4">
    {#if error}
      <div class="flex items-center justify-center h-full">
        <p class="text-[0.78rem] text-destructive">{error}</p>
      </div>
    {:else if loading}
      <div class="flex items-center justify-center h-full gap-2 text-muted-foreground">
        <Spinner size="w-4 h-4" />
        <span class="text-[0.78rem]">{t("session.loading")}</span>
      </div>
    {:else}
      <!-- Session metadata header -->
      {#if sessionMeta}
        <div class="mb-4 rounded-xl border border-border dark:border-white/[0.06]
                    bg-white dark:bg-[#14141e] p-3">
          <div class="flex flex-wrap gap-x-6 gap-y-1.5">
            {#if sessionMeta.device_name}
              <div class="flex flex-col gap-0.5">
                <span class="text-[0.42rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.device")}</span>
                <span class="text-[0.65rem] font-medium">{sessionMeta.device_name}</span>
              </div>
            {/if}
            {#if sessionMeta.session_start_utc}
              <div class="flex flex-col gap-0.5">
                <span class="text-[0.42rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.startTime")}</span>
                <span class="text-[0.65rem] font-medium tabular-nums">
                  {fmtDate(sessionMeta.session_start_utc)} {fmtTime(sessionMeta.session_start_utc)}
                </span>
              </div>
            {/if}
            {#if sessionMeta.session_duration_s}
              <div class="flex flex-col gap-0.5">
                <span class="text-[0.42rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.duration")}</span>
                <span class="text-[0.65rem] font-medium">{fmtDuration(sessionMeta.session_duration_s)}</span>
              </div>
            {/if}
            {#if sessionMeta.firmware_version}
              <div class="flex flex-col gap-0.5">
                <span class="text-[0.42rem] text-muted-foreground/60 uppercase tracking-wider">Firmware</span>
                <span class="text-[0.65rem] font-medium">{sessionMeta.firmware_version}</span>
              </div>
            {/if}
            {#if sessionMeta.total_samples}
              <div class="flex flex-col gap-0.5">
                <span class="text-[0.42rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.samples")}</span>
                <span class="text-[0.65rem] font-medium tabular-nums">{sessionMeta.total_samples.toLocaleString()}</span>
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <SessionDetail
        {metrics}
        {timeseries}
        loading={false} />

      <!-- Sleep analysis (if session ≥30min) -->
      {#if sleepData && sleepAnalysisResult}
        {@const sa = sleepAnalysisResult}
        <div class="mt-4 flex flex-col gap-3">
          <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("sleep.title")}
          </span>

          <!-- Stats row -->
          <div class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] px-3.5 py-3">
            <div class="flex items-center gap-4 flex-wrap">
              <div class="flex flex-col">
                <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">Efficiency</span>
                <span class="text-[0.82rem] font-bold tabular-nums {sa.efficiency >= 85 ? 'text-emerald-500' : sa.efficiency >= 70 ? 'text-yellow-500' : 'text-red-400'}">
                  {sa.efficiency.toFixed(0)}%
                </span>
              </div>
              <div class="flex flex-col">
                <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">Onset</span>
                <span class="text-[0.82rem] font-bold tabular-nums">{sa.onsetLatencyMin.toFixed(0)}m</span>
              </div>
              {#if sa.remLatencyMin >= 0}
                <div class="flex flex-col">
                  <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">→ REM</span>
                  <span class="text-[0.82rem] font-bold tabular-nums">{sa.remLatencyMin.toFixed(0)}m</span>
                </div>
              {/if}
              <div class="flex flex-col">
                <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">Duration</span>
                <span class="text-[0.82rem] font-bold tabular-nums">{sa.totalMin.toFixed(0)}m</span>
              </div>
              <div class="flex flex-col">
                <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">Awakenings</span>
                <span class="text-[0.82rem] font-bold tabular-nums">{sa.awakenings}</span>
              </div>
            </div>
            <!-- Stage minutes -->
            <div class="flex items-center gap-3 mt-2 pt-2 border-t border-border/50 dark:border-white/[0.04] flex-wrap">
              {#each [
                { label: "Wake", min: sa.stageMinutes.wake, color: "#f59e0b" },
                { label: "N1",   min: sa.stageMinutes.n1,   color: "#38bdf8" },
                { label: "N2",   min: sa.stageMinutes.n2,   color: "#3b82f6" },
                { label: "N3",   min: sa.stageMinutes.n3,   color: "#6366f1" },
                { label: "REM",  min: sa.stageMinutes.rem,  color: "#a855f7" },
              ] as stage}
                <div class="flex items-center gap-1">
                  <span class="w-2 h-2 rounded-full shrink-0" style="background:{stage.color}"></span>
                  <span class="text-[0.55rem] font-medium" style="color:{stage.color}">{stage.label}</span>
                  <span class="text-[0.55rem] tabular-nums text-muted-foreground">{stage.min.toFixed(0)}m</span>
                </div>
              {/each}
            </div>
          </div>

          <!-- Hypnogram -->
          <div class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] p-2">
            <Hypnogram epochs={sleepData.epochs} summary={sleepData.summary} />
          </div>
        </div>
      {/if}
    {/if}
  </div>
  <DisclaimerFooter />
</main>
