<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  LLM Settings Tab
  ─────────────────
  • Family dropdown → shows all quants for the selected family
  • Progress bar per quant while downloading
  • Advanced inference settings (GPU layers, ctx size, etc.)
  • Server log viewer with auto-scroll
-->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { fmtGB } from "$lib/format";
import { t } from "$lib/i18n/index.svelte";
import {
  autoSelectFamily,
  buildFamilies,
  compareModelEntries,
  type DownloadState,
  familyOptionLabel,
  type LlmCatalog,
  type LlmModelEntry,
  type ModelFamily,
  runModeLabel,
  splitEntryGroups,
  tagColor,
  tagLabel,
  vendorLabel,
} from "$lib/llm-helpers";
import LlmInferenceSection from "$lib/llm/LlmInferenceSection.svelte";
import LlmServerLogSection from "$lib/llm/LlmServerLogSection.svelte";
import LlmServerSection from "$lib/llm/LlmServerSection.svelte";

// ── Types ──────────────────────────────────────────────────────────────────

interface LlmLogEntry {
  ts: number;
  level: string;
  message: string;
}

type ToolExecutionMode = "sequential" | "parallel";
interface WebSearchProvider {
  backend: "duckduckgo" | "brave" | "searxng";
  brave_api_key: string;
  searxng_url: string;
}

interface LlmToolsConfig {
  enabled: boolean;
  date: boolean;
  location: boolean;
  web_search: boolean;
  web_fetch: boolean;
  web_search_provider: WebSearchProvider;
  bash: boolean;
  read_file: boolean;
  write_file: boolean;
  edit_file: boolean;
  execution_mode: ToolExecutionMode;
  max_rounds: number;
  max_calls_per_round: number;
  retry: { max_retries: number; base_delay_ms: number };
}

interface LlmConfig {
  enabled: boolean;
  autostart: boolean;
  model_path: string | null;
  n_gpu_layers: number;
  ctx_size: number | null;
  parallel: number;
  api_key: string | null;
  tools: LlmToolsConfig;
  mmproj: string | null;
  mmproj_n_threads: number;
  no_mmproj_gpu: boolean;
  autoload_mmproj: boolean;
  verbose: boolean;
  gpu_memory_threshold: number;
  gpu_memory_gen_threshold: number;
}

interface ModelFamily {
  id: string;
  name: string;
  desc: string;
  tags: string[];
  vendors: string[];
  entries: LlmModelEntry[]; // non-mmproj, in catalog order
  mmproj: LlmModelEntry[];
  recommended: LlmModelEntry | undefined;
  downloaded: LlmModelEntry[];
}

interface ModelHardwareFit {
  filename: string;
  fitLevel: "perfect" | "good" | "marginal" | "too_tight";
  runMode: "gpu" | "moe" | "cpu_gpu" | "cpu";
  memoryRequiredGb: number;
  memoryAvailableGb: number;
  estimatedTps: number;
  score: number;
  notes: string[];
}

// Re-export fmtGB as fmtSize for template use
const fmtSize = fmtGB;

// ── State ──────────────────────────────────────────────────────────────────

let hardwareFits = $state<Map<string, ModelHardwareFit>>(new Map());

let catalog = $state<LlmCatalog>({ entries: [], active_model: "", active_mmproj: "" });
let config = $state<LlmConfig>({
  enabled: false,
  autostart: false,
  model_path: null,
  n_gpu_layers: 4294967295,
  ctx_size: null,
  parallel: 1,
  api_key: null,
  tools: {
    enabled: true,
    date: true,
    location: true,
    web_search: true,
    web_fetch: true,
    web_search_provider: { backend: "duckduckgo", brave_api_key: "", searxng_url: "" },
    bash: false,
    read_file: false,
    write_file: false,
    edit_file: false,
    execution_mode: "parallel" as ToolExecutionMode,
    max_rounds: 15,
    max_calls_per_round: 4,
    retry: { max_retries: 2, base_delay_ms: 1000 },
  },
  mmproj: null,
  mmproj_n_threads: 4,
  no_mmproj_gpu: false,
  autoload_mmproj: true,
  verbose: false,
  gpu_memory_threshold: 0.5,
  gpu_memory_gen_threshold: 0.3,
});

let configSaving = $state(false);
let wsPort = $state(8375);
let serverStatus = $state<"stopped" | "loading" | "running">("stopped");
let startError = $state("");
let showAllQuants = $state(false);

/** The family currently shown in the detail panel. */
let selectedFamilyId = $state<string>("");
let previousFamilyId = $state<string>("");

let logs = $state<LlmLogEntry[]>([]);

let pollTimer: ReturnType<typeof setInterval> | undefined;
let unlistenLog: (() => void) | undefined;
let unlistenStatus: (() => void) | undefined;

// ── Derived ────────────────────────────────────────────────────────────────

const families = $derived.by<ModelFamily[]>(() => buildFamilies(catalog.entries));

/** Auto-select a family when the list first loads or active model changes. */
$effect(() => {
  const next = autoSelectFamily(families, catalog, selectedFamilyId);
  if (next && next !== selectedFamilyId) selectedFamilyId = next;
});

const selectedFamily = $derived(families.find((f) => f.id === selectedFamilyId) ?? families[0] ?? null);

const selectedFamilyHasMultipleVendors = $derived((selectedFamily?.vendors.length ?? 0) > 1);

const orderedSelectedEntries = $derived.by<LlmModelEntry[]>(() => {
  if (!selectedFamily) return [];
  const active = catalog.active_model;
  return [...selectedFamily.entries].sort((a, b) => compareModelEntries(a, b, active));
});

const selectedEntryGroups = $derived.by(() => splitEntryGroups(orderedSelectedEntries, catalog.active_model));

const orderedSelectedMmproj = $derived.by<LlmModelEntry[]>(() => {
  if (!selectedFamily) return [];
  const active = catalog.active_model;
  return [...selectedFamily.mmproj].sort((a, b) => compareModelEntries(a, b, active));
});

const hasActive = $derived(
  catalog.entries.some((e) => !e.is_mmproj && e.filename === catalog.active_model && e.state === "downloaded"),
);

const activeEntry = $derived(catalog.entries.find((e) => !e.is_mmproj && e.filename === catalog.active_model) ?? null);

$effect(() => {
  if (selectedFamilyId !== previousFamilyId) {
    showAllQuants = false;
    previousFamilyId = selectedFamilyId;
  }
});

// ── Helpers ────────────────────────────────────────────────────────────────

function fitBadgeClass(level: string): string {
  switch (level) {
    case "perfect":
      return "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-500/30";
    case "good":
      return "bg-sky-500/15 text-sky-700 dark:text-sky-400 border-sky-500/30";
    case "marginal":
      return "bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30";
    case "too_tight":
      return "bg-red-500/15 text-red-700 dark:text-red-400 border-red-500/30";
    default:
      return "bg-slate-500/10 text-slate-500 border-slate-500/20";
  }
}

function fitBadgeIcon(level: string): string {
  switch (level) {
    case "perfect":
      return "🟢";
    case "good":
      return "🟡";
    case "marginal":
      return "🟠";
    case "too_tight":
      return "🔴";
    default:
      return "⚪";
  }
}

function fitBadgeLabel(level: string): string {
  switch (level) {
    case "perfect":
      return t("llm.fit.perfect");
    case "good":
      return t("llm.fit.good");
    case "marginal":
      return t("llm.fit.marginal");
    case "too_tight":
      return t("llm.fit.tooTight");
    default:
      return "";
  }
}
// ── Data loading ───────────────────────────────────────────────────────────

async function loadCatalog() {
  try {
    catalog = await invoke<LlmCatalog>("get_llm_catalog");
  } catch (e) {}
}

async function loadHardwareFit() {
  try {
    const fits = await invoke<ModelHardwareFit[]>("get_model_hardware_fit");
    const map = new Map<string, ModelHardwareFit>();
    for (const f of fits) map.set(f.filename, f);
    hardwareFits = map;
  } catch (e) {}
}

async function loadConfig() {
  try {
    config = await invoke<LlmConfig>("get_llm_config");
  } catch (e) {}
  try {
    const [, port] = await invoke<[string, number]>("get_ws_config");
    wsPort = port;
  } catch (e) {}
}

async function saveConfig() {
  configSaving = true;
  try {
    await invoke("set_llm_config", { config });
  } finally {
    configSaving = false;
  }
}

// ── Actions ────────────────────────────────────────────────────────────────

async function download(filename: string) {
  await invoke("download_llm_model", { filename });
  // Immediately refresh the catalog so the frontend state flips to
  // "downloading" before the poll timer fires.  Without this the timer
  // condition `catalog.entries.some(e => e.state === "downloading")` would
  // be false on the very first tick and the progress bar would never appear.
  await loadCatalog();
}

async function cancelDownload(filename: string) {
  await invoke("cancel_llm_download", { filename });
}

async function deleteModel(filename: string) {
  await invoke("delete_llm_model", { filename });
  await loadCatalog();
}

async function selectModel(filename: string) {
  startError = "";
  // Atomic switch: stop → set model → start in one backend call.
  invoke("switch_llm_model", { filename }).catch((e: unknown) => {
    startError = typeof e === "string" ? e : e instanceof Error ? e.message : "Failed to switch model";
  });
  await loadCatalog();
}

async function selectMmproj(filename: string) {
  const next = catalog.active_mmproj === filename ? "" : filename;
  await invoke("set_llm_active_mmproj", { filename: next });
  await loadCatalog();
}

async function refreshCache() {
  await invoke("refresh_llm_catalog");
  await loadCatalog();
}

async function startServer() {
  startError = "";
  // start_llm_server is fire-and-forget on the Rust side — returns immediately
  // with "starting"; the 2-second poll picks up Loading → Running transitions
  // and surfaces any start_error from the background task.
  invoke("start_llm_server").catch((e: unknown) => {
    startError = typeof e === "string" ? e : e instanceof Error ? e.message : "Unknown error";
  });
}

async function stopServer() {
  startError = "";
  // stop_llm_server is also fire-and-forget — actor join runs in background.
  invoke("stop_llm_server").catch((_e) => {});
}

async function openChat() {
  try {
    await invoke("open_chat_window");
  } catch (e) {}
}

async function openDownloads() {
  try {
    await invoke("open_downloads_window");
  } catch (e) {}
}

// ── Lifecycle ──────────────────────────────────────────────────────────────

onMount(async () => {
  await Promise.all([loadCatalog(), loadConfig(), loadHardwareFit()]);
  try {
    const s = await invoke<{
      status: "stopped" | "loading" | "running";
      start_error: string | null;
    }>("get_llm_server_status");
    serverStatus = s.status;
    if (s.start_error) startError = s.start_error;
  } catch (e) {}
  try {
    unlistenStatus = await listen<{ status: string }>("llm:status", (ev) => {
      const p = ev.payload as Record<string, string>;
      if (p.status) serverStatus = p.status as typeof serverStatus;
    });
  } catch (e) {}
  try {
    logs = await invoke<LlmLogEntry[]>("get_llm_logs");
  } catch (e) {}
  try {
    unlistenLog = await listen<LlmLogEntry>("llm:log", async (ev) => {
      logs = [...logs.slice(-499), ev.payload];
    });
  } catch (e) {}
  // Poll catalog + server status every second.  The catalog call is a cheap
  // in-memory read on the backend.  Always polling (instead of only when a
  // download is detected) ensures that re-opening the settings window after
  // closing it mid-download still picks up in-flight progress immediately.
  pollTimer = setInterval(async () => {
    await loadCatalog();
    // Poll server status so Loading → Running and start_error are reflected
    // without relying solely on push events.
    try {
      const s = await invoke<{
        status: "stopped" | "loading" | "running";
        start_error: string | null;
      }>("get_llm_server_status");
      serverStatus = s.status;
      if (s.start_error) startError = s.start_error;
    } catch (e) {}
  }, 1000);
});

onDestroy(() => {
  clearInterval(pollTimer);
  unlistenLog?.();
  unlistenStatus?.();
});

</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Server status card                                                          -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<LlmServerSection
  enabled={config.enabled}
  autostart={config.autostart}
  hasActive={hasActive}
  activeModel={catalog.active_model}
  serverStatus={serverStatus}
  activeFamilyName={activeEntry?.family_name ?? null}
  activeQuant={activeEntry?.quant ?? null}
  activeSizeGb={activeEntry?.size_gb ?? null}
  wsPort={wsPort}
  startError={startError}
  onToggleEnabled={async () => { config = { ...config, enabled: !config.enabled }; await saveConfig(); }}
  onToggleAutostart={async () => { config = { ...config, autostart: !config.autostart }; await saveConfig(); }}
  onStart={startServer}
  onStop={stopServer}
  onOpenChat={openChat}
/>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Model picker                                                                -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.models")}
    </span>
    <button onclick={openDownloads}
      class="ml-auto text-[0.56rem] text-muted-foreground/60 hover:text-foreground
             transition-colors cursor-pointer select-none">
      {t("downloads.windowTitle")}
    </button>
    <button onclick={refreshCache}
      class="text-[0.56rem] text-muted-foreground/60 hover:text-foreground
             transition-colors cursor-pointer select-none">
      {t("llm.btn.refresh")}
    </button>
  </div>

  {#if families.length === 0}
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e]">
      <CardContent class="flex flex-col items-center gap-2 py-8">
        <span class="text-3xl">🤖</span>
        <p class="text-[0.72rem] text-muted-foreground">{t("llm.noFeature")}</p>
      </CardContent>
    </Card>
  {:else}

    <!-- Hardware summary -->
    {#if hardwareFits.size > 0}
      {@const anyFit = hardwareFits.values().next().value}
      {#if anyFit}
        <div class="flex items-center gap-2 text-[0.56rem] text-muted-foreground/60 px-0.5 -mt-0.5 mb-0.5">
          <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3 shrink-0 opacity-40">
            <path d="M2 4a2 2 0 012-2h8a2 2 0 012 2v5a2 2 0 01-2 2H8l-4 3V11H4a2 2 0 01-2-2V4z"/>
          </svg>
          <span>
            {t("llm.fit.memLabel")}: {anyFit.memoryAvailableGb} GB
          </span>
        </div>
      {/if}
    {/if}

    <!-- Family dropdown -->
    <div class="relative">
      <select
        bind:value={selectedFamilyId}
        class="w-full appearance-none rounded-xl border border-border dark:border-white/[0.06]
               bg-white dark:bg-[#14141e] text-foreground text-[0.78rem] font-semibold
               px-3.5 py-2.5 pr-9 cursor-pointer focus:outline-none
               focus-visible:ring-2 focus-visible:ring-ring/50">
        {#each families as f (f.id)}
          <option value={f.id}>{familyOptionLabel(f, catalog.active_model)}</option>
        {/each}
      </select>
      <!-- Custom caret -->
      <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground">
        <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3">
          <path d="M3 6l5 5 5-5H3z"/>
        </svg>
      </span>
    </div>

    <!-- Selected family detail panel -->
    {#if selectedFamily}
      {@const hasVision = selectedFamily.tags.some((t: string) => t === "vision" || t === "multimodal")}

      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="py-0 px-0 flex flex-col">

          <!-- Family header: description + tags -->
          <div class="px-4 pt-3.5 pb-3 flex flex-col gap-1.5">
            <p class="text-[0.68rem] text-muted-foreground leading-snug">
              {selectedFamily.desc}
            </p>
            <div class="flex items-center gap-1 flex-wrap">
              {#each selectedFamily.tags.filter((t: string) => !["tiny","small","medium","large"].includes(t)) as tag}
                <Badge variant="outline" class="text-[0.5rem] py-0 px-1.5 {tagColor(tag)}">
                  {tagLabel(tag)}
                </Badge>
              {/each}
              <div class="ml-auto flex items-center gap-1 flex-wrap justify-end">
                {#each selectedFamily.vendors as vendor}
                  <Badge variant="outline"
                    class="text-[0.5rem] py-0 px-1.5 border-slate-500/20 bg-slate-500/10 text-slate-600 dark:text-slate-300">
                    {vendor}
                  </Badge>
                {/each}
              </div>
            </div>
            <div class="flex items-center gap-2 flex-wrap text-[0.58rem] text-muted-foreground/70">
              <span>{selectedFamily.entries.length} quants</span>
              {#if selectedFamily.downloaded.length > 0}
                <span>{selectedFamily.downloaded.length} downloaded</span>
              {/if}
              {#if selectedEntryGroups.extra.length > 0}
                <button
                  onclick={() => showAllQuants = !showAllQuants}
                  class="rounded-full border border-border/70 dark:border-white/[0.08] px-2 py-0.5
                         hover:text-foreground hover:border-border transition-colors cursor-pointer">
                  {showAllQuants
                    ? `Hide ${selectedEntryGroups.extra.length} extra quants`
                    : `Show ${selectedEntryGroups.extra.length} extra quants`}
                </button>
              {/if}
            </div>
          </div>

          <!-- Column headers -->
          <div class="grid grid-cols-[4rem_4rem_1fr_auto] gap-x-2 items-center
                       px-4 py-1.5 border-t border-b border-border/40 dark:border-white/[0.04]
                       bg-slate-50 dark:bg-[#111118]">
            <span class="text-[0.54rem] font-semibold uppercase tracking-widest text-muted-foreground/60">Quant</span>
            <span class="text-[0.54rem] font-semibold uppercase tracking-widest text-muted-foreground/60">Size</span>
            <span class="text-[0.54rem] font-semibold uppercase tracking-widest text-muted-foreground/60">Notes</span>
            <span></span>
          </div>

          <!-- Quant rows -->
          <div class="flex flex-col divide-y divide-border/40 dark:divide-white/[0.04]">
            {#each [...selectedEntryGroups.primary, ...(showAllQuants ? selectedEntryGroups.extra : [])] as entry (entry.filename)}
              {@const isActive    = catalog.active_model === entry.filename}
              {@const downloading = entry.state === "downloading"}
              {@const downloaded  = entry.state === "downloaded"}
              {@const failed      = entry.state === "failed" || entry.state === "cancelled"}
              {@const fit         = hardwareFits.get(entry.filename)}

              <div class="flex flex-col gap-1 px-4 py-2.5
                           {isActive ? 'bg-violet-50/60 dark:bg-violet-950/20' : ''}">

                <!-- Main row -->
                <div class="grid grid-cols-[4rem_4rem_1fr_auto] gap-x-2 items-center min-w-0">

                  <!-- Quant badge -->
                  <span class="text-[0.74rem] font-bold font-mono text-foreground truncate">
                    {entry.quant}
                    {#if entry.recommended}
                      <span class="text-[0.52rem] text-violet-500 font-sans not-italic ml-0.5">★</span>
                    {/if}
                  </span>

                  <!-- Size -->
                  <span class="text-[0.72rem] tabular-nums font-semibold
                                {downloaded ? 'text-foreground/80' : 'text-muted-foreground'}">
                    {fmtSize(entry.size_gb)}
                  </span>

                  <!-- Description + status badges -->
                  <div class="flex items-center gap-1.5 min-w-0">
                    {#if selectedFamilyHasMultipleVendors}
                      <span class="shrink-0 rounded-full border border-slate-500/20 bg-slate-500/10
                                   px-1.5 py-0.5 text-[0.5rem] font-semibold text-slate-600 dark:text-slate-300">
                        {vendorLabel(entry.repo)}
                      </span>
                    {/if}
                    {#if fit}
                      <span class="shrink-0 rounded-full border px-1.5 py-0.5 text-[0.5rem] font-semibold
                                   {fitBadgeClass(fit.fitLevel)}"
                            title="{runModeLabel(fit.runMode)} · {fit.memoryRequiredGb} / {fit.memoryAvailableGb} GB · ~{fit.estimatedTps} tok/s">
                        {fitBadgeIcon(fit.fitLevel)} {fitBadgeLabel(fit.fitLevel)}
                      </span>
                    {/if}
                    <span class="text-[0.63rem] text-muted-foreground/70 truncate">
                      {entry.description}
                    </span>
                    {#if isActive}
                      <span class="shrink-0 text-[0.52rem] font-semibold
                                   text-emerald-600 dark:text-emerald-400">✓ active</span>
                    {:else if downloaded}
                      <span class="shrink-0 text-[0.52rem] font-semibold
                                   text-sky-600 dark:text-sky-400">downloaded</span>
                    {/if}
                    {#if downloading}
                      <span class="shrink-0 text-[0.52rem] text-blue-500 animate-pulse">downloading…</span>
                    {/if}
                    {#if failed}
                      <span class="shrink-0 text-[0.52rem] text-red-500">failed</span>
                    {/if}
                  </div>

                  <!-- Action buttons -->
                  <div class="flex items-center gap-1 shrink-0 justify-end">
                    {#if downloading}
                      <Button size="sm" variant="outline"
                        class="h-6 text-[0.6rem] px-2 text-destructive border-destructive/30 hover:bg-destructive/10"
                        onclick={() => cancelDownload(entry.filename)}>
                        Cancel
                      </Button>

                    {:else if downloaded}
                      <Button size="sm" variant="ghost"
                        class="h-6 text-[0.6rem] px-2 text-muted-foreground/60 hover:text-red-500"
                        onclick={() => deleteModel(entry.filename)}>
                        Delete
                      </Button>
                      <Button size="sm"
                        class="h-6 text-[0.6rem] px-2.5
                               {isActive
                                 ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20'
                                 : 'bg-violet-600 hover:bg-violet-700 text-white'}"
                        onclick={() => selectModel(entry.filename)}>
                        {isActive ? "Active" : "Use"}
                      </Button>

                    {:else}
                      <Button size="sm"
                        class="h-6 text-[0.6rem] px-2.5 bg-violet-600 hover:bg-violet-700 text-white"
                        onclick={() => download(entry.filename)}>
                        {failed ? "Retry" : `Download ${fmtSize(entry.size_gb)}`}{entry.shard_files?.length > 1 ? ` (${entry.shard_files.length} parts)` : ""}
                      </Button>
                    {/if}
                  </div>
                </div>

                <!-- Progress bar -->
                {#if downloading}
                  <div class="h-1 w-full rounded-full bg-muted overflow-hidden mt-0.5">
                    {#if entry.progress > 0}
                      <div class="h-full rounded-full bg-blue-500 transition-all duration-300"
                           style="width:{(entry.progress * 100).toFixed(1)}%"></div>
                    {:else}
                      <!-- Indeterminate pulse -->
                      <div class="h-full w-2/5 rounded-full bg-blue-500
                                  animate-[progress-indeterminate_1.6s_ease-in-out_infinite]">
                      </div>
                    {/if}
                  </div>
                  {#if entry.status_msg}
                    <p class="text-[0.58rem] text-blue-500 truncate">{entry.status_msg}</p>
                  {/if}
                {/if}

                <!-- Error -->
                {#if failed && entry.status_msg}
                  <p class="text-[0.6rem] text-destructive/80 font-mono break-all leading-relaxed
                             rounded bg-destructive/5 border border-destructive/10 px-2 py-1">
                    {entry.status_msg}
                  </p>
                {/if}

                <!-- Local path -->
                {#if downloaded && entry.local_path}
                  <p class="text-[0.53rem] font-mono text-muted-foreground/40 break-all leading-tight">
                    {entry.local_path}
                  </p>
                {/if}

                <!-- Hardware fit detail -->
                {#if fit}
                  <div class="flex items-center gap-2 flex-wrap text-[0.54rem] text-muted-foreground/60 mt-0.5">
                    <span>{runModeLabel(fit.runMode)}</span>
                    <span class="opacity-40">·</span>
                    <span>{t("llm.fit.memLabel")}: {fit.memoryRequiredGb} / {fit.memoryAvailableGb} GB</span>
                    <span class="opacity-40">·</span>
                    <span>~{fit.estimatedTps} {t("llm.fit.tokSec")}</span>
                    {#if fit.score > 0}
                      <span class="opacity-40">·</span>
                      <span>{t("llm.fit.scoreLabel")}: {fit.score.toFixed(1)}</span>
                    {/if}
                  </div>
                {/if}

              </div>
            {/each}
          </div>

          <!-- Vision projector section -->
          {#if hasVision && selectedFamily.mmproj.length > 0}
            <div class="border-t border-border dark:border-white/[0.06]
                         px-4 py-3 bg-amber-50/30 dark:bg-amber-950/10">
              <p class="text-[0.6rem] font-semibold text-amber-700 dark:text-amber-400 mb-2">
                Vision projector (required for image input)
              </p>
              <p class="text-[0.58rem] text-amber-700/80 dark:text-amber-300/80 mb-2 leading-snug">
                Multimodal projectors extend the active LLM. They are loaded with a compatible text model, not used as standalone models.
              </p>
              <div class="flex flex-col gap-1.5">
                {#each orderedSelectedMmproj as mp (mp.filename)}
                  {@const isActiveMm  = catalog.active_mmproj === mp.filename}
                  {@const mpDl        = mp.state === "downloading"}
                  {@const mpDownloaded = mp.state === "downloaded"}

                  <div class="flex flex-col gap-1">
                    <div class="flex items-center gap-2">
                      <div class="flex-1 min-w-0 flex items-center gap-1.5">
                          {#if selectedFamilyHasMultipleVendors}
                            <span class="shrink-0 rounded-full border border-slate-500/20 bg-slate-500/10
                                         px-1.5 py-0.5 text-[0.5rem] font-semibold text-slate-600 dark:text-slate-300">
                              {vendorLabel(mp.repo)}
                            </span>
                          {/if}
                        <span class="text-[0.68rem] font-mono text-foreground truncate">{mp.filename}</span>
                        <span class="text-[0.62rem] text-muted-foreground shrink-0">{fmtSize(mp.size_gb)}</span>
                        {#if mp.recommended}
                          <span class="text-[0.52rem] text-violet-500">★</span>
                        {/if}
                        {#if isActiveMm}
                          <span class="text-[0.52rem] font-semibold text-amber-600 dark:text-amber-400 shrink-0">
                            ✓ active
                          </span>
                        {/if}
                      </div>
                      <div class="flex items-center gap-1 shrink-0">
                        {#if mpDl}
                          <Button size="sm" variant="outline"
                            class="h-5 text-[0.58rem] px-1.5 text-destructive border-destructive/30"
                            onclick={() => cancelDownload(mp.filename)}>Cancel</Button>
                        {:else if mpDownloaded}
                          <Button size="sm" variant="ghost"
                            class="h-5 text-[0.58rem] px-1.5 text-muted-foreground/60 hover:text-red-500"
                            onclick={() => deleteModel(mp.filename)}>Delete</Button>
                          <Button size="sm"
                            class="h-5 text-[0.58rem] px-2
                                   {isActiveMm
                                     ? 'bg-amber-500/15 text-amber-700 dark:text-amber-400 border border-amber-500/30'
                                     : 'bg-amber-600 hover:bg-amber-700 text-white'}"
                            onclick={() => selectMmproj(mp.filename)}>
                            {isActiveMm ? "Active" : "Use"}
                          </Button>
                        {:else}
                          <Button size="sm"
                            class="h-5 text-[0.58rem] px-2 bg-amber-600 hover:bg-amber-700 text-white"
                            onclick={() => download(mp.filename)}>
                            Download {fmtSize(mp.size_gb)}
                          </Button>
                        {/if}
                      </div>
                    </div>

                    {#if mpDl}
                      <div class="h-1 w-full rounded-full bg-muted overflow-hidden">
                        {#if mp.progress > 0}
                          <div class="h-full rounded-full bg-amber-500 transition-all duration-300"
                               style="width:{(mp.progress * 100).toFixed(1)}%"></div>
                        {:else}
                          <div class="h-full w-2/5 rounded-full bg-amber-500
                                      animate-[progress-indeterminate_1.6s_ease-in-out_infinite]"></div>
                        {/if}
                      </div>
                      {#if mp.status_msg}
                        <p class="text-[0.56rem] text-amber-600 truncate">{mp.status_msg}</p>
                      {/if}
                    {/if}
                  </div>
                {/each}
              </div>
            </div>
          {/if}

        </CardContent>
      </Card>
    {/if}
  {/if}
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Advanced inference settings (collapsible)                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<LlmInferenceSection
  config={config}
  {configSaving}
  {wsPort}
  activeMaxCtx={activeEntry?.max_context_length || 0}
  hasAnyMmproj={catalog.entries.some((e) => e.is_mmproj)}
  hasDownloadedMmproj={catalog.entries.some((e) => e.is_mmproj && e.state === "downloaded")}
  onSetGpuLayers={async (val) => { config = { ...config, n_gpu_layers: val }; await saveConfig(); }}
  onSetCtxSize={async (val) => { config = { ...config, ctx_size: val }; await saveConfig(); }}
  onSetParallel={async (val) => { config = { ...config, parallel: val }; await saveConfig(); }}
  onSetApiKey={async (val) => { config = { ...config, api_key: val }; await saveConfig(); }}
  onToggleAutoloadMmproj={async () => { config = { ...config, autoload_mmproj: !config.autoload_mmproj }; await saveConfig(); }}
  onToggleNoMmprojGpu={async () => { config = { ...config, no_mmproj_gpu: !config.no_mmproj_gpu }; await saveConfig(); }}
  onToggleVerbose={async () => { config = { ...config, verbose: !config.verbose }; await saveConfig(); }}
  onSetGpuMemoryThreshold={async (val) => { config = { ...config, gpu_memory_threshold: val }; await saveConfig(); }}
  onSetGpuMemoryGenThreshold={async (val) => { config = { ...config, gpu_memory_gen_threshold: val }; await saveConfig(); }}
/>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Server log                                                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<LlmServerLogSection
  {logs}
  onClear={() => { logs = []; }}
/>
