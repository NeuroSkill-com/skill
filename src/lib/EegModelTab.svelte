<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- EEG Model tab — Encoder status · HNSW index · Model source -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { relaunch }           from "@tauri-apps/plugin-process";
  import { Badge }              from "$lib/components/ui/badge";
  import { Button }             from "$lib/components/ui/button";
  import { Card, CardContent }  from "$lib/components/ui/card";
  import { t }                  from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────
  interface EegModelConfig {
    hf_repo:              string;
    hnsw_m:               number;
    hnsw_ef_construction: number;
    data_norm:            number;
  }
  interface EegModelStatus {
    encoder_loaded:         boolean;
    encoder_describe:       string | null;
    weights_found:          boolean;
    weights_path:           string | null;
    embeddings_today:       number;
    daily_db_path:          string;
    daily_hnsw_path:        string;
    downloading_weights:    boolean;
    download_status_msg:    string | null;
    download_needs_restart: boolean;
    download_retry_attempt: number;
    download_retry_in_secs: number;
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let modelConfig = $state<EegModelConfig>({
    hf_repo: "Zyphra/ZUNA", hnsw_m: 16, hnsw_ef_construction: 200, data_norm: 10,
  });
  let modelStatus = $state<EegModelStatus>({
    encoder_loaded: false, encoder_describe: null,
    weights_found: false, weights_path: null,
    embeddings_today: 0, daily_db_path: "", daily_hnsw_path: "",
    downloading_weights: false, download_status_msg: null,
    download_needs_restart: false,
    download_retry_attempt: 0, download_retry_in_secs: 0,
  });
  let modelConfigSaving = $state(false);

  const HNSW_M_PRESETS:  number[] = [8, 16, 32, 64];
  const HNSW_EF_PRESETS: number[] = [50, 100, 200, 400];

  let restarting = $state(false);

  // ── Actions ────────────────────────────────────────────────────────────────
  async function refreshStatus() {
    modelStatus = await invoke<EegModelStatus>("get_eeg_model_status");
  }

  async function saveModelConfig(patch: Partial<EegModelConfig>) {
    modelConfig = { ...modelConfig, ...patch };
    modelConfigSaving = true;
    try { await invoke("set_eeg_model_config", { config: modelConfig }); }
    finally { modelConfigSaving = false; }
  }

  async function startDownload() {
    await invoke("trigger_weights_download");
    // Status updates will arrive via the 2-second poll.
  }

  async function cancelDownload() {
    await invoke("cancel_weights_download");
  }

  async function restartApp() {
    restarting = true;
    try { await relaunch(); } catch { restarting = false; }
  }

  // Derived state helpers
  const isDownloading    = $derived(modelStatus.downloading_weights);
  const isAutoRetrying   = $derived(
    !modelStatus.downloading_weights &&
    !modelStatus.weights_found &&
    modelStatus.download_retry_in_secs > 0
  );
  const hasFailed        = $derived(
    !modelStatus.downloading_weights &&
    !modelStatus.weights_found &&
    !isAutoRetrying &&
    modelStatus.download_status_msg !== null &&
    modelStatus.download_status_msg !== "Download cancelled."
  );
  const wasCancelled     = $derived(
    !modelStatus.downloading_weights &&
    !isAutoRetrying &&
    modelStatus.download_status_msg === "Download cancelled."
  );
  const needsDownload    = $derived(
    !modelStatus.weights_found &&
    !modelStatus.downloading_weights &&
    !isAutoRetrying &&
    !hasFailed
  );
  const needsRestart     = $derived(modelStatus.download_needs_restart);
  const encoderLoading   = $derived(
    modelStatus.weights_found &&
    !modelStatus.encoder_loaded &&
    !modelStatus.download_needs_restart
  );

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  let statusTimer: ReturnType<typeof setInterval> | undefined;

  onMount(async () => {
    modelConfig = await invoke<EegModelConfig>("get_eeg_model_config");
    modelStatus = await invoke<EegModelStatus>("get_eeg_model_status");
    statusTimer = setInterval(refreshStatus, 2000);
  });
  onDestroy(() => clearInterval(statusTimer));
</script>

<!-- ── Encoder status ────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("model.encoder")}
    </span>
    <!-- Live status dot -->
    {#if isDownloading}
      <!-- Pulsing blue while downloading -->
      <span class="w-1.5 h-1.5 rounded-full bg-blue-500 animate-pulse"></span>
    {:else if modelStatus.encoder_loaded}
      <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
    {:else if needsRestart}
      <span class="w-1.5 h-1.5 rounded-full bg-amber-400"></span>
    {:else}
      <span class="w-1.5 h-1.5 rounded-full bg-slate-400"></span>
    {/if}
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- ── State: encoder ready ────────────────────────────────────────── -->
      {#if modelStatus.encoder_loaded}
        <div class="flex items-center gap-3 px-4 py-3.5">
          <div class="flex flex-col gap-0.5 min-w-0 flex-1">
            <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
            {#if modelStatus.encoder_describe}
              <span class="text-[0.65rem] text-muted-foreground font-mono truncate">
                {modelStatus.encoder_describe}
              </span>
            {/if}
          </div>
          <Badge variant="outline"
            class="shrink-0 text-[0.56rem] py-0 px-1.5
                   bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20">
            {t("model.ready")}
          </Badge>
        </div>

      <!-- ── State: downloading ──────────────────────────────────────────── -->
      {:else if isDownloading}
        <div class="flex flex-col gap-3 px-4 py-4">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
              <span class="text-[0.65rem] text-blue-500 dark:text-blue-400 truncate">
                {modelStatus.download_status_msg ?? t("model.downloading")}
              </span>
            </div>
            <Badge variant="outline"
              class="shrink-0 text-[0.56rem] py-0 px-1.5
                     bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20">
              {t("model.downloading")}
            </Badge>
          </div>

          <!-- Indeterminate progress bar -->
          <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
            <div class="h-full rounded-full bg-blue-500 animate-[progress-indeterminate_1.6s_ease-in-out_infinite]"
                 style="width:40%"></div>
          </div>

          <!-- Step label -->
          {#if modelStatus.download_status_msg}
            <p class="text-[0.6rem] text-muted-foreground/70 leading-relaxed -mt-1">
              {modelStatus.download_status_msg}
            </p>
          {/if}

          <!-- Cancel button -->
          <div class="flex justify-end">
            <Button variant="outline" size="sm"
                    class="h-7 text-[0.65rem] px-3 text-destructive border-destructive/30
                           hover:bg-destructive/10 hover:text-destructive"
                    onclick={cancelDownload}>
              {t("model.cancelDownload")}
            </Button>
          </div>
        </div>

      <!-- ── State: auto-retrying (backoff countdown) ─────────────────────── -->
      {:else if isAutoRetrying}
        <div class="flex flex-col gap-3 px-4 py-4">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
              <span class="text-[0.65rem] text-amber-600 dark:text-amber-400">
                {t("model.autoRetryIn", { secs: String(modelStatus.download_retry_in_secs) })}
                · {t("model.autoRetryAttempt", { n: String(modelStatus.download_retry_attempt + 1) })}
              </span>
            </div>
            <Badge variant="outline"
              class="shrink-0 text-[0.56rem] py-0 px-1.5
                     bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20">
              {t("model.unavailable")}
            </Badge>
          </div>
          {#if modelStatus.download_status_msg && modelStatus.download_status_msg !== "Download cancelled."}
            <p class="text-[0.58rem] text-destructive/80 font-mono break-all leading-relaxed
                       rounded-md bg-destructive/5 border border-destructive/10 px-2.5 py-2">
              {modelStatus.download_status_msg}
            </p>
          {/if}
          <div class="flex items-center gap-3">
            <div class="relative w-8 h-8 shrink-0 flex items-center justify-center">
              <svg class="absolute inset-0 w-8 h-8 -rotate-90" viewBox="0 0 32 32">
                <circle cx="16" cy="16" r="13" fill="none" stroke="currentColor"
                  stroke-width="2" class="text-muted/40" />
                <circle cx="16" cy="16" r="13" fill="none"
                  stroke-width="2.5" stroke-linecap="round"
                  class="text-amber-500 dark:text-amber-400"
                  stroke="currentColor"
                  stroke-dasharray="{2 * Math.PI * 13}"
                  stroke-dashoffset="{2 * Math.PI * 13 * (modelStatus.download_retry_in_secs / Math.max(1, modelStatus.download_retry_in_secs))}"
                  style="transition: stroke-dashoffset 1s linear" />
              </svg>
              <span class="text-[0.58rem] font-bold tabular-nums text-amber-600 dark:text-amber-400">
                {modelStatus.download_retry_in_secs}
              </span>
            </div>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed flex-1">
              {t("model.downloadFailed")} — {t("model.autoRetryIn", { secs: String(modelStatus.download_retry_in_secs) })}
            </p>
          </div>
          <div class="flex justify-end gap-2">
            <Button variant="outline" size="sm"
                    class="h-7 text-[0.65rem] px-3 text-destructive border-destructive/30
                           hover:bg-destructive/10 hover:text-destructive"
                    onclick={cancelDownload}>
              {t("model.cancelAutoRetry")}
            </Button>
            <Button size="sm" class="h-7 text-[0.65rem] px-3" onclick={startDownload}>
              {t("model.retry")}
            </Button>
          </div>
        </div>

      <!-- ── State: download failed ──────────────────────────────────────── -->
      {:else if hasFailed}
        <div class="flex flex-col gap-3 px-4 py-4">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
              <span class="text-[0.65rem] text-destructive truncate">
                {t("model.downloadFailed")}
              </span>
            </div>
            <Badge variant="outline"
              class="shrink-0 text-[0.56rem] py-0 px-1.5
                     bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20">
              {t("model.unavailable")}
            </Badge>
          </div>
          <!-- Error detail -->
          {#if modelStatus.download_status_msg}
            <p class="text-[0.58rem] text-destructive/80 font-mono break-all leading-relaxed
                       rounded-md bg-destructive/5 border border-destructive/10 px-2.5 py-2">
              {modelStatus.download_status_msg}
            </p>
          {/if}
          <div class="flex justify-end">
            <Button size="sm" class="h-7 text-[0.65rem] px-3" onclick={startDownload}>
              {t("model.retry")}
            </Button>
          </div>
        </div>

      <!-- ── State: cancelled ────────────────────────────────────────────── -->
      {:else if wasCancelled}
        <div class="flex items-center gap-3 px-4 py-3.5">
          <div class="flex flex-col gap-0.5 min-w-0 flex-1">
            <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
            <span class="text-[0.65rem] text-muted-foreground">{t("model.downloadCancelled")}</span>
          </div>
          <Button size="sm" variant="outline"
                  class="shrink-0 h-7 text-[0.65rem] px-3"
                  onclick={startDownload}>
            {t("model.download")}
          </Button>
        </div>

      <!-- ── State: needs restart after manual download ──────────────────── -->
      {:else if needsRestart}
        <div class="flex flex-col gap-3 px-4 py-4">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
              <span class="text-[0.65rem] text-amber-600 dark:text-amber-400">
                {t("model.restartToLoad")}
              </span>
            </div>
            <Badge variant="outline"
              class="shrink-0 text-[0.56rem] py-0 px-1.5
                     bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20">
              {t("model.ready")}
            </Badge>
          </div>
          <!-- Success progress bar (full, green) -->
          <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
            <div class="h-full w-full rounded-full bg-emerald-500 transition-all duration-700"></div>
          </div>
          <div class="flex justify-end">
            <Button size="sm"
                    class="h-7 text-[0.65rem] px-3 bg-amber-500 hover:bg-amber-600 text-white"
                    disabled={restarting}
                    onclick={restartApp}>
              {restarting ? "…" : t("model.restartNow")}
            </Button>
          </div>
        </div>

      <!-- ── State: encoder loading (weights present, GPU compiling) ─────── -->
      {:else if encoderLoading}
        <div class="flex flex-col gap-3 px-4 py-4">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
              <span class="text-[0.65rem] text-muted-foreground">{t("model.encoderLoading")}</span>
            </div>
            <Badge variant="outline"
              class="shrink-0 text-[0.56rem] py-0 px-1.5
                     bg-slate-500/10 text-slate-500 border-slate-500/20">
              {t("common.loading")}
            </Badge>
          </div>
          <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
            <div class="h-full rounded-full bg-slate-400
                        animate-[progress-indeterminate_1.6s_ease-in-out_infinite]"
                 style="width:40%"></div>
          </div>
        </div>

      <!-- ── State: no weights, ready to download ───────────────────────── -->
      {:else}
        <div class="flex flex-col gap-3 px-4 py-4">
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.78rem] font-semibold text-foreground">{t("model.zunaEncoder")}</span>
              <span class="text-[0.65rem] text-muted-foreground/70">
                {t("model.notFoundInCache")}
              </span>
            </div>
            <Badge variant="outline"
              class="shrink-0 text-[0.56rem] py-0 px-1.5
                     bg-slate-500/10 text-slate-500 border-slate-500/20">
              {t("model.unavailable")}
            </Badge>
          </div>
          <!-- Repo hint + download button -->
          <div class="flex items-center gap-2 rounded-lg bg-muted/30 px-3 py-2.5
                      border border-border dark:border-white/[0.06]">
            <div class="flex flex-col gap-0.5 flex-1 min-w-0">
              <span class="text-[0.6rem] text-muted-foreground/70 font-mono truncate">
                {modelConfig.hf_repo}
              </span>
            </div>
            <Button size="sm" class="shrink-0 h-7 text-[0.65rem] px-3" onclick={startDownload}>
              {t("model.download")}
            </Button>
          </div>
        </div>
      {/if}

      <!-- ── Weights path (always shown when found) ──────────────────────── -->
      {#if modelStatus.weights_path}
        <div class="flex flex-col gap-0.5 px-4 py-3">
          <span class="text-[0.62rem] font-medium text-foreground">{t("model.weightsPath")}</span>
          <span class="text-[0.58rem] text-muted-foreground font-mono break-all leading-relaxed">
            {modelStatus.weights_path}
          </span>
        </div>
      {/if}

      <!-- ── Today's storage ─────────────────────────────────────────────── -->
      <div class="flex items-start gap-6 flex-wrap px-4 py-3 bg-slate-50 dark:bg-[#111118]">
        <div class="flex flex-col gap-0.5 min-w-0 flex-1">
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("model.todaysDb")}
          </span>
          <span class="text-[0.6rem] font-mono text-muted-foreground break-all leading-relaxed">
            {modelStatus.daily_db_path || "—"}
          </span>
        </div>
        <div class="flex flex-col gap-0.5 shrink-0 items-end">
          <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("model.embeddingsToday")}
          </span>
          <span class="text-[1rem] font-bold tabular-nums text-foreground leading-none">
            {modelStatus.embeddings_today}
          </span>
        </div>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ── HNSW index ────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("model.hnswIndex")}
    </span>
    {#if modelConfigSaving}
      <span class="text-[0.56rem] text-muted-foreground">{t("common.saving")}</span>
    {/if}
    <span class="ml-auto text-[0.56rem] text-muted-foreground/60">{t("model.takesEffectRestart")}</span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- M -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-[0.78rem] font-semibold text-foreground">
            {t("model.connectivity")} <code class="text-[0.7rem] text-muted-foreground">M</code>
          </span>
          <span class="text-[0.68rem] text-muted-foreground">{t("model.currently", { n: modelConfig.hnsw_m })}</span>
        </div>
        <p class="text-[0.68rem] text-muted-foreground leading-relaxed -mt-0.5">
          {t("model.connectivityDesc")}
        </p>
        <div class="flex items-center gap-1.5">
          {#each HNSW_M_PRESETS as m}
            <button
              onclick={() => saveModelConfig({ hnsw_m: m })}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {modelConfig.hnsw_m === m
                       ? 'border-emerald-500/50 bg-emerald-500/10 dark:bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              {m}
            </button>
          {/each}
        </div>
      </div>

      <!-- ef_construction -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-[0.78rem] font-semibold text-foreground">
            {t("model.buildQuality")} <code class="text-[0.7rem] text-muted-foreground">ef</code>
          </span>
          <span class="text-[0.68rem] text-muted-foreground">{t("model.currently", { n: modelConfig.hnsw_ef_construction })}</span>
        </div>
        <p class="text-[0.68rem] text-muted-foreground leading-relaxed -mt-0.5">
          {t("model.buildQualityDesc")}
        </p>
        <div class="flex items-center gap-1.5">
          {#each HNSW_EF_PRESETS as ef}
            <button
              onclick={() => saveModelConfig({ hnsw_ef_construction: ef })}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {modelConfig.hnsw_ef_construction === ef
                       ? 'border-emerald-500/50 bg-emerald-500/10 dark:bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              {ef}
            </button>
          {/each}
        </div>
      </div>

      <!-- Summary -->
      <div class="flex items-center gap-2 flex-wrap px-4 py-3 bg-slate-50 dark:bg-[#111118]">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">{t("model.index")}</span>
        <Badge variant="outline"
          class="text-[0.56rem] py-0 px-1.5 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20">
          M = {modelConfig.hnsw_m}
        </Badge>
        <Badge variant="outline"
          class="text-[0.56rem] py-0 px-1.5 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20">
          ef = {modelConfig.hnsw_ef_construction}
        </Badge>
        <Badge variant="outline"
          class="text-[0.56rem] py-0 px-1.5 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20">
          {t("model.cosineDistance")}
        </Badge>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ── Model source ──────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("model.modelSource")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <span class="text-[0.78rem] font-semibold text-foreground">{t("model.hfRepo")}</span>
        <span class="text-[0.68rem] font-mono text-muted-foreground">{modelConfig.hf_repo}</span>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("model.dataNormalisation")}</span>
          <span class="text-[0.65rem] text-muted-foreground">{t("model.dataNormDesc")}</span>
        </div>
        <span class="text-[0.78rem] font-mono font-semibold text-foreground">{modelConfig.data_norm}</span>
      </div>

    </CardContent>
  </Card>
</section>
