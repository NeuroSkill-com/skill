<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  LLM Settings Tab
  ─────────────────
  • Model families from multiple Unsloth GGUF repos
  • Simple card per family, showing recommended quant + size prominently
  • "Show all quants" expands to full list with all sizes
  • Download / cancel / delete / select per quant
  • Advanced inference settings (GPU layers, ctx size, etc.)
  • Server log viewer with auto-scroll
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

  interface LlmLogEntry { ts: number; level: string; message: string; }

  type DownloadState = "not_downloaded"|"downloading"|"downloaded"|"failed"|"cancelled";

  interface LlmModelEntry {
    repo:        string;
    filename:    string;
    quant:       string;
    size_gb:     number;
    description: string;
    family_id:   string;
    family_name: string;
    family_desc: string;
    tags:        string[];
    is_mmproj:   boolean;
    recommended: boolean;
    advanced:    boolean;
    local_path:  string | null;
    state:       DownloadState;
    status_msg:  string | null;
    progress:    number;
  }

  interface LlmCatalog { entries: LlmModelEntry[]; active_model: string; active_mmproj: string; }

  interface LlmConfig {
    enabled: boolean; model_path: string | null; n_gpu_layers: number;
    ctx_size: number | null; parallel: number; api_key: string | null;
    mmproj: string | null; mmproj_n_threads: number; no_mmproj_gpu: boolean;
  }

  // Model family derived from entries sharing the same family_id
  interface ModelFamily {
    id:       string;
    name:     string;
    desc:     string;
    tags:     string[];
    entries:  LlmModelEntry[];     // non-mmproj entries, sorted
    mmproj:   LlmModelEntry[];
    recommended: LlmModelEntry | undefined;
    downloaded:  LlmModelEntry[];
  }

  // ── State ──────────────────────────────────────────────────────────────────

  let catalog = $state<LlmCatalog>({ entries: [], active_model: "", active_mmproj: "" });
  let config  = $state<LlmConfig>({
    enabled: false, model_path: null, n_gpu_layers: 4294967295,
    ctx_size: null, parallel: 1, api_key: null,
    mmproj: null, mmproj_n_threads: 4, no_mmproj_gpu: false,
  });

  let configSaving    = $state(false);
  let wsPort          = $state(8375);
  let apiKeyVisible   = $state(false);
  let ctxSizeInput    = $state("");
  let serverStatus    = $state<"stopped"|"loading"|"running">("stopped");
  let startError      = $state("");
  let showAdvanced    = $state(false);  // Advanced inference settings section

  // Per-family "show all quants" expand state: Set<family_id>
  let expandedFamilies = $state<Set<string>>(new Set());

  // Log state
  let logs          = $state<LlmLogEntry[]>([]);
  let logAutoScroll = $state(true);
  let logEl         = $state<HTMLElement | null>(null);

  let pollTimer:      ReturnType<typeof setInterval> | undefined;
  let unlistenLog:    (() => void) | undefined;
  let unlistenStatus: (() => void) | undefined;

  // ── Derived ────────────────────────────────────────────────────────────────

  // Group entries into model families
  const families = $derived.by<ModelFamily[]>(() => {
    const map = new Map<string, ModelFamily>();
    for (const e of catalog.entries) {
      if (!map.has(e.family_id)) {
        map.set(e.family_id, {
          id: e.family_id, name: e.family_name || e.family_id,
          desc: e.family_desc || "", tags: [],
          entries: [], mmproj: [],
          recommended: undefined, downloaded: [],
        });
      }
      const f = map.get(e.family_id)!;
      for (const tag of e.tags) { if (!f.tags.includes(tag)) f.tags.push(tag); }
      if (e.is_mmproj) {
        f.mmproj.push(e);
      } else {
        f.entries.push(e);
        if (e.recommended && !f.recommended) f.recommended = e;
        if (e.state === "downloaded") f.downloaded.push(e);
      }
    }
    return Array.from(map.values()).sort((a, b) => {
      const aDl = a.downloaded.length > 0 ? 0 : 1;
      const bDl = b.downloaded.length > 0 ? 0 : 1;
      if (aDl !== bDl) return aDl - bDl;
      return a.name.localeCompare(b.name);
    });
  });

  const hasActive = $derived(
    catalog.entries.some(e => !e.is_mmproj && e.filename === catalog.active_model && e.state === "downloaded")
  );

  const activeEntry = $derived(
    catalog.entries.find(e => !e.is_mmproj && e.filename === catalog.active_model) ?? null
  );

  // ── Helpers ────────────────────────────────────────────────────────────────

  function fmtSize(gb: number): string {
    if (gb < 0.1) return `${(gb * 1024).toFixed(0)} MB`;
    if (gb < 1)   return `${(gb * 1024).toFixed(0)} MB`;
    return `${gb.toFixed(1)} GB`;
  }

  function tagColor(tag: string): string {
    switch (tag) {
      case "chat":       return "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20";
      case "reasoning":  return "bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20";
      case "coding":     return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
      case "vision":     case "multimodal":
                         return "bg-amber-500/10 text-amber-700 dark:text-amber-400 border-amber-500/20";
      case "tiny":       return "bg-slate-500/10 text-slate-600 dark:text-slate-400 border-slate-500/20";
      default:           return "bg-slate-500/10 text-slate-500 border-slate-500/20";
    }
  }

  function tagLabel(tag: string): string {
    const MAP: Record<string,string> = {
      chat: "Chat", reasoning: "Reasoning", coding: "Coding",
      vision: "Vision", multimodal: "Multimodal",
      tiny: "Tiny", small: "Small", medium: "Medium", large: "Large",
    };
    return MAP[tag] ?? tag;
  }

  function stateBadgeClass(s: DownloadState): string {
    switch (s) {
      case "downloaded":  return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
      case "downloading": return "bg-blue-500/10 text-blue-600 dark:text-blue-400 border-blue-500/20";
      case "failed":      return "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20";
      case "cancelled":   return "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20";
      default:            return "bg-slate-500/10 text-slate-500 border-slate-500/20";
    }
  }

  /** Quants to show for a family: recommended + non-advanced in simple mode; all in expanded. */
  function visibleEntries(f: ModelFamily): LlmModelEntry[] {
    if (expandedFamilies.has(f.id)) return f.entries;
    // Simple mode: downloaded + recommended + Q4_K_M (the "also good" quant)
    return f.entries.filter(e =>
      e.state === "downloaded" ||
      e.filename === catalog.active_model ||
      e.recommended ||
      !e.advanced
    );
  }

  function toggleFamily(fid: string) {
    const s = new Set(expandedFamilies);
    if (s.has(fid)) s.delete(fid); else s.add(fid);
    expandedFamilies = s;
  }

  // ── Data loading ───────────────────────────────────────────────────────────

  async function loadCatalog() {
    try { catalog = await invoke<LlmCatalog>("get_llm_catalog"); } catch {}
  }

  async function loadConfig() {
    try {
      config = await invoke<LlmConfig>("get_llm_config");
      ctxSizeInput = config.ctx_size !== null ? String(config.ctx_size) : "";
    } catch {}
    try {
      const [, port] = await invoke<[string, number]>("get_ws_config");
      wsPort = port;
    } catch {}
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
    // Auto-restart the server with the new model in the background.
    // Stop first (no-op if not running), then start.
    try { await invoke("stop_llm_server"); } catch {}
    startError = "";
    invoke("start_llm_server").catch((e: any) => {
      startError = typeof e === "string" ? e : (e?.message ?? "Failed to start LLM server");
    });
    // Give it a moment then refresh status so the UI shows "Loading…"
    await new Promise(r => setTimeout(r, 300));
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
    try { await invoke("start_llm_server"); }
    catch (e: any) { startError = typeof e === "string" ? e : (e?.message ?? "Unknown error"); }
  }

  async function stopServer() {
    startError = "";
    try { await invoke("stop_llm_server"); } catch {}
  }

  async function openChat() {
    try { await invoke("open_chat_window"); } catch {}
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  onMount(async () => {
    await Promise.all([loadCatalog(), loadConfig()]);
    try {
      const s = await invoke<{ status: "stopped"|"loading"|"running" }>("get_llm_server_status");
      serverStatus = s.status;
    } catch {}
    try {
      unlistenStatus = await listen<{ status: "stopped"|"loading"|"running" }>(
        "llm:status", ev => { serverStatus = (ev.payload as any).status ?? serverStatus; }
      );
    } catch {}
    try {
      logs = await invoke<LlmLogEntry[]>("get_llm_logs");
      await scrollToBottom();
    } catch {}
    try {
      unlistenLog = await listen<LlmLogEntry>("llm:log", async ev => {
        logs = [...logs.slice(-499), ev.payload];
        if (logAutoScroll) await scrollToBottom();
      });
    } catch {}
    pollTimer = setInterval(async () => {
      if (catalog.entries.some(e => e.state === "downloading")) await loadCatalog();
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
    logAutoScroll = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 40;
  }
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Server status card                                                          -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.server")}
    </span>
    <span class="w-1.5 h-1.5 rounded-full {hasActive && config.enabled ? 'bg-emerald-500' : 'bg-slate-400'}"></span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- Enable toggle -->
      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.enabled")}</span>
          <span class="text-[0.65rem] text-muted-foreground leading-relaxed">{t("llm.enabledDesc")}</span>
        </div>
        <button role="switch" aria-checked={config.enabled} aria-label={t("llm.enabled")}
          onclick={async () => { config = { ...config, enabled: !config.enabled }; await saveConfig(); }}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {config.enabled ? 'bg-emerald-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {config.enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

      <!-- Status + controls -->
      <div class="flex items-center justify-between gap-4 px-4 py-3">
        <div class="flex items-center gap-2">
          <span class="w-2 h-2 rounded-full shrink-0
            {serverStatus === 'running'  ? 'bg-emerald-500'
            : serverStatus === 'loading' ? 'bg-amber-500 animate-pulse'
            :                             'bg-slate-400/50'}"></span>
          <span class="text-[0.78rem] font-semibold text-foreground">
            {serverStatus === "running"  ? (activeEntry?.family_name ?? "Running")
            : serverStatus === "loading" ? "Loading…"
            :                             "Stopped"}
          </span>
          {#if serverStatus === "running" && activeEntry}
            <span class="text-[0.62rem] text-muted-foreground/60 font-mono">
              {activeEntry.quant} · {fmtSize(activeEntry.size_gb)}
            </span>
          {/if}
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
            Chat…
          </Button>
        </div>
      </div>

      {#if startError}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-red-500/10 border border-red-500/20
                    text-[0.68rem] text-red-600 dark:text-red-400 leading-snug">
          {startError}
        </div>
      {/if}

      {#if serverStatus === "stopped" && catalog.active_model && !hasActive}
        <div class="mx-4 mb-2 px-3 py-2 rounded-lg bg-amber-500/10 border border-amber-500/20
                    text-[0.68rem] text-amber-700 dark:text-amber-400 leading-snug">
          <strong>{catalog.active_model}</strong> is not downloaded yet.
          Find it in Models below and click Download.
        </div>
      {/if}

      <!-- Endpoint row -->
      <div class="flex flex-col gap-0.5 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
          {t("llm.endpoint")}
        </span>
        <div class="flex flex-wrap gap-1">
          {#each ["/v1/chat/completions","/v1/completions","/v1/embeddings","/v1/models","/health"] as ep}
            <code class="text-[0.6rem] font-mono text-muted-foreground
                          bg-muted dark:bg-white/5 rounded px-1.5 py-0.5">{ep}</code>
          {/each}
        </div>
        <span class="text-[0.58rem] text-muted-foreground/60 mt-0.5">
          http://localhost:{wsPort} · {t("llm.endpointHint")}
        </span>
      </div>

    </CardContent>
  </Card>
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Model families                                                              -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.section.models")}
    </span>
    <button onclick={refreshCache}
      class="ml-auto text-[0.56rem] text-muted-foreground/60 hover:text-foreground
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
    {#each families as family (family.id)}
      {@const isExpanded = expandedFamilies.has(family.id)}
      {@const shown      = visibleEntries(family)}
      {@const hasHidden  = shown.length < family.entries.length}
      {@const hasVision  = family.tags.some((t: string) => t === "vision" || t === "multimodal")}

      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="py-0 px-0 flex flex-col">

          <!-- Family header -->
          <div class="flex items-start gap-3 px-4 pt-3.5 pb-2">
            <div class="flex flex-col gap-1 flex-1 min-w-0">
              <div class="flex items-center gap-1.5 flex-wrap">
                <span class="text-[0.82rem] font-bold text-foreground">{family.name}</span>
                {#each family.tags.filter((t: string) => !["tiny","small","medium","large"].includes(t)) as tag}
                  <Badge variant="outline" class="text-[0.5rem] py-0 px-1.5 {tagColor(tag)}">
                    {tagLabel(tag)}
                  </Badge>
                {/each}
                {#if family.downloaded.length > 0}
                  <Badge variant="outline"
                    class="text-[0.5rem] py-0 px-1.5 bg-emerald-500/10 text-emerald-600
                           dark:text-emerald-400 border-emerald-500/20">
                    {family.downloaded.length} downloaded
                  </Badge>
                {/if}
              </div>
              <p class="text-[0.65rem] text-muted-foreground leading-snug">{family.desc}</p>
            </div>

            <!-- Repo link -->
            <span class="text-[0.55rem] text-muted-foreground/50 font-mono shrink-0 mt-0.5 truncate max-w-[45%]">
              {family.entries[0]?.repo ?? ""}
            </span>
          </div>

          <!-- Quant rows -->
          <div class="flex flex-col divide-y divide-border/40 dark:divide-white/[0.04]">
            {#each shown as entry (entry.filename)}
              {@const isActive     = catalog.active_model === entry.filename}
              {@const downloading  = entry.state === "downloading"}
              {@const downloaded   = entry.state === "downloaded"}
              {@const failed       = entry.state === "failed" || entry.state === "cancelled"}

              <div class="flex flex-col gap-1.5 px-4 py-2.5
                          {isActive ? 'bg-violet-50/60 dark:bg-violet-950/20' : ''}">

                <!-- Main row: quant · size · badges · actions -->
                <div class="flex items-center gap-2 min-w-0">
                  <!-- Quant name + size -->
                  <div class="flex items-baseline gap-1.5 flex-1 min-w-0">
                    <span class="text-[0.8rem] font-bold font-mono text-foreground shrink-0">
                      {entry.quant}
                    </span>
                    <span class="text-[0.72rem] font-semibold tabular-nums text-muted-foreground shrink-0
                                  {downloaded ? 'text-foreground/70' : ''}">
                      {fmtSize(entry.size_gb)}
                    </span>
                    {#if entry.recommended}
                      <span class="text-[0.58rem] text-violet-600 dark:text-violet-400 shrink-0">
                        ★ recommended
                      </span>
                    {/if}
                    {#if isActive}
                      <span class="text-[0.58rem] text-emerald-600 dark:text-emerald-400 shrink-0">
                        ✓ active
                      </span>
                    {/if}
                    <!-- short description (truncated) -->
                    <span class="text-[0.62rem] text-muted-foreground/60 truncate hidden sm:block">
                      {entry.description}
                    </span>
                  </div>

                  <!-- Action buttons -->
                  <div class="flex items-center gap-1 shrink-0">
                    {#if downloading}
                      <Button size="sm" variant="outline"
                        class="h-6 text-[0.6rem] px-2 text-destructive border-destructive/30
                               hover:bg-destructive/10"
                        onclick={() => cancelDownload(entry.filename)}>
                        Cancel
                      </Button>

                    {:else if downloaded}
                      <Button size="sm" variant="ghost"
                        class="h-6 text-[0.6rem] px-2 text-muted-foreground hover:text-red-500"
                        onclick={() => deleteModel(entry.filename)}>
                        Delete
                      </Button>
                      <Button size="sm"
                        class="h-6 text-[0.6rem] px-2.5
                               {isActive
                                 ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border border-emerald-500/30 hover:bg-emerald-500/20'
                                 : 'bg-violet-600 hover:bg-violet-700 text-white'}"
                        onclick={() => selectModel(entry.filename)}>
                        {isActive ? "Selected" : "Use"}
                      </Button>

                    {:else}
                      <Button size="sm"
                        class="h-6 text-[0.6rem] px-2.5 bg-violet-600 hover:bg-violet-700 text-white"
                        onclick={() => download(entry.filename)}>
                        {failed ? "Retry" : `Download ${fmtSize(entry.size_gb)}`}
                      </Button>
                    {/if}
                  </div>
                </div>

                <!-- Progress bar -->
                {#if downloading}
                  <div class="h-1 w-full rounded-full bg-muted overflow-hidden">
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
                    <p class="text-[0.58rem] text-blue-500 truncate -mt-0.5">{entry.status_msg}</p>
                  {/if}
                {/if}

                <!-- Error message -->
                {#if failed && entry.status_msg}
                  <p class="text-[0.6rem] text-destructive/80 font-mono break-all leading-relaxed
                             rounded bg-destructive/5 border border-destructive/10 px-2 py-1 -mt-0.5">
                    {entry.status_msg}
                  </p>
                {/if}

                <!-- Local path (when downloaded) -->
                {#if downloaded && entry.local_path}
                  <p class="text-[0.56rem] font-mono text-muted-foreground/50 break-all leading-tight -mt-0.5">
                    {entry.local_path}
                  </p>
                {/if}

              </div>
            {/each}
          </div>

          <!-- Expand / collapse quants footer -->
          <button
            onclick={() => toggleFamily(family.id)}
            class="flex items-center justify-center gap-1.5 py-2 text-[0.6rem]
                   text-muted-foreground/60 hover:text-muted-foreground
                   border-t border-border/40 dark:border-white/[0.04]
                   bg-slate-50/50 dark:bg-[#111118]/50 w-full
                   transition-colors cursor-pointer select-none">
            {#if isExpanded}
              <svg viewBox="0 0 16 16" fill="currentColor" class="w-2.5 h-2.5">
                <path d="M3 10l5-5 5 5H3z"/>
              </svg>
              Show less
            {:else}
              <svg viewBox="0 0 16 16" fill="currentColor" class="w-2.5 h-2.5">
                <path d="M3 6l5 5 5-5H3z"/>
              </svg>
              {hasHidden
                ? `Show all ${family.entries.length} quants`
                : `${family.entries.length} quant${family.entries.length !== 1 ? "s" : ""}`}
            {/if}
          </button>

          <!-- mmproj section (vision models only) -->
          {#if hasVision && family.mmproj.length > 0}
            <div class="border-t border-border dark:border-white/[0.06] px-4 py-2.5 bg-amber-50/30 dark:bg-amber-950/10">
              <p class="text-[0.6rem] font-semibold text-amber-700 dark:text-amber-400 mb-2">
                Vision projector (required for image input)
              </p>
              {#each family.mmproj as mp (mp.filename)}
                {@const isActiveMm = catalog.active_mmproj === mp.filename}
                <div class="flex items-center gap-2 py-1">
                  <div class="flex-1 min-w-0">
                    <span class="text-[0.7rem] font-mono text-foreground">{mp.filename}</span>
                    <span class="text-[0.62rem] text-muted-foreground ml-2">{fmtSize(mp.size_gb)}</span>
                    {#if mp.recommended}
                      <span class="text-[0.58rem] text-violet-600 dark:text-violet-400 ml-1.5">★</span>
                    {/if}
                  </div>
                  <div class="flex items-center gap-1 shrink-0">
                    {#if mp.state === "downloading"}
                      <Button size="sm" variant="outline"
                        class="h-5 text-[0.58rem] px-1.5 text-destructive border-destructive/30"
                        onclick={() => cancelDownload(mp.filename)}>Cancel</Button>
                    {:else if mp.state === "downloaded"}
                      <Button size="sm" variant="ghost"
                        class="h-5 text-[0.58rem] px-1.5 text-muted-foreground hover:text-red-500"
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
              {/each}
            </div>
          {/if}

        </CardContent>
      </Card>
    {/each}
  {/if}
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Advanced inference settings (collapsible)                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <button
    onclick={() => showAdvanced = !showAdvanced}
    class="flex items-center gap-2 px-0.5 cursor-pointer select-none group">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground
                  group-hover:text-foreground transition-colors">
      {t("llm.section.inference")}
    </span>
    <svg viewBox="0 0 16 16" fill="currentColor"
         class="w-2.5 h-2.5 text-muted-foreground/50 transition-transform
                {showAdvanced ? 'rotate-180' : ''}">
      <path d="M3 6l5 5 5-5H3z"/>
    </svg>
    {#if configSaving}<span class="text-[0.56rem] text-muted-foreground">saving…</span>{/if}
  </button>

  {#if showAdvanced}
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

      <!-- GPU layers -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <div class="flex items-baseline justify-between">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.inference.gpuLayers")}</span>
          <span class="text-[0.68rem] text-muted-foreground tabular-nums">
            {config.n_gpu_layers === 0 ? "CPU only" : config.n_gpu_layers >= 4294967295 ? "All layers" : config.n_gpu_layers}
          </span>
        </div>
        <p class="text-[0.65rem] text-muted-foreground -mt-1">{t("llm.inference.gpuLayersDesc")}</p>
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each [[0,"CPU"],[8,"8"],[16,"16"],[32,"32"],[4294967295,"All"]] as [val, label]}
            <button
              onclick={async () => { config = { ...config, n_gpu_layers: val as number }; await saveConfig(); }}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold transition-all cursor-pointer
                     {config.n_gpu_layers === val
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
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
            {config.ctx_size !== null ? config.ctx_size + " tokens" : "auto"}
          </span>
        </div>
        <p class="text-[0.65rem] text-muted-foreground -mt-1">{t("llm.inference.ctxSizeDesc")}</p>
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each [[null,"auto"],[2048,"2K"],[4096,"4K"],[8192,"8K"],[16384,"16K"],[32768,"32K"]] as [val, label]}
            <button
              onclick={async () => { ctxSizeInput = val !== null ? String(val) : ""; config = { ...config, ctx_size: val as number|null }; await saveConfig(); }}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold transition-all cursor-pointer
                     {config.ctx_size === val
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
              {label}
            </button>
          {/each}
        </div>
      </div>

      <!-- Parallel -->
      <div class="flex items-center justify-between gap-4 px-4 py-3.5">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.inference.parallel")}</span>
          <span class="text-[0.65rem] text-muted-foreground">{t("llm.inference.parallelDesc")}</span>
        </div>
        <div class="flex items-center gap-1.5">
          {#each [1,2,4] as val}
            <button
              onclick={async () => { config = { ...config, parallel: val }; await saveConfig(); }}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold transition-all cursor-pointer
                     {config.parallel === val
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
              {val}
            </button>
          {/each}
        </div>
      </div>

      <!-- API key -->
      <div class="flex flex-col gap-2 px-4 py-3.5">
        <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.inference.apiKey")}</span>
        <p class="text-[0.65rem] text-muted-foreground -mt-1">{t("llm.inference.apiKeyDesc")}</p>
        <div class="flex items-center gap-2">
          <input type={apiKeyVisible ? "text" : "password"}
            placeholder={t("llm.inference.apiKeyPlaceholder")}
            bind:value={config.api_key}
            onblur={saveConfig}
            class="flex-1 min-w-0 text-[0.73rem] font-mono px-2 py-1 rounded-md
                   border border-border bg-background text-foreground placeholder:text-muted-foreground/40" />
          <button onclick={() => apiKeyVisible = !apiKeyVisible}
            class="shrink-0 text-[0.62rem] text-muted-foreground hover:text-foreground cursor-pointer">
            {apiKeyVisible ? "hide" : "show"}
          </button>
          {#if config.api_key}
            <button onclick={async () => { config = { ...config, api_key: null }; await saveConfig(); }}
              class="shrink-0 text-[0.62rem] text-muted-foreground hover:text-red-500 cursor-pointer">
              clear
            </button>
          {/if}
        </div>
      </div>

      <!-- Multimodal settings (only if mmproj downloaded) -->
      {#if catalog.entries.some(e => e.is_mmproj && e.state === "downloaded")}
        <div class="flex items-center justify-between gap-4 px-4 py-3.5">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.mmproj.noGpu")}</span>
            <span class="text-[0.65rem] text-muted-foreground">{t("llm.mmproj.noGpuDesc")}</span>
          </div>
          <button role="switch" aria-checked={config.no_mmproj_gpu} aria-label={t("llm.mmproj.noGpu")}
            onclick={async () => { config = { ...config, no_mmproj_gpu: !config.no_mmproj_gpu }; await saveConfig(); }}
            class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                   border-transparent transition-colors duration-200
                   {config.no_mmproj_gpu ? 'bg-blue-500' : 'bg-muted dark:bg-white/10'}">
            <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                          transform transition-transform duration-200
                          {config.no_mmproj_gpu ? 'translate-x-4' : 'translate-x-0'}"></span>
          </button>
        </div>
      {/if}

      <!-- curl quick test -->
      <div class="flex flex-col gap-1.5 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
        <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">Quick test</span>
        <pre class="text-[0.58rem] font-mono text-muted-foreground/80 whitespace-pre-wrap break-all leading-relaxed">curl http://localhost:{wsPort}/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '&#123;"model":"default","messages":[&#123;"role":"user","content":"Hello!"&#125;]&#125;'</pre>
      </div>

    </CardContent>
  </Card>
  {/if}
</section>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Server log                                                                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      Server log
    </span>
    <span class="flex items-center gap-1 text-[0.52rem] text-muted-foreground/50">
      <span class="w-1 h-1 rounded-full {logs.length > 0 ? 'bg-emerald-500 animate-pulse' : 'bg-slate-400'}"></span>
      {logs.length} line{logs.length !== 1 ? "s" : ""}
    </span>
    <button
      onclick={() => { logAutoScroll = !logAutoScroll; if (logAutoScroll) scrollToBottom(); }}
      class="ml-auto text-[0.52rem] cursor-pointer select-none transition-colors
             {logAutoScroll ? 'text-emerald-600 dark:text-emerald-400' : 'text-muted-foreground/50 hover:text-foreground'}">
      auto-scroll {logAutoScroll ? "on" : "off"}
    </button>
    <button onclick={() => { logs = []; }}
      class="text-[0.52rem] text-muted-foreground/50 hover:text-muted-foreground cursor-pointer select-none">
      clear
    </button>
  </div>

  <div bind:this={logEl} onscroll={handleLogScroll}
       class="h-64 overflow-y-auto rounded-xl border border-border dark:border-white/[0.06]
              bg-[#0d0d14] font-mono text-[0.62rem] leading-5
              scrollbar-thin scrollbar-track-transparent scrollbar-thumb-white/10">
    {#if logs.length === 0}
      <div class="flex items-center justify-center h-full text-muted-foreground/30 text-[0.65rem]">
        No log output yet.
      </div>
    {:else}
      <div class="px-3 py-2 flex flex-col gap-0">
        {#each logs as entry (entry.ts + entry.message)}
          {@const ts  = new Date(entry.ts).toISOString().slice(11, 23)}
          {@const col = entry.level === "error" ? "text-red-400" : entry.level === "warn" ? "text-amber-400" : "text-emerald-300/80"}
          <div class="flex items-start gap-2 min-w-0">
            <span class="shrink-0 text-white/20 tabular-nums">{ts}</span>
            <span class="shrink-0 w-8 text-center rounded text-[0.5rem] px-0.5
                          {entry.level === 'error' ? 'bg-red-500/20 text-red-400'
                          : entry.level === 'warn' ? 'bg-amber-500/20 text-amber-400'
                          :                         'bg-emerald-500/10 text-emerald-400'}">
              {entry.level}
            </span>
            <span class="break-all {col}">{entry.message}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>
