<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help window — sidebar navigation + search. -->

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import HelpDashboard          from "$lib/help/HelpDashboard.svelte";
  import HelpSettings           from "$lib/help/HelpSettings.svelte";
  import HelpWindows            from "$lib/help/HelpWindows.svelte";
  import HelpApi                from "$lib/help/HelpApi.svelte";
  import HelpFaqTab             from "$lib/help/HelpFaqTab.svelte";
  import HelpPrivacy            from "$lib/help/HelpPrivacy.svelte";
  import HelpElectrodes         from "$lib/help/HelpElectrodes.svelte";
  import HelpReferences         from "$lib/help/HelpReferences.svelte";
  import HelpTts                from "$lib/help/HelpTts.svelte";
  import { t }                  from "$lib/i18n/index.svelte";
  import { useWindowTitle }     from "$lib/window-title.svelte";
  import { helpTitlebarState }  from "$lib/help-search-state.svelte";
  import DisclaimerFooter       from "$lib/DisclaimerFooter.svelte";

  type Tab = "dashboard" | "electrodes" | "settings" | "windows" | "api" | "tts" | "privacy" | "references" | "faq";
  let tab = $state<Tab>("dashboard");
  let searchQuery = $derived(helpTitlebarState.query);

  const TAB_IDS: Tab[] = ["dashboard", "electrodes", "settings", "windows", "api", "tts", "privacy", "references", "faq"];
  const helpTabLabel = (id: Tab) => t(`helpTabs.${id}`);
  const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
  const modKey = isMac ? "⌘" : "Ctrl+";

  // ── Icons per tab ─────────────────────────────────────────────────────────
  const TAB_ICONS: Record<Tab, string> = {
    dashboard:  `<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>`,
    electrodes: `<circle cx="12" cy="8" r="4"/><path d="M12 12v4M8 16h8M6 20h12"/>`,
    settings:   `<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>`,
    windows:    `<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/>`,
    api:        `<path d="M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3"/><path d="m9 9 6 6M15 9l-6 6"/>`,
    tts:        `<path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v4M8 23h8"/>`,
    privacy:    `<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>`,
    references: `<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>`,
    faq:        `<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>`,
  };

  // ── Searchable help item registry ────────────────────────────────────────
  type SearchEntry = { tab: Tab; titleKey: string; bodyKey: string };

  const searchIndex: SearchEntry[] = [
    // ── Dashboard ──────────────────────────────────────────────────────────
    { tab: "dashboard", titleKey: "helpDash.statusHero",     bodyKey: "helpDash.statusHeroBody" },
    { tab: "dashboard", titleKey: "helpDash.battery",        bodyKey: "helpDash.batteryBody" },
    { tab: "dashboard", titleKey: "helpDash.signalQuality",  bodyKey: "helpDash.signalQualityBody" },
    { tab: "dashboard", titleKey: "helpDash.eegChannelGrid", bodyKey: "helpDash.eegChannelGridBody" },
    { tab: "dashboard", titleKey: "helpDash.uptimeSamples",  bodyKey: "helpDash.uptimeSamplesBody" },
    { tab: "dashboard", titleKey: "helpDash.csvRecording",   bodyKey: "helpDash.csvRecordingBody" },
    { tab: "dashboard", titleKey: "helpDash.bandPowers",     bodyKey: "helpDash.bandPowersBody" },
    { tab: "dashboard", titleKey: "helpDash.faa",            bodyKey: "helpDash.faaBody" },
    { tab: "dashboard", titleKey: "helpDash.eegWaveforms",   bodyKey: "helpDash.eegWaveformsBody" },
    { tab: "dashboard", titleKey: "helpDash.gpuUtilisation", bodyKey: "helpDash.gpuUtilisationBody" },
    { tab: "dashboard", titleKey: "helpDash.trayGrey",       bodyKey: "helpDash.trayGreyDesc" },
    { tab: "dashboard", titleKey: "helpDash.trayAmber",      bodyKey: "helpDash.trayAmberDesc" },
    { tab: "dashboard", titleKey: "helpDash.trayGreen",      bodyKey: "helpDash.trayGreenDesc" },
    { tab: "dashboard", titleKey: "helpDash.trayRed",        bodyKey: "helpDash.trayRedDesc" },
    // ── Settings ───────────────────────────────────────────────────────────
    { tab: "settings", titleKey: "helpSettings.pairedDevices",    bodyKey: "helpSettings.pairedDevicesBody" },
    { tab: "settings", titleKey: "helpSettings.signalProcessing", bodyKey: "helpSettings.signalProcessingBody" },
    { tab: "settings", titleKey: "helpSettings.eegEmbedding",     bodyKey: "helpSettings.eegEmbeddingBody" },
    { tab: "settings", titleKey: "helpSettings.calibration",      bodyKey: "helpSettings.calibrationBody" },
    { tab: "settings", titleKey: "helpSettings.calibrationTts",   bodyKey: "helpSettings.calibrationTtsBody" },
    { tab: "settings", titleKey: "helpSettings.globalShortcuts",  bodyKey: "helpSettings.globalShortcutsBody" },
    { tab: "settings", titleKey: "helpSettings.debugLogging",     bodyKey: "helpSettings.debugLoggingBody" },
    { tab: "settings", titleKey: "helpSettings.updates",          bodyKey: "helpSettings.updatesBody" },
    { tab: "settings", titleKey: "helpSettings.appearanceTab",    bodyKey: "helpSettings.appearanceTabBody" },
    { tab: "settings", titleKey: "helpSettings.goalsTab",         bodyKey: "helpSettings.goalsTabBody" },
    { tab: "settings", titleKey: "helpSettings.embeddingsTab",    bodyKey: "helpSettings.embeddingsTabBody" },
    { tab: "settings", titleKey: "helpSettings.shortcutsTab",     bodyKey: "helpSettings.shortcutsTabBody" },
    { tab: "settings", titleKey: "helpSettings.umapTab",          bodyKey: "helpSettings.umapTabBody" },
    { tab: "settings", titleKey: "helpSettings.encoderStatus",    bodyKey: "helpSettings.encoderStatusBody" },
    { tab: "settings", titleKey: "helpSettings.embeddingsToday",  bodyKey: "helpSettings.embeddingsTodayBody" },
    { tab: "settings", titleKey: "helpSettings.hnswParams",       bodyKey: "helpSettings.hnswParamsBody" },
    { tab: "settings", titleKey: "helpSettings.dataNorm",         bodyKey: "helpSettings.dataNormBody" },
    // ── OpenBCI ────────────────────────────────────────────────────────────
    { tab: "settings", titleKey: "helpSettings.openbciBoard",    bodyKey: "helpSettings.openbciBoardBody" },
    { tab: "settings", titleKey: "helpSettings.openbciGanglion", bodyKey: "helpSettings.openbciGanglionBody" },
    { tab: "settings", titleKey: "helpSettings.openbciSerial",   bodyKey: "helpSettings.openbciSerialBody" },
    { tab: "settings", titleKey: "helpSettings.openbciWifi",     bodyKey: "helpSettings.openbciWifiBody" },
    { tab: "settings", titleKey: "helpSettings.openbciGalea",    bodyKey: "helpSettings.openbciGaleaBody" },
    { tab: "settings", titleKey: "helpSettings.openbciChannels", bodyKey: "helpSettings.openbciChannelsBody" },
    // ── Activity Tracking ──────────────────────────────────────────────────
    { tab: "settings", titleKey: "helpSettings.activeWindowHelp",       bodyKey: "helpSettings.activeWindowHelpBody" },
    { tab: "settings", titleKey: "helpSettings.inputActivityHelp",      bodyKey: "helpSettings.inputActivityHelpBody" },
    { tab: "settings", titleKey: "helpSettings.activityStorageHelp",    bodyKey: "helpSettings.activityStorageHelpBody" },
    { tab: "settings", titleKey: "helpSettings.activityPermissionsHelp",bodyKey: "helpSettings.activityPermissionsHelpBody" },
    { tab: "settings", titleKey: "helpSettings.activityDisablingHelp",  bodyKey: "helpSettings.activityDisablingHelpBody" },
    // ── Permissions tab ────────────────────────────────────────────────────
    { tab: "faq", titleKey: "helpFaq.q51", bodyKey: "helpFaq.a51" },
    { tab: "faq", titleKey: "helpFaq.q52", bodyKey: "helpFaq.a52" },
    { tab: "faq", titleKey: "helpFaq.q53", bodyKey: "helpFaq.a53" },
    { tab: "faq", titleKey: "helpFaq.q54", bodyKey: "helpFaq.a54" },
    { tab: "faq", titleKey: "helpFaq.q55", bodyKey: "helpFaq.a55" },
    // ── Windows ────────────────────────────────────────────────────────────
    { tab: "windows", titleKey: "helpWindows.labelTitle",              bodyKey: "helpWindows.labelBody" },
    { tab: "windows", titleKey: "helpWindows.searchTitle",             bodyKey: "helpWindows.searchBody" },
    { tab: "windows", titleKey: "helpWindows.searchEegTitle",          bodyKey: "helpWindows.searchEegBody" },
    { tab: "windows", titleKey: "helpWindows.searchTextTitle",         bodyKey: "helpWindows.searchTextBody" },
    { tab: "windows", titleKey: "helpWindows.searchInteractiveTitle",  bodyKey: "helpWindows.searchInteractiveBody" },
    { tab: "windows", titleKey: "helpWindows.calTitle",                bodyKey: "helpWindows.calBody" },
    { tab: "windows", titleKey: "helpWindows.settingsTitle",           bodyKey: "helpWindows.settingsBody" },
    { tab: "windows", titleKey: "helpWindows.helpTitle",               bodyKey: "helpWindows.helpBody" },
    { tab: "windows", titleKey: "helpWindows.onboardingTitle",         bodyKey: "helpWindows.onboardingBody" },
    { tab: "windows", titleKey: "helpWindows.apiTitle",                bodyKey: "helpWindows.apiBody" },
    { tab: "windows", titleKey: "helpWindows.sleepTitle",              bodyKey: "helpWindows.sleepBody" },
    { tab: "windows", titleKey: "helpWindows.compareTitle",            bodyKey: "helpWindows.compareBody" },
    { tab: "windows", titleKey: "helpWindows.cmdPaletteTitle",         bodyKey: "helpWindows.cmdPaletteBody" },
    { tab: "windows", titleKey: "helpWindows.shortcutsOverlayTitle",   bodyKey: "helpWindows.shortcutsOverlayBody" },
    // ── API ────────────────────────────────────────────────────────────────
    { tab: "api", titleKey: "helpApi.liveStreaming",   bodyKey: "helpApi.liveStreamingBody" },
    { tab: "api", titleKey: "helpApi.commands",        bodyKey: "helpApi.commandsBody" },
    { tab: "api", titleKey: "helpApi.cmdStatus",       bodyKey: "helpApi.cmdStatusDesc" },
    { tab: "api", titleKey: "helpApi.cmdCalibrate",    bodyKey: "helpApi.cmdCalibrateDesc" },
    { tab: "api", titleKey: "helpApi.cmdLabel",        bodyKey: "helpApi.cmdLabelDesc" },
    { tab: "api", titleKey: "helpApi.cmdSearch",       bodyKey: "helpApi.cmdSearchDesc" },
    { tab: "api", titleKey: "helpApi.cmdSessions",     bodyKey: "helpApi.cmdSessionsDesc" },
    { tab: "api", titleKey: "helpApi.cmdCompare",      bodyKey: "helpApi.cmdCompareDesc" },
    { tab: "api", titleKey: "helpApi.cmdSleep",        bodyKey: "helpApi.cmdSleepDesc" },
    { tab: "api", titleKey: "helpApi.cmdUmap",         bodyKey: "helpApi.cmdUmapDesc" },
    { tab: "api", titleKey: "helpApi.cmdUmapPoll",     bodyKey: "helpApi.cmdUmapPollDesc" },
    { tab: "api", titleKey: "helpApi.cmdSay",          bodyKey: "helpApi.cmdSayDesc" },
    // ── Voice / TTS ────────────────────────────────────────────────────────
    { tab: "tts", titleKey: "helpTts.overviewTitle",     bodyKey: "helpTts.overviewBody" },
    { tab: "tts", titleKey: "helpTts.howItWorksTitle",   bodyKey: "helpTts.howItWorksBody" },
    { tab: "tts", titleKey: "helpTts.modelTitle",        bodyKey: "helpTts.modelBody" },
    { tab: "tts", titleKey: "helpTts.requirementsTitle", bodyKey: "helpTts.requirementsBody" },
    { tab: "tts", titleKey: "helpTts.calibrationTitle",  bodyKey: "helpTts.calibrationBody" },
    { tab: "tts", titleKey: "helpTts.apiTitle",          bodyKey: "helpTts.apiBody" },
    { tab: "tts", titleKey: "helpTts.loggingTitle",      bodyKey: "helpTts.loggingBody" },
    // ── Privacy ────────────────────────────────────────────────────────────
    { tab: "privacy", titleKey: "helpPrivacy.allLocal",       bodyKey: "helpPrivacy.allLocalBody" },
    { tab: "privacy", titleKey: "helpPrivacy.noAccounts",     bodyKey: "helpPrivacy.noAccountsBody" },
    { tab: "privacy", titleKey: "helpPrivacy.dataLocation",   bodyKey: "helpPrivacy.dataLocationBody" },
    { tab: "privacy", titleKey: "helpPrivacy.noTelemetry",    bodyKey: "helpPrivacy.noTelemetryBody" },
    { tab: "privacy", titleKey: "helpPrivacy.localWs",        bodyKey: "helpPrivacy.localWsBody" },
    { tab: "privacy", titleKey: "helpPrivacy.mdns",           bodyKey: "helpPrivacy.mdnsBody" },
    { tab: "privacy", titleKey: "helpPrivacy.updateChecks",   bodyKey: "helpPrivacy.updateChecksBody" },
    { tab: "privacy", titleKey: "helpPrivacy.ble",            bodyKey: "helpPrivacy.bleBody" },
    { tab: "privacy", titleKey: "helpPrivacy.osPermissions",  bodyKey: "helpPrivacy.osPermissionsBody" },
    { tab: "privacy", titleKey: "helpPrivacy.deviceIds",      bodyKey: "helpPrivacy.deviceIdsBody" },
    { tab: "privacy", titleKey: "helpPrivacy.gpuLocal",       bodyKey: "helpPrivacy.gpuLocalBody" },
    { tab: "privacy", titleKey: "helpPrivacy.filtering",      bodyKey: "helpPrivacy.filteringBody" },
    { tab: "privacy", titleKey: "helpPrivacy.nnSearch",       bodyKey: "helpPrivacy.nnSearchBody" },
    { tab: "privacy", titleKey: "helpPrivacy.access",              bodyKey: "helpPrivacy.accessBody" },
    { tab: "privacy", titleKey: "helpPrivacy.delete",              bodyKey: "helpPrivacy.deleteBody" },
    { tab: "privacy", titleKey: "helpPrivacy.export",              bodyKey: "helpPrivacy.exportBody" },
    { tab: "privacy", titleKey: "helpPrivacy.encrypt",             bodyKey: "helpPrivacy.encryptBody" },
    { tab: "privacy", titleKey: "helpPrivacy.activityTracking",    bodyKey: "helpPrivacy.activityTrackingBody" },
    { tab: "privacy", titleKey: "helpPrivacy.activityPermission",  bodyKey: "helpPrivacy.activityPermissionBody" },
    // ── FAQ ────────────────────────────────────────────────────────────────
    ...Array.from({ length: 50 }, (_, i) => ({
      tab: "faq" as Tab,
      titleKey: `helpFaq.q${i + 1}`,
      bodyKey:  `helpFaq.a${i + 1}`,
    })),
  ];

  // ── Derived search results (reactive to locale changes via t()) ──────────
  const searchResults = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return [] as SearchEntry[];
    return searchIndex.filter((item) => {
      const title = t(item.titleKey as Parameters<typeof t>[0]).toLowerCase();
      const body  = t(item.bodyKey  as Parameters<typeof t>[0]).toLowerCase();
      return title.includes(q) || body.includes(q);
    });
  });

  function goToTab(id: Tab) {
    tab = id;
    helpTitlebarState.query = "";
  }

  // ── Search-result navigation ──────────────────────────────────────────────
  let pendingScrollKey = "";

  function goToItem(targetTab: Tab, titleKey: string) {
    pendingScrollKey = titleKey;
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

  /* ── Cmd/Ctrl + 1‥9 to switch tabs ────────────────────────────────────── */
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

  onMount(async () => {
    helpTitlebarState.version = await invoke<string>("get_app_version");
    window.addEventListener("keydown", onKeydown);
  });
  onDestroy(() => {
    helpTitlebarState.query = "";
    if (typeof window !== "undefined") window.removeEventListener("keydown", onKeydown);
  });

  useWindowTitle("window.title.help");
</script>

<main class="h-full min-h-0 flex flex-col overflow-hidden">

  <!-- ── Body: sidebar + content ──────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 flex overflow-hidden">

    <!-- Sidebar nav (always visible, dims when searching) -->
    <nav class="w-40 shrink-0 border-r border-border dark:border-white/[0.07]
                overflow-y-auto py-2 flex flex-col gap-0.5
                bg-muted/20 dark:bg-white/[0.015]
                transition-opacity {searchQuery ? 'opacity-40' : 'opacity-100'}"
         aria-label="Help sections">
      {#each TAB_IDS as id, i}
        {@const active = tab === id && !searchQuery}
        <button
          onclick={() => goToTab(id)}
          title="{helpTabLabel(id)}{i < 9 ? ` (${modKey}${i + 1})` : ''}"
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

          <span class="flex-1 leading-none">{helpTabLabel(id)}</span>

          {#if i < 9}
            <kbd class="text-[0.5rem] font-mono tabular-nums shrink-0
                        {active ? 'text-foreground/35' : 'text-muted-foreground/25 group-hover:text-muted-foreground/40'}">
              {modKey}{i + 1}
            </kbd>
          {/if}
        </button>
      {/each}
    </nav>

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
              {@const title  = t(item.titleKey as Parameters<typeof t>[0])}
              {@const body   = t(item.bodyKey  as Parameters<typeof t>[0])}
              <button
                onclick={() => goToItem(item.tab, item.titleKey)}
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
                <span class="text-[0.78rem] font-semibold text-foreground leading-snug">{title}</span>
                <span class="text-[0.72rem] leading-relaxed text-muted-foreground line-clamp-2">
                  {body.length > 160 ? body.slice(0, 160) + "…" : body}
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
