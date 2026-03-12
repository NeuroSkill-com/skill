<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import SettingsTab      from "$lib/SettingsTab.svelte";
  import AppearanceTab    from "$lib/AppearanceTab.svelte";
  import EegModelTab      from "$lib/EegModelTab.svelte";
  import ShortcutsTab     from "$lib/ShortcutsTab.svelte";
  import UmapTab          from "$lib/UmapTab.svelte";
  import GoalsTab         from "$lib/GoalsTab.svelte";
  import CalibrationTab   from "$lib/CalibrationTab.svelte";
  import EmbeddingsTab    from "$lib/EmbeddingsTab.svelte";
  import UpdatesTab       from "$lib/UpdatesTab.svelte";
  import TtsTab           from "$lib/TtsTab.svelte";
  import PermissionsTab   from "$lib/PermissionsTab.svelte";
  import LlmTab           from "$lib/LlmTab.svelte";
  import { t }            from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";

  type Tab = "goals" | "calibration" | "embeddings" | "appearance" | "settings" | "shortcuts" | "model" | "umap" | "updates" | "tts" | "permissions" | "llm";
  let tab = $state<Tab>("goals");

  const TAB_IDS: Tab[] = ["goals", "calibration", "tts", "llm", "model", "embeddings", "appearance", "settings", "shortcuts", "umap", "updates", "permissions"];
  const TAB_LABELS: Record<Tab, () => string> = {
    goals:       () => t("settingsTabs.goals"),
    calibration: () => t("settingsTabs.calibration"),
    tts:         () => t("settingsTabs.tts"),
    llm:         () => t("settingsTabs.llm"),
    embeddings:  () => t("settingsTabs.embeddings"),
    appearance:  () => t("settingsTabs.appearance"),
    settings:    () => t("settingsTabs.settings"),
    shortcuts:   () => t("settingsTabs.shortcuts"),
    model:       () => t("settingsTabs.eegModel"),
    umap:        () => t("settingsTabs.umap"),
    updates:     () => t("settingsTabs.updates"),
    permissions: () => t("settingsTabs.permissions"),
  };

  // ── Icons per tab (16×16 stroked) ────────────────────────────────────────
  const TAB_ICONS: Record<Tab, string> = {
    goals:       `<path d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 8-8 8 8 0 0 1-8 8zm0-14a6 6 0 1 0 6 6 6 6 0 0 0-6-6zm0 10a4 4 0 1 1 4-4 4 4 0 0 1-4 4zm0-6a2 2 0 1 0 2 2 2 2 0 0 0-2-2z"/>`,
    calibration: `<path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"/><circle cx="12" cy="12" r="3"/>`,
    tts:         `<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8"/>`,
    llm:         `<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>`,
    model:       `<path d="M9.5 2A2.5 2.5 0 0 1 12 4.5v15a2.5 2.5 0 0 1-4.96.44 2.5 2.5 0 0 1-2.96-3.08 3 3 0 0 1-.34-5.58 2.5 2.5 0 0 1 1.32-4.24 2.5 2.5 0 0 1 4.44-1.04z"/><path d="M14.5 2A2.5 2.5 0 0 0 12 4.5v15a2.5 2.5 0 0 0 4.96.44 2.5 2.5 0 0 0 2.96-3.08 3 3 0 0 0 .34-5.58 2.5 2.5 0 0 0-1.32-4.24 2.5 2.5 0 0 0-4.44-1.04z"/>`,
    embeddings:  `<circle cx="12" cy="12" r="2"/><circle cx="4" cy="6" r="2"/><circle cx="20" cy="6" r="2"/><circle cx="4" cy="18" r="2"/><circle cx="20" cy="18" r="2"/><path d="m6 6.5 4 4.5M14 6.5l-2 4M18 7l-4 4.5M6 17l4-4.5M14 17.5l2-4M18 17l-4-4.5"/>`,
    appearance:  `<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>`,
    settings:    `<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>`,
    shortcuts:   `<rect x="3" y="11" width="4" height="6" rx="1"/><rect x="10" y="5" width="4" height="12" rx="1"/><rect x="17" y="8" width="4" height="9" rx="1"/>`,
    umap:        `<circle cx="6" cy="18" r="2"/><circle cx="18" cy="6" r="2"/><circle cx="6" cy="6" r="2"/><circle cx="18" cy="18" r="2"/><circle cx="12" cy="12" r="2"/><path d="M6 8v6M18 8v6M8 6h6M8 18h6"/>`,
    updates:     `<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>`,
    permissions: `<rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>`,
  };

  const tabLabel = (id: Tab) => TAB_LABELS[id]();
  const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
  const modKey = isMac ? "⌘" : "Ctrl+";

  /* ── Cmd/Ctrl + 1‥12 to switch tabs ───────────────────────────────────── */
  // Only Cmd/Ctrl+1–9 are reachable as single keystrokes; don't register beyond that.
  const SHORTCUT_TABS = TAB_IDS.slice(0, 9);

  function onKeydown(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey)) return;
    const n = parseInt(e.key, 10);
    if (n >= 1 && n <= SHORTCUT_TABS.length) {
      e.preventDefault();
      tab = SHORTCUT_TABS[n - 1];
    }
  }

  let unlisten: UnlistenFn | null = null;

  onMount(async () => {
    window.addEventListener("keydown", onKeydown);

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
    unlisten?.();
  });

  useWindowTitle("window.title.settings");
</script>

<main class="h-full min-h-0 flex flex-col overflow-hidden"
      aria-label={t("settingsTabs.settings")}>

  <!-- ── Body: sidebar + content ──────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 flex overflow-hidden">

    <!-- Sidebar nav -->
    <nav class="w-44 shrink-0 border-r border-border dark:border-white/[0.07]
                overflow-y-auto py-2 flex flex-col gap-0.5
                bg-muted/20 dark:bg-white/[0.015]"
         aria-label={t("settingsTabs.settings")}>
      {#each TAB_IDS as id, i}
        {@const active = tab === id}
        <button
          onclick={() => tab = id}
          role="tab"
          aria-selected={active}
          aria-controls="tab-panel-{id}"
          title="{tabLabel(id)}{i < 9 ? ` (${modKey}${i + 1})` : ''}"
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
          <span class="flex-1 leading-none">{tabLabel(id)}</span>

          <!-- Kbd hint -->
          {#if i < 9}
            <kbd class="text-[0.5rem] font-mono tabular-nums shrink-0
                        {active ? 'text-foreground/35' : 'text-muted-foreground/25 group-hover:text-muted-foreground/40'}">
              {modKey}{i + 1}
            </kbd>
          {/if}
        </button>
      {/each}
    </nav>

    <!-- Content area -->
    <div id="tab-panel-{tab}" role="tabpanel" aria-label={tabLabel(tab)}
         class="flex-1 overflow-y-auto px-5 py-5 flex flex-col gap-4">
      {#if tab === "settings"}
        <SettingsTab />
      {:else if tab === "appearance"}
        <AppearanceTab />
      {:else if tab === "shortcuts"}
        <ShortcutsTab />
      {:else if tab === "goals"}
        <GoalsTab />
      {:else if tab === "calibration"}
        <CalibrationTab />
      {:else if tab === "embeddings"}
        <EmbeddingsTab />
      {:else if tab === "tts"}
        <TtsTab />
      {:else if tab === "llm"}
        <LlmTab />
      {:else if tab === "umap"}
        <UmapTab />
      {:else if tab === "updates"}
        <UpdatesTab />
      {:else if tab === "permissions"}
        <PermissionsTab />
      {:else}
        <EegModelTab />
      {/if}

      <DisclaimerFooter />
    </div>

  </div>

</main>
