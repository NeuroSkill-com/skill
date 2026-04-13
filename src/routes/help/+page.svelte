<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help window — sidebar navigation + search. -->

<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";
import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
import HelpApi from "$lib/help/HelpApi.svelte";
import HelpDashboard from "$lib/help/HelpDashboard.svelte";
import HelpElectrodes from "$lib/help/HelpElectrodes.svelte";
import HelpFaqTab from "$lib/help/HelpFaqTab.svelte";
import HelpHooks from "$lib/help/HelpHooks.svelte";
import HelpLlm from "$lib/help/HelpLlm.svelte";
import HelpPrivacy from "$lib/help/HelpPrivacy.svelte";
import HelpReferences from "$lib/help/HelpReferences.svelte";
import HelpSettings from "$lib/help/HelpSettings.svelte";
import HelpTts from "$lib/help/HelpTts.svelte";
import HelpWindows from "$lib/help/HelpWindows.svelte";
import { getFaqContent, getHelpContent } from "$lib/help/help-loader";
import { getLocale, t } from "$lib/i18n/index.svelte";
import { helpTitlebarState } from "$lib/stores/titlebar.svelte";
import { useWindowTitle } from "$lib/stores/window-title.svelte";

const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
const modKey = isMac ? "⌘" : "Ctrl+";

type Tab =
  | "dashboard"
  | "electrodes"
  | "settings"
  | "windows"
  | "api"
  | "tts"
  | "llm"
  | "hooks"
  | "privacy"
  | "references"
  | "faq";
let tab = $state<Tab>("dashboard");
let searchQuery = $derived(helpTitlebarState.query);

const TAB_IDS: Tab[] = [
  "dashboard",
  "electrodes",
  "settings",
  "windows",
  "api",
  "tts",
  "llm",
  "hooks",
  "privacy",
  "references",
  "faq",
];
const helpTabLabel = (id: Tab) => t(`helpTabs.${id}`);

// ── Icons per tab ─────────────────────────────────────────────────────────
const TAB_ICONS: Record<Tab, string> = {
  dashboard: `<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>`,
  electrodes: `<circle cx="12" cy="8" r="4"/><path d="M12 12v4M8 16h8M6 20h12"/>`,
  settings: `<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>`,
  windows: `<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/>`,
  api: `<path d="M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3"/><path d="m9 9 6 6M15 9l-6 6"/>`,
  tts: `<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8"/>`,
  llm: `<path d="M12 2a4 4 0 0 0-4 4v2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8a2 2 0 0 0-2-2h-2V6a4 4 0 0 0-4-4z"/><circle cx="9" cy="14" r="1"/><circle cx="15" cy="14" r="1"/>`,
  hooks: `<path d="M10 2v4M14 2v4"/><path d="M6 6h12a2 2 0 0 1 2 2v2a6 6 0 0 1-6 6h0a6 6 0 0 1-6-6V8a2 2 0 0 1 2-2z"/><path d="M12 16v4M8 22h8"/>`,
  privacy: `<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>`,
  references: `<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>`,
  faq: `<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>`,
};

// ── Searchable help item registry (built dynamically from markdown) ──────
type SearchEntry = { tab: Tab; id: string; title: string; body: string };

const SEARCH_TABS: Tab[] = ["dashboard", "settings", "windows", "api", "privacy", "hooks", "llm", "tts"];

const searchIndex = $derived.by(() => {
  const loc = getLocale();
  const entries: SearchEntry[] = [];
  for (const tab of SEARCH_TABS) {
    for (const section of getHelpContent(tab, loc)) {
      for (const item of section.items) {
        entries.push({ tab, id: item.id, title: item.title, body: item.body });
      }
    }
  }
  for (const entry of getFaqContent(loc)) {
    entries.push({ tab: "faq", id: entry.id, title: entry.question, body: entry.answer });
  }
  return entries;
});

// ── Derived search results (reactive to locale changes) ─────────────────
const searchResults = $derived.by(() => {
  const q = searchQuery.trim().toLowerCase();
  if (!q) return [] as SearchEntry[];
  return searchIndex.filter((item) => item.title.toLowerCase().includes(q) || item.body.toLowerCase().includes(q));
});

function goToTab(id: Tab) {
  tab = id;
  helpTitlebarState.query = "";
}

// ── Search-result navigation ──────────────────────────────────────────────
let pendingScrollKey = "";

function goToItem(targetTab: Tab, id: string) {
  pendingScrollKey = id;
  tab = targetTab;
  helpTitlebarState.query = "";
}

$effect(() => {
  void tab;
  void helpTitlebarState.query;
  const key = pendingScrollKey;
  if (!key) return;
  pendingScrollKey = "";
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      const el = document.getElementById(key);
      if (!el) return;
      if (el instanceof HTMLDetailsElement) {
        el.open = true;
        requestAnimationFrame(() => {
          el.scrollIntoView({ behavior: "smooth", block: "start" });
          el.classList.add("help-highlight");
          setTimeout(() => el.classList.remove("help-highlight"), 1600);
        });
      } else {
        el.scrollIntoView({ behavior: "smooth", block: "center" });
        el.classList.add("help-highlight");
        setTimeout(() => el.classList.remove("help-highlight"), 1600);
      }
    });
  });
});

/* ── Keyboard shortcuts for tabs ──────────────────────────────────────── */
// ⌘1–⌘9 → tabs 1–9, ⌘0 → tab 10
// ⌃⌘1–⌃⌘9 → tabs 11–19 (Ctrl+Cmd on Mac, Ctrl+Alt on Windows/Linux)

function digitForTab(i: number): string | null {
  if (i < 9) return String(i + 1);
  if (i === 9) return "0";
  if (i >= 10 && i < 19) return String(i - 9);
  return null;
}

function modifierForTab(i: number): string {
  if (i < 10) return modKey;
  return isMac ? "⌃⌘" : "Ctrl+Alt+";
}

function onKeydown(e: KeyboardEvent) {
  const digit = e.key >= "0" && e.key <= "9" ? parseInt(e.key, 10) : -1;
  if (digit < 0) return;
  if (!(e.metaKey || e.ctrlKey)) return;

  const isExtended = isMac ? e.ctrlKey && e.metaKey : e.ctrlKey && e.altKey;

  if (isExtended) {
    if (digit >= 1 && digit <= 9) {
      const idx = 10 + digit - 1;
      if (idx < TAB_IDS.length) {
        e.preventDefault();
        tab = TAB_IDS[idx];
      }
    }
  } else {
    if (digit >= 1 && digit <= 9) {
      const idx = digit - 1;
      if (idx < TAB_IDS.length) {
        e.preventDefault();
        tab = TAB_IDS[idx];
      }
    } else if (digit === 0 && TAB_IDS.length >= 10) {
      e.preventDefault();
      tab = TAB_IDS[9];
    }
  }
}

/* ── Resizable sidebar ────────────────────────────────────────────────── */
let splitRoot: HTMLDivElement | null = null;
let navEl: HTMLElement | null = null;
let navWidth = $state(176);
let resizingNav = false;

const NAV_WIDTH_MIN = 140;
const NAV_WIDTH_MAX = 480;
const NAV_WIDTH_KEY = "help.nav.width";

function clampNavWidth(px: number): number {
  return Math.max(NAV_WIDTH_MIN, Math.min(NAV_WIDTH_MAX, Math.round(px)));
}

function persistNavWidth(px: number): void {
  try {
    localStorage.setItem(NAV_WIDTH_KEY, String(px));
  } catch (e) {}
}

function ensureNavFitsContent(): void {
  if (!navEl) return;
  const prev = navEl.style.width;
  navEl.style.width = "max-content";
  const natural = navEl.scrollWidth;
  navEl.style.width = prev;
  const needed = clampNavWidth(natural);
  if (navWidth < needed) {
    navWidth = needed;
    persistNavWidth(navWidth);
  }
}

function setNavWidthFromPointer(clientX: number): void {
  if (!splitRoot) return;
  const rect = splitRoot.getBoundingClientRect();
  const next = clampNavWidth(clientX - rect.left);
  navWidth = next;
}

function onResizeMove(e: MouseEvent): void {
  if (!resizingNav) return;
  e.preventDefault();
  setNavWidthFromPointer(e.clientX);
}

function stopResize(): void {
  if (!resizingNav) return;
  resizingNav = false;
  persistNavWidth(navWidth);
  if (typeof document !== "undefined") {
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  }
  window.removeEventListener("mousemove", onResizeMove);
  window.removeEventListener("mouseup", stopResize);
}

function startResize(e: MouseEvent): void {
  e.preventDefault();
  resizingNav = true;
  if (typeof document !== "undefined") {
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }
  window.addEventListener("mousemove", onResizeMove);
  window.addEventListener("mouseup", stopResize);
}

let fontObserver: MutationObserver | null = null;

onMount(async () => {
  helpTitlebarState.version = await invoke<string>("get_app_version");
  window.addEventListener("keydown", onKeydown);

  // Restore persisted nav width
  try {
    const stored = Number(localStorage.getItem(NAV_WIDTH_KEY) ?? "");
    if (!Number.isNaN(stored) && stored > 0) navWidth = clampNavWidth(stored);
  } catch (e) {}

  // Ensure sidebar fits content at current font size
  ensureNavFitsContent();

  // Re-check when the root font-size changes
  fontObserver = new MutationObserver(() => {
    requestAnimationFrame(() => ensureNavFitsContent());
  });
  fontObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["style"] });
});
onDestroy(() => {
  helpTitlebarState.query = "";
  if (typeof window !== "undefined") window.removeEventListener("keydown", onKeydown);
  stopResize();
  fontObserver?.disconnect();
});

useWindowTitle("window.title.help");
</script>

<main class="h-full min-h-0 flex flex-col overflow-hidden">

  <!-- ── Body: sidebar + content ──────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 flex overflow-hidden" bind:this={splitRoot}>

    <!-- Sidebar nav (always visible, dims when searching) -->
    <nav bind:this={navEl} style={`width:${navWidth}px;min-width:max-content`}
         class="shrink-0 border-r border-border dark:border-white/[0.07]
                overflow-y-auto py-2 flex flex-col gap-0.5
                bg-muted/20 dark:bg-white/[0.015]
                transition-opacity {searchQuery ? 'opacity-40' : 'opacity-100'}"
         aria-label="Help sections">
      {#each TAB_IDS as id, i}
        {@const active = tab === id && !searchQuery}
        <button
          onclick={() => goToTab(id)}
          title="{helpTabLabel(id)}{digitForTab(i) ? ` (${modifierForTab(i)}${digitForTab(i)})` : ''}"
          class="group relative mx-2 flex items-center gap-2.5 px-2.5 py-2
                 rounded-lg text-left transition-colors text-[0.75rem] font-medium
                 {active
                   ? 'bg-foreground/[0.08] dark:bg-white/[0.08] text-foreground'
                   : 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.04] dark:hover:bg-white/[0.04]'}">

          {#if active}
            <span class="absolute left-0 top-1/2 -translate-y-1/2 w-0.5 h-5
                         rounded-full bg-foreground/60 dark:bg-white/60"></span>
          {/if}

          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"
               class="w-3.5 h-3.5 shrink-0 {active ? 'opacity-80' : 'opacity-40 group-hover:opacity-60'}">
            {@html TAB_ICONS[id]}
          </svg>

          <span class="flex-1 leading-none whitespace-nowrap">{helpTabLabel(id)}</span>

          {#if digitForTab(i)}
            <kbd class="text-[0.5rem] font-mono tabular-nums shrink-0
                        {active ? 'text-foreground/35' : 'text-muted-foreground/25 group-hover:text-muted-foreground/40'}">
              {modifierForTab(i)}{digitForTab(i)}
            </kbd>
          {/if}
        </button>
      {/each}
    </nav>

    <button
      type="button"
      class="w-1 shrink-0 cursor-col-resize bg-border/30 hover:bg-primary/40 transition-colors"
      aria-label="Resize sidebar"
      onmousedown={startResize}
    ></button>

    <!-- Content / search results -->
    <div class="flex-1 overflow-y-auto px-5 py-4 flex flex-col gap-4">

      {#if searchQuery.trim()}
        <!-- ── Search results ──────────────────────────────────────────────── -->
        {#if searchResults.length === 0}
          <div class="flex flex-col items-center justify-center gap-2 py-12 text-center">
            <svg class="w-8 h-8 text-muted-foreground/30" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="1.5">
              <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
            </svg>
            <p class="text-[0.78rem] text-muted-foreground">
              {t("help.searchNoResults").replace("{query}", searchQuery.trim())}
            </p>
          </div>
        {:else}
          <div class="flex flex-col gap-1.5 pb-6">
            <p class="text-[0.65rem] uppercase tracking-widest font-semibold
                      text-muted-foreground/60 pl-0.5 pb-1">
              {searchResults.length} {searchResults.length === 1 ? "result" : "results"}
            </p>
            {#each searchResults as item}
              {@const tLabel = helpTabLabel(item.tab)}
              <button
                onclick={() => goToItem(item.tab, item.id)}
                class="group text-left rounded-xl border border-border dark:border-white/[0.06]
                       bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-1.5
                       hover:border-foreground/20 dark:hover:border-white/[0.12]
                       transition-colors">
                <span class="inline-flex items-center rounded-md
                             bg-violet-50 dark:bg-violet-500/10
                             px-2 py-0.5 text-[0.6rem] font-semibold
                             text-violet-600 dark:text-violet-400 w-fit">
                  {tLabel}
                </span>
                <span class="text-[0.78rem] font-semibold text-foreground leading-snug">{item.title}</span>
                <span class="text-[0.72rem] leading-relaxed text-muted-foreground line-clamp-2">
                  {item.body.length > 160 ? item.body.slice(0, 160) + "…" : item.body}
                </span>
              </button>
            {/each}
          </div>
        {/if}
      {:else}
        <!-- ── Active tab content ──────────────────────────────────────────── -->
        {#if tab === "dashboard"}
          <HelpDashboard />
        {:else if tab === "electrodes"}
          <HelpElectrodes />
        {:else if tab === "settings"}
          <HelpSettings />
        {:else if tab === "windows"}
          <HelpWindows />
        {:else if tab === "api"}
          <HelpApi />
        {:else if tab === "tts"}
          <HelpTts />
        {:else if tab === "llm"}
          <HelpLlm />
        {:else if tab === "hooks"}
          <HelpHooks />
        {:else if tab === "privacy"}
          <HelpPrivacy />
        {:else if tab === "references"}
          <HelpReferences />
        {:else}
          <HelpFaqTab />
        {/if}
      {/if}

      <DisclaimerFooter />
    </div>

  </div>

</main>

<style>
  /* Flash ring shown on the target item after search navigation */
  :global(.help-highlight) {
    animation: help-flash 1.6s ease-out forwards;
  }
  @keyframes help-flash {
    0%   { box-shadow: 0 0 0 3px rgb(139 92 246 / 0.55); }
    60%  { box-shadow: 0 0 0 3px rgb(139 92 246 / 0.25); }
    100% { box-shadow: 0 0 0 3px rgb(139 92 246 / 0);    }
  }
</style>
