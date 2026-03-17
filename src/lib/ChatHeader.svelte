<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Chat top bar — sidebar toggle, tools badge, EEG badge, server controls, settings. -->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { ServerStatus, BandSnapshot } from "$lib/chat-types";

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
  }: Props = $props();
</script>

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

  <!-- Context usage circular indicator -->
  {#if nCtx > 0}
    {@const ctxPct = liveUsedTokens > 0 ? Math.min(Math.round((liveUsedTokens / nCtx) * 100), 100) : 0}
    {@const ctxIsEstimate = realPromptTokens === null && liveUsedTokens > 0}
    {@const ringStroke = ctxPct >= 90 ? 'stroke-red-500' : ctxPct >= 70 ? 'stroke-amber-500' : 'stroke-primary'}
    {@const circumference = 2 * Math.PI * 7}
    {@const dashOffset = circumference - (circumference * ctxPct / 100)}
    <div class="flex items-center gap-1 shrink-0 select-none"
         title="{t('chat.ctxUsage')}: {ctxIsEstimate ? '~' : ''}{liveUsedTokens.toLocaleString()}/{nCtx.toLocaleString()} ({ctxPct}%)">
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
    </div>
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
