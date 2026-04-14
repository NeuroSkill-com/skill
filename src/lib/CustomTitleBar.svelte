<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onDestroy, onMount } from "svelte";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { t } from "$lib/i18n/index.svelte";
import type { LlmCatalog, LlmModelEntry } from "$lib/llm-helpers";
import { openHelp, openHistory, openLabel } from "$lib/navigation";
import { isBtOff } from "$lib/stores/bt-status.svelte";
import { chatTitlebarState, hBar, hCbs, helpTitlebarState, labelTitlebarState } from "$lib/stores/titlebar.svelte";
import LanguagePicker from "./LanguagePicker.svelte";
import ThemeToggle from "./ThemeToggle.svelte";

// ── State ───────────────────────────────────────────────────────────────
let osType: string | null = $state(null);
let windowLabel = $state("main");
let windowTitle = $state("");
let searchMode = $state<"eeg" | "text" | "interactive" | "images">("interactive");
let titleObserver: MutationObserver | null = null;

// ── Derived ─────────────────────────────────────────────────────────────
const isMac = $derived(osType === "Darwin");
const isMainWindow = $derived(windowLabel === "main");
const isSettingsWindow = $derived(windowLabel === "settings");
const isSearchWindow = $derived(windowLabel === "search");
const isApiWindow = $derived(windowLabel === "api");
const isHelpWindow = $derived(windowLabel === "help");
const isDownloadsWindow = $derived(windowLabel === "downloads");
const isHistoryWindow = $derived(windowLabel === "history");
const isLabelWindow = $derived(windowLabel === "label");
const isChatWindow = $derived(windowLabel === "chat");
const btUnavailable = $derived(isMainWindow && isBtOff());

const SEARCH_MODE_EVENT = "skill:search-mode";
const SEARCH_SET_MODE_EVENT = "skill:search-set-mode";
const API_REFRESH_EVENT = "skill:api-refresh";
const HISTORY_VIEW_MODES = ["year", "month", "week", "day"] as const;

// ── Helpers ─────────────────────────────────────────────────────────────
function emitApiRefresh() {
  window.dispatchEvent(new CustomEvent(API_REFRESH_EVENT));
}
function emitSearchModeSwitch(mode: "eeg" | "text" | "interactive" | "images") {
  window.dispatchEvent(new CustomEvent(SEARCH_SET_MODE_EVENT, { detail: { mode } }));
}
function normalizeSearchMode(v: unknown): "eeg" | "text" | "interactive" | "images" {
  return v === "eeg" || v === "text" || v === "interactive" || v === "images" ? v : "interactive";
}
async function minimizeWindow() {
  await getCurrentWindow().minimize();
}

// Manual maximize/restore: toggleMaximize() on macOS with decorations:false
// doesn't reliably restore the previous window geometry (especially width).
// We track the pre-maximise bounds ourselves.
let savedBounds: { x: number; y: number; width: number; height: number } | null = null;
let isManuallyMaximized = false;

async function toggleMaximizeWindow() {
  const win = getCurrentWindow();
  if (isManuallyMaximized && savedBounds) {
    await win.unmaximize();
    await win.setSize(new LogicalSize(savedBounds.width, savedBounds.height));
    await win.setPosition(new LogicalPosition(savedBounds.x, savedBounds.y));
    isManuallyMaximized = false;
    savedBounds = null;
    // Resume auto-fit after geometry is applied
    if (isMainWindow) {
      setTimeout(() => window.dispatchEvent(new CustomEvent("skill:autofit-resume")), 500);
    }
  } else {
    // Pause auto-fit for the entire time the window is maximized
    if (isMainWindow) window.dispatchEvent(new CustomEvent("skill:autofit-pause"));
    const pos = await win.outerPosition();
    const size = await win.outerSize();
    const factor = await win.scaleFactor();
    savedBounds = {
      x: pos.x / factor,
      y: pos.y / factor,
      width: size.width / factor,
      height: size.height / factor,
    };
    await win.maximize();
    isManuallyMaximized = true;
  }
}

async function closeWindow() {
  await getCurrentWindow().close();
}

// ── Model picker (chat window) ──────────────────────────────────────────
let modelPickerOpen = $state(false);
let downloadedModels = $state<LlmModelEntry[]>([]);
let activeFilename = $state("");
let modelSwitching = $state(false);
let pickerBtnEl = $state<HTMLButtonElement | null>(null);
let dropdownX = $state(0);
let dropdownY = $state(0);
/** Set of repos that have at least one downloaded mmproj. */
let visionRepos = $state(new Set<string>());

function prettyModelName(filename: string): string {
  return filename.replace(/\.gguf$/i, "").replace(/-(\d{5})-of-\d{5}$/, "");
}
function modelDisplayLabel(entry: LlmModelEntry): string {
  if (entry.family_name) return `${entry.family_name} (${entry.quant})`;
  return prettyModelName(entry.filename);
}

async function openModelPicker() {
  if (modelSwitching) return;
  try {
    const catalog = await daemonInvoke<LlmCatalog>("get_llm_catalog");
    downloadedModels = catalog.entries.filter((e) => e.state === "downloaded" && !e.is_mmproj);
    activeFilename = catalog.active_model;
    // Collect repos that have a downloaded vision projector
    visionRepos = new Set(catalog.entries.filter((e) => e.is_mmproj && e.state === "downloaded").map((e) => e.repo));
  } catch (e) {
    downloadedModels = [];
    visionRepos = new Set();
  }
  if (downloadedModels.length === 0) return;
  // Position dropdown below the button
  if (pickerBtnEl) {
    const rect = pickerBtnEl.getBoundingClientRect();
    dropdownX = rect.left + rect.width / 2;
    dropdownY = rect.bottom + 4;
  }
  modelPickerOpen = true;
}

function closeModelPicker() {
  modelPickerOpen = false;
}

async function switchToModel(filename: string) {
  if (filename === activeFilename || modelSwitching) return;
  modelSwitching = true;
  modelPickerOpen = false;
  try {
    await daemonInvoke("switch_llm_model", { filename });
  } catch (e) {
  } finally {
    modelSwitching = false;
  }
}

let dropdownEl = $state<HTMLDivElement | null>(null);

function onPickerOutsideClick(e: MouseEvent) {
  if (!modelPickerOpen) return;
  const target = e.target as Node;
  if (pickerBtnEl?.contains(target)) return;
  if (dropdownEl?.contains(target)) return;
  closeModelPicker();
}

const pickerGroups = $derived.by(() => {
  const map = new Map<string, LlmModelEntry[]>();
  for (const e of downloadedModels) {
    const key = e.family_name || e.family_id || "Other";
    if (!map.has(key)) map.set(key, []);
    map.get(key)?.push(e);
  }
  return [...map.entries()].map(([family, entries]) => ({ family, entries }));
});

// ── Lifecycle ───────────────────────────────────────────────────────────
$effect(() => {
  const ua = navigator.userAgent;
  if (ua.includes("Mac OS")) osType = "Darwin";
  else if (ua.includes("Windows")) osType = "Windows";
  else if (ua.includes("Linux")) osType = "Linux";
});

onMount(() => {
  const win = getCurrentWindow();
  windowLabel = win.label;
  windowTitle = document.title || "NeuroSkill™";
  if (win.label === "search") searchMode = normalizeSearchMode(new URLSearchParams(window.location.search).get("mode"));

  const onSearchMode = (e: Event) => {
    searchMode = normalizeSearchMode((e as CustomEvent<{ mode?: unknown }>).detail?.mode);
  };
  window.addEventListener(SEARCH_MODE_EVENT, onSearchMode as EventListener);

  const titleEl = document.querySelector("title");
  if (titleEl) {
    titleObserver = new MutationObserver(() => {
      const next = document.title || "NeuroSkill™";
      if (next !== windowTitle) windowTitle = next;
    });
    titleObserver.observe(titleEl, { childList: true, subtree: true, characterData: true });
  }
  return () => window.removeEventListener(SEARCH_MODE_EVENT, onSearchMode as EventListener);
});

onDestroy(() => {
  titleObserver?.disconnect();
  titleObserver = null;
});
</script>

<svelte:window onclick={onPickerOutsideClick} />

<!-- ─── Reusable SVG icons ─────────────────────────────────────────────── -->
{#snippet iconChevronLeft()}
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
       stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5">
    <polyline points="15 18 9 12 15 6"/>
  </svg>
{/snippet}
{#snippet iconChevronRight()}
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
       stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5">
    <polyline points="9 18 15 12 9 6"/>
  </svg>
{/snippet}
{#snippet iconClose()}
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
    <path fill="currentColor" d="M13.46 12L19 17.54V19h-1.46L12 13.46L6.46 19H5v-1.46L10.54 12L5 6.46V5h1.46L12 10.54L17.54 5H19v1.46z"/>
  </svg>
{/snippet}
{#snippet iconMaximize()}
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
    <path fill="currentColor" d="M4 4h16v16H4zm2 4v10h12V8z"/>
  </svg>
{/snippet}
{#snippet iconMinimize()}
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
    <path fill="currentColor" d="M19 13H5v-2h14z"/>
  </svg>
{/snippet}

<!-- ─── Reusable titlebar action buttons ───────────────────────────────── -->
{#snippet tbBtn(title: string, label: string, onclick: () => void, iconPath: string, activeClass?: string)}
  <button type="button" {title} aria-label={label} {onclick}
    class="flex items-center justify-center w-6 h-6 rounded-md transition-colors
           {activeClass || 'text-muted-foreground hover:text-foreground hover:bg-accent'}">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
         stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
      {@html iconPath}
    </svg>
  </button>
{/snippet}

<!-- ─── Window controls (close / maximize / minimize) ──────────────────── -->
{#snippet windowControls(order: "mac" | "win")}
  <div class="titlebar-controls">
    {#if order === "mac"}
      <button type="button" title="close" aria-label="Close" onclick={closeWindow}>{@render iconClose()}</button>
      <button type="button" title="maximize" aria-label="Maximize" onclick={toggleMaximizeWindow}>{@render iconMaximize()}</button>
      <button type="button" title="minimize" aria-label="Minimize" onclick={minimizeWindow}>{@render iconMinimize()}</button>
    {:else}
      <button type="button" title="minimize" aria-label="Minimize" onclick={minimizeWindow}>{@render iconMinimize()}</button>
      <button type="button" title="maximize" aria-label="Maximize" onclick={toggleMaximizeWindow}>{@render iconMaximize()}</button>
      <button type="button" title="close" aria-label="Close" onclick={closeWindow}>{@render iconClose()}</button>
    {/if}
  </div>
{/snippet}

<!-- ─── Per-window center content ──────────────────────────────────────── -->
{#snippet centerContent()}
  {#if isSearchWindow}
    <div class="search-window-head">
      <div class="search-mode-switch" role="tablist" aria-label={t("search.title")}>
        {#each (["eeg","text","images","interactive"] as const) as mode}
          <button type="button" role="tab" aria-selected={searchMode === mode}
                  class="search-mode-button {searchMode === mode ? 'active' : ''}"
                  onclick={() => emitSearchModeSwitch(mode)}>
            {t(`search.mode${mode[0].toUpperCase()}${mode.slice(1)}`)}
          </button>
        {/each}
      </div>
    </div>
  {:else if isDownloadsWindow}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="downloads-window-head" data-tauri-drag-region ondblclick={toggleMaximizeWindow}>
      <span class="downloads-window-title">{t("downloads.windowTitle")}</span>
      <span class="downloads-window-sub">{t("downloads.subtitle")}</span>
    </div>
  {:else if isHistoryWindow}
    <div class="history-head">
      {@render historyHead()}
    </div>
  {:else if isHelpWindow}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="help-window-head" data-tauri-drag-region ondblclick={toggleMaximizeWindow}>
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
  {:else if isChatWindow}
    <div class="chat-window-head">
      <span class="chat-status-dot
        {chatTitlebarState.status === 'running'  ? 'chat-status-running'
        : chatTitlebarState.status === 'loading' ? 'chat-status-loading'
        :                                          'chat-status-stopped'}"></span>
      {#if chatTitlebarState.status === 'running' && chatTitlebarState.modelName}
        <button class="chat-model-picker-btn" bind:this={pickerBtnEl}
                onclick={(e) => { e.stopPropagation(); modelPickerOpen ? closeModelPicker() : openModelPicker(); }}
                disabled={modelSwitching}>
          <span class="chat-model-picker-text">
            {modelSwitching ? t("chat.status.loading") : prettyModelName(chatTitlebarState.modelName)}
          </span>
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" stroke-linejoin="round"
               class="chat-model-chevron {modelPickerOpen ? 'chat-model-chevron-open' : ''}">
            <path d="M4 6l4 4 4-4"/>
          </svg>
        </button>
      {:else}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="chat-model-label" data-tauri-drag-region ondblclick={toggleMaximizeWindow}>
          {#if chatTitlebarState.status === 'loading'}
            {t("chat.status.loading")}
          {:else if chatTitlebarState.status === 'running'}
            {t("chat.status.running")}
          {:else}
            {t("chat.status.stopped")}
          {/if}
        </span>
      {/if}
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="titlebar-title" data-tauri-drag-region ondblclick={toggleMaximizeWindow}>
      {#if !isMainWindow}<span>{windowTitle}</span>{/if}
    </div>
  {/if}
{/snippet}

<!-- ─── Per-window right-side action buttons ───────────────────────────── -->
{#snippet actionButtons()}
  <div class="titlebar-actions">
    {#if isMainWindow}
      {@render tbBtn("Add Label", "Add Label", openLabel,
        '<path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/>')}
      {@render tbBtn("History", "History", openHistory,
        '<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>')}
    {:else if isHistoryWindow}
      {@render tbBtn(
        hBar.compareMode ? t("history.exitCompare") : t("history.compare"),
        "Compare", hCbs.toggleCompare,
        '<polyline points="22 7 13 7 13 2 22 7"/><polyline points="2 17 11 17 11 22 2 17"/><line x1="22" y1="17" x2="11" y2="17"/><line x1="13" y1="7" x2="2" y2="7"/><line x1="2" y1="12" x2="22" y2="12"/>',
        hBar.compareMode ? 'text-blue-500 bg-blue-500/10' : undefined
      )}
      {#if hBar.compareMode && hBar.compareCount >= 2}
        <button type="button" title={t("common.openComparison")} onclick={hCbs.openCompare}
          class="flex items-center justify-center h-6 px-1.5 rounded-md text-[0.58rem] font-semibold bg-blue-600 text-white hover:bg-blue-700 transition-colors">
          {hBar.compareCount}/2
        </button>
      {/if}
      {@render tbBtn(t("history.labels"), "Labels", hCbs.toggleLabels,
        '<path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/>',
        hBar.showLabels ? 'text-amber-500 bg-amber-500/10' : undefined
      )}
      {@render tbBtn("Reload", "Reload", hCbs.reload,
        '<polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/>')}
    {:else if isSettingsWindow}
      {@render tbBtn("Help", "Help", openHelp,
        '<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>')}
    {:else if isApiWindow}
      {@render tbBtn(t("apiStatus.refresh"), t("apiStatus.refresh"), emitApiRefresh,
        '<path d="M23 4v6h-6"/><path d="M1 20v-6h6"/><path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>')}
    {/if}
    <ThemeToggle />
    <LanguagePicker />
  </div>
{/snippet}

<!-- ─── History center content ─────────────────────────────────────────── -->
{#snippet historyHead()}
  <div class="history-viewmode-seg">
    {#each HISTORY_VIEW_MODES as mode}
      <button class="history-viewmode-btn {hBar.viewMode === mode ? 'active' : ''}"
              onclick={() => hCbs.setViewMode(mode)}>
        {t(`history.view.${mode}`)}
      </button>
    {/each}
  </div>

  {#if hBar.viewMode === "day"}
    <div class="history-daynav">
      <button class="history-daynav-btn" disabled={hBar.daysLoading || hBar.currentDayIdx === 0}
              onclick={hCbs.prev} title={t("common.newer") + " (←)"}>{@render iconChevronLeft()}</button>
      <span class="history-daynav-label">
        {#if hBar.daysLoading}<span class="history-daynav-skeleton"></span>{:else}{hBar.currentDayLabel || "—"}{/if}
      </span>
      <button class="history-daynav-btn" disabled={hBar.daysLoading || hBar.currentDayIdx >= hBar.dayCount - 1}
              onclick={hCbs.next} title={t("common.older") + " (→)"}>{@render iconChevronRight()}</button>
      {#if !hBar.daysLoading && hBar.dayCount > 0}
        <span class="history-daynav-pos">{hBar.currentDayIdx + 1}/{hBar.dayCount}</span>
      {/if}
    </div>
  {:else}
    <div class="history-daynav">
      <button class="history-daynav-btn" onclick={hCbs.calendarPrev} title={t("common.prev")}>{@render iconChevronLeft()}</button>
      <span class="history-daynav-label">{hBar.calendarLabel || "—"}</span>
      <button class="history-daynav-btn" onclick={hCbs.calendarNext} title={t("common.next")}>{@render iconChevronRight()}</button>
    </div>
  {/if}
{/snippet}

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<!-- TITLEBAR LAYOUT — single template, platform-aware ordering            -->
<!-- ═══════════════════════════════════════════════════════════════════════ -->
<div class="titlebar {btUnavailable ? 'titlebar--bt-off' : ''}">
  {#if isMac}
    {@render windowControls("mac")}
    {@render centerContent()}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="titlebar-drag-region" data-tauri-drag-region ondblclick={toggleMaximizeWindow}></div>
    {@render actionButtons()}
  {:else}
    {#if isMainWindow}
      {@render actionButtons()}
    {/if}
    {@render centerContent()}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="titlebar-drag-region" data-tauri-drag-region ondblclick={toggleMaximizeWindow}></div>
    {#if !isMainWindow}
      {@render actionButtons()}
    {/if}
    {@render windowControls("win")}
  {/if}

  {#if isLabelWindow && labelTitlebarState.active}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="label-window-countdown-center" data-tauri-drag-region ondblclick={toggleMaximizeWindow}>
      <span class="label-window-countdown-pill">{t("label.eegWindow", { elapsed: labelTitlebarState.elapsed })}</span>
    </div>
  {/if}
</div>

<!-- Model picker dropdown — rendered outside titlebar to avoid overflow clipping -->
{#if modelPickerOpen && downloadedModels.length > 0}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="chat-model-dropdown" bind:this={dropdownEl}
       style="left: {dropdownX}px; top: {dropdownY}px;"
       onclick={(e) => e.stopPropagation()}>
    {#each pickerGroups as group}
      {#if pickerGroups.length > 1}
        <div class="chat-model-group-label">{group.family}</div>
      {/if}
      {#each group.entries as entry}
        {@const isActive = entry.filename === activeFilename}
        {@const hasVision = visionRepos.has(entry.repo)}
        <button class="chat-model-item {isActive ? 'chat-model-item-active' : ''}"
                onclick={() => switchToModel(entry.filename)}>
          <span class="chat-model-dot {isActive ? 'chat-model-dot-active' : ''}"></span>
          <span class="chat-model-item-name">{modelDisplayLabel(entry)}</span>
          {#if hasVision}
            <svg class="chat-model-vision-icon" viewBox="0 0 20 20" fill="none"
                 stroke="currentColor" stroke-width="1.6" stroke-linecap="round"
                 stroke-linejoin="round">
              <path d="M1 10s3.5-6 9-6 9 6 9 6-3.5 6-9 6-9-6-9-6z"/>
              <circle cx="10" cy="10" r="3"/>
            </svg>
          {/if}
          <span class="chat-model-item-size">{entry.size_gb.toFixed(1)} GB</span>
        </button>
      {/each}
    {/each}
  </div>
{/if}

<style>
  /* ── Titlebar shell ──────────────────────────────────────────────────── */
  .titlebar {
    height: 30px;
    background: var(--color-surface);
    user-select: none;
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    position: fixed;
    top: 0; left: 0; right: 0;
    z-index: 1000;
    border-bottom: 1px solid var(--color-border);
    gap: 0;
    overflow: hidden;
  }
  .titlebar--bt-off {
    background: color-mix(in oklab, var(--color-error, #ef4444) 18%, var(--color-surface));
    border-bottom-color: color-mix(in oklab, var(--color-error, #ef4444) 30%, var(--color-border));
  }

  /* ── Drag region ─────────────────────────────────────────────────────── */
  .titlebar-drag-region {
    flex: 1;
    cursor: grab;
    pointer-events: auto;
    height: 100%;
  }
  .titlebar-drag-region:active { cursor: grabbing; }

  /* ── Generic title ───────────────────────────────────────────────────── */
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

  /* ── Controls & actions ──────────────────────────────────────────────── */
  .titlebar-actions,
  .titlebar-controls {
    display: flex;
    gap: 0;
    align-items: center;
    pointer-events: auto;
    position: relative;
    z-index: 2;
  }
  .titlebar button {
    appearance: none;
    padding: 0; margin: 0; border: none;
    display: inline-flex;
    justify-content: center;
    align-items: center;
    width: 30px; height: 30px;
    background-color: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition: background-color 0.2s;
    pointer-events: auto;
  }
  .titlebar-actions button { width: 30px; height: 30px; }
  .titlebar-controls button { width: 30px; height: 30px; }
  .titlebar button:hover { background-color: var(--color-hover); }
  .titlebar button:active { background-color: var(--color-active); }
  .titlebar button svg { width: 18px; height: 18px; }

  /* ── Chat window ─────────────────────────────────────────────────────── */
  .chat-window-head {
    position: absolute;
    left: 50%; transform: translateX(-50%);
    display: flex; align-items: center; justify-content: center; gap: 5px;
    max-width: min(400px, calc(100vw - 200px));
    min-width: 0; padding: 0 10px; height: 100%;
    overflow: hidden; pointer-events: auto; z-index: 1;
  }
  .chat-status-dot {
    width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0;
  }
  .chat-status-running  { background: #22c55e; }
  .chat-status-loading  { background: #f59e0b; animation: pulse 1.5s ease-in-out infinite; }
  .chat-status-stopped  { background: color-mix(in oklab, var(--color-text) 25%, transparent); }
  .chat-model-label {
    font-size: 0.66rem; font-weight: 600; letter-spacing: 0.01em;
    color: var(--color-text);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    user-select: none;
  }

  /* ── Model picker button ─────────────────────────────────────────────── */
  .chat-model-picker-btn {
    display: flex; align-items: center; gap: 3px;
    padding: 0 6px; height: 20px; border-radius: 4px;
    background: transparent; border: none;
    color: var(--color-text); cursor: pointer;
    transition: background-color 0.15s;
    pointer-events: auto; min-width: 0; max-width: 100%;
    width: auto !important;
  }
  .chat-model-picker-btn:hover { background: color-mix(in oklab, var(--color-text) 10%, transparent); }
  .chat-model-picker-btn:disabled { opacity: 0.5; cursor: default; }
  .chat-model-picker-text {
    font-size: 0.66rem; font-weight: 600; letter-spacing: 0.01em;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    user-select: none;
  }
  .chat-model-chevron {
    width: 8px; height: 8px; flex-shrink: 0;
    opacity: 0.45; transition: transform 0.15s ease;
  }
  .chat-model-chevron-open { transform: rotate(180deg); }

  /* ── Model picker dropdown ───────────────────────────────────────────── */
  .chat-model-dropdown {
    position: fixed;
    transform: translateX(-50%);
    min-width: 240px; max-width: 360px; max-height: 400px;
    overflow-y: auto; overscroll-behavior: contain;
    border-radius: 8px;
    border: 1px solid var(--color-border);
    background: var(--popover);
    color: var(--popover-foreground);
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.22), 0 2px 8px rgba(0, 0, 0, 0.10);
    padding: 4px 0;
    z-index: 10000;
    pointer-events: auto;
  }
  .chat-model-group-label {
    padding: 6px 10px 2px;
    font-size: 0.5rem; font-weight: 700;
    text-transform: uppercase; letter-spacing: 0.06em;
    color: var(--muted-foreground);
    user-select: none;
  }
  .chat-model-item {
    display: flex; align-items: center; gap: 6px;
    width: 100%; padding: 5px 10px;
    border: none; background: transparent;
    color: var(--popover-foreground);
    font-size: 0.64rem; text-align: left;
    cursor: pointer; transition: background 0.12s;
    height: auto !important;
  }
  .chat-model-item:hover { background: var(--accent); }
  .chat-model-item-active {
    color: var(--primary);
    font-weight: 600;
    background: oklch(from var(--primary) l c h / 10%);
  }
  .chat-model-dot {
    width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0;
    background: transparent;
  }
  .chat-model-dot-active { background: var(--primary); }
  .chat-model-item-name {
    flex: 1; min-width: 0;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .chat-model-vision-icon {
    width: 12px; height: 12px; flex-shrink: 0;
    color: var(--muted-foreground);
    opacity: 0.7;
  }
  .chat-model-item-size {
    font-size: 0.52rem; flex-shrink: 0;
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
  }

  /* ── Search window ───────────────────────────────────────────────────── */
  .search-window-head {
    position: absolute;
    left: 50%; transform: translateX(-50%);
    display: flex; align-items: center; justify-content: center;
    width: min(920px, calc(100vw - 200px));
    min-width: 0; padding: 0 10px; height: 100%;
    overflow: hidden; pointer-events: auto; z-index: 1;
  }
  .search-mode-switch {
    display: inline-flex; align-items: center;
    border: 1px solid var(--color-border); border-radius: 6px;
    overflow: hidden; height: 22px;
    width: min(760px, calc(100vw - 240px));
    min-width: 320px; flex: 0 1 auto;
  }
  .search-mode-button {
    width: auto; min-width: 0; flex: 1 1 0;
    height: 22px; padding: 0 10px; border: 0;
    background: transparent;
    color: color-mix(in oklab, var(--color-text) 72%, transparent);
    font-size: 0.62rem; font-weight: 600; line-height: 1; white-space: nowrap;
  }
  .search-mode-button + .search-mode-button { border-left: 1px solid var(--color-border); }
  .search-mode-button.active {
    background: color-mix(in oklab, var(--color-text) 15%, transparent);
    color: var(--color-text);
  }

  /* ── Downloads window ────────────────────────────────────────────────── */
  .downloads-window-head {
    display: flex; align-items: center; gap: 8px;
    padding: 0 10px; height: 100%; min-width: 0; pointer-events: auto;
  }
  .downloads-window-title {
    color: var(--color-text); font-size: 0.72rem; font-weight: 600;
    letter-spacing: 0.01em; white-space: nowrap; flex-shrink: 0;
  }
  .downloads-window-sub {
    color: color-mix(in oklab, var(--color-text) 48%, transparent);
    font-size: 0.58rem; white-space: nowrap;
    overflow: hidden; text-overflow: ellipsis;
  }

  /* ── Label countdown overlay ─────────────────────────────────────────── */
  .label-window-countdown-center {
    position: absolute;
    left: 50%; top: 50%; transform: translate(-50%, -50%);
    max-width: min(18rem, calc(100vw - 320px));
    pointer-events: none;
    display: flex; justify-content: center; align-items: center;
  }
  .label-window-countdown-pill {
    display: flex; align-items: center; justify-content: center;
    padding: 4px 11px; border-radius: 999px;
    background: color-mix(in oklab, var(--color-surface) 84%, var(--color-border));
    border: 1px solid color-mix(in oklab, var(--color-border) 82%, transparent);
    font-size: 0.58rem;
    color: color-mix(in oklab, var(--color-text) 52%, transparent);
    font-variant-numeric: tabular-nums;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  /* ── History window ──────────────────────────────────────────────────── */
  .history-head {
    display: flex; align-items: center; gap: 6px;
    padding: 0 8px; height: 100%;
    min-width: 0; flex-shrink: 0; pointer-events: auto;
  }
  .history-viewmode-seg {
    display: flex; align-items: center;
    border-radius: 6px;
    border: 1px solid color-mix(in oklab, var(--color-border) 60%, transparent);
    overflow: hidden; flex-shrink: 0;
  }
  .history-viewmode-btn {
    padding: 2px 7px; font-size: 0.5rem; font-weight: 600;
    line-height: 1; white-space: nowrap;
    color: color-mix(in oklab, var(--color-text) 45%, transparent);
    transition: background 0.15s, color 0.15s;
    cursor: pointer; user-select: none;
  }
  .history-viewmode-btn:hover {
    color: var(--color-text);
    background: color-mix(in oklab, var(--color-text) 6%, transparent);
  }
  .history-viewmode-btn.active {
    color: var(--color-primary);
    background: color-mix(in oklab, var(--color-primary) 14%, transparent);
  }
  .history-daynav {
    display: flex; align-items: center; gap: 1px;
    height: 100%; margin-left: 2px; pointer-events: auto;
  }
  .history-daynav button { width: 20px; height: 20px; border-radius: 4px; flex-shrink: 0; }
  .history-daynav button:disabled { opacity: 0.3; pointer-events: none; }
  .history-daynav-label {
    font-size: 0.6rem; font-weight: 500;
    color: color-mix(in oklab, var(--color-text) 80%, transparent);
    min-width: 5.8rem; text-align: center; white-space: nowrap;
    font-variant-numeric: tabular-nums; user-select: none; padding: 0 2px;
  }
  .history-daynav-skeleton {
    display: inline-block; width: 4.5rem; height: 0.6rem;
    border-radius: 3px;
    background: color-mix(in oklab, var(--color-text) 8%, transparent);
    animation: pulse 1.5s ease-in-out infinite;
  }
  .history-daynav-pos {
    font-size: 0.48rem;
    color: color-mix(in oklab, var(--color-text) 35%, transparent);
    font-variant-numeric: tabular-nums; user-select: none;
    padding-left: 2px; flex-shrink: 0;
  }
  @keyframes pulse { 0%, 100% { opacity: 0.4; } 50% { opacity: 0.8; } }

  /* ── Help window ─────────────────────────────────────────────────────── */
  .help-window-head {
    display: flex; align-items: center; gap: 4px;
    flex: 1; min-width: 0; padding: 0 6px; height: 100%;
    cursor: grab; pointer-events: auto;
  }
  .help-window-head:active { cursor: grabbing; }
  .help-search-wrap {
    position: relative; display: flex; align-items: center;
    flex: 1; min-width: 0;
  }
  .help-search-icon {
    position: absolute; left: 6px; width: 10px; height: 10px;
    color: color-mix(in oklab, var(--color-text) 50%, transparent);
    pointer-events: none; flex-shrink: 0;
  }
  .help-search-input {
    width: 100%; height: 22px; border-radius: 4px;
    border: 1px solid var(--color-border);
    background: transparent; color: var(--color-text);
    font-size: 0.7rem; padding: 0 20px 0 22px;
    outline: none; pointer-events: auto; caret-color: var(--color-text);
  }
  .help-search-input:focus { box-shadow: 0 0 0 1px color-mix(in oklab, var(--color-text) 20%, transparent); }
  .help-search-input::placeholder { color: color-mix(in oklab, var(--color-text) 35%, transparent); }
  .help-search-clear {
    position: absolute; right: 4px;
    width: 14px !important; height: 14px !important;
    display: flex; align-items: center; justify-content: center;
    color: color-mix(in oklab, var(--color-text) 50%, transparent);
    pointer-events: auto; background: transparent !important;
    border: none; cursor: pointer; padding: 0;
  }
  .help-search-clear:hover { color: var(--color-text); background-color: transparent !important; }
  .help-search-clear:active { background-color: transparent !important; }
  .help-search-clear svg { width: 9px !important; height: 9px !important; }
  .help-version-badge {
    font-size: 0.5rem; color: color-mix(in oklab, var(--color-text) 40%, transparent);
    white-space: nowrap; flex-shrink: 0; pointer-events: none; user-select: none; padding-left: 2px;
  }
  .help-license-badge {
    font-size: 0.45rem; color: color-mix(in oklab, var(--color-text) 30%, transparent);
    white-space: nowrap; flex-shrink: 0; pointer-events: none; user-select: none;
  }
</style>
