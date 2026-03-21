<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Chat top bar — sidebar toggle, model picker, tools badge, EEG badge, server controls, settings. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n/index.svelte";
  import type { ServerStatus, BandSnapshot } from "$lib/chat-types";
  import type { LlmCatalog, LlmModelEntry } from "$lib/llm-helpers";

  interface Props {
    sidebarOpen: boolean;
    showSettings: boolean;
    showTools: boolean;
    status: ServerStatus;
    modelName: string;
    supportsTools: boolean;
    enabledToolCount: number;
    nCtx: number;
    liveUsedTokens: number;
    realPromptTokens: number | null;
    eegContext: boolean;
    latestBands: BandSnapshot | null;
    canStart: boolean;
    canStop: boolean;
    onToggleSidebar: () => void;
    onToggleSettings: () => void;
    onToggleTools: () => void;
    onStartServer: () => void;
    onStopServer: () => void;
    onNewChat: () => void;
    onToggleEeg: () => void;
    onToggleContextBreakdown: () => void;
  }

  let {
    sidebarOpen,
    showSettings,
    showTools,
    status,
    modelName,
    supportsTools,
    enabledToolCount,
    nCtx,
    liveUsedTokens,
    realPromptTokens,
    eegContext,
    latestBands,
    canStart,
    canStop,
    onToggleSidebar,
    onToggleSettings,
    onToggleTools,
    onStartServer,
    onStopServer,
    onNewChat,
    onToggleEeg,
    onToggleContextBreakdown,
  }: Props = $props();

  // ── Model picker state ─────────────────────────────────────────────────────
  let pickerOpen = $state(false);
  let downloadedModels = $state<LlmModelEntry[]>([]);
  let activeFilename = $state("");
  let switching = $state(false);
  let pickerEl = $state<HTMLDivElement | null>(null);

  /** Pretty-print a model filename for display. */
  function prettyName(filename: string): string {
    return filename
      .replace(/\.gguf$/i, "")
      .replace(/-(\d{5})-of-\d{5}$/, "");  // strip shard suffix
  }

  /** Short display label: family name + quant if available, else prettified filename. */
  function displayLabel(entry: LlmModelEntry): string {
    if (entry.family_name) return `${entry.family_name} (${entry.quant})`;
    return prettyName(entry.filename);
  }

  async function openPicker() {
    if (switching) return;
    try {
      const catalog = await invoke<LlmCatalog>("get_llm_catalog");
      downloadedModels = catalog.entries.filter(
        (e) => e.state === "downloaded" && !e.is_mmproj,
      );
      activeFilename = catalog.active_model;
    } catch (e) {
      console.warn("[chat] get_llm_catalog failed:", e);
      downloadedModels = [];
    }
    if (downloadedModels.length === 0) return;
    pickerOpen = true;
  }

  function closePicker() {
    pickerOpen = false;
  }

  async function selectModel(filename: string) {
    if (filename === activeFilename || switching) return;
    switching = true;
    pickerOpen = false;
    try {
      await invoke("switch_llm_model", { filename });
    } catch (e) {
      console.warn("[chat] switch_llm_model failed:", e);
    } finally {
      switching = false;
    }
  }

  /** Close picker on outside click. */
  function onWindowClick(e: MouseEvent) {
    if (pickerOpen && pickerEl && !pickerEl.contains(e.target as Node)) {
      closePicker();
    }
  }

  /** Derive the display name for the current model. */
  const currentDisplayName = $derived.by(() => {
    if (!modelName) return "";
    return prettyName(modelName);
  });

  /** Group downloaded models by family for the picker dropdown. */
  const groupedModels = $derived.by(() => {
    const groups: { family: string; entries: LlmModelEntry[] }[] = [];
    const map = new Map<string, LlmModelEntry[]>();
    for (const e of downloadedModels) {
      const key = e.family_name || e.family_id || "Other";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(e);
    }
    for (const [family, entries] of map) {
      groups.push({ family, entries });
    }
    return groups;
  });
</script>

<svelte:window onclick={onWindowClick} />

<header class="relative flex flex-nowrap items-center gap-2 px-3 py-2 border-b border-border dark:border-white/[0.06]
                bg-white dark:bg-[#0f0f18] shrink-0 overflow-hidden min-h-0"
        data-tauri-drag-region>

  <!-- Sidebar toggle -->
  <button
    onclick={onToggleSidebar}
    title={sidebarOpen ? "Hide conversations" : "Show conversations"}
    class="p-1.5 rounded-lg transition-colors cursor-pointer shrink-0
           {sidebarOpen
             ? 'text-violet-600 dark:text-violet-400 bg-violet-500/10'
             : 'text-muted-foreground/60 hover:text-foreground hover:bg-muted'}">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
         stroke-linecap="round" class="w-3.5 h-3.5">
      <line x1="3" y1="6"  x2="21" y2="6"/>
      <line x1="3" y1="12" x2="21" y2="12"/>
      <line x1="3" y1="18" x2="21" y2="18"/>
    </svg>
  </button>

  <!-- Model name / picker -->
  <div class="relative min-w-0 shrink" bind:this={pickerEl}>
    {#if modelName || switching}
      <button
        onclick={(e) => { e.stopPropagation(); pickerOpen ? closePicker() : openPicker(); }}
        disabled={switching}
        class="flex items-center gap-1 min-w-0 px-1.5 py-0.5 rounded-md transition-colors cursor-pointer
               text-[0.65rem] font-medium truncate
               {switching
                 ? 'text-muted-foreground/50'
                 : pickerOpen
                   ? 'bg-primary/10 text-primary'
                   : 'text-muted-foreground/70 hover:text-foreground hover:bg-muted/60'}">
        <span class="truncate">
          {#if switching}
            {t("chat.status.loading")}
          {:else}
            {currentDisplayName}
          {/if}
        </span>
        <!-- Chevron -->
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round"
             class="w-2.5 h-2.5 shrink-0 opacity-50 transition-transform
                    {pickerOpen ? 'rotate-180' : ''}">
          <path d="M4 6l4 4 4-4"/>
        </svg>
      </button>

      <!-- Dropdown -->
      {#if pickerOpen && downloadedModels.length > 0}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="absolute left-0 top-full mt-1 z-50 min-w-[220px] max-w-[340px] max-h-[360px]
                 overflow-y-auto overscroll-contain rounded-lg border border-border
                 dark:border-white/[0.08] bg-white dark:bg-[#161622] shadow-xl
                 py-1 text-[0.7rem]"
          onclick={(e) => e.stopPropagation()}>
          {#each groupedModels as group}
            {#if groupedModels.length > 1}
              <div class="px-2.5 pt-1.5 pb-0.5 text-[0.55rem] font-semibold uppercase tracking-wider
                          text-muted-foreground/50 select-none">
                {group.family}
              </div>
            {/if}
            {#each group.entries as entry}
              {@const isActive = entry.filename === activeFilename}
              <button
                onclick={() => selectModel(entry.filename)}
                class="w-full flex items-center gap-2 px-2.5 py-1.5 text-left transition-colors cursor-pointer
                       {isActive
                         ? 'bg-primary/10 text-primary font-semibold'
                         : 'text-foreground/80 hover:bg-muted/70'}">
                <!-- Active indicator dot -->
                <span class="w-1.5 h-1.5 rounded-full shrink-0
                             {isActive ? 'bg-primary' : 'bg-transparent'}"></span>
                <span class="flex-1 min-w-0 truncate">{displayLabel(entry)}</span>
                <span class="text-[0.55rem] text-muted-foreground/50 tabular-nums shrink-0">
                  {entry.size_gb.toFixed(1)} GB
                </span>
              </button>
            {/each}
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  <div class="flex-1 min-w-0" data-tauri-drag-region></div>

  <!-- Tools badge -->
  {#if supportsTools}
    <button
      onclick={onToggleTools}
      title="{enabledToolCount} tool{enabledToolCount !== 1 ? 's' : ''} enabled"
      class="flex items-center gap-1 px-1.5 py-0.5 rounded-md transition-colors cursor-pointer
             shrink-0 text-[0.6rem] font-semibold
             {showTools
               ? 'bg-primary/20 text-primary ring-1 ring-primary/30'
               : enabledToolCount > 0
                 ? 'bg-primary/10 text-primary hover:bg-primary/20'
                 : 'bg-muted text-muted-foreground/50 hover:bg-muted/80'}">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"
           stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
        <path d="M14.7 6.3a1 1 0 0 0 0-1.4l-.6-.6a1 1 0 0 0-1.4 0L6.3 10.7a1 1 0 0 0 0 1.4l.6.6a1 1 0 0 0 1.4 0z"/>
        <path d="M16 2l2 2-1.5 1.5L14.5 3.5z"/>
        <path d="M2 18l4-1 9.3-9.3-3-3L3 14z"/>
      </svg>
      <span>{t("chat.tools.badge")}</span>
      {#if enabledToolCount > 0}
        <span class="tabular-nums opacity-70">{enabledToolCount}</span>
      {/if}
    </button>
  {/if}

  <!-- Context usage circular indicator (clickable for breakdown) -->
  {#if nCtx > 0}
    {@const ctxPct = liveUsedTokens > 0 ? Math.min(Math.round((liveUsedTokens / nCtx) * 100), 100) : 0}
    {@const ctxIsEstimate = realPromptTokens === null && liveUsedTokens > 0}
    {@const ringStroke = ctxPct >= 90 ? 'stroke-red-500' : ctxPct >= 70 ? 'stroke-amber-500' : 'stroke-primary'}
    {@const circumference = 2 * Math.PI * 7}
    {@const dashOffset = circumference - (circumference * ctxPct / 100)}
    <button
      onclick={onToggleContextBreakdown}
      class="flex items-center gap-1 shrink-0 select-none rounded-md px-1 py-0.5
             hover:bg-muted/60 transition-colors cursor-pointer"
      title="{t('chat.ctxUsage')}: {ctxIsEstimate ? '~' : ''}{liveUsedTokens.toLocaleString()}/{nCtx.toLocaleString()} ({ctxPct}%) — {t('chat.ctx.clickToInspect')}">
      <svg viewBox="0 0 18 18" class="w-4 h-4 -rotate-90">
        <circle cx="9" cy="9" r="7" fill="none" stroke-width="2.2"
                class="stroke-muted-foreground/15" />
        <circle cx="9" cy="9" r="7" fill="none" stroke-width="2.2"
                class="{ringStroke} transition-all duration-150"
                stroke-linecap="round"
                stroke-dasharray="{circumference}"
                stroke-dashoffset="{dashOffset}" />
      </svg>
      <span class="text-[0.5rem] tabular-nums font-semibold
                    {ctxPct >= 90 ? 'text-red-500' : ctxPct >= 70 ? 'text-amber-500' : 'text-muted-foreground/60'}">
        {ctxPct}%
      </span>
    </button>
  {/if}

  <!-- EEG context badge -->
  {#if latestBands}
    <button
      onclick={onToggleEeg}
      title={eegContext ? t("chat.eeg.on") : t("chat.eeg.off")}
      class="flex items-center gap-1 px-1.5 py-0.5 rounded-md transition-colors cursor-pointer
             shrink-0 text-[0.6rem] font-semibold
             {eegContext
               ? 'bg-cyan-500/15 text-cyan-600 dark:text-cyan-400 hover:bg-cyan-500/25'
               : 'bg-muted text-muted-foreground/40 hover:bg-muted/80'}">
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6"
           stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
        <path d="M2 10 Q4 6 6 10 Q8 14 10 10 Q12 6 14 10 Q16 14 18 10"/>
      </svg>
      <span>{t("chat.eeg.label")}</span>
      {#if eegContext && latestBands}
        <span class="tabular-nums opacity-70">{(latestBands.snr ?? 0).toFixed(1)}dB</span>
      {/if}
    </button>
  {/if}

  <!-- Control buttons -->
  {#if canStart}
    <button
      onclick={onStartServer}
      class="flex items-center gap-1 text-[0.65rem] font-semibold px-2.5 py-1
             rounded-lg bg-violet-600 hover:bg-violet-700 text-white transition-colors cursor-pointer">
      <svg viewBox="0 0 24 24" fill="currentColor" class="w-3 h-3">
        <polygon points="5,3 19,12 5,21"/>
      </svg>
      {t("chat.btn.start")}
    </button>
  {:else if canStop}
    <button
      onclick={onStopServer}
      class="flex items-center gap-1 text-[0.65rem] font-semibold px-2.5 py-1
             rounded-lg border border-red-500/40 text-red-500 hover:bg-red-500/10
             transition-colors cursor-pointer">
      <svg viewBox="0 0 24 24" fill="currentColor" class="w-3 h-3">
        <rect x="4" y="4" width="16" height="16" rx="2"/>
      </svg>
      {status === "loading" ? t("chat.btn.cancel") : t("chat.btn.stop")}
    </button>
  {/if}

  <!-- New chat -->
  <button
    onclick={onNewChat}
    title={t("chat.btn.newChat")}
    class="p-1.5 rounded-lg text-muted-foreground/60 hover:text-foreground hover:bg-muted
           disabled:opacity-30 disabled:cursor-not-allowed transition-colors cursor-pointer">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
         stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
      <path d="M12 5v14M5 12h14"/>
    </svg>
  </button>

  <!-- Settings toggle -->
  <button
    onclick={onToggleSettings}
    title={t("chat.btn.params")}
    class="p-1.5 rounded-lg transition-colors cursor-pointer
           {showSettings
             ? 'text-violet-600 bg-violet-500/10'
             : 'text-muted-foreground/60 hover:text-foreground hover:bg-muted'}">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
         stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
      <circle cx="12" cy="12" r="3"/>
      <path d="M19.07 4.93a10 10 0 0 1 0 14.14"/>
      <path d="M4.93 4.93a10 10 0 0 0 0 14.14"/>
    </svg>
  </button>
</header>
