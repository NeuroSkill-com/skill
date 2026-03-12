<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "$lib/i18n/index.svelte";
  import LanguagePicker from "./LanguagePicker.svelte";
  import ThemeToggle from "./ThemeToggle.svelte";
  import { hBar, hCbs } from "$lib/history-titlebar.svelte";
  import { helpTitlebarState } from "$lib/help-search-state.svelte";
  import { labelTitlebarState } from "$lib/label-titlebar.svelte";

  let osType: string | null = $state(null);
  let windowLabel = $state("main");
  let windowTitle = $state("");
  let searchMode = $state<"eeg" | "text" | "interactive">("interactive");
  let titleObserver: MutationObserver | null = null;

  const isMainWindow = $derived(windowLabel === "main");
  const isSettingsWindow = $derived(windowLabel === "settings");
  const isSearchWindow = $derived(windowLabel === "search");
  const isApiWindow = $derived(windowLabel === "api");
  const isHelpWindow = $derived(windowLabel === "help");
  const isDownloadsWindow = $derived(windowLabel === "downloads");
  const isHistoryWindow   = $derived(windowLabel === "history");
  const isLabelWindow     = $derived(windowLabel === "label");

  const SEARCH_MODE_EVENT = "skill:search-mode";
  const SEARCH_SET_MODE_EVENT = "skill:search-set-mode";
  const API_REFRESH_EVENT = "skill:api-refresh";

  function emitApiRefresh() {
    window.dispatchEvent(new CustomEvent(API_REFRESH_EVENT));
  }

  function normalizeSearchMode(value: unknown): "eeg" | "text" | "interactive" {
    if (value === "eeg" || value === "text" || value === "interactive") return value;
    return "interactive";
  }

  function emitSearchModeSwitch(mode: "eeg" | "text" | "interactive") {
    window.dispatchEvent(new CustomEvent(SEARCH_SET_MODE_EVENT, { detail: { mode } }));
  }

  $effect(() => {
    const ua = navigator.userAgent;
    if (ua.includes("Mac OS")) {
      osType = "Darwin";
    } else if (ua.includes("Windows")) {
      osType = "Windows";
    } else if (ua.includes("Linux")) {
      osType = "Linux";
    }
  });

  onMount(() => {
    const win = getCurrentWindow();
    windowLabel = win.label;
    windowTitle = document.title || "NeuroSkill™";

    if (win.label === "search") {
      searchMode = normalizeSearchMode(new URLSearchParams(window.location.search).get("mode"));
    }

    const onSearchMode = (event: Event) => {
      const next = normalizeSearchMode((event as CustomEvent<{ mode?: unknown }>).detail?.mode);
      searchMode = next;
    };
    window.addEventListener(SEARCH_MODE_EVENT, onSearchMode as EventListener);

    const titleEl = document.querySelector("title");
    if (titleEl) {
      titleObserver = new MutationObserver(() => {
        windowTitle = document.title || "NeuroSkill™";
      });
      titleObserver.observe(titleEl, {
        childList: true,
        subtree: true,
        characterData: true,
      });
    }

    return () => {
      window.removeEventListener(SEARCH_MODE_EVENT, onSearchMode as EventListener);
    };
  });

  onDestroy(() => {
    titleObserver?.disconnect();
    titleObserver = null;
  });

  async function minimizeWindow() {
    await getCurrentWindow().minimize();
  }

  async function toggleMaximizeWindow() {
    await getCurrentWindow().toggleMaximize();
  }

  async function closeWindow() {
    await getCurrentWindow().close();
  }

  async function openLabel() {
    await invoke("open_label_window");
  }

  async function openHistory() {
    await invoke("open_history_window");
  }

  async function openHelp() {
    await invoke("open_help_window");
  }
</script>

<div class="titlebar">
  {#if osType === "Darwin"}
    <!-- macOS: controls on left, spacer, actions on right -->
    <div class="titlebar-controls">
      <button type="button" title="minimize" aria-label="Minimize" onclick={minimizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M19 13H5v-2h14z" />
        </svg>
      </button>
      <button type="button" title="maximize" aria-label="Maximize" onclick={toggleMaximizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M4 4h16v16H4zm2 4v10h12V8z" />
        </svg>
      </button>
      <button type="button" title="close" aria-label="Close" onclick={closeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path
            fill="currentColor"
            d="M13.46 12L19 17.54V19h-1.46L12 13.46L6.46 19H5v-1.46L10.54 12L5 6.46V5h1.46L12 10.54L17.54 5H19v1.46z"
          />
        </svg>
      </button>
    </div>

    {#if isSearchWindow}
      <div class="search-window-head">
        <div class="search-mode-switch" role="tablist" aria-label={t("search.title")}>
          <button type="button" role="tab" aria-selected={searchMode === "eeg"}
                  class="search-mode-button {searchMode === 'eeg' ? 'active' : ''}"
                  onclick={() => emitSearchModeSwitch("eeg")}>{t("search.modeEeg")}</button>
          <button type="button" role="tab" aria-selected={searchMode === "text"}
                  class="search-mode-button {searchMode === 'text' ? 'active' : ''}"
                  onclick={() => emitSearchModeSwitch("text")}>{t("search.modeText")}</button>
          <button type="button" role="tab" aria-selected={searchMode === "interactive"}
                  class="search-mode-button {searchMode === 'interactive' ? 'active' : ''}"
                  onclick={() => emitSearchModeSwitch("interactive")}>{t("search.modeInteractive")}</button>
        </div>
      </div>
    {:else if isDownloadsWindow}
      <div class="downloads-window-head" data-tauri-drag-region>
        <span class="downloads-window-title">{t("downloads.windowTitle")}</span>
        <span class="downloads-window-sub">{t("downloads.subtitle")}</span>
      </div>
    {:else if isHistoryWindow}
      <div class="history-head">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0 text-muted-foreground pointer-events-none">
          <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
        </svg>
        <span class="history-title-text">{t("history.title")}</span>
        {#if !hBar.daysLoading && hBar.dayCount > 0}
          <div class="history-daynav">
            <button class="history-daynav-btn" disabled={hBar.currentDayIdx === 0} onclick={hCbs.prev} title="Newer day (←)">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5"><polyline points="15 18 9 12 15 6"/></svg>
            </button>
            <span class="history-daynav-label">{hBar.currentDayLabel || "—"}</span>
            <button class="history-daynav-btn" disabled={hBar.currentDayIdx >= hBar.dayCount - 1} onclick={hCbs.next} title="Older day (→)">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5"><polyline points="9 18 15 12 9 6"/></svg>
            </button>
            <span class="history-daynav-pos">{hBar.currentDayIdx + 1}/{hBar.dayCount}</span>
          </div>
        {/if}
      </div>
    {:else if isHelpWindow}
      <div class="help-window-head" data-tauri-drag-region>
        <div class="help-search-wrap">
          <svg class="help-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" aria-hidden="true">
            <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
          </svg>
          <input type="search" class="help-search-input" bind:value={helpTitlebarState.query}
                 placeholder={t("help.searchPlaceholder")} autocomplete="off" spellcheck="false" />
          {#if helpTitlebarState.query}
            <button class="help-search-clear" onclick={() => helpTitlebarState.query = ""}
                    aria-label="Clear search" type="button">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M18 6 6 18M6 6l12 12"/>
              </svg>
            </button>
          {/if}
        </div>
        <span class="help-version-badge">v{helpTitlebarState.version}</span>
        <span class="help-license-badge" title="GNU General Public License v3.0">{t("settings.license")}</span>
      </div>
    {:else if isLabelWindow}
      <div class="titlebar-title" data-tauri-drag-region>
        <span>{windowTitle}</span>
      </div>
    {:else}
      <div class="titlebar-title" data-tauri-drag-region>
        {#if !isMainWindow}
        <span>{windowTitle}</span>
        {/if}
      </div>
    {/if}

    <div class="titlebar-drag-region" data-tauri-drag-region></div>

    {#if isMainWindow}
      <div class="titlebar-actions">
        <!-- Label button -->
        <button type="button" title="Add Label" aria-label="Add Label" onclick={openLabel}
          class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
            <line x1="7" y1="7" x2="7.01" y2="7"/>
          </svg>
        </button>
        <!-- History button -->
        <button type="button" title="History" aria-label="History" onclick={openHistory}
          class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <circle cx="12" cy="12" r="10"/>
            <polyline points="12 6 12 12 16 14"/>
          </svg>
        </button>
        <ThemeToggle />
        <LanguagePicker />
      </div>
    {:else if isHistoryWindow}
      <div class="titlebar-actions">
        <!-- Compare toggle -->
        <button type="button" title={hBar.compareMode ? t("history.exitCompare") : t("history.compare")} onclick={hCbs.toggleCompare}
          class="flex items-center justify-center w-6 h-6 rounded-md transition-colors {hBar.compareMode ? 'text-blue-500 bg-blue-500/10' : 'text-muted-foreground hover:text-foreground hover:bg-accent'}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <polyline points="22 7 13 7 13 2 22 7"/><polyline points="2 17 11 17 11 22 2 17"/>
            <line x1="22" y1="17" x2="11" y2="17"/><line x1="13" y1="7" x2="2" y2="7"/>
            <line x1="2" y1="12" x2="22" y2="12"/>
          </svg>
        </button>
        {#if hBar.compareMode && hBar.compareCount >= 2}
          <button type="button" title="Open comparison" onclick={hCbs.openCompare}
            class="flex items-center justify-center h-6 px-1.5 rounded-md text-[0.58rem] font-semibold bg-blue-600 text-white hover:bg-blue-700 transition-colors">
            {hBar.compareCount}/2
          </button>
        {/if}
        <!-- Labels toggle -->
        <button type="button" title={t("history.labels")} onclick={hCbs.toggleLabels}
          class="flex items-center justify-center w-6 h-6 rounded-md transition-colors {hBar.showLabels ? 'text-amber-500 bg-amber-500/10' : 'text-muted-foreground hover:text-foreground hover:bg-accent'}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
            <line x1="7" y1="7" x2="7.01" y2="7"/>
          </svg>
        </button>
        <!-- Reload -->
        <button type="button" title="Reload" onclick={hCbs.reload}
          class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <polyline points="23 4 23 10 17 10"/>
            <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
          </svg>
        </button>
        <ThemeToggle />
        <LanguagePicker />
      </div>
    {:else}
      <div class="titlebar-actions">
        {#if isSettingsWindow}
          <button type="button" title="Help" aria-label="Help" onclick={openHelp}
            class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
              <circle cx="12" cy="12" r="10"/>
              <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/>
              <line x1="12" y1="17" x2="12.01" y2="17"/>
            </svg>
          </button>
        {:else if isApiWindow}
          <button type="button" title={t("apiStatus.refresh")} aria-label={t("apiStatus.refresh")} onclick={emitApiRefresh}
            class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
              <path d="M23 4v6h-6"/><path d="M1 20v-6h6"/>
              <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
            </svg>
          </button>
        {/if}
        <ThemeToggle />
        <LanguagePicker />
      </div>
    {/if}
  {:else}
    <!-- Windows/Linux: actions on left, spacer, controls on right -->
    {#if isMainWindow}
      <div class="titlebar-actions">
        <!-- Label button -->
        <button type="button" title="Add Label" aria-label="Add Label" onclick={openLabel}
          class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
            <line x1="7" y1="7" x2="7.01" y2="7"/>
          </svg>
        </button>
        <!-- History button -->
        <button type="button" title="History" aria-label="History" onclick={openHistory}
          class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <circle cx="12" cy="12" r="10"/>
            <polyline points="12 6 12 12 16 14"/>
          </svg>
        </button>
        <ThemeToggle />
        <LanguagePicker />
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
    {:else if isSearchWindow}
      <div class="search-window-head">
        <div class="search-mode-switch" role="tablist" aria-label={t("search.title")}>
          <button type="button" role="tab" aria-selected={searchMode === "eeg"}
                  class="search-mode-button {searchMode === 'eeg' ? 'active' : ''}"
                  onclick={() => emitSearchModeSwitch("eeg")}>{t("search.modeEeg")}</button>
          <button type="button" role="tab" aria-selected={searchMode === "text"}
                  class="search-mode-button {searchMode === 'text' ? 'active' : ''}"
                  onclick={() => emitSearchModeSwitch("text")}>{t("search.modeText")}</button>
          <button type="button" role="tab" aria-selected={searchMode === "interactive"}
                  class="search-mode-button {searchMode === 'interactive' ? 'active' : ''}"
                  onclick={() => emitSearchModeSwitch("interactive")}>{t("search.modeInteractive")}</button>
        </div>
      </div>
      <div class="titlebar-actions">
        <ThemeToggle />
        <LanguagePicker />
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
    {:else if isDownloadsWindow}
      <div class="downloads-window-head" data-tauri-drag-region>
        <span class="downloads-window-title">{t("downloads.windowTitle")}</span>
        <span class="downloads-window-sub">{t("downloads.subtitle")}</span>
      </div>
      <div class="titlebar-actions">
        <ThemeToggle />
        <LanguagePicker />
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
    {:else if isHistoryWindow}
      <div class="history-head">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0 text-muted-foreground pointer-events-none">
          <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
        </svg>
        <span class="history-title-text">{t("history.title")}</span>
        {#if !hBar.daysLoading && hBar.dayCount > 0}
          <div class="history-daynav">
            <button class="history-daynav-btn" disabled={hBar.currentDayIdx === 0} onclick={hCbs.prev} title="Newer day (←)">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5"><polyline points="15 18 9 12 15 6"/></svg>
            </button>
            <span class="history-daynav-label">{hBar.currentDayLabel || "—"}</span>
            <button class="history-daynav-btn" disabled={hBar.currentDayIdx >= hBar.dayCount - 1} onclick={hCbs.next} title="Older day (→)">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5"><polyline points="9 18 15 12 9 6"/></svg>
            </button>
            <span class="history-daynav-pos">{hBar.currentDayIdx + 1}/{hBar.dayCount}</span>
          </div>
        {/if}
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
      <div class="titlebar-actions">
        <!-- Compare toggle -->
        <button type="button" title={hBar.compareMode ? t("history.exitCompare") : t("history.compare")} onclick={hCbs.toggleCompare}
          class="flex items-center justify-center w-6 h-6 rounded-md transition-colors {hBar.compareMode ? 'text-blue-500 bg-blue-500/10' : 'text-muted-foreground hover:text-foreground hover:bg-accent'}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <polyline points="22 7 13 7 13 2 22 7"/><polyline points="2 17 11 17 11 22 2 17"/>
            <line x1="22" y1="17" x2="11" y2="17"/><line x1="13" y1="7" x2="2" y2="7"/>
            <line x1="2" y1="12" x2="22" y2="12"/>
          </svg>
        </button>
        {#if hBar.compareMode && hBar.compareCount >= 2}
          <button type="button" title="Open comparison" onclick={hCbs.openCompare}
            class="flex items-center justify-center h-6 px-1.5 rounded-md text-[0.58rem] font-semibold bg-blue-600 text-white hover:bg-blue-700 transition-colors">
            {hBar.compareCount}/2
          </button>
        {/if}
        <!-- Labels toggle -->
        <button type="button" title={t("history.labels")} onclick={hCbs.toggleLabels}
          class="flex items-center justify-center w-6 h-6 rounded-md transition-colors {hBar.showLabels ? 'text-amber-500 bg-amber-500/10' : 'text-muted-foreground hover:text-foreground hover:bg-accent'}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
            <line x1="7" y1="7" x2="7.01" y2="7"/>
          </svg>
        </button>
        <!-- Reload -->
        <button type="button" title="Reload" onclick={hCbs.reload}
          class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
            <polyline points="23 4 23 10 17 10"/>
            <path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>
          </svg>
        </button>
        <ThemeToggle />
        <LanguagePicker />
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
    {:else if isHelpWindow}
      <div class="help-window-head" data-tauri-drag-region>
        <div class="help-search-wrap">
          <svg class="help-search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" aria-hidden="true">
            <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
          </svg>
          <input type="search" class="help-search-input" bind:value={helpTitlebarState.query}
                 placeholder={t("help.searchPlaceholder")} autocomplete="off" spellcheck="false" />
          {#if helpTitlebarState.query}
            <button class="help-search-clear" onclick={() => helpTitlebarState.query = ""}
                    aria-label="Clear search" type="button">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M18 6 6 18M6 6l12 12"/>
              </svg>
            </button>
          {/if}
        </div>
        <span class="help-version-badge">v{helpTitlebarState.version}</span>
        <span class="help-license-badge" title="GNU General Public License v3.0">{t("settings.license")}</span>
      </div>
      <div class="titlebar-actions">
        <ThemeToggle />
        <LanguagePicker />
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
    {:else if isLabelWindow}
      <div class="titlebar-title" data-tauri-drag-region>
        <span>{windowTitle}</span>
      </div>
      <div class="titlebar-actions">
        <ThemeToggle />
        <LanguagePicker />
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
    {:else}
      <div class="titlebar-title" data-tauri-drag-region>
        <span>{windowTitle}</span>
      </div>
      <div class="titlebar-actions">
        {#if isSettingsWindow}
          <button type="button" title="Help" aria-label="Help" onclick={openHelp}
            class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
              <circle cx="12" cy="12" r="10"/>
              <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/>
              <line x1="12" y1="17" x2="12.01" y2="17"/>
            </svg>
          </button>
        {:else if isApiWindow}
          <button type="button" title={t("apiStatus.refresh")} aria-label={t("apiStatus.refresh")} onclick={emitApiRefresh}
            class="flex items-center justify-center w-6 h-6 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
              <path d="M23 4v6h-6"/><path d="M1 20v-6h6"/>
              <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
            </svg>
          </button>
        {/if}
        <ThemeToggle />
        <LanguagePicker />
      </div>
      <div class="titlebar-drag-region" data-tauri-drag-region></div>
    {/if}

    <div class="titlebar-controls">
      <button type="button" title="minimize" aria-label="Minimize" onclick={minimizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M19 13H5v-2h14z" />
        </svg>
      </button>
      <button type="button" title="maximize" aria-label="Maximize" onclick={toggleMaximizeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path fill="currentColor" d="M4 4h16v16H4zm2 4v10h12V8z" />
        </svg>
      </button>
      <button type="button" title="close" aria-label="Close" onclick={closeWindow}>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
          <path
            fill="currentColor"
            d="M13.46 12L19 17.54V19h-1.46L12 13.46L6.46 19H5v-1.46L10.54 12L5 6.46V5h1.46L12 10.54L17.54 5H19v1.46z"
          />
        </svg>
      </button>
    </div>
  {/if}
  {#if isLabelWindow && labelTitlebarState.active}
    <div class="label-window-countdown-center" data-tauri-drag-region>
      <span class="label-window-countdown-pill">{t("label.eegWindow", { elapsed: labelTitlebarState.elapsed })}</span>
    </div>
  {/if}
</div>

<style>
  .titlebar {
    height: 30px;
    background: var(--color-surface);
    user-select: none;
    display: flex;
    align-items: center;
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 1000;
    border-bottom: 1px solid var(--color-border);
    gap: 0;
  }

  .titlebar-drag-region {
    flex: 1;
    cursor: grab;
    pointer-events: auto;
    height: 100%;
  }

  .titlebar-drag-region:active {
    cursor: grabbing;
  }

  .titlebar-title {
    display: flex;
    align-items: center;
    min-width: 0;
    max-width: 46vw;
    padding: 0 10px;
    height: 100%;
    color: var(--color-text);
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: auto;
    user-select: none;
  }

  .titlebar-actions {
    display: flex;
    gap: 0;
    align-items: center;
    pointer-events: auto;
  }

  .titlebar-controls {
    display: flex;
    gap: 0;
    pointer-events: auto;
  }

  .titlebar button {
    appearance: none;
    padding: 0;
    margin: 0;
    border: none;
    display: inline-flex;
    justify-content: center;
    align-items: center;
    width: 30px;
    height: 30px;
    background-color: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition: background-color 0.2s;
    pointer-events: auto;
  }

  .titlebar-actions button {
    width: 30px;
    height: 30px;
  }

  .titlebar-controls button {
    width: 46px;
    height: 30px;
  }

  .titlebar button:hover {
    background-color: var(--color-hover);
  }

  .titlebar button:active {
    background-color: var(--color-active);
  }

  .titlebar button svg {
    width: 18px;
    height: 18px;
  }

  .search-window-head {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    justify-content: center;
    width: min(920px, calc(100vw - 200px));
    min-width: 0;
    padding: 0 10px;
    height: 100%;
    overflow: hidden;
    pointer-events: auto;
    z-index: 1;
  }

  .search-mode-switch {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    overflow: hidden;
    margin-left: 0;
    height: 22px;
    width: min(760px, calc(100vw - 240px));
    min-width: 320px;
    flex: 0 1 auto;
  }

  .search-mode-button {
    width: auto;
    min-width: 0;
    flex: 1 1 0;
    height: 22px;
    padding: 0 10px;
    border: 0;
    background: transparent;
    color: color-mix(in oklab, var(--color-text) 72%, transparent);
    font-size: 0.62rem;
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
  }

  .titlebar-actions,
  .titlebar-controls {
    position: relative;
    z-index: 2;
  }

  .search-mode-button + .search-mode-button {
    border-left: 1px solid var(--color-border);
  }

  .search-mode-button.active {
    background: color-mix(in oklab, var(--color-text) 15%, transparent);
    color: var(--color-text);
  }

  .downloads-window-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    height: 100%;
    min-width: 0;
    pointer-events: auto;
  }

  .downloads-window-title {
    color: var(--color-text);
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .downloads-window-sub {
    color: color-mix(in oklab, var(--color-text) 48%, transparent);
    font-size: 0.58rem;
    font-weight: 400;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .label-window-countdown-center {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    max-width: min(18rem, calc(100vw - 320px));
    pointer-events: none;
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .label-window-countdown-pill {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    max-width: 100%;
    padding: 4px 11px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--color-surface) 84%, var(--color-border));
    border: 1px solid color-mix(in oklab, var(--color-border) 82%, transparent);
    font-size: 0.58rem;
    color: color-mix(in oklab, var(--color-text) 52%, transparent);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── History window titlebar ─────────────────────────────────────────── */
  .history-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px;
    height: 100%;
    min-width: 0;
    flex-shrink: 0;
    pointer-events: auto;
  }

  .history-title-text {
    color: color-mix(in oklab, var(--color-text) 70%, transparent);
    font-size: 0.62rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    white-space: nowrap;
    user-select: none;
    flex-shrink: 0;
  }

  .history-daynav {
    display: flex;
    align-items: center;
    gap: 1px;
    height: 100%;
    margin-left: 4px;
    pointer-events: auto;
  }

  .history-daynav button {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .history-daynav button:disabled {
    opacity: 0.3;
    pointer-events: none;
  }

  .history-daynav-label {
    font-size: 0.62rem;
    font-weight: 500;
    color: color-mix(in oklab, var(--color-text) 80%, transparent);
    min-width: 5.8rem;
    text-align: center;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
    user-select: none;
    padding: 0 2px;
  }

  .history-daynav-pos {
    font-size: 0.5rem;
    color: color-mix(in oklab, var(--color-text) 38%, transparent);
    font-variant-numeric: tabular-nums;
    user-select: none;
    padding-left: 2px;
    flex-shrink: 0;
  }

  /* ── History window titlebar ─────────────────────────────────────────── */
  .history-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px;
    height: 100%;
    min-width: 0;
    flex-shrink: 0;
    pointer-events: auto;
  }

  .history-title-text {
    color: color-mix(in oklab, var(--color-text) 70%, transparent);
    font-size: 0.62rem;
    font-weight: 600;
    letter-spacing: 0.01em;
    white-space: nowrap;
    user-select: none;
    flex-shrink: 0;
  }

  .history-daynav {
    display: flex;
    align-items: center;
    gap: 1px;
    height: 100%;
    margin-left: 4px;
    pointer-events: auto;
  }

  .history-daynav button {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .history-daynav button:disabled {
    opacity: 0.3;
    pointer-events: none;
  }

  .history-daynav-label {
    font-size: 0.62rem;
    font-weight: 500;
    color: color-mix(in oklab, var(--color-text) 80%, transparent);
    min-width: 5.8rem;
    text-align: center;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
    user-select: none;
    padding: 0 2px;
  }

  .history-daynav-pos {
    font-size: 0.5rem;
    color: color-mix(in oklab, var(--color-text) 38%, transparent);
    font-variant-numeric: tabular-nums;
    user-select: none;
    padding-left: 2px;
    flex-shrink: 0;
  }

  /* ── Help window titlebar ─────────────────────────────────────────────── */
  .help-window-head {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    min-width: 0;
    padding: 0 6px;
    height: 100%;
    cursor: grab;
    pointer-events: auto;
  }

  .help-window-head:active {
    cursor: grabbing;
  }

  .help-search-wrap {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 0;
  }

  .help-search-icon {
    position: absolute;
    left: 6px;
    width: 10px;
    height: 10px;
    color: color-mix(in oklab, var(--color-text) 50%, transparent);
    pointer-events: none;
    flex-shrink: 0;
  }

  .help-search-input {
    width: 100%;
    height: 22px;
    border-radius: 4px;
    border: 1px solid var(--color-border);
    background: transparent;
    color: var(--color-text);
    font-size: 0.7rem;
    padding: 0 20px 0 22px;
    outline: none;
    pointer-events: auto;
    caret-color: var(--color-text);
  }

  .help-search-input:focus {
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--color-text) 20%, transparent);
  }

  .help-search-input::placeholder {
    color: color-mix(in oklab, var(--color-text) 35%, transparent);
  }

  /* Override .titlebar button sizing for the inline clear button */
  .help-search-clear {
    position: absolute;
    right: 4px;
    width: 14px !important;
    height: 14px !important;
    display: flex;
    align-items: center;
    justify-content: center;
    color: color-mix(in oklab, var(--color-text) 50%, transparent);
    pointer-events: auto;
    background: transparent !important;
    border: none;
    cursor: pointer;
    padding: 0;
  }

  .help-search-clear:hover {
    color: var(--color-text);
    background-color: transparent !important;
  }

  .help-search-clear:active {
    background-color: transparent !important;
  }

  .help-search-clear svg {
    width: 9px !important;
    height: 9px !important;
  }

  .help-version-badge {
    font-size: 0.5rem;
    color: color-mix(in oklab, var(--color-text) 40%, transparent);
    white-space: nowrap;
    flex-shrink: 0;
    pointer-events: none;
    user-select: none;
    padding-left: 2px;
  }

  .help-license-badge {
    font-size: 0.45rem;
    color: color-mix(in oklab, var(--color-text) 30%, transparent);
    white-space: nowrap;
    flex-shrink: 0;
    pointer-events: none;
    user-select: none;
  }
</style>
