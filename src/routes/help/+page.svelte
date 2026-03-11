<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help window — tabbed reference for every part of Skill. -->

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
  import DisclaimerFooter       from "$lib/DisclaimerFooter.svelte";
  import LanguagePicker         from "$lib/LanguagePicker.svelte";
  import ThemeToggle            from "$lib/ThemeToggle.svelte";

  type Tab = "dashboard" | "electrodes" | "settings" | "windows" | "api" | "tts" | "privacy" | "references" | "faq";
  let tab = $state<Tab>("dashboard");
  let appVersion = $state("…");
  let searchQuery = $state("");

  const TAB_IDS: Tab[] = ["dashboard", "electrodes", "settings", "windows", "api", "tts", "privacy", "references", "faq"];
  const helpTabLabel = (id: Tab) => t(`helpTabs.${id}`);
  const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);
  const modKey = isMac ? "⌘" : "Ctrl+";

  // ── Searchable help item registry ────────────────────────────────────────
  // Each entry maps to one HelpItem across the help tabs.
  // Electrodes (visual interactive) and References are excluded.
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

  // ── Derived search results (reactive to locale changes via t()) ───────────
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
    searchQuery = "";
  }

  // ── Search-result navigation: switch tab then scroll to the exact item ────
  // Plain variable (not $state) so reads/writes don't trigger reactive tracking.
  let pendingScrollKey = "";

  function goToItem(targetTab: Tab, titleKey: string) {
    pendingScrollKey = titleKey;
    tab = targetTab;      // triggers $effect below (subscribed to `tab`)
    searchQuery = "";     // clears search overlay so the tab content renders
  }

  // Runs after `tab` or `searchQuery` change (both are $state).
  // When pendingScrollKey is set we defer two frames so Svelte has fully
  // painted the new tab content before we query the DOM.
  $effect(() => {
    void tab;
    void searchQuery;
    const key = pendingScrollKey;
    if (!key) return;
    pendingScrollKey = "";   // clear immediately (plain write, no reactivity)
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const el = document.getElementById(key);
        if (!el) return;
        // Open <details> (FAQ) before scrolling so its height is correct.
        if (el instanceof HTMLDetailsElement) {
          el.open = true;
          // Extra frame so the browser lays out the now-open details.
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

  /* ── Tab button refs — used to scroll the active tab into view ───── */
  let tabBtnEls: HTMLButtonElement[] = $state([]);

  function scrollActiveTab() {
    const idx = TAB_IDS.indexOf(tab);
    tabBtnEls[idx]?.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
  }

  $effect(() => {
    void tab;
    requestAnimationFrame(scrollActiveTab);
  });

  /* ── Cmd/Ctrl + 1‥9 to switch tabs ────────────────────────────────── */
  function onKeydown(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey)) return;
    const n = parseInt(e.key, 10);
    if (n >= 1 && n <= TAB_IDS.length) {
      e.preventDefault();
      tab = TAB_IDS[n - 1];
    }
  }

  onMount(async () => {
    appVersion = await invoke<string>("get_app_version");
    window.addEventListener("keydown", onKeydown);
  });
  onDestroy(() => {
    if (typeof window !== "undefined") window.removeEventListener("keydown", onKeydown);
  });

  useWindowTitle("window.title.help");
</script>

<main class="h-screen flex flex-col overflow-hidden">

  <!-- ── Sticky header: search bar + tab bar ───────────────────────────── -->
  <div class="shrink-0 px-4 pt-5 pb-0 flex flex-col gap-4">

    <!-- Search bar -->
    <div class="relative">
      <svg class="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground/50 pointer-events-none"
           viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
        <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
      </svg>
      <input
        type="search"
        bind:value={searchQuery}
        placeholder={t("help.searchPlaceholder")}
        class="w-full rounded-lg border border-border dark:border-white/[0.07]
               bg-muted/40 dark:bg-white/[0.04] pl-8 pr-3 py-2
               text-[0.78rem] text-foreground placeholder:text-muted-foreground/50
               focus:outline-none focus:ring-1 focus:ring-foreground/20
               transition-colors"
      />
      {#if searchQuery}
        <button
          onclick={() => searchQuery = ""}
          class="absolute right-2.5 top-1/2 -translate-y-1/2
                 text-muted-foreground/50 hover:text-muted-foreground transition-colors"
          aria-label="Clear search">
          <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
            <path d="M18 6 6 18M6 6l12 12"/>
          </svg>
        </button>
      {/if}
    </div>

    <!-- Tab bar -->
    <div class="flex items-end border-b border-border dark:border-white/[0.07] pb-0">
      <div class="flex items-end gap-1 overflow-x-auto scrollbar-none min-w-0">
        {#each TAB_IDS as id, i}
          <button
            bind:this={tabBtnEls[i]}
            onclick={() => goToTab(id)}
            class="px-3 py-2 text-[0.78rem] font-medium rounded-t-md transition-colors
                   whitespace-nowrap shrink-0 flex items-center gap-1.5
                   {tab === id && !searchQuery
                     ? 'text-foreground border-b-2 border-foreground -mb-px'
                     : 'text-muted-foreground hover:text-foreground'}"
            title="{helpTabLabel(id)} ({modKey}{i + 1})">
            {helpTabLabel(id)}
            <kbd class="kbd-hint text-[0.56rem] font-mono leading-none px-1 py-0.5
                        rounded border tabular-nums
                        {tab === id && !searchQuery
                          ? 'border-foreground/20 text-foreground/50'
                          : 'border-transparent text-muted-foreground/40'}">{modKey}{i + 1}</kbd>
          </button>
        {/each}
      </div>
      <div class="ml-auto flex items-center gap-1 pb-1.5 shrink-0 pl-2">
        <ThemeToggle />
        <LanguagePicker />
        <span class="text-[0.56rem] text-muted-foreground/40 tabular-nums pl-1">
          v{appVersion}
        </span>
        <span class="text-[0.48rem] text-muted-foreground/30 pl-0.5 select-none"
              title="GNU General Public License v3.0">
          {t("settings.license")}
        </span>
      </div>
    </div>

  </div>

  <!-- ── Scrollable content ────────────────────────────────────────────── -->
  <div class="flex-1 overflow-y-auto px-4 py-4 flex flex-col gap-4">

    <!-- Search results OR active tab -->
    {#if searchQuery.trim()}
      <!-- Search results panel -->
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
          <p class="text-[0.65rem] uppercase tracking-widest font-semibold text-muted-foreground/60 pl-0.5 pb-1">
            {searchResults.length} {searchResults.length === 1 ? "result" : "results"}
          </p>
          {#each searchResults as item}
            {@const tabLabel = helpTabLabel(item.tab)}
            {@const title    = t(item.titleKey as Parameters<typeof t>[0])}
            {@const body     = t(item.bodyKey  as Parameters<typeof t>[0])}
            <button
              onclick={() => goToItem(item.tab, item.titleKey)}
              class="group text-left rounded-xl border border-border dark:border-white/[0.06]
                     bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-1.5
                     hover:border-foreground/20 dark:hover:border-white/[0.12]
                     transition-colors">
              <!-- Tab badge -->
              <span class="inline-flex items-center rounded-md
                           bg-violet-50 dark:bg-violet-500/10
                           px-2 py-0.5 text-[0.6rem] font-semibold
                           text-violet-600 dark:text-violet-400 w-fit">
                {tabLabel}
              </span>
              <!-- Title -->
              <span class="text-[0.78rem] font-semibold text-foreground leading-snug">{title}</span>
              <!-- Body snippet (first 160 chars) -->
              <span class="text-[0.72rem] leading-relaxed text-muted-foreground line-clamp-2">
                {body.length > 160 ? body.slice(0, 160) + "…" : body}
              </span>
            </button>
          {/each}
        </div>
      {/if}
    {:else}
      <!-- Active tab content -->
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

</main>

<style>
  /* Hide scrollbar but keep scroll functional */
  .scrollbar-none {
    -ms-overflow-style: none;
    scrollbar-width: none;
  }
  .scrollbar-none::-webkit-scrollbar {
    display: none;
  }

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
