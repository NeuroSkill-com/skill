<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Screenshots tab — capture, embedding model, re-embed -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Button }    from "$lib/components/ui/button";
  import { Card, CardContent } from "$lib/components/ui/card";
  import { Separator } from "$lib/components/ui/separator";
  import { t }         from "$lib/i18n/index.svelte";
  import {
    EMBEDDING_EPOCH_SECS,
    SCREENSHOT_INTERVAL_MIN_SECS,
    SCREENSHOT_INTERVAL_MAX_SECS,
    SCREENSHOT_INTERVAL_STEP_SECS,
  } from "$lib/constants";

  // ── Types ──────────────────────────────────────────────────────────────────
  interface ScreenshotConfig {
    enabled:         boolean;
    interval_secs:   number;
    image_size:      number;
    quality:         number;
    session_only:    boolean;
    embed_backend:   string;
    fastembed_model: string;
    ocr_enabled:     boolean;
    ocr_engine:      string;
    use_gpu:         boolean;
  }
  interface ConfigChangeResult {
    model_changed: boolean;
    stale_count:   number;
  }
  interface ReembedEstimate {
    total:        number;
    stale:        number;
    unembedded:   number;
    per_image_ms: number;
    eta_secs:     number;
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let config = $state<ScreenshotConfig>({
    enabled: false,
    interval_secs: 5,
    image_size: 768,
    quality: 60,
    session_only: true,
    embed_backend: "fastembed",
    fastembed_model: "clip-vit-b-32",
    ocr_enabled: true,
    ocr_engine: "ocrs",
    use_gpu: true,
  });

  let saving      = $state(false);
  let reembedding = $state(false);
  let screenPermission = $state<boolean | null>(null);
  /** The app-wide text embedding model (from Settings → Embeddings).
   *  OCR text embeddings use this shared model now. */
  let sharedTextModel = $state("");
  const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  let estimate    = $state<ReembedEstimate | null>(null);
  let progress    = $state<{ done: number; total: number; elapsed_secs: number; eta_secs: number } | null>(null);
  let modelChanged = $state(false);
  let staleCount   = $state(0);
  let unlisten: UnlistenFn | null = null;

  // ── OCR search state ──────────────────────────────────────────────────────


  // ── Pipeline metrics ──────────────────────────────────────────────────────
  interface PipelineMetrics {
    captures: number; capture_errors: number; drops: number;
    capture_us: number; ocr_us: number; resize_us: number; save_us: number; capture_total_us: number;
    embeds: number; embed_errors: number;
    vision_embed_us: number; text_embed_us: number; embed_total_us: number;
    queue_depth: number;
    last_capture_unix: number; last_embed_unix: number;
    backoff_multiplier: number;
  }
  let pipeMetrics = $state<PipelineMetrics | null>(null);
  let metricsTimer: ReturnType<typeof setInterval> | null = null;

  // ── Rolling history for charts (last 60 samples @ 2s = 2 minutes) ─────
  const HISTORY_LEN = 60;
  let captureHistory   = $state<number[]>([]);   // capture_total_us in ms
  let embedHistory     = $state<number[]>([]);   // embed_total_us in ms
  let queueHistory     = $state<number[]>([]);   // queue_depth
  let dropsHistory     = $state<number[]>([]);   // cumulative drops
  let captureBreakdown = $state<{capture: number; ocr: number; resize: number; save: number}>({capture:0,ocr:0,resize:0,save:0});
  let embedBreakdown   = $state<{vision: number; text: number}>({vision:0,text:0});

  function pushHistory(arr: number[], val: number): number[] {
    const next = [...arr, val];
    return next.length > HISTORY_LEN ? next.slice(next.length - HISTORY_LEN) : next;
  }

  function fmtUs(us: number): string {
    if (us < 1000) return `${us}µs`;
    if (us < 1_000_000) return `${(us / 1000).toFixed(1)}ms`;
    return `${(us / 1_000_000).toFixed(2)}s`;
  }

  function fmtMs(ms: number): string {
    if (ms < 1) return `${(ms * 1000).toFixed(0)}µs`;
    if (ms < 1000) return `${ms.toFixed(1)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  }

  /// Build an SVG polyline `points` string from an array of values.
  /// Maps values into a viewBox of width×height with optional Y padding.
  function sparklinePath(data: number[], w: number, h: number, pad = 2): string {
    if (data.length < 2) return "";
    const maxV = Math.max(...data, 1);
    const usableH = h - pad * 2;
    return data.map((v, i) => {
      const x = (i / (data.length - 1)) * w;
      const y = pad + usableH - (v / maxV) * usableH;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    }).join(" ");
  }

  /// Build an SVG polygon points string for a filled area chart.
  function areaPath(data: number[], w: number, h: number, pad = 2): string {
    if (data.length < 2) return "";
    const line = sparklinePath(data, w, h, pad);
    return `0,${h} ${line} ${w},${h}`;
  }

  // ── Recommended image size for current model ──────────────────────────────
  const recommendedSize = $derived.by(() => {
    if (config.embed_backend === "mmproj" || config.embed_backend === "llm-vlm") return 768;
    if (config.fastembed_model === "nomic-embed-vision-v1.5") return 768;
    return 768;
  });

  // ── Load ───────────────────────────────────────────────────────────────────
  async function load() {
    config = await invoke<ScreenshotConfig>("get_screenshot_config");
    try {
      estimate = await invoke<ReembedEstimate | null>("estimate_screenshot_reembed");
    } catch { estimate = null; }
    try {
      sharedTextModel = await invoke<string>("get_embedding_model");
    } catch { sharedTextModel = ""; }

    if (isMac) {
      try { screenPermission = await invoke<boolean>("check_screen_recording_permission"); }
      catch { screenPermission = null; }
    }
  }

  // ── Save config ────────────────────────────────────────────────────────────
  async function save() {
    saving = true;
    try {
      const result = await invoke<ConfigChangeResult>("set_screenshot_config", { config });
      modelChanged = result.model_changed;
      staleCount = result.stale_count;
      if (result.model_changed) {
        try { estimate = await invoke<ReembedEstimate | null>("estimate_screenshot_reembed"); } catch (e) { console.warn("[screenshots] estimate_screenshot_reembed failed:", e); }
      }
    } finally {
      saving = false;
    }
  }

  // ── Toggle helpers (auto-save) ─────────────────────────────────────────────
  async function toggleEnabled() {
    config.enabled = !config.enabled;
    await save();
  }
  async function toggleSessionOnly() {
    config.session_only = !config.session_only;
    await save();
  }
  async function toggleOcr() {
    config.ocr_enabled = !config.ocr_enabled;
    await save();
  }
  async function toggleGpu() {
    config.use_gpu = !config.use_gpu;
    await save();
  }

  // ── Model change → auto-update image_size ─────────────────────────────────
  function onModelChange() {
    config.image_size = recommendedSize;
  }

  // ── Re-embed ───────────────────────────────────────────────────────────────
  async function reembed() {
    reembedding = true;
    progress = null;
    try {
      await invoke("rebuild_screenshot_embeddings");
      modelChanged = false;
      staleCount = 0;
      try { estimate = await invoke<ReembedEstimate | null>("estimate_screenshot_reembed"); } catch (e) { console.warn("[screenshots] estimate_screenshot_reembed failed:", e); }
    } finally {
      reembedding = false;
      progress = null;
    }
  }

  function fmtEta(secs: number): string {
    if (secs < 60) return `${Math.round(secs)}s`;
    const m = Math.floor(secs / 60);
    const s = Math.round(secs % 60);
    return `${m}m ${s}s`;
  }

  async function refreshMetrics() {
    try {
      const m = await invoke<PipelineMetrics>("get_screenshot_metrics");
      pipeMetrics = m;
      // Push to rolling history (convert µs → ms)
      captureHistory = pushHistory(captureHistory, m.capture_total_us / 1000);
      embedHistory   = pushHistory(embedHistory, m.embed_total_us / 1000);
      queueHistory   = pushHistory(queueHistory, m.queue_depth);
      dropsHistory   = pushHistory(dropsHistory, m.drops);
      captureBreakdown = {
        capture: m.capture_us / 1000,
        ocr:     m.ocr_us / 1000,
        resize:  m.resize_us / 1000,
        save:    m.save_us / 1000,
      };
      embedBreakdown = {
        vision: m.vision_embed_us / 1000,
        text:   m.text_embed_us / 1000,
      };
    } catch (e) { console.warn("[screenshots] get_screenshot_metrics failed:", e); }
  }

  onMount(async () => {
    await load();
    await refreshMetrics();
    unlisten = await listen<{ done: number; total: number; elapsed_secs: number; eta_secs: number }>(
      "screenshot-reembed-progress", e => { progress = e.payload; }
    );
    // Poll metrics every 2s when enabled
    metricsTimer = setInterval(() => { if (config.enabled) refreshMetrics(); }, 2000);
  });
  onDestroy(() => {
    unlisten?.();
    if (metricsTimer) clearInterval(metricsTimer);
  });
</script>

<section class="flex flex-col gap-5">

  <!-- ── Section header ──────────────────────────────────────────────────── -->
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("screenshots.title")}
    </span>
  </div>

  <!-- ── Screen recording permission warning (macOS only) ──────────────── -->
  {#if isMac && screenPermission === false}
    <div class="rounded-xl border border-red-500/30 bg-red-500/5
                dark:bg-red-500/10 px-4 py-3 flex flex-col gap-2">
      <div class="flex items-center gap-2">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round"
             class="w-4 h-4 shrink-0 text-red-500">
          <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
          <line x1="12" y1="9" x2="12" y2="13"/>
          <line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        <span class="text-[0.72rem] font-semibold text-red-600 dark:text-red-400">
          {t("screenshots.permissionRequired")}
        </span>
      </div>
      <p class="text-[0.62rem] text-red-600/80 dark:text-red-400/80 leading-relaxed">
        {t("screenshots.permissionDesc")}
      </p>
      <div class="flex gap-2 mt-1">
        <Button size="sm" variant="outline" class="text-[0.62rem] h-7 px-3"
                onclick={() => invoke("open_screen_recording_settings")}>
          {t("screenshots.openPermissionSettings")}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
               class="w-3 h-3 ml-1 shrink-0">
            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
            <polyline points="15 3 21 3 21 9"/>
            <line x1="10" y1="14" x2="21" y2="3"/>
          </svg>
        </Button>
      </div>
    </div>
  {:else if isMac && screenPermission === true}
    <div class="rounded-xl border border-green-500/20 bg-green-500/5
                dark:bg-green-500/10 px-3 py-2 flex items-center gap-2">
      <span class="w-1.5 h-1.5 rounded-full bg-green-500 shrink-0"></span>
      <span class="text-[0.62rem] text-green-700 dark:text-green-400">
        {t("screenshots.permissionGranted")}
      </span>
    </div>
  {/if}

  <!-- ── Enable + Session-only toggles ───────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">

      <!-- Enable toggle -->
      <button
        onclick={toggleEnabled}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {config.enabled ? 'bg-primary' : 'bg-muted dark:bg-white/[0.08]'}">
          <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {config.enabled ? 'translate-x-4' : 'translate-x-0.5'}"></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("screenshots.enableToggle")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("screenshots.enableDesc")}
          </span>
        </div>
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {config.enabled ? 'text-primary' : 'text-muted-foreground/50'}">
          {config.enabled ? t("common.on") : t("common.off")}
        </span>
      </button>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- Session-only toggle -->
      <button
        onclick={toggleSessionOnly}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {config.session_only ? 'bg-primary' : 'bg-muted dark:bg-white/[0.08]'}">
          <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {config.session_only ? 'translate-x-4' : 'translate-x-0.5'}"></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("screenshots.sessionOnlyToggle")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("screenshots.sessionOnlyDesc")}
          </span>
        </div>
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {config.session_only ? 'text-primary' : 'text-muted-foreground/50'}">
          {config.session_only ? t("common.on") : t("common.off")}
        </span>
      </button>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- OCR toggle -->
      <button
        onclick={toggleOcr}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {config.ocr_enabled ? 'bg-primary' : 'bg-muted dark:bg-white/[0.08]'}">
          <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {config.ocr_enabled ? 'translate-x-4' : 'translate-x-0.5'}"></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("screenshots.ocrToggle")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("screenshots.ocrToggleDesc")}
          </span>
        </div>
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {config.ocr_enabled ? 'text-primary' : 'text-muted-foreground/50'}">
          {config.ocr_enabled ? t("common.on") : t("common.off")}
        </span>
      </button>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- GPU toggle -->
      <button
        onclick={toggleGpu}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {config.use_gpu ? 'bg-primary' : 'bg-muted dark:bg-white/[0.08]'}">
          <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {config.use_gpu ? 'translate-x-4' : 'translate-x-0.5'}"></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("screenshots.gpuToggle")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("screenshots.gpuToggleDesc")}
          </span>
        </div>
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {config.use_gpu ? 'text-primary' : 'text-muted-foreground/50'}">
          {config.use_gpu ? 'GPU' : 'CPU'}
        </span>
      </button>

    </CardContent>
  </Card>

  <!-- ── Capture settings ────────────────────────────────────────────────── -->
  {#if config.enabled}
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-4 px-4 flex flex-col gap-4">

      <!-- Interval (epoch-aligned: multiples of 5 s) -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="ss-interval" class="text-[0.72rem] font-semibold text-foreground">
            {t("screenshots.interval")}
          </label>
          <span class="text-[0.58rem] text-muted-foreground tabular-nums">
            {config.interval_secs}{t("screenshots.intervalUnit")} ({Math.round(config.interval_secs / EMBEDDING_EPOCH_SECS)}× {t("screenshots.intervalEpoch")})
          </span>
        </div>
        <input id="ss-interval" type="range"
               min={SCREENSHOT_INTERVAL_MIN_SECS} max={SCREENSHOT_INTERVAL_MAX_SECS}
               step={SCREENSHOT_INTERVAL_STEP_SECS}
               bind:value={config.interval_secs}
               class="w-full accent-primary h-1.5" />
        <span class="text-[0.54rem] text-muted-foreground/60">{t("screenshots.intervalDesc")}</span>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- Image size -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="ss-size" class="text-[0.72rem] font-semibold text-foreground">
            {t("screenshots.imageSize")}
          </label>
          <span class="text-[0.58rem] text-muted-foreground tabular-nums">
            {config.image_size} {t("screenshots.imageSizeUnit")}
          </span>
        </div>
        <input id="ss-size" type="range" min="224" max="1536" step="32"
               bind:value={config.image_size}
               class="w-full accent-primary h-1.5" />
        <span class="text-[0.54rem] text-muted-foreground/60">
          {t("screenshots.imageSizeDesc")}
          <span class="font-semibold"> {t("screenshots.imageSizeRecommended")} {recommendedSize}{t("screenshots.imageSizeUnit")}</span>
        </span>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- Quality -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="ss-quality" class="text-[0.72rem] font-semibold text-foreground">
            {t("screenshots.quality")}
          </label>
          <span class="text-[0.58rem] text-muted-foreground tabular-nums">{config.quality}</span>
        </div>
        <input id="ss-quality" type="range" min="10" max="100" step="5"
               bind:value={config.quality}
               class="w-full accent-primary h-1.5" />
        <span class="text-[0.54rem] text-muted-foreground/60">{t("screenshots.qualityDesc")}</span>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- Embedding backend -->
      <div class="flex flex-col gap-1.5">
        <span class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.embeddingModel")}</span>
        <span class="text-[0.54rem] text-muted-foreground/60">{t("screenshots.embeddingModelDesc")}</span>

        <div class="flex flex-col gap-1">
          <!-- Backend select -->
          <select bind:value={config.embed_backend}
                  onchange={onModelChange}
                  class="w-full rounded-lg border border-border dark:border-white/[0.08]
                         bg-white dark:bg-[#14141e] px-3 py-2
                         text-[0.72rem] text-foreground
                         focus:outline-none focus:ring-1 focus:ring-ring/50">
            <option value="fastembed">{t("screenshots.backendFastembed")}</option>
            <option value="mmproj">{t("screenshots.backendMmproj")}</option>
            <option value="llm-vlm">{t("screenshots.backendLlmVlm")}</option>
          </select>

          <!-- fastembed model select (only when fastembed is selected) -->
          {#if config.embed_backend === "fastembed"}
            <select bind:value={config.fastembed_model}
                    onchange={onModelChange}
                    class="w-full rounded-lg border border-border dark:border-white/[0.08]
                           bg-white dark:bg-[#14141e] px-3 py-2
                           text-[0.72rem] text-foreground
                           focus:outline-none focus:ring-1 focus:ring-ring/50">
              <option value="clip-vit-b-32">{t("screenshots.modelClip")}</option>
              <option value="nomic-embed-vision-v1.5">{t("screenshots.modelNomic")}</option>
            </select>
          {/if}
        </div>
      </div>

      <!-- Apply button -->
      <div class="flex justify-end">
        <Button size="sm" onclick={save} disabled={saving}
                class="text-[0.65rem] h-7 px-4">
          {saving ? t("common.saving") : t("common.apply")}
        </Button>
      </div>

    </CardContent>
  </Card>
  {/if}

  <!-- ── Model changed banner ────────────────────────────────────────────── -->
  {#if modelChanged && staleCount > 0}
    <div class="rounded-xl border border-amber-500/30 bg-amber-500/5
                dark:bg-amber-500/10 px-4 py-3 flex flex-col gap-2">
      <div class="flex items-center gap-2">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round"
             class="w-4 h-4 shrink-0 text-amber-500">
          <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
          <line x1="12" y1="9" x2="12" y2="13"/>
          <line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        <span class="text-[0.72rem] font-semibold text-amber-600 dark:text-amber-400">
          {t("screenshots.modelChanged")}
        </span>
      </div>
      <p class="text-[0.62rem] text-amber-600/80 dark:text-amber-400/80 leading-relaxed">
        {staleCount} {t("screenshots.modelChangedDesc")}
      </p>
      {#if estimate}
        <p class="text-[0.58rem] text-amber-600/60 dark:text-amber-400/60">
          {t("screenshots.estimate")} ~{fmtEta(estimate.eta_secs)}
        </p>
      {/if}
      <div class="flex gap-2 mt-1">
        <Button size="sm" onclick={reembed} disabled={reembedding}
                class="text-[0.62rem] h-7 px-3">
          {reembedding ? t("screenshots.reembedding") : t("screenshots.reembedNowBtn")}
        </Button>
        <Button size="sm" variant="ghost" onclick={() => { modelChanged = false; }}
                class="text-[0.62rem] h-7 px-3 text-muted-foreground">
          {t("common.dismiss")}
        </Button>
      </div>
    </div>
  {/if}

  <!-- ── Re-embed & Reindex section (always visible) ─────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center justify-between">
      <div class="flex flex-col gap-0.5">
        <div class="flex items-center gap-2">
          <span class="text-[0.72rem] font-semibold text-foreground">
            {t("screenshots.reembed")}
          </span>
          {#if estimate && estimate.stale > 0}
            <span class="rounded-full px-1.5 py-0 text-[0.55rem] font-semibold
                         bg-amber-500/15 text-amber-600 dark:text-amber-400 border border-amber-500/25">
              {estimate.stale} {t("screenshots.stale")}
            </span>
          {/if}
          {#if estimate && estimate.unembedded > 0}
            <span class="rounded-full px-1.5 py-0 text-[0.55rem] font-semibold
                         bg-primary/15 text-primary border border-primary/25">
              {estimate.unembedded} {t("screenshots.unembedded")}
            </span>
          {/if}
        </div>
        <span class="text-[0.6rem] text-muted-foreground/60">
          {t("screenshots.reembedDesc")}
          {#if estimate && estimate.eta_secs > 0}
            — {t("screenshots.estimate")} ~{fmtEta(estimate.eta_secs)}
          {/if}
        </span>
      </div>
      <Button size="sm" variant="outline" onclick={reembed} disabled={reembedding}
              class="text-[0.65rem] h-7 px-3 shrink-0">
        {reembedding ? t("screenshots.reembedding") : t("screenshots.reembedBtn")}
      </Button>
    </div>

    <!-- Progress bar -->
    {#if progress !== null}
      {@const pct = progress.total > 0 ? Math.round(progress.done / progress.total * 100) : 0}
      <div class="flex flex-col gap-1">
        <div class="h-1.5 rounded-full bg-muted dark:bg-white/[0.06] overflow-hidden">
          <div class="h-full rounded-full bg-primary transition-all duration-300"
               style="width: {pct}%"></div>
        </div>
        <span class="text-[0.58rem] text-muted-foreground/60 tabular-nums">
          {progress.done} / {progress.total} — {pct}%
          {#if progress.eta_secs > 0}
            — ETA {fmtEta(progress.eta_secs)}
          {/if}
        </span>
      </div>
    {/if}
  </div>

  <!-- ── Stats ───────────────────────────────────────────────────────────── -->
  {#if estimate}
    <div class="flex flex-col gap-1 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground/50">
        {t("screenshots.stats")}
      </span>
      <div class="grid grid-cols-2 gap-x-4 gap-y-1 text-[0.62rem]">
        <span class="text-muted-foreground">{t("screenshots.embeddedCount")}</span>
        <span class="text-foreground tabular-nums">{estimate.total}</span>
        <span class="text-muted-foreground">{t("screenshots.unembeddedCount")}</span>
        <span class="text-foreground tabular-nums">{estimate.unembedded}</span>
        <span class="text-muted-foreground">{t("screenshots.staleCount")}</span>
        <span class="text-foreground tabular-nums">{estimate.stale}</span>
      </div>
    </div>
  {/if}

  <!-- ── OCR Text Extraction ─────────────────────────────────────────────── -->
  {#if config.ocr_enabled}
  <div class="flex items-center gap-2 px-0.5 pt-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("screenshots.ocrTitle")}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="px-4 py-3.5 flex flex-col gap-3">

      <!-- OCR description -->
      <div class="flex items-start gap-3">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75"
             stroke-linecap="round" stroke-linejoin="round"
             class="w-5 h-5 shrink-0 text-primary/70 mt-0.5">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="16" y1="13" x2="8" y2="13"/>
          <line x1="16" y1="17" x2="8" y2="17"/>
          <polyline points="10 9 9 9 8 9"/>
        </svg>
        <div class="flex flex-col gap-1 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.ocrEngine")}</span>
          <p class="text-[0.6rem] text-muted-foreground leading-relaxed">
            {t("screenshots.ocrDesc")}
          </p>
        </div>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- OCR engine select -->
      <div class="flex flex-col gap-1.5">
        <span class="text-[0.72rem] font-semibold text-foreground">{t("screenshots.ocrEngineSelect")}</span>
        <select bind:value={config.ocr_engine}
                class="w-full rounded-lg border border-border dark:border-white/[0.08]
                       bg-white dark:bg-[#14141e] px-3 py-2
                       text-[0.72rem] text-foreground
                       focus:outline-none focus:ring-1 focus:ring-ring/50">
          {#if isMac}
            <option value="apple-vision">{t("screenshots.ocrEngineAppleVision")}</option>
          {/if}
          <option value="ocrs">{t("screenshots.ocrEngineOcrs")}</option>
        </select>
        {#if isMac && config.ocr_engine !== "apple-vision"}
          <span class="text-[0.5rem] text-amber-600 dark:text-amber-400">
            {t("screenshots.ocrAppleVisionHint")}
          </span>
        {/if}
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- Apply button for OCR config changes -->
      <div class="flex justify-end">
        <Button size="sm" onclick={save} disabled={saving}
                class="text-[0.65rem] h-7 px-4">
          {saving ? t("common.saving") : t("common.apply")}
        </Button>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- OCR model info -->
      <div class="flex flex-col gap-2">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground/50">
          {t("screenshots.ocrActiveModels")}
        </span>
        <div class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-[0.62rem]">
          <span class="text-muted-foreground">{t("screenshots.ocrDetModel")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">text-detection.rten</span>
          <span class="text-muted-foreground">{t("screenshots.ocrRecModel")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">text-recognition.rten</span>
          <span class="text-muted-foreground">{t("screenshots.ocrTextEmbed")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">
            {sharedTextModel || "—"}
            <span class="text-muted-foreground/50 font-sans ml-1">(Embeddings)</span>
          </span>
          <span class="text-muted-foreground">{t("screenshots.ocrIndex")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">screenshots_ocr.hnsw</span>
          <span class="text-muted-foreground">{t("screenshots.ocrInference")}</span>
          <span class="text-foreground font-mono text-[0.58rem]">{config.use_gpu ? 'GPU' : 'CPU'}</span>
        </div>
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- Search hint — directs users to the Search window Images tab -->
      <div class="flex items-center gap-2 px-1 py-1">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
             class="w-3.5 h-3.5 text-primary/50 shrink-0">
          <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
        </svg>
        <span class="text-[0.58rem] text-muted-foreground leading-relaxed">
          {t("screenshots.ocrSearchHint")}
        </span>
      </div>

    </CardContent>
  </Card>
  {/if}

  <!-- ── Pipeline Performance ──────────────────────────────────────────── -->
  {#if config.enabled && pipeMetrics && pipeMetrics.captures > 0}
  {#if true}
  {@const CW = 280}
  {@const CH = 56}
  <div class="flex items-center gap-2 px-0.5 pt-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("screenshots.perfTitle")}
    </span>
    <button onclick={refreshMetrics}
            class="text-[0.48rem] text-muted-foreground/40 hover:text-muted-foreground transition-colors">
      ↻
    </button>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="px-4 py-3.5 flex flex-col gap-4">

      <!-- ── Capture thread chart + breakdown ─────────────────────────── -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center gap-2">
          <span class="w-1.5 h-1.5 rounded-full bg-green-500 shrink-0"></span>
          <span class="text-[0.66rem] font-semibold text-foreground">{t("screenshots.perfCapture")}</span>
          <span class="ml-auto text-[0.54rem] text-muted-foreground tabular-nums">
            {pipeMetrics.captures} {t("screenshots.perfTotal")}
            {#if pipeMetrics.capture_errors > 0}
              · <span class="text-red-500">{pipeMetrics.capture_errors} {t("screenshots.perfErrors")}</span>
            {/if}
          </span>
        </div>

        <!-- Area chart -->
        <div class="rounded-lg bg-muted/30 dark:bg-white/[0.02] border border-border/50 dark:border-white/[0.04] overflow-hidden">
          <svg viewBox="0 0 {CW} {CH}" class="w-full h-14" preserveAspectRatio="none">
            {#if captureHistory.length >= 2}
              <polygon points={areaPath(captureHistory, CW, CH)}
                       fill="currentColor" class="text-emerald-500/15 dark:text-emerald-400/10" />
              <polyline points={sparklinePath(captureHistory, CW, CH)}
                        fill="none" stroke="currentColor" stroke-width="1.5"
                        class="text-emerald-500 dark:text-emerald-400" />
            {/if}
          </svg>
        </div>

        <!-- Stacked breakdown bar -->
        {#if (captureBreakdown.capture + captureBreakdown.ocr + captureBreakdown.resize + captureBreakdown.save) > 0}
          {@const capTotal = captureBreakdown.capture + captureBreakdown.ocr + captureBreakdown.resize + captureBreakdown.save}
          <div class="flex h-2 rounded-full overflow-hidden bg-muted/40 dark:bg-white/[0.04]">
            <div class="bg-blue-500" style="width:{(captureBreakdown.capture / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfWindowCapture')}: {fmtMs(captureBreakdown.capture)}"></div>
            <div class="bg-violet-500" style="width:{(captureBreakdown.ocr / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfOcr')}: {fmtMs(captureBreakdown.ocr)}"></div>
            <div class="bg-amber-500" style="width:{(captureBreakdown.resize / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfResize')}: {fmtMs(captureBreakdown.resize)}"></div>
            <div class="bg-emerald-500" style="width:{(captureBreakdown.save / capTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfSave')}: {fmtMs(captureBreakdown.save)}"></div>
          </div>
          <div class="flex flex-wrap gap-x-3 gap-y-0.5 text-[0.5rem] text-muted-foreground">
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-blue-500 shrink-0"></span>{t("screenshots.perfWindowCapture")} {fmtMs(captureBreakdown.capture)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-violet-500 shrink-0"></span>{t("screenshots.perfOcr")} {fmtMs(captureBreakdown.ocr)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-amber-500 shrink-0"></span>{t("screenshots.perfResize")} {fmtMs(captureBreakdown.resize)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-emerald-500 shrink-0"></span>{t("screenshots.perfSave")} {fmtMs(captureBreakdown.save)}</span>
            <span class="font-semibold text-foreground/70">{t("screenshots.perfIterTotal")} {fmtUs(pipeMetrics.capture_total_us)}</span>
          </div>
        {/if}
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- ── Embed thread chart + breakdown ───────────────────────────── -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center gap-2">
          <span class="w-1.5 h-1.5 rounded-full shrink-0
                       {pipeMetrics.queue_depth > 2 ? 'bg-amber-500' : 'bg-green-500'}"></span>
          <span class="text-[0.66rem] font-semibold text-foreground">{t("screenshots.perfEmbed")}</span>
          <span class="ml-auto text-[0.54rem] text-muted-foreground tabular-nums">
            {pipeMetrics.embeds} {t("screenshots.perfTotal")}
          </span>
        </div>

        <!-- Area chart -->
        <div class="rounded-lg bg-muted/30 dark:bg-white/[0.02] border border-border/50 dark:border-white/[0.04] overflow-hidden">
          <svg viewBox="0 0 {CW} {CH}" class="w-full h-14" preserveAspectRatio="none">
            {#if embedHistory.length >= 2}
              <polygon points={areaPath(embedHistory, CW, CH)}
                       fill="currentColor" class="text-blue-500/15 dark:text-blue-400/10" />
              <polyline points={sparklinePath(embedHistory, CW, CH)}
                        fill="none" stroke="currentColor" stroke-width="1.5"
                        class="text-blue-500 dark:text-blue-400" />
            {/if}
          </svg>
        </div>

        <!-- Stacked breakdown bar -->
        {#if (embedBreakdown.vision + embedBreakdown.text) > 0}
          {@const embTotal = embedBreakdown.vision + embedBreakdown.text}
          <div class="flex h-2 rounded-full overflow-hidden bg-muted/40 dark:bg-white/[0.04]">
            <div class="bg-blue-500" style="width:{(embedBreakdown.vision / embTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfVisionEmbed')}: {fmtMs(embedBreakdown.vision)}"></div>
            <div class="bg-violet-500" style="width:{(embedBreakdown.text / embTotal * 100).toFixed(1)}%"
                 title="{t('screenshots.perfTextEmbed')}: {fmtMs(embedBreakdown.text)}"></div>
          </div>
          <div class="flex flex-wrap gap-x-3 gap-y-0.5 text-[0.5rem] text-muted-foreground">
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-blue-500 shrink-0"></span>{t("screenshots.perfVisionEmbed")} {fmtMs(embedBreakdown.vision)}</span>
            <span class="flex items-center gap-1"><span class="w-1.5 h-1.5 rounded-sm bg-violet-500 shrink-0"></span>{t("screenshots.perfTextEmbed")} {fmtMs(embedBreakdown.text)}</span>
            <span class="font-semibold text-foreground/70">{t("screenshots.perfIterTotal")} {fmtUs(pipeMetrics.embed_total_us)}</span>
          </div>
        {/if}
      </div>

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- ── Queue depth chart + drops ────────────────────────────────── -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center gap-2">
          <span class="text-[0.62rem] font-semibold text-foreground">{t("screenshots.perfQueue")}</span>
          <span class="text-[0.58rem] tabular-nums font-semibold
                       {pipeMetrics.queue_depth > 2 ? 'text-amber-500' : 'text-foreground'}">
            {pipeMetrics.queue_depth}/4
          </span>
          <span class="text-[0.54rem] text-muted-foreground ml-2">{t("screenshots.perfDrops")}</span>
          <span class="text-[0.58rem] tabular-nums font-semibold
                       {pipeMetrics.drops > 0 ? 'text-red-500' : 'text-foreground'}">
            {pipeMetrics.drops}
          </span>
        </div>

        <!-- Queue depth chart (step-style) -->
        <div class="rounded-lg bg-muted/30 dark:bg-white/[0.02] border border-border/50 dark:border-white/[0.04] overflow-hidden">
          <svg viewBox="0 0 {CW} 32" class="w-full h-8" preserveAspectRatio="none">
            <!-- Capacity line at y = 4/4 = top -->
            <line x1="0" y1="4" x2={CW} y2="4" stroke="currentColor" stroke-width="0.5"
                  stroke-dasharray="4 3" class="text-red-500/30" />
            {#if queueHistory.length >= 2}
              {@const qPoints = queueHistory.map((v, i) => {
                const maxQ = 4;
                const x = (i / (queueHistory.length - 1)) * CW;
                const y = 30 - (Math.min(v, maxQ) / maxQ) * 26;
                return `${x.toFixed(1)},${y.toFixed(1)}`;
              }).join(" ")}
              <polygon points="0,32 {qPoints} {CW},32"
                       fill="currentColor" class="text-amber-500/15 dark:text-amber-400/10" />
              <polyline points={qPoints}
                        fill="none" stroke="currentColor" stroke-width="1.5"
                        class="text-amber-500 dark:text-amber-400" />
            {/if}
          </svg>
        </div>

        {#if pipeMetrics.backoff_multiplier > 1}
          <div class="flex items-center gap-1.5">
            <span class="text-muted-foreground">{t("screenshots.perfBackoff")}</span>
            <span class="tabular-nums font-semibold text-amber-500">
              {pipeMetrics.backoff_multiplier}×
            </span>
          </div>
        {/if}
        {#if pipeMetrics.drops > 0}
          <span class="text-[0.5rem] text-red-500/70">
            {t("screenshots.perfDropsHint")}
          </span>
        {/if}
      </div>

    </CardContent>
  </Card>
  {/if}
  {/if}

  <!-- ── Privacy note ────────────────────────────────────────────────────── -->
  <div class="rounded-xl border border-primary/20 bg-primary/5
              dark:bg-primary/10 px-4 py-3 flex gap-3">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
         stroke-linecap="round" stroke-linejoin="round"
         class="w-4 h-4 shrink-0 text-primary mt-0.5">
      <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
      <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
    </svg>
    <div class="flex flex-col gap-0.5">
      <p class="text-[0.62rem] text-primary leading-relaxed">
        {t("screenshots.privacyNote")}
      </p>
      <p class="text-[0.54rem] text-primary/60 font-mono">
        {t("screenshots.storagePath")}
      </p>
    </div>
  </div>

</section>
