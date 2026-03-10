<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  LLM Settings Tab
  ─────────────────
  • Shows the embedded OpenAI-compatible server configuration
  • Lists all Qwen3.5-27B GGUF quantisations from unsloth/Qwen3.5-27B-GGUF
  • Shows multimodal projector (mmproj) files
  • Supports per-file download / cancel / delete
  • Lets the user select the active model + mmproj
  • Exposes GPU layers, context size, parallel, API key settings
-->
<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { invoke }                   from "@tauri-apps/api/core";
  import { listen }                   from "@tauri-apps/api/event";
  import { Badge }                    from "$lib/components/ui/badge";
  import { Button }                   from "$lib/components/ui/button";
  import { Card, CardContent }        from "$lib/components/ui/card";
  import { t }                        from "$lib/i18n/index.svelte";

  // ── Types ──────────────────────────────────────────────────────────────────

  interface LlmLogEntry {
    /** Unix timestamp in milliseconds */
    ts:      number;
    /** "info" | "warn" | "error" */
    level:   string;
    message: string;
  }

  type DownloadState =
    | "not_downloaded"
    | "downloading"
    | "downloaded"
    | "failed"
    | "cancelled";

  interface LlmModelEntry {
    repo:        string;
    filename:    string;
    quant:       string;
    size_gb:     number;
    description: string;
    is_mmproj:   boolean;
    recommended: boolean;
    local_path:  string | null;
    state:       DownloadState;
    status_msg:  string | null;
    progress:    number;
  }

  interface LlmCatalog {
    entries:       LlmModelEntry[];
    active_model:  string;
    active_mmproj: string;
  }

  interface LlmConfig {
    enabled:          boolean;
    model_path:       string | null;
    n_gpu_layers:     number;
    ctx_size:         number | null;
    parallel:         number;
    api_key:          string | null;
    mmproj:           string | null;
    mmproj_n_threads: number;
    no_mmproj_gpu:    boolean;
  }

  // ── State ──────────────────────────────────────────────────────────────────

  const FEATURE_ENABLED = true; // set to false when compiled without `llm`

  let catalog = $state<LlmCatalog>({
    entries: [], active_model: "", active_mmproj: "",
  });

  let config = $state<LlmConfig>({
    enabled: false, model_path: null, n_gpu_layers: 4294967295,
    ctx_size: null, parallel: 1, api_key: null,
    mmproj: null, mmproj_n_threads: 4, no_mmproj_gpu: false,
  });

  let configSaving  = $state(false);
  let wsPort        = $state(8375);
  let apiKeyVisible = $state(false);
  let ctxSizeInput  = $state("");

  // ── Log state ──────────────────────────────────────────────────────────────
  let logs         = $state<LlmLogEntry[]>([]);
  let logAutoScroll = $state(true);
  let logEl        = $state<HTMLElement | null>(null);

  // Derived
  const mainModels  = $derived(catalog.entries.filter(e => !e.is_mmproj));
  const mmprojFiles = $derived(catalog.entries.filter(e => e.is_mmproj));
  // hasActive = a model is both selected AND fully downloaded on disk.
  const hasActive   = $derived(
    catalog.entries.some(e =>
      !e.is_mmproj &&
      e.filename === catalog.active_model &&
      e.state === "downloaded"
    )
  );
  const hasMmproj   = $derived(catalog.active_mmproj !== "");

  // ── Helpers ────────────────────────────────────────────────────────────────

  function stateLabel(s: DownloadState): string {
    switch (s) {
      case "not_downloaded": return t("llm.state.notDownloaded");
      case "downloading":    return t("llm.state.downloading");
      case "downloaded":     return t("llm.state.downloaded");
      case "failed":         return t("llm.state.failed");
      case "cancelled":      return t("llm.state.cancelled");
    }
  }

  function stateBadgeClass(s: DownloadState): string {
    switch (s) {
      case "downloaded":     return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
      case "downloading":    return "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20";
      case "failed":         return "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20";
      case "cancelled":      return "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20";
      default:               return "bg-slate-500/10 text-slate-500 border-slate-500/20";
    }
  }

  function fmtSize(gb: number): string {
    if (gb < 1) return `${(gb * 1024).toFixed(0)} MB`;
    return `${gb.toFixed(1)} GB`;
  }

  // ── Data loading ───────────────────────────────────────────────────────────

  async function loadCatalog() {
    try {
      catalog = await invoke<LlmCatalog>("get_llm_catalog");
    } catch {
      // `llm` feature not compiled — catalog stays empty
    }
  }

  async function loadConfig() {
    try {
      config = await invoke<LlmConfig>("get_llm_config");
      ctxSizeInput = config.ctx_size !== null ? String(config.ctx_size) : "";
    } catch { /* ignore */ }
    try {
      const [, port] = await invoke<[string, number]>("get_ws_config");
      wsPort = port;
    } catch { /* ignore */ }
  }

  async function saveConfig() {
    configSaving = true;
    const ctx = ctxSizeInput.trim() === "" ? null : parseInt(ctxSizeInput, 10) || null;
    config = { ...config, ctx_size: ctx };
    try { await invoke("set_llm_config", { config }); }
    finally { configSaving = false; }
  }

  // ── Actions ────────────────────────────────────────────────────────────────

  async function download(filename: string) {
    await invoke("download_llm_model", { filename });
  }

  async function cancelDownload(filename: string) {
    await invoke("cancel_llm_download", { filename });
  }

  async function deleteModel(filename: string) {
    await invoke("delete_llm_model", { filename });
    await loadCatalog();
  }

  async function selectModel(filename: string) {
    await invoke("set_llm_active_model", { filename });
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

  let startError = $state("");

  async function startServer() {
    startError = "";
    try {
      await invoke("start_llm_server");
    } catch (e: any) {
      startError = typeof e === "string" ? e : (e?.message ?? "Unknown error");
      console.error("start_llm_server:", e);
    }
  }

  async function stopServer() {
    startError = "";
    try { await invoke("stop_llm_server"); } catch (e) { console.error(e); }
  }

  async function openChat() {
    try { await invoke("open_chat_window"); } catch (e) { console.error(e); }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  let serverStatus  = $state<"stopped"|"loading"|"running">("stopped");
  let pollTimer:      ReturnType<typeof setInterval> | undefined;
  let unlistenLog:    (() => void) | undefined;
  let unlistenStatus: (() => void) | undefined;

  onMount(async () => {
    await Promise.all([loadCatalog(), loadConfig()]);

    // Initial server status
    try {
      const s = await invoke<{ status: "stopped"|"loading"|"running"; model_name: string }>(
        "get_llm_server_status"
      );
      serverStatus = s.status;
    } catch {}

    // Live status events
    try {
      unlistenStatus = await listen<{ status: "stopped"|"loading"|"running" }>(
        "llm:status",
        ev => { serverStatus = (ev.payload as any).status ?? serverStatus; }
      );
    } catch {}

    // Load buffered logs
    try {
      logs = await invoke<LlmLogEntry[]>("get_llm_logs");
      await scrollToBottom();
    } catch {}

    // Subscribe to live log events
    try {
      unlistenLog = await listen<LlmLogEntry>("llm:log", async (ev) => {
        logs = [...logs.slice(-499), ev.payload];
        if (logAutoScroll) await scrollToBottom();
      });
    } catch {}

    // Poll every 1.5 s while downloads are in progress
    pollTimer = setInterval(async () => {
      const downloading = catalog.entries.some(e => e.state === "downloading");
      if (downloading) await loadCatalog();
    }, 1500);
  });

  onDestroy(() => {
    clearInterval(pollTimer);
    unlistenLog?.();
    unlistenStatus?.();
  });

  async function scrollToBottom() {
    await tick();
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  }

  function handleLogScroll() {
    if (!logEl) return;
    const atBottom = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40;
    logAutoScroll = atBottom;
  }
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Server enable / status                                                      -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.server")}
    </span>
    <!-- Live dot -->
    {#if config.enabled && hasActive}
      <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
    {:else}
      <span class="w-1.5 h-1.5 rounded-full bg-slate-400"></span>
    {/if}
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Enable toggle -->
      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5 flex-1 min-w-0">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.enabled")}</span>
          <span class="text-[0.65rem] text-muted-foreground leading-relaxed">
            {t("llm.enabledDesc")}
          </span>
        </div>
        <button
          role="switch"
          aria-checked={config.enabled}
          onclick={async () => { config = { ...config, enabled: !config.enabled }; await saveConfig(); }}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200 focus-visible:outline-none
                 {config.enabled ? 'bg-emerald-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {config.enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <!-- Server status + start/stop/chat -->
      <div class="flex items-center justify-between gap-4 px-4 py-3">
        <div class="flex items-center gap-2">
          <span class="w-2 h-2 rounded-full shrink-0
            {serverStatus === 'running'  ? 'bg-emerald-500'
            : serverStatus === 'loading' ? 'bg-amber-500 animate-pulse'
            :                             'bg-slate-400/50'}"></span>
          <span class="text-[0.78rem] font-semibold text-foreground">
            {serverStatus === "running"  ? t("llm.status.running")
            : serverStatus === "loading" ? "Loading…"
            :                             t("llm.status.disabled")}
          </span>
        </div>
        <div class="flex items-center gap-1.5">
          {#if serverStatus === "stopped"}
            <Button size="sm"
              class="h-6 text-[0.62rem] px-2.5 bg-violet-600 hover:bg-violet-700 text-white
                     disabled:opacity-40 disabled:cursor-not-allowed"
              onclick={startServer} disabled={!hasActive}>
              Start
            </Button>
          {:else}
            <Button size="sm" variant="outline"
              class="h-6 text-[0.62rem] px-2 text-red-500 border-red-500/30 hover:bg-red-500/10"
              onclick={stopServer}>
              {serverStatus === "loading" ? "Cancel" : "Stop"}
            </Button>
          {/if}
          <Button size="sm" variant="outline"
            class="h-6 text-[0.62rem] px-2.5 border-violet-500/40 text-violet-700
                   dark:text-violet-400 hover:bg-violet-500/10"
            onclick={openChat}>
            Open Chat
          </Button>
        </div>
      </div>

      <!-- Start-error banner -->
      {#if startError}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20
                    text-[0.68rem] text-red-600 dark:text-red-400 leading-snug">
          {startError}
        </div>
      {/if}

      <!-- Hint when model is selected but not yet downloaded -->
      {#if serverStatus === "stopped" && catalog.active_model && !hasActive}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-amber-500/10 border border-amber-500/20
                    text-[0.68rem] text-amber-700 dark:text-amber-400 leading-snug">
          <strong>{catalog.active_model}</strong> is not downloaded yet.
          Download it in the Models section below, then click Start.
        </div>
      {/if}

      <!-- Endpoint display -->
      <div class="flex flex-col gap-0.5 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
          {t("llm.endpoint")}
        </span>
        <div class="flex items-center gap-2 flex-wrap">
          {#each ["/v1/chat/completions", "/v1/completions", "/v1/embeddings", "/v1/files", "/v1/models", "/health"] as ep}
            <code class="text-[0.6rem] font-mono text-muted-foreground
                          bg-muted dark:bg-white/5 rounded px-1.5 py-0.5">
              {ep}
            </code>
          {/each}
        </div>
        <span class="text-[0.58rem] text-muted-foreground/60 mt-0.5">
          http://localhost:{wsPort}  ·  {t("llm.endpointHint")}
        </span>
      </div>

      <!-- Restart notice -->
      <div class="flex items-center gap-2 px-4 py-2.5">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 shrink-0 text-amber-500">
          <circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/>
          <line x1="12" y1="16" x2="12.01" y2="16"/>
        </svg>
        <span class="text-[0.62rem] text-muted-foreground/70">{t("llm.restartRequired")}</span>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Language model catalog                                                      -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.models")}
    </span>
    <span class="text-[0.56rem] text-muted-foreground/60 font-mono">
      unsloth/Qwen3.5-27B-GGUF
    </span>
    <button
      onclick={refreshCache}
      class="ml-auto text-[0.56rem] text-muted-foreground/60 hover:text-foreground
             transition-colors cursor-pointer select-none">
      {t("llm.btn.refresh")}
    </button>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      {#if mainModels.length === 0}
        <div class="flex flex-col items-center gap-2 py-8 text-center">
          <span class="text-3xl">🤖</span>
          <p class="text-[0.72rem] text-muted-foreground">{t("llm.noFeature")}</p>
          <p class="text-[0.62rem] text-muted-foreground/70 max-w-xs leading-relaxed">
            {t("llm.noFeatureHint")}
          </p>
        </div>
      {:else}
        {#each mainModels as entry (entry.filename)}
          {@const isActive     = catalog.active_model === entry.filename}
          {@const downloading  = entry.state === "downloading"}
          {@const downloaded   = entry.state === "downloaded"}
          {@const failed       = entry.state === "failed" || entry.state === "cancelled"}

          <div class="flex flex-col gap-2 px-4 py-3.5
                      {isActive ? 'bg-violet-50/60 dark:bg-violet-950/20' : ''}">

            <!-- Header row -->
            <div class="flex items-start gap-2">
              <!-- Quant label + badges -->
              <div class="flex flex-col gap-0.5 flex-1 min-w-0">
                <div class="flex items-center gap-1.5 flex-wrap">
                  <span class="text-[0.82rem] font-bold font-mono text-foreground">
                    {entry.quant}
                  </span>
                  {#if entry.recommended}
                    <Badge variant="outline"
                      class="text-[0.52rem] py-0 px-1
                             bg-violet-500/10 text-violet-600 dark:text-violet-400
                             border-violet-500/20">
                      {t("llm.recommended")}
                    </Badge>
                  {/if}
                  {#if isActive}
                    <Badge variant="outline"
                      class="text-[0.52rem] py-0 px-1
                             bg-emerald-500/10 text-emerald-600 dark:text-emerald-400
                             border-emerald-500/20">
                      {t("llm.active")}
                    </Badge>
                  {/if}
                </div>
                <span class="text-[0.65rem] text-muted-foreground leading-snug">
                  {entry.description}
                </span>
                <span class="text-[0.6rem] font-mono text-muted-foreground/60">
                  {entry.filename}  ·  {fmtSize(entry.size_gb)}
                </span>
              </div>

              <!-- State badge -->
              <Badge variant="outline"
                class="shrink-0 text-[0.52rem] py-0 px-1.5 {stateBadgeClass(entry.state)}">
                {stateLabel(entry.state)}
              </Badge>
            </div>

            <!-- Progress bar (while downloading) -->
            {#if downloading}
              <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
                {#if entry.progress > 0}
                  <div class="h-full rounded-full bg-blue-500 transition-all duration-300"
                       style="width:{entry.progress * 100}%"></div>
                {:else}
                  <div class="h-full rounded-full bg-blue-500
                              animate-[progress-indeterminate_1.6s_ease-in-out_infinite]"
                       style="width:40%"></div>
                {/if}
              </div>
              {#if entry.status_msg}
                <p class="text-[0.6rem] text-blue-600 dark:text-blue-400 truncate -mt-1">
                  {entry.status_msg}
                </p>
              {/if}
            {/if}

            <!-- Error/cancel message -->
            {#if failed && entry.status_msg}
              <p class="text-[0.6rem] text-destructive/80 font-mono break-all leading-relaxed
                         rounded bg-destructive/5 border border-destructive/10 px-2 py-1.5">
                {entry.status_msg}
              </p>
            {/if}

            <!-- Local path (when downloaded) -->
            {#if downloaded && entry.local_path}
              <p class="text-[0.58rem] font-mono text-muted-foreground/60 break-all leading-relaxed -mt-1">
                {entry.local_path}
              </p>
            {/if}

            <!-- Action buttons -->
            <div class="flex items-center gap-1.5 justify-end -mt-0.5">
              {#if downloading}
                <Button size="sm" variant="outline"
                  class="h-6 text-[0.62rem] px-2.5
                         text-destructive border-destructive/30
                         hover:bg-destructive/10 hover:text-destructive"
                  onclick={() => cancelDownload(entry.filename)}>
                  {t("llm.btn.cancel")}
                </Button>

              {:else if downloaded}
                <Button size="sm" variant="ghost"
                  class="h-6 text-[0.62rem] px-2 text-muted-foreground hover:text-red-500"
                  onclick={() => deleteModel(entry.filename)}>
                  {t("llm.btn.delete")}
                </Button>
                <Button size="sm"
                  variant={isActive ? "secondary" : "outline"}
                  class="h-6 text-[0.62rem] px-2.5
                         {isActive
                           ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-500/30'
                           : 'border-violet-500/40 text-violet-700 dark:text-violet-400 hover:bg-violet-500/10'}"
                  onclick={() => selectModel(entry.filename)}>
                  {isActive ? t("llm.btn.selected") : t("llm.btn.select")}
                </Button>

              {:else}
                <!-- Not downloaded / failed / cancelled -->
                <Button size="sm"
                  class="h-6 text-[0.62rem] px-2.5 bg-violet-600 hover:bg-violet-700 text-white"
                  onclick={() => download(entry.filename)}>
                  {failed ? "Retry" : t("llm.btn.download")}
                </Button>
              {/if}
            </div>

          </div>
        {/each}
      {/if}

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Multimodal projectors (mmproj)                                             -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.mmproj")}
    </span>
    <span class="text-[0.56rem] text-muted-foreground/60">
      enables image / audio inputs in chat completions
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      {#each mmprojFiles as entry (entry.filename)}
        {@const isActive    = catalog.active_mmproj === entry.filename}
        {@const downloading = entry.state === "downloading"}
        {@const downloaded  = entry.state === "downloaded"}
        {@const failed      = entry.state === "failed" || entry.state === "cancelled"}

        <div class="flex flex-col gap-2 px-4 py-3
                    {isActive ? 'bg-blue-50/60 dark:bg-blue-950/20' : ''}">

          <div class="flex items-start gap-2">
            <div class="flex flex-col gap-0.5 flex-1 min-w-0">
              <div class="flex items-center gap-1.5 flex-wrap">
                <span class="text-[0.78rem] font-semibold font-mono text-foreground">
                  {entry.filename}
                </span>
                {#if entry.recommended}
                  <Badge variant="outline"
                    class="text-[0.52rem] py-0 px-1
                           bg-violet-500/10 text-violet-600 dark:text-violet-400
                           border-violet-500/20">
                    {t("llm.recommended")}
                  </Badge>
                {/if}
                {#if isActive}
                  <Badge variant="outline"
                    class="text-[0.52rem] py-0 px-1
                           bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20">
                    {t("llm.active")}
                  </Badge>
                {/if}
              </div>
              <span class="text-[0.65rem] text-muted-foreground leading-snug">
                {entry.description}  ·  {fmtSize(entry.size_gb)}
              </span>
            </div>

            <Badge variant="outline"
              class="shrink-0 text-[0.52rem] py-0 px-1.5 {stateBadgeClass(entry.state)}">
              {stateLabel(entry.state)}
            </Badge>
          </div>

          <!-- Progress -->
          {#if downloading}
            <div class="h-1.5 w-full rounded-full bg-muted overflow-hidden">
              <div class="h-full rounded-full bg-blue-500
                          animate-[progress-indeterminate_1.6s_ease-in-out_infinite]"
                   style="width:40%"></div>
            </div>
          {/if}

          <!-- local path -->
          {#if downloaded && entry.local_path}
            <p class="text-[0.58rem] font-mono text-muted-foreground/60 break-all leading-relaxed -mt-0.5">
              {entry.local_path}
            </p>
          {/if}

          <!-- Buttons -->
          <div class="flex items-center gap-1.5 justify-end -mt-0.5">
            {#if downloading}
              <Button size="sm" variant="outline"
                class="h-6 text-[0.62rem] px-2.5
                       text-destructive border-destructive/30
                       hover:bg-destructive/10 hover:text-destructive"
                onclick={() => cancelDownload(entry.filename)}>
                {t("llm.btn.cancel")}
              </Button>
            {:else if downloaded}
              <Button size="sm" variant="ghost"
                class="h-6 text-[0.62rem] px-2 text-muted-foreground hover:text-red-500"
                onclick={() => deleteModel(entry.filename)}>
                {t("llm.btn.delete")}
              </Button>
              <Button size="sm"
                variant={isActive ? "secondary" : "outline"}
                class="h-6 text-[0.62rem] px-2.5
                       {isActive
                         ? 'bg-blue-500/15 text-blue-600 dark:text-blue-400 border-blue-500/30'
                         : 'border-blue-500/40 text-blue-700 dark:text-blue-400 hover:bg-blue-500/10'}"
                onclick={() => selectMmproj(entry.filename)}>
                {isActive ? t("llm.btn.selected") : t("llm.btn.select")}
              </Button>
            {:else}
              <Button size="sm"
                class="h-6 text-[0.62rem] px-2.5 bg-blue-600 hover:bg-blue-700 text-white"
                onclick={() => download(entry.filename)}>
                {failed ? "Retry" : t("llm.btn.download")}
              </Button>
            {/if}
          </div>

        </div>
      {/each}

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Inference settings                                                          -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.inference")}
    </span>
    {#if configSaving}
      <span class="text-[0.56rem] text-muted-foreground">saving…</span>
    {/if}
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- GPU layers -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.inference.gpuLayers")}</span>
          <span class="text-[0.68rem] text-muted-foreground tabular-nums">
            {config.n_gpu_layers === 0 ? "CPU only" : config.n_gpu_layers === 4294967295 ? "All layers" : config.n_gpu_layers}
          </span>
        </div>
        <p class="text-[0.65rem] text-muted-foreground leading-relaxed -mt-0.5">
          {t("llm.inference.gpuLayersDesc")}
        </p>
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each [[0,"CPU only"],[8,"8"],[16,"16"],[32,"32"],[99,"All"]] as [val, label]}
            <button
              onclick={async () => { config = { ...config, n_gpu_layers: val as number }; await saveConfig(); }}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {config.n_gpu_layers === val
                       ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              {label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Context size -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.inference.ctxSize")}</span>
          <span class="text-[0.68rem] text-muted-foreground tabular-nums">
            {config.ctx_size !== null ? config.ctx_size + " tokens" : "model default (≤ 4096)"}
          </span>
        </div>
        <p class="text-[0.65rem] text-muted-foreground leading-relaxed -mt-0.5">
          {t("llm.inference.ctxSizeDesc")}
        </p>
        <div class="flex items-center gap-2">
          {#each [null, 2048, 4096, 8192, 16384, 32768] as val}
            <button
              onclick={async () => {
                ctxSizeInput = val !== null ? String(val) : "";
                config = { ...config, ctx_size: val };
                await saveConfig();
              }}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {config.ctx_size === val
                       ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              {val === null ? "auto" : val >= 1024 ? `${val/1024}K` : val}
            </button>
          {/each}
        </div>
      </div>

      <!-- Parallel requests -->
      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5 flex-1 min-w-0">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.inference.parallel")}</span>
          <span class="text-[0.65rem] text-muted-foreground leading-relaxed">
            {t("llm.inference.parallelDesc")}
          </span>
        </div>
        <div class="flex items-center gap-1.5 shrink-0">
          {#each [1,2,4] as val}
            <button
              onclick={async () => { config = { ...config, parallel: val }; await saveConfig(); }}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {config.parallel === val
                       ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground'}">
              {val}
            </button>
          {/each}
        </div>
      </div>

      <!-- API key -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.inference.apiKey")}</span>
        <p class="text-[0.65rem] text-muted-foreground leading-relaxed -mt-0.5">
          {t("llm.inference.apiKeyDesc")}
        </p>
        <div class="flex items-center gap-2">
          <input
            type={apiKeyVisible ? "text" : "password"}
            placeholder={t("llm.inference.apiKeyPlaceholder")}
            bind:value={config.api_key}
            onblur={saveConfig}
            class="flex-1 min-w-0 text-[0.73rem] font-mono px-2 py-1 rounded-md
                   border border-border bg-background text-foreground
                   placeholder:text-muted-foreground/40" />
          <button
            onclick={() => { apiKeyVisible = !apiKeyVisible; }}
            class="shrink-0 text-[0.62rem] text-muted-foreground hover:text-foreground
                   transition-colors select-none cursor-pointer">
            {apiKeyVisible ? "hide" : "show"}
          </button>
          {#if config.api_key}
            <button
              onclick={async () => { config = { ...config, api_key: null }; await saveConfig(); }}
              class="shrink-0 text-[0.62rem] text-muted-foreground hover:text-red-500
                     transition-colors select-none cursor-pointer">
              clear
            </button>
          {/if}
        </div>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Multimodal inference settings                                              -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
{#if hasMmproj || mmprojFiles.some(e => e.state === "downloaded")}
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    Multimodal Inference
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Encoder threads -->
      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5 flex-1">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.mmproj.nThreads")}</span>
          <span class="text-[0.65rem] text-muted-foreground">{t("llm.mmproj.nThreadsDesc")}</span>
        </div>
        <div class="flex items-center gap-1.5 shrink-0">
          {#each [1,2,4,8] as val}
            <button
              onclick={async () => { config = { ...config, mmproj_n_threads: val }; await saveConfig(); }}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {config.mmproj_n_threads === val
                       ? 'border-blue-500/50 bg-blue-500/10 dark:bg-blue-500/15 text-blue-600 dark:text-blue-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground'}">
              {val}
            </button>
          {/each}
        </div>
      </div>

      <!-- CPU-only mmproj -->
      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5 flex-1">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.mmproj.noGpu")}</span>
          <span class="text-[0.65rem] text-muted-foreground">{t("llm.mmproj.noGpuDesc")}</span>
        </div>
        <button
          role="switch"
          aria-checked={config.no_mmproj_gpu}
          onclick={async () => { config = { ...config, no_mmproj_gpu: !config.no_mmproj_gpu }; await saveConfig(); }}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {config.no_mmproj_gpu ? 'bg-blue-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {config.no_mmproj_gpu ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

    </CardContent>
  </Card>
</section>
{/if}

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Active configuration summary                                               -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    Active configuration
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <div class="flex items-center justify-between gap-4 px-4 py-2.5">
        <span class="text-[0.68rem] text-muted-foreground">Model</span>
        <span class="text-[0.68rem] font-mono text-foreground truncate max-w-[55%]">
          {catalog.active_model || "—"}
        </span>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-2.5">
        <span class="text-[0.68rem] text-muted-foreground">mmproj</span>
        <span class="text-[0.68rem] font-mono text-foreground">
          {catalog.active_mmproj || "—"}
        </span>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-2.5">
        <span class="text-[0.68rem] text-muted-foreground">GPU layers</span>
        <span class="text-[0.68rem] font-mono text-foreground">{config.n_gpu_layers}</span>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-2.5">
        <span class="text-[0.68rem] text-muted-foreground">Context</span>
        <span class="text-[0.68rem] font-mono text-foreground">
          {config.ctx_size !== null ? config.ctx_size + " tokens" : "auto"}
        </span>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-2.5">
        <span class="text-[0.68rem] text-muted-foreground">Parallel</span>
        <span class="text-[0.68rem] font-mono text-foreground">{config.parallel}</span>
      </div>

      <div class="flex items-center justify-between gap-4 px-4 py-2.5">
        <span class="text-[0.68rem] text-muted-foreground">Auth</span>
        <Badge variant="outline"
          class="text-[0.54rem] py-0 px-1.5 {config.api_key
            ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20'
            : 'bg-slate-500/10 text-slate-500 border-slate-500/20'}">
          {config.api_key ? "Bearer token set" : "open"}
        </Badge>
      </div>

      <!-- Curl example -->
      <div class="flex flex-col gap-1.5 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
          Quick test
        </span>
        <pre class="text-[0.58rem] font-mono text-muted-foreground/80 whitespace-pre-wrap break-all leading-relaxed">curl http://localhost:{wsPort}/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '&#123;"model":"default","messages":[&#123;"role":"user","content":"Hello!"&#125;]&#125;'</pre>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Server log                                                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      Server log
    </span>

    <!-- live indicator -->
    <span class="flex items-center gap-1 text-[0.52rem] text-muted-foreground/50">
      <span class="w-1 h-1 rounded-full {logs.length > 0 ? 'bg-emerald-500 animate-pulse' : 'bg-slate-400'}"></span>
      {logs.length} line{logs.length !== 1 ? "s" : ""}
    </span>

    <!-- auto-scroll toggle -->
    <button
      onclick={() => { logAutoScroll = !logAutoScroll; if (logAutoScroll) scrollToBottom(); }}
      class="ml-auto text-[0.52rem] select-none cursor-pointer transition-colors
             {logAutoScroll
               ? 'text-emerald-600 dark:text-emerald-400'
               : 'text-muted-foreground/50 hover:text-foreground'}">
      auto-scroll {logAutoScroll ? "on" : "off"}
    </button>

    <!-- clear -->
    <button
      onclick={() => { logs = []; }}
      class="text-[0.52rem] text-muted-foreground/50 hover:text-muted-foreground
             select-none cursor-pointer transition-colors">
      clear
    </button>
  </div>

  <!-- Terminal box -->
  <div
    bind:this={logEl}
    onscroll={handleLogScroll}
    class="h-64 overflow-y-auto rounded-xl border border-border dark:border-white/[0.06]
           bg-[#0d0d14] font-mono text-[0.62rem] leading-5
           scrollbar-thin scrollbar-track-transparent scrollbar-thumb-white/10">

    {#if logs.length === 0}
      <div class="flex items-center justify-center h-full text-muted-foreground/30 text-[0.65rem]">
        No log output yet.
        {#if !config.enabled}
          Enable the LLM server to start seeing logs.
        {/if}
      </div>
    {:else}
      <div class="px-3 py-2 flex flex-col gap-0">
        {#each logs as entry (entry.ts + entry.message)}
          {@const ts  = new Date(entry.ts).toISOString().slice(11, 23)}
          {@const col = entry.level === "error" ? "text-red-400"
                      : entry.level === "warn"  ? "text-amber-400"
                      :                           "text-emerald-300/80"}
          <div class="flex items-start gap-2 min-w-0">
            <!-- timestamp -->
            <span class="shrink-0 text-white/20 tabular-nums">{ts}</span>
            <!-- level pill -->
            <span class="shrink-0 w-8 text-center rounded text-[0.5rem] px-0.5
                          {entry.level === 'error' ? 'bg-red-500/20 text-red-400'
                          : entry.level === 'warn'  ? 'bg-amber-500/20 text-amber-400'
                          :                          'bg-emerald-500/10 text-emerald-400'}">
              {entry.level}
            </span>
            <!-- message -->
            <span class="break-all {col}">{entry.message}</span>
          </div>
        {/each}
      </div>
    {/if}

  </div>
</section>
