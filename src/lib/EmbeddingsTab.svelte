<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { Separator } from "$lib/components/ui/separator";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { t } from "$lib/i18n/index.svelte";
import { addToast } from "$lib/stores/toast.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
interface ModelInfo {
  code: string;
  dim: number;
  description: string;
}

// ── State ──────────────────────────────────────────────────────────────────
let models = $state<ModelInfo[]>([]);
let currentCode = $state("");
let staleCount = $state(0);
let saving = $state(false);
let reembedding = $state(false);
let progress = $state<{ done: number; total: number } | null>(null);
let unlisten: UnlistenFn | null = null;

// ── Reembed config ────────────────────────────────────────────────────────
interface ReembedConfig {
  auto_labels: boolean;
  auto_eeg: boolean;
  auto_screenshots: boolean;
  batch_size: number;
  batch_delay_ms: number;
}
let reembedCfg = $state<ReembedConfig>({
  auto_labels: false,
  auto_eeg: false,
  auto_screenshots: false,
  batch_size: 10,
  batch_delay_ms: 50,
});
let savingCfg = $state(false);

async function loadReembedConfig() {
  try {
    reembedCfg = await daemonInvoke<ReembedConfig>("get_reembed_config");
  } catch (_) {}
}

// ── Daemon watchdog config ─────────────────────────────────────────────────
interface WatchdogConfig {
  enabled: boolean;
  timeout_secs: number;
}
let watchdogCfg = $state<WatchdogConfig>({ enabled: true, timeout_secs: 10 });

async function loadWatchdogConfig() {
  try {
    watchdogCfg = await daemonInvoke<WatchdogConfig>("get_daemon_watchdog");
  } catch (_) {}
}

async function saveWatchdogConfig() {
  try {
    await daemonInvoke("set_daemon_watchdog", watchdogCfg);
  } catch (_) {}
}

async function saveReembedConfig() {
  savingCfg = true;
  try {
    await daemonInvoke("set_reembed_config", reembedCfg);
  } finally {
    savingCfg = false;
  }
}

const activeModel = $derived(models.find((m) => m.code === currentCode) ?? null);

// Group models by family for the dropdown
const grouped = $derived.by(() => {
  const families: Record<string, ModelInfo[]> = {};
  for (const m of models) {
    const family = m.code.split("/")[0];
    // biome-ignore lint/suspicious/noAssignInExpressions: idiomatic grouped-push pattern
    (families[family] ??= []).push(m);
  }
  return families;
});

// ── Load ───────────────────────────────────────────────────────────────────
async function load() {
  // All fastembed-supported text embedding models.
  // The daemon currently uses nomic-embed-text-v1.5 (hardcoded).
  // Switching requires re-embedding all labels.
  const knownModels: ModelInfo[] = [
    {
      code: "nomic-ai/nomic-embed-text-v1.5",
      dim: 768,
      description: "Nomic Embed Text v1.5 — high-quality 768d, recommended",
    },
    {
      code: "nomic-ai/nomic-embed-text-v1.5-Q",
      dim: 768,
      description: "Nomic Embed Text v1.5 quantized — faster, slightly less accurate",
    },
    { code: "nomic-ai/nomic-embed-text-v1", dim: 768, description: "Nomic Embed Text v1 — previous generation 768d" },
    { code: "BAAI/bge-small-en-v1.5", dim: 384, description: "BGE Small EN v1.5 — fast, compact 384d" },
    { code: "BAAI/bge-small-en-v1.5-Q", dim: 384, description: "BGE Small EN v1.5 quantized — fastest, 384d" },
    { code: "BAAI/bge-base-en-v1.5", dim: 768, description: "BGE Base EN v1.5 — balanced 768d" },
    { code: "BAAI/bge-large-en-v1.5", dim: 1024, description: "BGE Large EN v1.5 — highest quality 1024d" },
    {
      code: "sentence-transformers/all-MiniLM-L6-v2",
      dim: 384,
      description: "All-MiniLM-L6-v2 — lightweight 384d, good for general use",
    },
    {
      code: "sentence-transformers/all-MiniLM-L12-v2",
      dim: 384,
      description: "All-MiniLM-L12-v2 — deeper 384d variant",
    },
    {
      code: "sentence-transformers/all-mpnet-base-v2",
      dim: 768,
      description: "MPNet Base v2 — strong 768d general-purpose",
    },
    {
      code: "sentence-transformers/paraphrase-MiniLM-L12-v2",
      dim: 384,
      description: "Paraphrase MiniLM — 384d, good for semantic similarity",
    },
    { code: "intfloat/multilingual-e5-small", dim: 384, description: "Multilingual E5 Small — 384d, 100+ languages" },
    { code: "intfloat/multilingual-e5-base", dim: 768, description: "Multilingual E5 Base — 768d, 100+ languages" },
    { code: "mixedbread-ai/mxbai-embed-large-v1", dim: 1024, description: "MxBAI Embed Large — top-tier 1024d" },
    { code: "Alibaba-NLP/gte-base-en-v1.5", dim: 768, description: "GTE Base EN v1.5 — strong 768d from Alibaba" },
    { code: "BAAI/bge-m3", dim: 1024, description: "BGE-M3 — multilingual, multi-granularity 1024d" },
  ];
  try {
    const raw = await daemonInvoke<ModelInfo[] | Record<string, unknown>>("list_embedding_models");
    models = Array.isArray(raw) && raw.length > 0 ? raw : knownModels;
  } catch {
    models = knownModels;
  }
  try {
    const r = await daemonInvoke<{ model: string }>("get_embedding_model");
    currentCode = r.model || models[0]?.code || "";
  } catch {
    currentCode = models[0]?.code ?? "";
  }
  staleCount = 0;
  models.sort((a, b) => {
    if (a.code === "nomic-ai/nomic-embed-text-v1.5") return -1;
    if (b.code === "nomic-ai/nomic-embed-text-v1.5") return 1;
    return a.code.localeCompare(b.code);
  });
}

// ── Apply model change ─────────────────────────────────────────────────────
async function applyModel() {
  saving = true;
  try {
    const r = await daemonInvoke<{ ok: boolean; model?: string; error?: string }>("set_embedding_model", {
      model: currentCode,
    });
    if (r.ok) {
      addToast("success", t("embeddings.modelApplied"), currentCode, 3000);
      staleCount = 0;
    } else {
      addToast("warning", t("embeddings.modelFailed"), r.error ?? "Unknown error", 5000);
    }
  } catch (e) {
    addToast("warning", t("embeddings.modelFailed"), String(e), 5000);
  } finally {
    saving = false;
  }
}

// ── Re-embed all ──────────────────────────────────────────────────────────
async function reembed() {
  reembedding = true;
  progress = null;
  try {
    await daemonInvoke("reembed_all_labels");
    staleCount = 0;
  } finally {
    reembedding = false;
    progress = null;
  }
}

onMount(async () => {
  await Promise.all([load(), loadReembedConfig(), loadWatchdogConfig()]);
  unlisten = await listen<{ done: number; total: number }>("embed-progress", (e) => {
    progress = e.payload;
  });
});
onDestroy(() => unlisten?.());

// Dim badge colour
function dimColor(_dim: number) {
  return "bg-primary/10 text-primary border-primary/20";
}
</script>

<section class="flex flex-col gap-5">

  <!-- ── Model picker ────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center justify-between">
      <span class="text-[0.78rem] font-semibold text-foreground">
        {t("embeddings.model")}
      </span>
      <Button size="sm" onclick={applyModel} disabled={saving}
              class="text-[0.65rem] h-7 px-3">
        {saving ? t("common.saving") : t("common.apply")}
      </Button>
    </div>

    <select
      aria-label="Embedding code"
      bind:value={currentCode}
      class="w-full rounded-lg border border-border dark:border-white/[0.08]
             bg-white dark:bg-[#14141e] px-3 py-2
             text-[0.75rem] text-foreground
              focus:outline-none focus:ring-1 focus:ring-ring/50">
      {#each Object.entries(grouped) as [family, mods]}
        <optgroup label={family}>
          {#each mods as m}
            <option value={m.code}>{m.code.split("/")[1]} — {m.dim}d</option>
          {/each}
        </optgroup>
      {/each}
    </select>

    <!-- Active model info card -->
    {#if activeModel}
      <div class="rounded-xl border border-border dark:border-white/[0.07]
                  bg-white dark:bg-[#14141e] px-4 py-3 flex items-start gap-3">
        <div class="flex flex-col gap-1 flex-1 min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            <span class="text-[0.72rem] font-semibold text-foreground truncate">
              {activeModel.code}
            </span>
            <span class="rounded border px-1.5 py-0 text-[0.56rem] font-semibold
                         {dimColor(activeModel.dim)}">
              {activeModel.dim}d
            </span>
          </div>
          <p class="text-[0.65rem] text-muted-foreground/70 leading-snug">
            {activeModel.description}
          </p>
        </div>
      </div>
    {/if}
  </div>

  <!-- ── Info callout ────────────────────────────────────────────────────── -->
    <div class="rounded-xl border border-primary/20 bg-primary/5
          dark:bg-primary/10 px-4 py-3 flex gap-3">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
         stroke-linecap="round" stroke-linejoin="round"
        class="w-4 h-4 shrink-0 text-primary mt-0.5">
      <circle cx="12" cy="12" r="10"/>
      <line x1="12" y1="8" x2="12" y2="12"/>
      <line x1="12" y1="16" x2="12.01" y2="16"/>
    </svg>
    <div class="flex flex-col gap-1">
      <p class="text-[0.65rem] text-primary leading-relaxed">
        {t("embeddings.info")}
      </p>
      <p class="text-[0.6rem] text-primary/70 leading-relaxed">
        {t("embeddings.sharedNote")}
      </p>
    </div>
  </div>

  <!-- ── Re-embed all ────────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <div class="flex items-center justify-between">
      <div class="flex flex-col gap-0.5">
        <div class="flex items-center gap-2">
          <span class="text-[0.72rem] font-semibold text-foreground">
            {t("embeddings.reembed")}
          </span>
          {#if staleCount > 0}
            <span class="rounded-full px-1.5 py-0 text-[0.55rem] font-semibold
                         bg-amber-500/15 text-amber-600 dark:text-amber-400 border border-amber-500/25">
              {staleCount} {t("embeddings.stale")}
            </span>
          {/if}
        </div>
        <span class="text-[0.6rem] text-muted-foreground/60">
          {t("embeddings.reembedDesc")}
        </span>
      </div>
      <Button size="sm" variant="outline" onclick={reembed} disabled={reembedding}
              class="text-[0.65rem] h-7 px-3 shrink-0">
        {reembedding ? t("embeddings.reembedding") : t("embeddings.reembedBtn")}
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
        </span>
      </div>
    {/if}
  </div>

  <!-- ── Dim legend ──────────────────────────────────────────────────────── -->
  <div class="flex items-center gap-3 pt-1">
    <span class="text-[0.56rem] font-semibold uppercase tracking-wider text-muted-foreground/50 shrink-0">
      {t("embeddings.dimLegend")}
    </span>
    {#each [[384,"≤384d"],[768,"≤768d"],[1024,"≤1024d"]] as [,label]}
      <span class="rounded border px-1.5 py-0 text-[0.56rem] font-semibold
                   bg-primary/10 text-primary border-primary/20">
        {label}
      </span>
    {/each}
    <span class="text-[0.56rem] text-muted-foreground/40">{t("embeddings.dimHint")}</span>
  </div>

  <Separator />

  <!-- ── Auto re-embed settings ──────────────────────────────────────────── -->
  <div class="flex flex-col gap-3">
    <span class="text-[0.72rem] font-semibold text-foreground">
      {t("embeddings.autoReembed.title")}
    </span>
    <p class="text-[0.6rem] text-muted-foreground/60 leading-relaxed -mt-1">
      {t("embeddings.autoReembed.desc")}
    </p>

    <!-- Toggle switches -->
    <div class="flex flex-col gap-2">
      <label class="flex items-center justify-between gap-2 cursor-pointer">
        <span class="text-[0.68rem] text-foreground">{t("embeddings.autoReembed.labels")}</span>
        <input type="checkbox" bind:checked={reembedCfg.auto_labels} onchange={saveReembedConfig}
               class="w-8 h-4 rounded-full appearance-none bg-muted dark:bg-white/[0.08]
                      checked:bg-primary relative cursor-pointer transition-colors
                      after:content-[''] after:absolute after:top-0.5 after:left-0.5
                      after:w-3 after:h-3 after:rounded-full after:bg-white after:transition-transform
                      checked:after:translate-x-4" />
      </label>
      <label class="flex items-center justify-between gap-2 cursor-pointer">
        <span class="text-[0.68rem] text-foreground">{t("embeddings.autoReembed.eeg")}</span>
        <input type="checkbox" bind:checked={reembedCfg.auto_eeg} onchange={saveReembedConfig}
               class="w-8 h-4 rounded-full appearance-none bg-muted dark:bg-white/[0.08]
                      checked:bg-primary relative cursor-pointer transition-colors
                      after:content-[''] after:absolute after:top-0.5 after:left-0.5
                      after:w-3 after:h-3 after:rounded-full after:bg-white after:transition-transform
                      checked:after:translate-x-4" />
      </label>
      <label class="flex items-center justify-between gap-2 cursor-pointer">
        <span class="text-[0.68rem] text-foreground">{t("embeddings.autoReembed.screenshots")}</span>
        <input type="checkbox" bind:checked={reembedCfg.auto_screenshots} onchange={saveReembedConfig}
               class="w-8 h-4 rounded-full appearance-none bg-muted dark:bg-white/[0.08]
                      checked:bg-primary relative cursor-pointer transition-colors
                      after:content-[''] after:absolute after:top-0.5 after:left-0.5
                      after:w-3 after:h-3 after:rounded-full after:bg-white after:transition-transform
                      checked:after:translate-x-4" />
      </label>
    </div>

    <!-- Backpressure controls -->
    <div class="grid grid-cols-2 gap-3 mt-1">
      <div class="flex flex-col gap-1">
        <label for="batch-size" class="text-[0.6rem] text-muted-foreground/60">
          {t("embeddings.autoReembed.batchSize")}
        </label>
        <input id="batch-size" type="number" min="1" max="100"
               bind:value={reembedCfg.batch_size} onchange={saveReembedConfig}
               class="w-full rounded-md border border-border dark:border-white/[0.08]
                      bg-white dark:bg-[#14141e] px-2.5 py-1.5
                      text-[0.7rem] text-foreground tabular-nums
                      focus:outline-none focus:ring-1 focus:ring-ring/50" />
      </div>
      <div class="flex flex-col gap-1">
        <label for="batch-delay" class="text-[0.6rem] text-muted-foreground/60">
          {t("embeddings.autoReembed.batchDelay")}
        </label>
        <input id="batch-delay" type="number" min="0" max="5000" step="10"
               bind:value={reembedCfg.batch_delay_ms} onchange={saveReembedConfig}
               class="w-full rounded-md border border-border dark:border-white/[0.08]
                      bg-white dark:bg-[#14141e] px-2.5 py-1.5
                      text-[0.7rem] text-foreground tabular-nums
                      focus:outline-none focus:ring-1 focus:ring-ring/50" />
      </div>
    </div>
  </div>

  <Separator />

  <!-- ── Daemon watchdog ─────────────────────────────────────────────────── -->
  <div class="flex flex-col gap-3">
    <span class="text-[0.72rem] font-semibold text-foreground">
      {t("embeddings.watchdog.title")}
    </span>
    <p class="text-[0.6rem] text-muted-foreground/60 leading-relaxed -mt-1">
      {t("embeddings.watchdog.desc")}
    </p>
    <label class="flex items-center justify-between gap-2 cursor-pointer">
      <span class="text-[0.68rem] text-foreground">{t("embeddings.watchdog.enabled")}</span>
      <input type="checkbox" bind:checked={watchdogCfg.enabled} onchange={saveWatchdogConfig}
             class="w-8 h-4 rounded-full appearance-none bg-muted dark:bg-white/[0.08]
                    checked:bg-primary relative cursor-pointer transition-colors
                    after:content-[''] after:absolute after:top-0.5 after:left-0.5
                    after:w-3 after:h-3 after:rounded-full after:bg-white after:transition-transform
                    checked:after:translate-x-4" />
    </label>
    {#if watchdogCfg.enabled}
      <div class="flex flex-col gap-1">
        <label for="watchdog-timeout" class="text-[0.6rem] text-muted-foreground/60">
          {t("embeddings.watchdog.timeout")}
        </label>
        <input id="watchdog-timeout" type="number" min="5" max="120" step="5"
               bind:value={watchdogCfg.timeout_secs} onchange={saveWatchdogConfig}
               class="w-32 rounded-md border border-border dark:border-white/[0.08]
                      bg-white dark:bg-[#14141e] px-2.5 py-1.5
                      text-[0.7rem] text-foreground tabular-nums
                      focus:outline-none focus:ring-1 focus:ring-ring/50" />
      </div>
    {/if}
  </div>

</section>
