<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Screenshots tab — capture, embedding model, re-embed -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { t } from "$lib/i18n/index.svelte";
import ScreenshotCaptureSettingsSection from "$lib/screenshots/ScreenshotCaptureSettingsSection.svelte";
import ScreenshotOcrSection from "$lib/screenshots/ScreenshotOcrSection.svelte";
import ScreenshotPerformanceSection from "$lib/screenshots/ScreenshotPerformanceSection.svelte";
import ScreenshotPermissionNotice from "$lib/screenshots/ScreenshotPermissionNotice.svelte";
import ScreenshotPrivacyNote from "$lib/screenshots/ScreenshotPrivacyNote.svelte";
import ScreenshotReembedSection from "$lib/screenshots/ScreenshotReembedSection.svelte";
import ScreenshotToggleCard from "$lib/screenshots/ScreenshotToggleCard.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
interface ScreenshotConfig {
  enabled: boolean;
  interval_secs: number;
  image_size: number;
  quality: number;
  session_only: boolean;
  embed_backend: string;
  fastembed_model: string;
  ocr_enabled: boolean;
  ocr_engine: string;
  use_gpu: boolean;
}
interface ConfigChangeResult {
  model_changed: boolean;
  stale_count: number;
}
interface ReembedEstimate {
  total: number;
  stale: number;
  unembedded: number;
  per_image_ms: number;
  eta_secs: number;
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

let saving = $state(false);
let reembedding = $state(false);
let screenPermission = $state<boolean | null>(null);
/** The app-wide text embedding model (from Settings → Embeddings).
 *  OCR text embeddings use this shared model now. */
let sharedTextModel = $state("");
const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
let estimate = $state<ReembedEstimate | null>(null);
let progress = $state<{ done: number; total: number; elapsed_secs: number; eta_secs: number } | null>(null);
let modelChanged = $state(false);
let staleCount = $state(0);
let unlisten: UnlistenFn | null = null;

// ── OCR search state ──────────────────────────────────────────────────────

// ── Pipeline metrics ──────────────────────────────────────────────────────
interface PipelineMetrics {
  captures: number;
  capture_errors: number;
  drops: number;
  capture_us: number;
  ocr_us: number;
  resize_us: number;
  save_us: number;
  capture_total_us: number;
  embeds: number;
  embed_errors: number;
  vision_embed_us: number;
  text_embed_us: number;
  embed_total_us: number;
  queue_depth: number;
  last_capture_unix: number;
  last_embed_unix: number;
  backoff_multiplier: number;
}
let pipeMetrics = $state<PipelineMetrics | null>(null);
let metricsTimer: ReturnType<typeof setInterval> | null = null;

// ── Rolling history for charts (last 60 samples @ 2s = 2 minutes) ─────
const HISTORY_LEN = 60;
let captureHistory = $state<number[]>([]); // capture_total_us in ms
let embedHistory = $state<number[]>([]); // embed_total_us in ms
let queueHistory = $state<number[]>([]); // queue_depth
let captureBreakdown = $state<{ capture: number; ocr: number; resize: number; save: number }>({
  capture: 0,
  ocr: 0,
  resize: 0,
  save: 0,
});
let embedBreakdown = $state<{ vision: number; text: number }>({ vision: 0, text: 0 });

function pushHistory(arr: number[], val: number): number[] {
  const next = [...arr, val];
  return next.length > HISTORY_LEN ? next.slice(next.length - HISTORY_LEN) : next;
}

// ── Recommended image size for current model ──────────────────────────────
const recommendedSize = $derived.by(() => {
  if (config.embed_backend === "mmproj" || config.embed_backend === "llm-vlm") return 768;
  if (config.fastembed_model === "nomic-embed-vision-v1.5") return 768;
  return 768;
});

// ── Load ───────────────────────────────────────────────────────────────────
async function load() {
  config = await daemonInvoke<ScreenshotConfig>("get_screenshot_config");
  try {
    estimate = await daemonInvoke<ReembedEstimate | null>("estimate_screenshot_reembed");
  } catch {
    estimate = null;
  }
  try {
    sharedTextModel = await daemonInvoke<string>("get_embedding_model");
  } catch {
    sharedTextModel = "";
  }

  if (isMac) {
    try {
      screenPermission = await invoke<boolean>("check_screen_recording_permission");
    } catch {
      screenPermission = null;
    }
  }
}

// ── Save config ────────────────────────────────────────────────────────────
async function save() {
  saving = true;
  try {
    const result = await daemonInvoke<ConfigChangeResult>("set_screenshot_config", { config });
    modelChanged = result.model_changed;
    staleCount = result.stale_count;
    if (result.model_changed) {
      try {
        estimate = await daemonInvoke<ReembedEstimate | null>("estimate_screenshot_reembed");
      } catch (e) {}
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

// ── Re-embed ───────────────────────────────────────────────────────────────
async function reembed() {
  reembedding = true;
  progress = null;
  try {
    await daemonInvoke("rebuild_screenshot_embeddings");
    modelChanged = false;
    staleCount = 0;
    try {
      estimate = await daemonInvoke<ReembedEstimate | null>("estimate_screenshot_reembed");
    } catch (e) {}
  } finally {
    reembedding = false;
    progress = null;
  }
}

async function refreshMetrics() {
  try {
    const m = await daemonInvoke<PipelineMetrics>("get_screenshot_metrics");
    pipeMetrics = m;
    // Push to rolling history (convert µs → ms)
    captureHistory = pushHistory(captureHistory, m.capture_total_us / 1000);
    embedHistory = pushHistory(embedHistory, m.embed_total_us / 1000);
    queueHistory = pushHistory(queueHistory, m.queue_depth);
    captureBreakdown = {
      capture: m.capture_us / 1000,
      ocr: m.ocr_us / 1000,
      resize: m.resize_us / 1000,
      save: m.save_us / 1000,
    };
    embedBreakdown = {
      vision: m.vision_embed_us / 1000,
      text: m.text_embed_us / 1000,
    };
  } catch (e) {}
}

onMount(async () => {
  await load();
  await refreshMetrics();
  unlisten = await listen<{ done: number; total: number; elapsed_secs: number; eta_secs: number }>(
    "screenshot-reembed-progress",
    (e) => {
      progress = e.payload;
    },
  );
  // Poll metrics every 2s when enabled
  metricsTimer = setInterval(() => {
    if (config.enabled) refreshMetrics();
  }, 2000);
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
  <ScreenshotPermissionNotice
    {isMac}
    {screenPermission}
    onOpenSettings={() => invoke("open_screen_recording_settings")}
  />


  <!-- ── Enable + Session-only toggles ───────────────────────────────────── -->
  <ScreenshotToggleCard
    enabled={config.enabled}
    sessionOnly={config.session_only}
    ocrEnabled={config.ocr_enabled}
    useGpu={config.use_gpu}
    onToggleEnabled={toggleEnabled}
    onToggleSessionOnly={toggleSessionOnly}
    onToggleOcr={toggleOcr}
    onToggleGpu={toggleGpu}
  />

  <!-- ── Capture settings ────────────────────────────────────────────────── -->
  <ScreenshotCaptureSettingsSection
    {config}
    {saving}
    {recommendedSize}
    onUpdate={(patch, adoptRecommended = false) => {
      config = { ...config, ...patch };
      if (adoptRecommended) config = { ...config, image_size: recommendedSize };
    }}
    onSave={save}
  />

  <!-- ── Model changed banner ────────────────────────────────────────────── -->
  <ScreenshotReembedSection
    {modelChanged}
    {staleCount}
    {estimate}
    {reembedding}
    {progress}
    onReembed={reembed}
    onDismissModelChanged={() => { modelChanged = false; }}
  />

  <!-- ── OCR Text Extraction ─────────────────────────────────────────────── -->
  <ScreenshotOcrSection
    enabled={config.ocr_enabled}
    {isMac}
    ocrEngine={config.ocr_engine}
    useGpu={config.use_gpu}
    {sharedTextModel}
    {saving}
    onSetOcrEngine={(engine) => { config.ocr_engine = engine; }}
    onSave={save}
  />

  <!-- ── Pipeline Performance ──────────────────────────────────────────── -->
  <ScreenshotPerformanceSection
    enabled={config.enabled}
    metrics={pipeMetrics}
    {captureHistory}
    {embedHistory}
    {queueHistory}
    {captureBreakdown}
    {embedBreakdown}
    onRefresh={refreshMetrics}
  />

  <!-- ── Privacy note ────────────────────────────────────────────────────── -->
  <ScreenshotPrivacyNote />

</section>
