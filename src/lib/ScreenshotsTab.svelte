<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Screenshots tab — capture, embedding model, re-embed -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Button }    from "$lib/components/ui/button";
  import { Card, CardContent } from "$lib/components/ui/card";
  import { Separator } from "$lib/components/ui/separator";
  import { t }         from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────
  interface ScreenshotConfig {
    enabled:         boolean;
    interval_secs:   number;
    image_size:      number;
    quality:         number;
    session_only:    boolean;
    embed_backend:   string;
    fastembed_model: string;
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
    image_size: 224,
    quality: 60,
    session_only: true,
    embed_backend: "fastembed",
    fastembed_model: "clip-vit-b-32",
  });

  let saving      = $state(false);
  let reembedding = $state(false);
  let screenPermission = $state<boolean | null>(null);
  const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  let estimate    = $state<ReembedEstimate | null>(null);
  let progress    = $state<{ done: number; total: number; elapsed_secs: number; eta_secs: number } | null>(null);
  let modelChanged = $state(false);
  let staleCount   = $state(0);
  let unlisten: UnlistenFn | null = null;

  // ── Recommended image size for current model ──────────────────────────────
  const recommendedSize = $derived.by(() => {
    if (config.embed_backend === "mmproj") return 384;
    if (config.fastembed_model === "nomic-embed-vision-v1.5") return 384;
    return 224;
  });

  // ── Load ───────────────────────────────────────────────────────────────────
  async function load() {
    config = await invoke<ScreenshotConfig>("get_screenshot_config");
    try {
      estimate = await invoke<ReembedEstimate | null>("estimate_screenshot_reembed");
    } catch { estimate = null; }
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
        try { estimate = await invoke<ReembedEstimate | null>("estimate_screenshot_reembed"); } catch {}
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
      try { estimate = await invoke<ReembedEstimate | null>("estimate_screenshot_reembed"); } catch {}
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

  onMount(async () => {
    await load();
    unlisten = await listen<{ done: number; total: number; elapsed_secs: number; eta_secs: number }>(
      "screenshot-reembed-progress", e => { progress = e.payload; }
    );
  });
  onDestroy(() => unlisten?.());
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

    </CardContent>
  </Card>

  <!-- ── Capture settings ────────────────────────────────────────────────── -->
  {#if config.enabled}
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-4 px-4 flex flex-col gap-4">

      <!-- Interval -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="ss-interval" class="text-[0.72rem] font-semibold text-foreground">
            {t("screenshots.interval")}
          </label>
          <span class="text-[0.58rem] text-muted-foreground tabular-nums">
            {config.interval_secs} {t("screenshots.intervalUnit")}
          </span>
        </div>
        <input id="ss-interval" type="range" min="1" max="30" step="1"
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
        <input id="ss-size" type="range" min="128" max="512" step="16"
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
