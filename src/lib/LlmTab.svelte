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
import LlmInferenceSection from "$lib/llm/LlmInferenceSection.svelte";
import LlmModelPickerSection from "$lib/llm/LlmModelPickerSection.svelte";
import LlmServerLogSection from "$lib/llm/LlmServerLogSection.svelte";
import LlmServerSection from "$lib/llm/LlmServerSection.svelte";
import type { LlmCatalog } from "$lib/llm-helpers";

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

let logs = $state<LlmLogEntry[]>([]);

let pollTimer: ReturnType<typeof setInterval> | undefined;
let unlistenLog: (() => void) | undefined;
let unlistenStatus: (() => void) | undefined;

// ── Derived ────────────────────────────────────────────────────────────────

const hasActive = $derived(
  catalog.entries.some((e) => !e.is_mmproj && e.filename === catalog.active_model && e.state === "downloaded"),
);

const activeEntry = $derived(catalog.entries.find((e) => !e.is_mmproj && e.filename === catalog.active_model) ?? null);

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
  startError = "";
  const next = catalog.active_mmproj === filename ? "" : filename;
  // Atomic switch: set mmproj → stop → start in one backend call (mirrors
  // selectModel / switch_llm_model behaviour so the server restarts with the
  // new projector immediately).
  invoke("switch_llm_mmproj", { filename: next }).catch((e: unknown) => {
    startError = typeof e === "string" ? e : e instanceof Error ? e.message : "Failed to switch mmproj";
  });
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
<LlmModelPickerSection
  {catalog}
  {hardwareFits}
  onOpenDownloads={openDownloads}
  onRefreshCache={refreshCache}
  onDownload={download}
  onCancelDownload={cancelDownload}
  onDeleteModel={deleteModel}
  onSelectModel={selectModel}
  onSelectMmproj={selectMmproj}
/>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Advanced inference settings (collapsible)                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<LlmInferenceSection
  config={config}
  {configSaving}
  {wsPort}
  activeMaxCtx={activeEntry?.max_context_length || 0}
  hasAnyMmproj={catalog.entries.some((e) => e.is_mmproj || e.filename.toLowerCase().includes("mmproj"))}
  hasDownloadedMmproj={catalog.entries.some((e) => (e.is_mmproj || e.filename.toLowerCase().includes("mmproj")) && e.state === "downloaded")}
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
