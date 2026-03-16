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
  type ToolExecutionMode = "sequential" | "parallel";
  interface LlmToolsConfig {
    date: boolean;
    location: boolean;
    web_search: boolean;
    web_fetch: boolean;
    bash: boolean;
    read_file: boolean;
    write_file: boolean;
    edit_file: boolean;
    execution_mode: ToolExecutionMode;
    max_rounds: number;
    max_calls_per_round: number;
  }

  interface LlmConfig {
    enabled: boolean; model_path: string | null; n_gpu_layers: number;
    ctx_size: number | null; parallel: number; api_key: string | null;
    tools: LlmToolsConfig;
    mmproj: string | null; mmproj_n_threads: number; no_mmproj_gpu: boolean;
    autoload_mmproj: boolean; verbose: boolean;
  }

  type LlmToolKey = "date" | "location" | "web_search" | "web_fetch" | "bash" | "read_file" | "write_file" | "edit_file";

  interface ModelFamily {
    id:          string;
    name:        string;
    desc:        string;
    tags:        string[];
    vendors:     string[];
    entries:     LlmModelEntry[];   // non-mmproj, in catalog order
    mmproj:      LlmModelEntry[];
    recommended: LlmModelEntry | undefined;
    downloaded:  LlmModelEntry[];
  }

  interface ModelHardwareFit {
    filename:          string;
    fitLevel:          "perfect"|"good"|"marginal"|"too_tight";
    runMode:           "gpu"|"moe"|"cpu_gpu"|"cpu";
    memoryRequiredGb:  number;
    memoryAvailableGb: number;
    estimatedTps:      number;
    score:             number;
    notes:             string[];
  }

  // ── State ──────────────────────────────────────────────────────────────────

  let hardwareFits = $state<Map<string, ModelHardwareFit>>(new Map());

  let catalog = $state<LlmCatalog>({ entries: [], active_model: "", active_mmproj: "" });
  let config  = $state<LlmConfig>({
    enabled: false, model_path: null, n_gpu_layers: 4294967295,
    ctx_size: null, parallel: 1, api_key: null,
    tools: { date: true, location: true, web_search: true, web_fetch: true, bash: false, read_file: false, write_file: false, edit_file: false, execution_mode: "parallel" as ToolExecutionMode, max_rounds: 3, max_calls_per_round: 4 },
    mmproj: null, mmproj_n_threads: 4, no_mmproj_gpu: false, autoload_mmproj: true,
    verbose: false,
  });

  let configSaving    = $state(false);
  let wsPort          = $state(8375);
  let apiKeyVisible   = $state(false);
  let ctxSizeInput    = $state("");
  let serverStatus    = $state<"stopped"|"loading"|"running">("stopped");
  let startError      = $state("");
  let showAdvanced    = $state(false);
  let showAllQuants   = $state(false);

  /** The family currently shown in the detail panel. */
  let selectedFamilyId = $state<string>("");
  let previousFamilyId = $state<string>("");

  let logs          = $state<LlmLogEntry[]>([]);
  let logAutoScroll = $state(true);
  let logEl         = $state<HTMLElement | null>(null);
  let logFilter     = $state<"all"|"info"|"warn"|"error">("all");
  let logSearch     = $state("");

  const filteredLogs = $derived.by(() => {
    let filtered = logs;
    if (logFilter !== "all") filtered = filtered.filter(e => e.level === logFilter);
    if (logSearch.trim()) {
      const q = logSearch.trim().toLowerCase();
      filtered = filtered.filter(e => e.message.toLowerCase().includes(q));
    }
    return filtered;
  });

  let pollTimer:      ReturnType<typeof setInterval> | undefined;
  let unlistenLog:    (() => void) | undefined;
  let unlistenStatus: (() => void) | undefined;

  let TOOL_ROWS = $derived<Array<{ key: LlmToolKey; label: string; desc: string; warn?: boolean }>>(
    [
      { key: "date",       label: t("llm.tools.date"),      desc: t("llm.tools.dateDesc") },
      { key: "location",   label: t("llm.tools.location"),  desc: t("llm.tools.locationDesc") },
      { key: "web_search", label: t("llm.tools.webSearch"), desc: t("llm.tools.webSearchDesc") },
      { key: "web_fetch",  label: t("llm.tools.webFetch"),  desc: t("llm.tools.webFetchDesc") },
      { key: "bash",       label: t("llm.tools.bash"),      desc: t("llm.tools.bashDesc"),      warn: true },
      { key: "read_file",  label: t("llm.tools.readFile"),  desc: t("llm.tools.readFileDesc") },
      { key: "write_file", label: t("llm.tools.writeFile"), desc: t("llm.tools.writeFileDesc"), warn: true },
      { key: "edit_file",  label: t("llm.tools.editFile"),  desc: t("llm.tools.editFileDesc"),  warn: true },
    ]
  );

  // ── Derived ────────────────────────────────────────────────────────────────

  const families = $derived.by<ModelFamily[]>(() => {
    const map = new Map<string, ModelFamily>();
    for (const e of catalog.entries) {
      if (!map.has(e.family_id)) {
        map.set(e.family_id, {
          id: e.family_id, name: e.family_name || e.family_id,
          desc: e.family_desc || "", tags: [], vendors: [],
          entries: [], mmproj: [],
          recommended: undefined, downloaded: [],
        });
      }
      const f = map.get(e.family_id)!;
      for (const tag of e.tags) { if (!f.tags.includes(tag)) f.tags.push(tag); }
      const vendor = vendorLabel(e.repo);
      if (!f.vendors.includes(vendor)) f.vendors.push(vendor);
      if (e.is_mmproj) {
        f.mmproj.push(e);
      } else {
        f.entries.push(e);
        if (e.recommended && !f.recommended) f.recommended = e;
        if (e.state === "downloaded") f.downloaded.push(e);
      }
    }
    // Sort dropdown families by model name, then by size.
    return Array.from(map.values())
      .filter(f => f.entries.length > 0)
      .sort((a, b) => {
      const byName = a.name.localeCompare(b.name);
      if (byName !== 0) return byName;

      const aSize = familyPrimarySize(a.entries);
      const bSize = familyPrimarySize(b.entries);
      if (aSize !== bSize) return aSize - bSize;

      const aTagSize = familySizeRank(a.tags);
      const bTagSize = familySizeRank(b.tags);
      if (aTagSize !== bTagSize) return aTagSize - bTagSize;

        return a.id.localeCompare(b.id);
      });
  });

  /** Auto-select a family when the list first loads or active model changes. */
  $effect(() => {
    if (families.length === 0) return;
    // If current selection still exists, keep it
    if (selectedFamilyId && families.some(f => f.id === selectedFamilyId)) return;
    // Otherwise pick: active model's family → first downloaded → first
    const activeEntry = catalog.entries.find(e => !e.is_mmproj && e.filename === catalog.active_model);
    if (activeEntry) { selectedFamilyId = activeEntry.family_id; return; }
    const dlFamily = families.find(f => f.downloaded.length > 0);
    selectedFamilyId = (dlFamily ?? families[0]).id;
  });

  const selectedFamily = $derived(
    families.find(f => f.id === selectedFamilyId) ?? families[0] ?? null
  );

  const selectedFamilyHasMultipleVendors = $derived((selectedFamily?.vendors.length ?? 0) > 1);

  const orderedSelectedEntries = $derived.by<LlmModelEntry[]>(() => {
    if (!selectedFamily) return [];
    return [...selectedFamily.entries].sort(compareModelEntries);
  });

  const selectedEntryGroups = $derived.by(() => {
    const pinned = new Set<string>();
    for (const entry of orderedSelectedEntries) {
      if (
        entry.filename === catalog.active_model ||
        entry.recommended ||
        entry.state === "downloaded" ||
        entry.state === "downloading" ||
        !entry.advanced
      ) {
        pinned.add(entry.filename);
      }
    }

    return {
      primary: orderedSelectedEntries.filter(entry => pinned.has(entry.filename)),
      extra: orderedSelectedEntries.filter(entry => !pinned.has(entry.filename)),
    };
  });

  const orderedSelectedMmproj = $derived.by<LlmModelEntry[]>(() => {
    if (!selectedFamily) return [];
    return [...selectedFamily.mmproj].sort(compareModelEntries);
  });

  const hasActive = $derived(
    catalog.entries.some(e => !e.is_mmproj && e.filename === catalog.active_model && e.state === "downloaded")
  );

  const activeEntry = $derived(
    catalog.entries.find(e => !e.is_mmproj && e.filename === catalog.active_model) ?? null
  );

  $effect(() => {
    if (selectedFamilyId !== previousFamilyId) {
      showAllQuants = false;
      previousFamilyId = selectedFamilyId;
    }
  });

  // ── Helpers ────────────────────────────────────────────────────────────────

  function fmtSize(gb: number): string {
    if (gb < 1) return `${(gb * 1024).toFixed(0)} MB`;
    return `${gb.toFixed(1)} GB`;
  }

  function vendorLabel(repo: string): string {
    const owner = repo.split("/")[0] ?? repo;
    const labels: Record<string, string> = {
      bartowski: "Bartowski",
      unsloth: "Unsloth",
      HauhauCS: "HauhauCS",
    };
    return labels[owner] ?? owner;
  }

  function familySizeRank(tags: string[]): number {
    if (tags.includes("tiny")) return 0;
    if (tags.includes("small")) return 1;
    if (tags.includes("medium")) return 2;
    if (tags.includes("large")) return 3;
    return 4;
  }

  function familyPrimarySize(entries: LlmModelEntry[]): number {
    const recommended = entries.find(entry => entry.recommended);
    if (recommended) return recommended.size_gb;

    const standard = entries.find(entry => !entry.advanced);
    if (standard) return standard.size_gb;

    return entries.reduce((smallest, entry) => Math.min(smallest, entry.size_gb), Number.POSITIVE_INFINITY);
  }

  function quantRank(quant: string): number {
    const order = [
      "Q4_K_M", "Q4_0", "Q4_K_S", "Q4_K_L", "Q4_1",
      "Q5_K_M", "Q5_K_S", "Q5_K_L",
      "Q6_K", "Q6_K_L",
      "Q8_0",
      "IQ4_XS", "IQ4_NL",
      "Q3_K_M", "Q3_K_L", "Q3_K_XL", "Q3_K_S",
      "IQ3_M", "IQ3_XS", "IQ3_XXS",
      "Q2_K", "Q2_K_L",
      "IQ2_M", "IQ2_S", "IQ2_XS", "IQ2_XXS",
      "BF16", "F16", "F32",
    ];
    const index = order.indexOf(quant.toUpperCase());
    return index === -1 ? order.length : index;
  }

  function compareModelEntries(a: LlmModelEntry, b: LlmModelEntry): number {
    const aPin =
      a.filename === catalog.active_model ? 0 :
      a.state === "downloading" ? 1 :
      a.state === "downloaded" ? 2 :
      a.recommended ? 3 :
      !a.advanced ? 4 : 5;
    const bPin =
      b.filename === catalog.active_model ? 0 :
      b.state === "downloading" ? 1 :
      b.state === "downloaded" ? 2 :
      b.recommended ? 3 :
      !b.advanced ? 4 : 5;

    if (aPin !== bPin) return aPin - bPin;

    const aQuant = quantRank(a.quant);
    const bQuant = quantRank(b.quant);
    if (aQuant !== bQuant) return aQuant - bQuant;

    if (a.size_gb !== b.size_gb) return a.size_gb - b.size_gb;
    return a.quant.localeCompare(b.quant) || a.filename.localeCompare(b.filename);
  }

  function fitBadgeClass(level: string): string {
    switch (level) {
      case "perfect":   return "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-500/30";
      case "good":      return "bg-sky-500/15 text-sky-700 dark:text-sky-400 border-sky-500/30";
      case "marginal":  return "bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30";
      case "too_tight": return "bg-red-500/15 text-red-700 dark:text-red-400 border-red-500/30";
      default:          return "bg-slate-500/10 text-slate-500 border-slate-500/20";
    }
  }

  function fitBadgeIcon(level: string): string {
    switch (level) {
      case "perfect":   return "🟢";
      case "good":      return "🟡";
      case "marginal":  return "🟠";
      case "too_tight": return "🔴";
      default:          return "⚪";
    }
  }

  function fitBadgeLabel(level: string): string {
    switch (level) {
      case "perfect":   return t("llm.fit.perfect");
      case "good":      return t("llm.fit.good");
      case "marginal":  return t("llm.fit.marginal");
      case "too_tight": return t("llm.fit.tooTight");
      default:          return "";
    }
  }

  function runModeLabel(mode: string): string {
    switch (mode) {
      case "gpu":     return "GPU";
      case "moe":     return "MoE offload";
      case "cpu_gpu": return "CPU + GPU";
      case "cpu":     return "CPU";
      default:        return mode;
    }
  }

  function tagColor(tag: string): string {
    switch (tag) {
      case "chat":      return "bg-primary/10 text-primary border-primary/20";
      case "reasoning": return "bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20";
      case "coding":    return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
      case "vision": case "multimodal":
                        return "bg-amber-500/10 text-amber-700 dark:text-amber-400 border-amber-500/20";
      default:          return "bg-slate-500/10 text-slate-500 border-slate-500/20";
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

  /** Option label for the <select> dropdown — concise with status hint. */
  function familyOptionLabel(f: ModelFamily): string {
    const active = f.entries.some(e => e.filename === catalog.active_model);
    const dlCount = f.downloaded.length;
    const loading = f.entries.some(e => e.state === "downloading");
    let prefix = "";
    if (active)        prefix = "✓ ";
    else if (loading)  prefix = "⬇ ";
    let suffix = "";
    if (dlCount > 0 && !active) suffix = ` (${dlCount} downloaded)`;
    return `${prefix}${f.name}${suffix}`;
  }

  // ── Data loading ───────────────────────────────────────────────────────────

  async function loadCatalog() {
    try { catalog = await invoke<LlmCatalog>("get_llm_catalog"); } catch {}
  }

  async function loadHardwareFit() {
    try {
      const fits = await invoke<ModelHardwareFit[]>("get_model_hardware_fit");
      const map = new Map<string, ModelHardwareFit>();
      for (const f of fits) map.set(f.filename, f);
      hardwareFits = map;
    } catch {}
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
    invoke("switch_llm_model", { filename }).catch((e: any) => {
      startError = typeof e === "string" ? e : (e?.message ?? "Failed to switch model");
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
    invoke("start_llm_server").catch((e: any) => {
      startError = typeof e === "string" ? e : (e?.message ?? "Unknown error");
    });
  }

  async function stopServer() {
    startError = "";
    // stop_llm_server is also fire-and-forget — actor join runs in background.
    invoke("stop_llm_server").catch(() => {});
  }

  async function openChat() {
    try { await invoke("open_chat_window"); } catch {}
  }

  async function openDownloads() {
    try { await invoke("open_downloads_window"); } catch {}
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  onMount(async () => {
    await Promise.all([loadCatalog(), loadConfig(), loadHardwareFit()]);
    try {
      const s = await invoke<{
        status: "stopped"|"loading"|"running";
        start_error: string | null;
      }>("get_llm_server_status");
      serverStatus = s.status;
      if (s.start_error) startError = s.start_error;
    } catch {}
    try {
      unlistenStatus = await listen<{ status: string }>(
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
    // Poll every 500 ms while a download is active so the progress bar stays
    // smooth.  The backend blob-monitor fires every 400 ms, so this gives
    // roughly one UI update per backend tick.
    pollTimer = setInterval(async () => {
      if (catalog.entries.some(e => e.state === "downloading")) await loadCatalog();
      // Poll server status so Loading → Running and start_error are reflected
      // without relying solely on push events.
      try {
        const s = await invoke<{
          status: "stopped"|"loading"|"running";
          start_error: string | null;
        }>("get_llm_server_status");
        serverStatus = s.status;
        if (s.start_error) startError = s.start_error;
      } catch {}
    }, 1000);
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

      <!-- Auto-start toggle -->
      <div class="flex items-center justify-between gap-4 px-4 py-3">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.autostart")}</span>
          <span class="text-[0.65rem] text-muted-foreground leading-relaxed">{t("llm.autostartDesc")}</span>
        </div>
        <button role="switch" aria-checked={config.autostart} aria-label={t("llm.autostart")}
          onclick={async () => { config = { ...config, autostart: !config.autostart }; await saveConfig(); }}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {config.autostart ? 'bg-emerald-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {config.autostart ? 'translate-x-4' : 'translate-x-0'}"></span>
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
          <option value={f.id}>{familyOptionLabel(f)}</option>
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
                        {failed ? "Retry" : `Download ${fmtSize(entry.size_gb)}`}
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

      <!-- Built-in chat tools -->
      <div class="flex flex-col gap-3 px-4 py-3.5 border-t border-border/40 dark:border-white/[0.04]">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.tools.section")}</span>
          <span class="text-[0.65rem] text-muted-foreground">
            {t("llm.tools.sectionDesc")}
          </span>
        </div>

        <div class="flex flex-col gap-2">
          {#each TOOL_ROWS as tool}
            <div class="flex items-center justify-between gap-4 rounded-xl border
                        {tool.warn && config.tools[tool.key]
                          ? 'border-amber-500/40 bg-amber-50/40 dark:bg-amber-950/15'
                          : 'border-border/60 dark:border-white/[0.06] bg-slate-50/60 dark:bg-[#111118]'}
                        px-3 py-2.5">
              <div class="flex flex-col gap-0.5">
                <div class="flex items-center gap-1.5">
                  <span class="text-[0.74rem] font-semibold text-foreground">{tool.label}</span>
                  {#if tool.warn}
                    <span class="text-[0.5rem] font-semibold rounded-full border px-1.5 py-0
                                 border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400">
                      {t("llm.tools.advanced")}
                    </span>
                  {/if}
                </div>
                <span class="text-[0.62rem] text-muted-foreground leading-relaxed">{tool.desc}</span>
              </div>
              <button role="switch" aria-checked={config.tools[tool.key]} aria-label={tool.label}
                onclick={async () => {
                  config = { ...config, tools: { ...config.tools, [tool.key]: !config.tools[tool.key] } };
                  await saveConfig();
                }}
                class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                       border-transparent transition-colors duration-200
                       {config.tools[tool.key]
                         ? (tool.warn ? 'bg-amber-500' : 'bg-blue-500')
                         : 'bg-muted dark:bg-white/10'}">
                <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                              transform transition-transform duration-200
                              {config.tools[tool.key] ? 'translate-x-4' : 'translate-x-0'}"></span>
              </button>
            </div>
          {/each}
        </div>

        <!-- Execution mode -->
        <div class="flex flex-col gap-1 mt-1">
          <span class="text-[0.65rem] text-muted-foreground">{t("llm.tools.executionMode")}</span>
          <div class="flex rounded-lg overflow-hidden border border-border text-[0.68rem] font-medium">
            {#each [
              { key: "parallel"   as ToolExecutionMode, label: t("llm.tools.parallel") },
              { key: "sequential" as ToolExecutionMode, label: t("llm.tools.sequential") },
            ] as mode}
              <button
                onclick={async () => {
                  config = { ...config, tools: { ...config.tools, execution_mode: mode.key } };
                  await saveConfig();
                }}
                class="flex-1 py-1.5 transition-colors cursor-pointer
                       {config.tools.execution_mode === mode.key
                         ? 'bg-primary text-primary-foreground'
                         : 'bg-background text-muted-foreground hover:bg-muted'}">
                {mode.label}
              </button>
            {/each}
          </div>
        </div>
      </div>

      <!-- Multimodal -->
      {#if catalog.entries.some(e => e.is_mmproj)}
        <!-- Auto-load vision encoder -->
        <div class="flex items-center justify-between gap-4 px-4 py-3.5
                    border-t border-border/40 dark:border-white/[0.04]">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.mmproj.autoload")}</span>
            <span class="text-[0.65rem] text-muted-foreground">{t("llm.mmproj.autoloadDesc")}</span>
          </div>
          <button role="switch" aria-checked={config.autoload_mmproj} aria-label={t("llm.mmproj.autoload")}
            onclick={async () => { config = { ...config, autoload_mmproj: !config.autoload_mmproj }; await saveConfig(); }}
            class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                   border-transparent transition-colors duration-200
                   {config.autoload_mmproj ? 'bg-blue-500' : 'bg-muted dark:bg-white/10'}">
            <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                          transform transition-transform duration-200
                          {config.autoload_mmproj ? 'translate-x-4' : 'translate-x-0'}"></span>
          </button>
        </div>

        <!-- No-GPU for mmproj (only relevant when a projector is downloaded) -->
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
      {/if}

      <!-- Verbose LLM logging -->
      <div class="flex items-center justify-between gap-4 px-4 py-3.5
                  border-t border-border/40 dark:border-white/[0.04]">
        <div class="flex flex-col gap-0.5">
          <span class="text-[0.78rem] font-semibold text-foreground">{t("llm.verbose")}</span>
          <span class="text-[0.65rem] text-muted-foreground">{t("llm.verboseDesc")}</span>
        </div>
        <button role="switch" aria-checked={config.verbose} aria-label={t("llm.verbose")}
          onclick={async () => { config = { ...config, verbose: !config.verbose }; await saveConfig(); }}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                 border-transparent transition-colors duration-200
                 {config.verbose ? 'bg-blue-500' : 'bg-muted dark:bg-white/10'}">
          <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                        transform transition-transform duration-200
                        {config.verbose ? 'translate-x-4' : 'translate-x-0'}"></span>
        </button>
      </div>

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
  <!-- Header: title + filter tabs + search + controls -->
  <div class="flex items-center gap-2 px-0.5 flex-wrap">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      Server log
    </span>
    <span class="flex items-center gap-1 text-[0.52rem] text-muted-foreground/50">
      <span class="w-1 h-1 rounded-full {logs.length > 0 ? 'bg-emerald-500 animate-pulse' : 'bg-slate-400'}"></span>
      {filteredLogs.length}{filteredLogs.length !== logs.length ? `/${logs.length}` : ""}
    </span>

    <!-- Level filter tabs -->
    <div class="flex rounded-md overflow-hidden border border-border/50 text-[0.5rem] font-medium ml-1">
      {#each [
        { key: "all"   as const, label: t("chat.logFilter.all"),   color: "" },
        { key: "info"  as const, label: t("chat.logFilter.info"),  color: "text-emerald-500" },
        { key: "warn"  as const, label: t("chat.logFilter.warn"),  color: "text-amber-500" },
        { key: "error" as const, label: t("chat.logFilter.error"), color: "text-red-500" },
      ] as tab}
        <button onclick={() => logFilter = tab.key}
          class="px-2 py-0.5 transition-colors cursor-pointer
                 {logFilter === tab.key
                   ? `bg-foreground/10 ${tab.color || 'text-foreground'} font-bold`
                   : 'bg-transparent text-muted-foreground/50 hover:text-muted-foreground'}">
          {tab.label}
        </button>
      {/each}
    </div>

    <!-- Search -->
    <input
      type="text"
      bind:value={logSearch}
      placeholder={t("chat.logFilter.search")}
      class="ml-auto h-5 w-28 text-[0.52rem] px-2 rounded-md border border-border/50
             bg-transparent text-muted-foreground placeholder:text-muted-foreground/30
             focus:outline-none focus:border-violet-500/50" />

    <button
      onclick={() => { logAutoScroll = !logAutoScroll; if (logAutoScroll) scrollToBottom(); }}
      class="text-[0.52rem] cursor-pointer select-none transition-colors
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
    {#if filteredLogs.length === 0}
      <div class="flex items-center justify-center h-full text-muted-foreground/30 text-[0.65rem]">
        {logs.length === 0 ? "No log output yet." : "No matching lines."}
      </div>
    {:else}
      <div class="px-3 py-2 flex flex-col gap-0">
        {#each filteredLogs as entry (entry.ts + entry.message)}
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
