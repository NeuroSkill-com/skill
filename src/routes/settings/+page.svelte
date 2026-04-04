<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onDestroy, onMount } from "svelte";
import AppearanceTab from "$lib/AppearanceTab.svelte";
import CalibrationTab from "$lib/CalibrationTab.svelte";
import ClientsTab from "$lib/ClientsTab.svelte";
import DevicesTab from "$lib/DevicesTab.svelte";
import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
import EmbeddingsTab from "$lib/EmbeddingsTab.svelte";
import ExgTab from "$lib/ExgTab.svelte";
import GoalsTab from "$lib/GoalsTab.svelte";
import HooksTab from "$lib/HooksTab.svelte";
import { t } from "$lib/i18n/index.svelte";
import LlmTab from "$lib/LlmTab.svelte";
import LslTab from "$lib/LslTab.svelte";
import PermissionsTab from "$lib/PermissionsTab.svelte";
import ScreenshotsTab from "$lib/ScreenshotsTab.svelte";
import SettingsTab from "$lib/SettingsTab.svelte";
import ShortcutsTab from "$lib/ShortcutsTab.svelte";
import SleepTab from "$lib/SleepTab.svelte";
import ToolsTab from "$lib/ToolsTab.svelte";
import TtsTab from "$lib/TtsTab.svelte";
import UmapTab from "$lib/UmapTab.svelte";
import UpdatesTab from "$lib/UpdatesTab.svelte";
import VirtualEegTab from "$lib/VirtualEegTab.svelte";

type Tab =
  | "goals"
  | "devices"
  | "exg"
  | "lsl"
  | "sleep"
  | "calibration"
  | "embeddings"
  | "hooks"
  | "appearance"
  | "settings"
  | "shortcuts"
  | "umap"
  | "updates"
  | "tts"
  | "permissions"
  | "llm"
  | "tools"
  | "clients"
  | "screenshots"
  | "virtual_eeg";
let tab = $state<Tab>("goals");

const TAB_IDS: Tab[] = [
  "goals",
  "devices",
  "exg",
  "lsl",
  "sleep",
  "calibration",
  "tts",
  "llm",
  "tools",
  "clients",
  "embeddings",
  "screenshots",
  "hooks",
  "appearance",
  "settings",
  "shortcuts",
  "umap",
  "updates",
  "permissions",
  "virtual_eeg",
];
const TAB_LABELS: Record<Tab, () => string> = {
  goals: () => t("settingsTabs.goals"),
  devices: () => t("settingsTabs.devices"),
  exg: () => t("settingsTabs.exg"),
  lsl: () => t("settingsTabs.lsl"),
  sleep: () => t("settingsTabs.sleep"),
  calibration: () => t("settingsTabs.calibration"),
  tts: () => t("settingsTabs.tts"),
  llm: () => t("settingsTabs.llm"),
  tools: () => t("settingsTabs.tools"),
  clients: () => "Clients",
  embeddings: () => t("settingsTabs.embeddings"),
  hooks: () => t("settingsTabs.hooks"),
  appearance: () => t("settingsTabs.appearance"),
  settings: () => t("settingsTabs.settings"),
  shortcuts: () => t("settingsTabs.shortcuts"),

  umap: () => t("settingsTabs.umap"),
  updates: () => t("settingsTabs.updates"),
  permissions: () => t("settingsTabs.permissions"),
  screenshots: () => t("settingsTabs.screenshots"),
  virtual_eeg: () => t("settingsTabs.virtualEeg"),
};

// ── Icons per tab (16×16 stroked) ────────────────────────────────────────
const TAB_ICONS: Record<Tab, string> = {
  goals: `<path d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 8-8 8 8 0 0 1-8 8zm0-14a6 6 0 1 0 6 6 6 6 0 0 0-6-6zm0 10a4 4 0 1 1 4-4 4 4 0 0 1-4 4zm0-6a2 2 0 1 0 2 2 2 2 0 0 0-2-2z"/>`,
  devices: `<path d="M22 12h-4l-3 9L9 3l-3 9H2"/>`,
  exg: `<path d="M2 12h2l3-7 4 14 4-14 3 7h2"/><circle cx="12" cy="12" r="1"/>`,
  lsl: `<path d="M4 6h4v12H4zM10 3h4v18h-4zM16 8h4v8h-4"/>`,
  sleep: `<path d="M21 12.79A9 9 0 1 1 11.21 3a7 7 0 0 0 9.79 9.79z"/>`,
  calibration: `<path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"/><circle cx="12" cy="12" r="3"/>`,
  tts: `<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8"/>`,
  llm: `<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>`,
  tools: `<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>`,
  clients: `<path d="M17 11V7a5 5 0 0 0-10 0v4"/><rect x="3" y="11" width="18" height="10" rx="2"/><circle cx="12" cy="16" r="1.5"/>`,
  embeddings: `<circle cx="12" cy="12" r="2"/><circle cx="4" cy="6" r="2"/><circle cx="20" cy="6" r="2"/><circle cx="4" cy="18" r="2"/><circle cx="20" cy="18" r="2"/><path d="m6 6.5 4 4.5M14 6.5l-2 4M18 7l-4 4.5M6 17l4-4.5M14 17.5l2-4M18 17l-4-4.5"/>`,
  hooks: `<path d="M10 13a5 5 0 0 1 0-7l1.5-1.5a5 5 0 0 1 7 7L17 13"/><path d="M14 11a5 5 0 0 1 0 7L12.5 19.5a5 5 0 1 1-7-7L7 11"/>`,
  appearance: `<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>`,
  settings: `<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>`,
  shortcuts: `<rect x="3" y="11" width="4" height="6" rx="1"/><rect x="10" y="5" width="4" height="12" rx="1"/><rect x="17" y="8" width="4" height="9" rx="1"/>`,
  umap: `<circle cx="6" cy="18" r="2"/><circle cx="18" cy="6" r="2"/><circle cx="6" cy="6" r="2"/><circle cx="18" cy="18" r="2"/><circle cx="12" cy="12" r="2"/><path d="M6 8v6M18 8v6M8 6h6M8 18h6"/>`,
  updates: `<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>`,
  permissions: `<rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>`,
  screenshots: `<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/>`,
  virtual_eeg: `<path d="M2 12h4l2-8 3 16 3-12 2 4h4"/><circle cx="20" cy="12" r="1.5" fill="currentColor"/>`,
};

const tabLabel = (id: Tab) => TAB_LABELS[id]();
const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
const modKey = isMac ? "⌘" : "Ctrl+";

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

  // ⌃⌘ (Ctrl+Cmd on Mac, Ctrl+Alt on other) → tabs 11–19
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

let unlisten: UnlistenFn | null = null;
let fontObserver: MutationObserver | null = null;
let splitRoot: HTMLDivElement | null = null;
let navEl: HTMLElement | null = null;
let navWidth = $state(176);
let resizingNav = false;
let lastSettingsWindowTitle = "";

const NAV_WIDTH_MIN = 140;
const NAV_WIDTH_MAX = 480;
const NAV_WIDTH_KEY = "settings.nav.width";

function clampNavWidth(px: number): number {
  return Math.max(NAV_WIDTH_MIN, Math.min(NAV_WIDTH_MAX, Math.round(px)));
}

/** Measure the nav's natural content width and ensure navWidth is at least that. */
function ensureNavFitsContent(): void {
  if (!navEl) return;
  // Temporarily remove the fixed width so we can measure natural content width
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

function persistNavWidth(px: number): void {
  try {
    localStorage.setItem(NAV_WIDTH_KEY, String(px));
  } catch (e) {}
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

onMount(async () => {
  try {
    const stored = Number(localStorage.getItem(NAV_WIDTH_KEY) ?? "");
    if (!Number.isNaN(stored) && stored > 0) navWidth = clampNavWidth(stored);
  } catch (e) {}

  window.addEventListener("keydown", onKeydown);

  // Ensure sidebar is wide enough for its content at the current font size
  ensureNavFitsContent();

  // Re-check when the root font-size changes (appearance settings)
  fontObserver = new MutationObserver(() => {
    // Wait a frame for layout to settle after font-size change
    requestAnimationFrame(() => ensureNavFitsContent());
  });
  fontObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["style"] });

  // Support ?tab=updates query param (used by open_updates_window)
  const params = new URLSearchParams(window.location.search);
  const qTab = params.get("tab");
  if (qTab && TAB_IDS.includes(qTab as Tab)) {
    tab = qTab as Tab;
  }

  // Listen for switch-tab events (emitted when settings is already open)
  unlisten = await listen<string>("switch-tab", (ev) => {
    if (TAB_IDS.includes(ev.payload as Tab)) {
      tab = ev.payload as Tab;
    }
  });
});
onDestroy(() => {
  if (typeof window !== "undefined") window.removeEventListener("keydown", onKeydown);
  stopResize();
  unlisten?.();
  fontObserver?.disconnect();
});

$effect(() => {
  const settingsTitle = t("settingsTabs.settings");
  const sectionTitle = tabLabel(tab);
  const title = `${settingsTitle} — ${sectionTitle}`;
  if (title === lastSettingsWindowTitle) return;
  lastSettingsWindowTitle = title;
  getCurrentWindow().setTitle(title);
});
</script>

<main class="h-full min-h-0 flex flex-col overflow-hidden"
      aria-label={t("settingsTabs.settings")}>

  <!-- ── Body: sidebar + content ──────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 flex overflow-hidden" bind:this={splitRoot}>

    <!-- Sidebar nav -->
    <nav bind:this={navEl} style={`width:${navWidth}px;min-width:max-content`} class="shrink-0 border-r border-border dark:border-white/[0.07]
                overflow-y-auto py-2 flex flex-col gap-0.5
                bg-muted/20 dark:bg-white/[0.015]"
         aria-label={t("settingsTabs.settings")}>
      {#each TAB_IDS as id, i (id)}
        {@const active = tab === id}
        <button
          onclick={() => tab = id}
          role="tab"
          aria-selected={active}
          aria-controls="tab-panel-{id}"
          title="{tabLabel(id)}{digitForTab(i) ? ` (${modifierForTab(i)}${digitForTab(i)})` : ''}"
          class="group relative mx-2 flex items-center gap-2.5 px-2.5 py-2
                 rounded-lg text-left transition-colors text-[0.75rem] font-medium
                 {active
                   ? 'bg-foreground/[0.08] dark:bg-white/[0.08] text-foreground'
                   : 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.04] dark:hover:bg-white/[0.04]'}">

          <!-- Active indicator bar -->
          {#if active}
            <span class="absolute left-0 top-1/2 -translate-y-1/2 w-0.5 h-5
                         rounded-full bg-foreground/60 dark:bg-white/60"></span>
          {/if}

          <!-- Icon -->
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
               stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"
               class="w-3.5 h-3.5 shrink-0 {active ? 'opacity-80' : 'opacity-40 group-hover:opacity-60'}">
            {@html TAB_ICONS[id]}
          </svg>

          <!-- Label -->
          <span class="flex-1 leading-none whitespace-nowrap">{tabLabel(id)}</span>

          <!-- Kbd hint -->
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
      aria-label={t("settingsTabs.settings")}
      onmousedown={startResize}
    ></button>

    <!-- Content area -->
    <div id="tab-panel-{tab}" role="tabpanel" aria-label={tabLabel(tab)}
         class="flex-1 overflow-y-auto px-5 py-5 flex flex-col gap-4">
      {#if tab === "devices"}
        <DevicesTab />
      {:else if tab === "exg"}
        <ExgTab />
      {:else if tab === "lsl"}
        <LslTab />
      {:else if tab === "settings"}
        <SettingsTab />
      {:else if tab === "appearance"}
        <AppearanceTab />
      {:else if tab === "shortcuts"}
        <ShortcutsTab />
      {:else if tab === "goals"}
        <GoalsTab />
      {:else if tab === "sleep"}
        <SleepTab />
      {:else if tab === "calibration"}
        <CalibrationTab />
      {:else if tab === "embeddings"}
        <EmbeddingsTab />
      {:else if tab === "hooks"}
        <HooksTab />
      {:else if tab === "tts"}
        <TtsTab />
      {:else if tab === "llm"}
        <LlmTab />
      {:else if tab === "tools"}
        <ToolsTab />
      {:else if tab === "umap"}
        <UmapTab />
      {:else if tab === "clients"}
        <ClientsTab />
      {:else if tab === "updates"}
        <UpdatesTab />
      {:else if tab === "screenshots"}
        <ScreenshotsTab />
      {:else if tab === "permissions"}
        <PermissionsTab />
      {:else if tab === "virtual_eeg"}
        <VirtualEegTab />
      {/if}

      <DisclaimerFooter />
    </div>

  </div>

</main>
