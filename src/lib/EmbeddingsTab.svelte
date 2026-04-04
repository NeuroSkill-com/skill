<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { t } from "$lib/i18n/index.svelte";

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
  [models, currentCode, staleCount] = await Promise.all([
    daemonInvoke<ModelInfo[]>("list_embedding_models"),
    daemonInvoke<string>("get_embedding_model"),
    daemonInvoke<number>("get_stale_label_count"),
  ]);
  // Sort: put default (bge-small-en) first, then alphabetically
  models.sort((a, b) => {
    if (a.code === "Xenova/bge-small-en-v1.5") return -1;
    if (b.code === "Xenova/bge-small-en-v1.5") return 1;
    return a.code.localeCompare(b.code);
  });
}

// ── Apply model change ─────────────────────────────────────────────────────
async function applyModel() {
  saving = true;
  try {
    await daemonInvoke("set_embedding_model", { modelCode: currentCode });
    staleCount = 0; // backfill runs in background after set_embedding_model
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
  await load();
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

</section>
