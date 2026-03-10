<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }    from "@tauri-apps/api/core";
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
  import { Button }       from "$lib/components/ui/button";
  import { t }            from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
  import LanguagePicker   from "$lib/LanguagePicker.svelte";
  import ThemeToggle      from "$lib/ThemeToggle.svelte";

  type Tab = "goals" | "calibration" | "embeddings" | "appearance" | "settings" | "shortcuts" | "model" | "umap" | "updates" | "tts" | "permissions" | "llm";
  let tab = $state<Tab>("goals");
  let appVersion = $state("…");

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
  const tabLabel = (id: Tab) => TAB_LABELS[id]();

  const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
  const modKey = isMac ? "⌘" : "Ctrl+";

  /* ── Tab button refs — scroll active tab into view ───────────────── */
  let tabBtnEls: HTMLButtonElement[] = $state([]);

  function scrollActiveTab() {
    const idx = TAB_IDS.indexOf(tab);
    tabBtnEls[idx]?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
  }

  $effect(() => {
    void tab;
    requestAnimationFrame(scrollActiveTab);
  });

  /* ── Cmd/Ctrl + 1‥5 to switch tabs ────────────────────────────────── */
  function onKeydown(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey)) return;
    const n = parseInt(e.key, 10);
    if (n >= 1 && n <= TAB_IDS.length) {
      e.preventDefault();
      tab = TAB_IDS[n - 1];
    }
  }

  async function openHelp() { await invoke("open_help_window"); }

  let unlisten: UnlistenFn | null = null;

  onMount(async () => {
    appVersion = await invoke<string>("get_app_version");
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

<main class="h-screen flex flex-col overflow-hidden"
      aria-label={t("settingsTabs.settings")}>

  <!-- ── Tab bar (scrollable) ──────────────────────────────────────────── -->
  <div class="flex items-end border-b border-border dark:border-white/[0.07] pb-0 px-4 pt-4 shrink-0">
    <div class="flex items-end gap-0.5 overflow-x-auto scrollbar-none min-w-0"
         role="tablist" aria-label={t("settingsTabs.settings")}>
      {#each TAB_IDS as id, i}
        <button
          bind:this={tabBtnEls[i]}
          onclick={() => tab = id}
          role="tab"
          aria-selected={tab === id}
          aria-controls="tab-panel-{id}"
          class="px-2.5 py-2 text-[0.72rem] font-medium rounded-t-md transition-colors
                 whitespace-nowrap shrink-0 flex items-center gap-1
                 {tab === id
                   ? 'text-foreground border-b-2 border-foreground -mb-px'
                   : 'text-muted-foreground hover:text-foreground'}"
          title="{tabLabel(id)} ({modKey}{i + 1})">
          {tabLabel(id)}
          <kbd class="text-[0.5rem] font-mono leading-none px-0.5 py-0.5
                      rounded border tabular-nums
                      {tab === id
                        ? 'border-foreground/20 text-foreground/50'
                        : 'border-transparent text-muted-foreground/30'}">{modKey}{i + 1}</kbd>
        </button>
      {/each}
    </div>

    <!-- Right side: Help button · Language picker · version -->
    <div class="ml-auto flex items-center gap-1 pb-1.5 shrink-0 pl-2">
      <Button size="sm" variant="ghost"
              class="text-[0.68rem] h-6 px-2 text-muted-foreground hover:text-foreground gap-1"
              onclick={openHelp}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 shrink-0">
          <circle cx="12" cy="12" r="10"/>
          <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/>
          <line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        {t("settingsTabs.help")}
      </Button>
      <ThemeToggle />
      <LanguagePicker />
      <span class="text-[0.52rem] text-muted-foreground/40 tabular-nums pl-1">
        v{appVersion}
      </span>
      <span class="text-[0.48rem] text-muted-foreground/30 pl-0.5 select-none"
            title="GNU General Public License v3.0">
        {t("settings.license")}
      </span>
    </div>
  </div>

  <!-- ── Active tab (scrollable content) ───────────────────────────────── -->
  <div id="tab-panel-{tab}" role="tabpanel" aria-label={tabLabel(tab)}
       class="flex-1 overflow-y-auto px-4 py-4 flex flex-col gap-4">
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

</main>

<style>
  .scrollbar-none { -ms-overflow-style: none; scrollbar-width: none; }
  .scrollbar-none::-webkit-scrollbar { display: none; }
</style>
