<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- EXG model picker — browse families from exg_catalog.json, download weights, select active model. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { onDaemonEvent } from "$lib/daemon/ws";
import { t } from "$lib/i18n/index.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
interface ExgFamily {
  name: string;
  description: string;
  repo: string;
  tags: string[];
  weights_file: string;
  config_file: string | null;
  params_m: number;
  embed_dim: number;
  paper: string;
  doi: string;
  weights_cached: boolean;
  /** Optional URL to a preview image shown in the model detail card. */
  preview_image?: string | null;
}
interface ExgModelEntry {
  family: string;
  filename: string;
  size_mb: number;
  description: string;
  recommended?: boolean;
}
interface ExgCatalog {
  active_model: string;
  families: Record<string, ExgFamily>;
  models: ExgModelEntry[];
}
interface ExgModelConfig {
  hf_repo: string;
  hnsw_m: number;
  hnsw_ef_construction: number;
  data_norm: number;
  model_backend: string;
  luna_variant: string;
  luna_hf_repo: string;
}
interface EegModelStatus {
  encoder_loaded: boolean;
  weights_found: boolean;
  downloading_weights: boolean;
  download_progress: number;
  download_status_msg: string | null;
}

interface Props {
  modelConfig: ExgModelConfig;
  modelStatus: EegModelStatus;
  onSaveConfig: (patch: Partial<ExgModelConfig>) => Promise<void>;
  onStartDownload: () => Promise<void>;
  onCancelDownload: () => Promise<void>;
}

let { modelConfig, modelStatus, onSaveConfig, onStartDownload, onCancelDownload }: Props = $props();

// ── State ──────────────────────────────────────────────────────────────────
let catalog = $state<ExgCatalog | null>(null);
let selectedFamilyId = $state("");
let loading = $state(true);
let loadError = $state<string | null>(null);

// ── Derived ────────────────────────────────────────────────────────────────
const familyIds = $derived(catalog ? Object.keys(catalog.families) : []);
const selectedFamily = $derived(catalog?.families[selectedFamilyId] ?? null);
const selectedModel = $derived(catalog?.models.find((m) => m.family === selectedFamilyId) ?? null);

// Map catalog family ID → ExgModelBackend string for config
function familyToBackend(id: string): string {
  if (id === "zuna") return "zuna";
  if (id.startsWith("luna-")) return "luna";
  if (id === "reve-base" || id === "reve-large") return "reve";
  if (id === "cbramod") return "cbramod";
  if (id === "eegpt") return "eegpt";
  if (id === "labram") return "labram";
  if (id === "signaljepa") return "signaljepa";
  if (id === "osf-base") return "osf";
  if (id === "sleepfm") return "sleepfm";
  if (id === "sleeplm") return "sleeplm";
  if (id === "sensorlm") return "sensorlm";
  if (id === "opentslm") return "opentslm";
  if (id.startsWith("steegformer-")) return "steegformer";
  return id;
}

// Determine active family from config
const activeFamilyId = $derived.by(() => {
  if (!catalog) return "";
  const backend = modelConfig.model_backend;
  if (backend === "luna") {
    const variant = modelConfig.luna_variant;
    return `luna-${variant}`;
  }
  if (backend === "zuna") return "zuna";
  // For other backends find matching family by backend name
  for (const [id, _fam] of Object.entries(catalog.families)) {
    if (familyToBackend(id) === backend) return id;
  }
  return "zuna";
});

const selectedIsActive = $derived(selectedFamilyId === activeFamilyId);
const RECOMMENDED_FAMILY = "zuna";

// ── Helpers ────────────────────────────────────────────────────────────────
function fmtMB(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}

function tagColor(tag: string): string {
  switch (tag) {
    case "eeg":
      return "bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20";
    case "ecg":
      return "bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/20";
    case "emg":
      return "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20";
    case "eog":
      return "bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border-cyan-500/20";
    case "multimodal":
      return "bg-pink-500/10 text-pink-600 dark:text-pink-400 border-pink-500/20";
    case "language":
      return "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20";
    case "sleep":
      return "bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 border-indigo-500/20";
    case "wearable":
      return "bg-teal-500/10 text-teal-600 dark:text-teal-400 border-teal-500/20";
    case "topology-agnostic":
      return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
    case "embedding":
      return "bg-slate-500/10 text-slate-500 border-slate-500/20";
    default:
      return "bg-slate-500/10 text-slate-500 border-slate-500/20";
  }
}

const SIZE_TAGS = new Set(["tiny", "small", "medium", "large", "default"]);

async function selectModel() {
  if (!selectedFamily || !selectedFamilyId) return;
  const id = selectedFamilyId;
  const backend = familyToBackend(id);

  if (backend === "luna") {
    const variant = id.replace("luna-", "");
    await onSaveConfig({
      model_backend: "luna",
      luna_variant: variant,
      luna_hf_repo: selectedFamily.repo,
    });
  } else {
    await onSaveConfig({
      model_backend: backend,
      hf_repo: selectedFamily.repo,
    });
  }
}

async function pickLocalWeights() {
  const file = await invoke<string | null>("pick_exg_weights_file");
  if (!file) return;
  // Set the repo to a local path sentinel, then trigger config update
  const backend = familyToBackend(selectedFamilyId);
  if (backend === "luna") {
    const variant = selectedFamilyId.replace("luna-", "");
    await onSaveConfig({
      model_backend: "luna",
      luna_variant: variant,
      luna_hf_repo: `local:${file}`,
    });
  } else {
    await onSaveConfig({
      model_backend: backend,
      hf_repo: `local:${file}`,
    });
  }
}

async function refreshCatalog() {
  loading = true;
  loadError = null;
  try {
    const result = await daemonInvoke<ExgCatalog>("get_exg_catalog");
    if (!result?.families) {
      throw new Error("Invalid catalog response: missing families");
    }
    catalog = result;
  } catch (e) {
    loadError = e instanceof Error ? e.message : String(e);
  } finally {
    loading = false;
  }
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
let unlistenDownload: (() => void) | undefined;

onMount(async () => {
  await refreshCatalog();
  if (activeFamilyId && catalog?.families[activeFamilyId]) {
    selectedFamilyId = activeFamilyId;
  } else if (familyIds.length > 0) {
    selectedFamilyId = familyIds[0];
  }

  // Refresh catalog after download completes so weights_cached updates
  const unsub1 = onDaemonEvent("ExgDownloadCompleted", () => refreshCatalog());
  const unsub2 = onDaemonEvent("ExgDownloadFailed", () => refreshCatalog());
  unlistenDownload = () => {
    unsub1();
    unsub2();
  };
});

onDestroy(() => {
  unlistenDownload?.();
});

$effect(() => {
  if (catalog && !selectedFamilyId && activeFamilyId) {
    selectedFamilyId = activeFamilyId;
  }
});
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span
      class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground"
    >
      {t("model.backend")}
    </span>
    <button
      onclick={refreshCatalog}
      class="ml-auto text-[0.56rem] text-muted-foreground/60 hover:text-foreground transition-colors cursor-pointer select-none"
    >
      {t("llm.btn.refresh")}
    </button>
  </div>

  {#if loading}
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e]">
      <CardContent class="flex items-center justify-center py-6">
        <span class="text-[0.72rem] text-muted-foreground">{t("common.loading")}</span>
      </CardContent>
    </Card>
  {:else if loadError || !catalog}
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e]">
      <CardContent class="flex flex-col items-center justify-center gap-2 py-6">
        <span class="text-[0.72rem] text-destructive">Failed to load model catalog</span>
        {#if loadError}
          <span class="text-[0.6rem] text-muted-foreground font-mono break-all px-4">{loadError}</span>
        {/if}
        <Button size="sm" variant="outline" class="h-6 text-[0.6rem] px-2" onclick={refreshCatalog}>Retry</Button>
      </CardContent>
    </Card>
  {:else}
    <!-- ── Active model banner ────────────────────────────────────────── -->
    {#if activeFamilyId && catalog.families[activeFamilyId]}
      {@const activeFam = catalog.families[activeFamilyId]}
      <button
        onclick={() => (selectedFamilyId = activeFamilyId)}
        class="flex items-center gap-2 w-full rounded-lg border px-3 py-2 text-left transition-all cursor-pointer
               {selectedFamilyId === activeFamilyId
          ? 'border-emerald-500/30 bg-emerald-500/8 dark:bg-emerald-950/20'
          : 'border-border/60 dark:border-white/[0.06] bg-white dark:bg-[#14141e] hover:border-emerald-500/30'}"
      >
        <span
          class="text-[0.52rem] font-semibold text-emerald-600 dark:text-emerald-400 shrink-0"
          >✓ ACTIVE</span
        >
        <span class="text-[0.68rem] font-semibold text-foreground truncate"
          >{activeFam.name}</span
        >
        <span class="text-[0.62rem] text-muted-foreground/60 shrink-0"
          >{activeFam.params_m}M params</span
        >
        <span class="text-[0.62rem] text-muted-foreground/60 shrink-0"
          >{activeFam.embed_dim}-dim</span
        >
      </button>
    {/if}

    <!-- ── Download progress banner ───────────────────────────────────── -->
    {#if modelStatus.downloading_weights}
      <div
        class="flex flex-col gap-1 w-full rounded-lg border border-blue-500/30 bg-blue-500/8 dark:bg-blue-950/20 px-3 py-2"
      >
        <div class="flex items-center gap-2 w-full">
          <span class="text-[0.52rem] font-semibold text-blue-500 shrink-0 animate-pulse"
            >⬇ DOWNLOADING</span
          >
          <span class="text-[0.68rem] font-semibold text-foreground truncate">
            {selectedFamily?.name ?? "Model"}
          </span>
        </div>
        <div class="h-1 w-full rounded-full bg-muted overflow-hidden">
          {#if modelStatus.download_progress > 0}
            <div
              class="h-full rounded-full bg-blue-500 transition-all duration-300"
              style="width:{(modelStatus.download_progress * 100).toFixed(1)}%"
            ></div>
          {:else}
            <div
              class="h-full w-2/5 rounded-full bg-blue-500 animate-[progress-indeterminate_1.6s_ease-in-out_infinite]"
            ></div>
          {/if}
        </div>
        {#if modelStatus.download_status_msg}
          <span class="text-[0.54rem] text-blue-500/80 truncate"
            >{modelStatus.download_status_msg}</span
          >
        {/if}
      </div>
    {/if}

    <!-- ── Family selector dropdown ───────────────────────────────────── -->
    <div class="relative">
      <select
        bind:value={selectedFamilyId}
        class="w-full appearance-none rounded-xl border border-border dark:border-white/[0.06]
               bg-white dark:bg-[#14141e] text-foreground text-[0.78rem] font-semibold
               px-3.5 py-2.5 pr-9 cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
      >
        {#each familyIds as id (id)}
          {@const fam = catalog.families[id]}
          <option value={id}>
            {fam.name} — {fam.params_m}M · {fam.embed_dim}-dim{id === activeFamilyId
              ? " ✓"
              : ""}{fam.weights_cached ? "" : " ⬇"}{id === RECOMMENDED_FAMILY
              ? " ★"
              : ""}
          </option>
        {/each}
      </select>
      <span
        class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground"
      >
        <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3"
          ><path d="M3 6l5 5 5-5H3z" /></svg
        >
      </span>
    </div>

    <!-- ── Selected family detail card ─────────────────────────────────── -->
    {#if selectedFamily && selectedModel}
      <Card
        class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden
               {!selectedFamily.weights_cached ? 'opacity-80' : ''}"
      >
        <CardContent class="py-0 px-0 flex flex-row">
          <!-- Preview image on the left (optional, e.g. brain visualisation for TRIBE v2) -->
          {#if selectedFamily.preview_image}
            <div class="shrink-0 w-36 self-stretch border-r border-border/40 dark:border-white/[0.04] bg-black/5 dark:bg-white/[0.03] overflow-hidden flex items-center justify-center">
              <img
                src={selectedFamily.preview_image}
                alt="{selectedFamily.name} preview"
                class="w-full h-full object-cover"
                onerror={(e) => { const p = (e.currentTarget as HTMLElement).closest('div'); if (p) (p as HTMLElement).style.display = 'none'; }}
              />
            </div>
          {/if}
          <!-- Right side content -->
          <div class="flex flex-col flex-1 min-w-0">
          <!-- Description + tags -->
          <div class="px-4 pt-3.5 pb-3 flex flex-col gap-1.5">
            <div class="flex items-center gap-1.5">
              {#if selectedFamilyId === RECOMMENDED_FAMILY}
                <span class="text-emerald-500 text-[0.8rem]" title="Recommended">✅</span>
              {/if}
              <span class="text-[0.82rem] font-bold text-foreground">{selectedFamily.name}</span>
              {#if selectedFamilyId === RECOMMENDED_FAMILY}
                <Badge
                  variant="outline"
                  class="text-[0.5rem] py-0 px-1.5 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
                  >Recommended</Badge
                >
              {/if}
            </div>
            <p class="text-[0.68rem] text-muted-foreground leading-snug">
              {selectedFamily.description}
            </p>
            <div class="flex items-center gap-1 flex-wrap">
              {#each selectedFamily.tags.filter((t: string) => !SIZE_TAGS.has(t)) as tag}
                <Badge variant="outline" class="text-[0.5rem] py-0 px-1.5 {tagColor(tag)}"
                  >{tag}</Badge
                >
              {/each}
            </div>
          </div>

          <!-- Model specs grid -->
          <div
            class="grid grid-cols-[auto_auto_auto_auto_1fr] gap-x-4 items-center px-4 py-2 border-t border-b border-border/40 dark:border-white/[0.04] bg-slate-50 dark:bg-[#111118]"
          >
            <span
              class="text-[0.54rem] font-semibold uppercase tracking-widest text-muted-foreground/60"
              >Params</span
            >
            <span
              class="text-[0.54rem] font-semibold uppercase tracking-widest text-muted-foreground/60"
              >Embed</span
            >
            <span
              class="text-[0.54rem] font-semibold uppercase tracking-widest text-muted-foreground/60"
              >Size</span
            >
            <span
              class="text-[0.54rem] font-semibold uppercase tracking-widest text-muted-foreground/60"
              >Repo</span
            >
            <span></span>
          </div>
          <div class="grid grid-cols-[auto_auto_auto_auto_1fr] gap-x-4 items-center px-4 py-2.5">
            <span class="text-[0.74rem] font-bold font-mono text-foreground"
              >{selectedFamily.params_m}M</span
            >
            <span class="text-[0.72rem] font-semibold tabular-nums text-muted-foreground"
              >{selectedFamily.embed_dim}-dim</span
            >
            <span class="text-[0.72rem] font-semibold tabular-nums text-muted-foreground"
              >{fmtMB(selectedModel.size_mb)}</span
            >
            <span class="text-[0.62rem] font-mono text-muted-foreground/70 truncate max-w-[10rem]"
              >{selectedFamily.repo}</span
            >
            <div class="flex items-center gap-1 justify-end">
              {#if selectedIsActive}
                <Badge
                  variant="outline"
                  class="text-[0.52rem] py-0 px-1.5 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20"
                >
                  ✓ Active
                </Badge>
              {:else if selectedFamily.weights_cached}
                <Badge
                  variant="outline"
                  class="text-[0.52rem] py-0 px-1.5 bg-sky-500/10 text-sky-600 dark:text-sky-400 border-sky-500/20"
                >
                  Downloaded
                </Badge>
              {:else}
                <Badge
                  variant="outline"
                  class="text-[0.52rem] py-0 px-1.5 bg-slate-500/10 text-slate-500 border-slate-500/20"
                >
                  Not downloaded
                </Badge>
              {/if}
            </div>
          </div>
          <div class="px-4 pb-2">
            <p class="text-[0.5rem] text-muted-foreground/70 font-mono break-all">🤗 hf download {selectedFamily.repo} {selectedFamily.weights_file}</p>
          </div>

          <!-- Actions -->
          <div
            class="flex items-center gap-2 px-4 py-3 border-t border-border/40 dark:border-white/[0.04]"
          >
            <!-- Paper link -->
            {#if selectedFamily.paper}
              <a
                href={selectedFamily.paper}
                target="_blank"
                rel="noopener noreferrer"
                class="text-[0.58rem] text-primary hover:underline truncate"
              >
                📄 Paper{selectedFamily.doi ? ` (${selectedFamily.doi})` : ""}
              </a>
            {/if}

            <div class="ml-auto flex items-center gap-1.5">
              {#if modelStatus.downloading_weights}
                <Button
                  size="sm"
                  variant="outline"
                  class="h-6 text-[0.6rem] px-2 text-destructive border-destructive/30 hover:bg-destructive/10"
                  onclick={onCancelDownload}
                >
                  Cancel
                </Button>
              {:else if selectedIsActive}
                <Button
                  size="sm"
                  class="h-6 text-[0.6rem] px-2.5 bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20"
                  disabled
                >
                  Active
                </Button>
              {:else if selectedFamily.weights_cached}
                <Button
                  size="sm"
                  class="h-6 text-[0.6rem] px-2.5 bg-violet-600 hover:bg-violet-700 text-white"
                  onclick={selectModel}
                >
                  Use this model
                </Button>
              {:else}
                <!-- Not downloaded: offer HF download or local file pick -->
                <Button
                  size="sm"
                  variant="outline"
                  class="h-6 text-[0.6rem] px-2"
                  onclick={pickLocalWeights}
                >
                  📂 Local file…
                </Button>
                <Button
                  size="sm"
                  class="h-6 text-[0.6rem] px-2.5 bg-violet-600 hover:bg-violet-700 text-white"
                  onclick={async () => {
                    await selectModel();
                    await onStartDownload();
                  }}
                >
                  ⬇ Download {fmtMB(selectedModel.size_mb)}
                </Button>
              {/if}
            </div>
          </div>
          </div><!-- end right side content -->
        </CardContent>
      </Card>
    {/if}
  {/if}
</section>
