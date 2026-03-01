<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Command Palette (⌘K / Ctrl+K)
  A quick-access dropdown listing every runnable action in the app.
  Supports fuzzy text filtering and keyboard navigation.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { fade }               from "svelte/transition";
  import { t }                  from "$lib/i18n/index.svelte";
  import { getHighContrast, toggleHighContrast } from "$lib/theme-store.svelte";
  import { addToast }           from "$lib/toast-store.svelte";

  let open   = $state(false);
  let query  = $state("");
  let active = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();

  // ── Command definitions ────────────────────────────────────────────────────

  interface Command {
    id:       string;
    icon:     string;
    label:    string;
    section:  string;
    keywords?: string;
    shortcut?: string;
    action:   () => void | Promise<void>;
  }

  const isMac = typeof navigator !== "undefined" && navigator.platform?.includes("Mac");
  const mod   = isMac ? "⌘" : "Ctrl";

  function commands(): Command[] {
    return [
      // ── Navigation ─────────────────────────────────────────────────────
      {
        id: "open-settings", icon: "⚙", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openSettings"), shortcut: `${mod},`,
        keywords: t("cmdK.kw.settings"),
        action: () => invoke("open_settings_window"),
      },
      {
        id: "open-help", icon: "?", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openHelp"),
        keywords: t("cmdK.kw.help"),
        action: () => invoke("open_help_window"),
      },
      {
        id: "open-history", icon: "🕐", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openHistory"),
        keywords: t("cmdK.kw.history"),
        action: () => invoke("open_history_window"),
      },
      {
        id: "open-compare", icon: "⚖", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openCompare"),
        keywords: t("cmdK.kw.compare"),
        action: () => invoke("open_compare_window"),
      },
      {
        id: "open-search", icon: "🔍", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openSearch"), shortcut: `${mod}⇧S`,
        keywords: t("cmdK.kw.search"),
        action: () => invoke("open_search_window"),
      },
      {
        id: "open-label", icon: "🏷", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openLabel"), shortcut: `${mod}⇧L`,
        keywords: t("cmdK.kw.label"),
        action: () => invoke("open_label_window"),
      },

      // ── Device ─────────────────────────────────────────────────────────
      {
        id: "retry-connect", icon: "📡", section: t("cmdK.sectionDevice"),
        label: t("cmdK.retryConnect"),
        keywords: t("cmdK.kw.retryConnect"),
        action: () => invoke("retry_connect"),
      },
      {
        id: "open-bt-settings", icon: "📶", section: t("cmdK.sectionDevice"),
        label: t("cmdK.openBtSettings"),
        keywords: t("cmdK.kw.btSettings"),
        action: () => invoke("open_bt_settings"),
      },

      // ── Calibration ────────────────────────────────────────────────────
      {
        id: "open-calibration", icon: "🎯", section: t("cmdK.sectionCalibration"),
        label: t("cmdK.openCalibration"), shortcut: `${mod}⇧C`,
        keywords: t("cmdK.kw.calibration"),
        action: async () => {
          try { await invoke("open_calibration_window"); }
          catch (e) { addToast("warning", t("cmdK.calibrationError"), String(e)); }
        },
      },

      {
        id: "open-api", icon: "🌐", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openApi"),
        keywords: t("cmdK.kw.api"),
        action: () => invoke("open_api_window"),
      },
      {
        id: "open-labels", icon: "🏷", section: t("cmdK.sectionNavigation"),
        label: t("labels.openLabels"),
        keywords: "labels annotations notes tags all browse edit delete manage",
        action: () => invoke("open_labels_window"),
      },
      {
        id: "open-focus-timer", icon: "⏱", section: t("cmdK.sectionNavigation"),
        label: t("focusTimer.openTimer"),
        keywords: "pomodoro focus timer work break productivity neurofeedback session",
        action: () => invoke("open_focus_timer_window"),
      },
      {
        id: "open-onboarding", icon: "🧭", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openOnboarding"),
        keywords: t("cmdK.kw.onboarding"),
        action: () => invoke("open_onboarding_window"),
      },
      {
        id: "open-electrodes", icon: "🧠", section: t("cmdK.sectionNavigation"),
        label: t("cmdK.openElectrodes"),
        keywords: t("cmdK.kw.electrodes"),
        action: () => invoke("open_help_window"),
      },

      // ── Utilities ──────────────────────────────────────────────────────
      {
        id: "show-shortcuts", icon: "⌨", section: t("cmdK.sectionUtilities"),
        label: t("cmdK.showShortcuts"), shortcut: "?",
        keywords: t("cmdK.kw.shortcuts"),
        action: () => {
          close();
          // Simulate pressing ? to open the shortcuts overlay
          window.dispatchEvent(new KeyboardEvent("keydown", { key: "?", bubbles: true }));
        },
      },
      {
        id: "toggle-hc", icon: "◑", section: t("cmdK.sectionUtilities"),
        label: getHighContrast() ? t("cmdK.highContrastOff") : t("cmdK.highContrastOn"),
        keywords: t("cmdK.kw.highContrast"),
        action: () => { toggleHighContrast(); close(); },
      },
      {
        id: "check-updates", icon: "⬆", section: t("cmdK.sectionUtilities"),
        label: t("cmdK.checkUpdates"),
        keywords: t("cmdK.kw.updates"),
        action: () => invoke("open_updates_window"),
      },
    ];
  }

  // ── Filtering ──────────────────────────────────────────────────────────────

  let filtered = $derived.by(() => {
    const cmds = commands();
    if (!query.trim()) return cmds;
    const q = query.toLowerCase().trim();
    return cmds.filter(c =>
      c.label.toLowerCase().includes(q) ||
      c.section.toLowerCase().includes(q) ||
      c.id.includes(q) ||
      (c.keywords && c.keywords.toLowerCase().includes(q))
    );
  });

  // Group by section for rendering
  let sections = $derived.by(() => {
    const map = new Map<string, Command[]>();
    for (const c of filtered) {
      if (!map.has(c.section)) map.set(c.section, []);
      map.get(c.section)!.push(c);
    }
    return [...map.entries()];
  });

  // ── Keyboard handling ──────────────────────────────────────────────────────

  function handleGlobalKeydown(e: KeyboardEvent) {
    // Cmd/Ctrl+K to toggle
    if (e.key === "k" && (isMac ? e.metaKey : e.ctrlKey)) {
      e.preventDefault();
      e.stopPropagation();
      if (open) close(); else openPalette();
      return;
    }
    // Escape to close
    if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  }

  function handleInputKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      active = Math.min(active + 1, filtered.length - 1);
      scrollActiveIntoView();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      active = Math.max(active - 1, 0);
      scrollActiveIntoView();
    } else if (e.key === "Enter" && filtered.length > 0) {
      e.preventDefault();
      runCommand(filtered[active]);
    }
  }

  function scrollActiveIntoView() {
    requestAnimationFrame(() => {
      const el = document.querySelector(`[data-cmd-index="${active}"]`);
      el?.scrollIntoView({ block: "nearest" });
    });
  }

  function openPalette() {
    query  = "";
    active = 0;
    open   = true;
    requestAnimationFrame(() => inputEl?.focus());
  }

  function close() {
    open = false;
  }

  function runCommand(cmd: Command) {
    close();
    cmd.action();
  }

  // Compute flat index for each command across grouped sections
  function flatIndex(sectionIdx: number, itemIdx: number): number {
    let idx = 0;
    for (let s = 0; s < sectionIdx; s++) {
      idx += sections[s][1].length;
    }
    return idx + itemIdx;
  }

  onMount(() => {
    window.addEventListener("keydown", handleGlobalKeydown, true);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", handleGlobalKeydown, true);
  });
</script>

{#if open}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-[9998] bg-black/40 backdrop-blur-sm"
    transition:fade={{ duration: 100 }}
    onclick={close}
    onkeydown={handleGlobalKeydown}
  ></div>

  <!-- Palette -->
  <div
    class="fixed top-[15%] left-[50%] z-[9999] w-full max-w-[480px]
           translate-x-[-50%]
           rounded-2xl border border-border dark:border-white/[0.1]
           bg-white dark:bg-[#18181f] shadow-2xl
           flex flex-col overflow-hidden"
    transition:fade={{ duration: 100 }}
    role="dialog"
    aria-modal="true"
    aria-label={t("cmdK.title")}
  >
    <!-- Search input -->
    <div class="flex items-center gap-2.5 px-4 py-3 border-b border-border dark:border-white/[0.06]">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
           stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
           class="w-4 h-4 text-muted-foreground shrink-0">
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={handleInputKeydown}
        type="text"
        placeholder={t("cmdK.placeholder")}
        class="flex-1 bg-transparent text-[0.82rem] text-foreground
               placeholder:text-muted-foreground/50
               focus:outline-none"
        spellcheck="false"
        autocomplete="off"
      />
      <kbd class="text-[0.55rem] font-mono text-muted-foreground/50 border border-border
                  dark:border-white/[0.08] rounded px-1.5 py-0.5 shrink-0">
        Esc
      </kbd>
    </div>

    <!-- Results -->
    <div class="max-h-[50vh] overflow-y-auto py-1.5">
      {#if filtered.length === 0}
        <p class="text-center text-[0.75rem] text-muted-foreground/50 py-6">
          {t("cmdK.noResults")}
        </p>
      {:else}
        {#each sections as [sectionLabel, cmds], sIdx}
          <div class="px-3 pt-2 pb-1">
            <p class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground/60 px-1">
              {sectionLabel}
            </p>
          </div>
          {#each cmds as cmd, cIdx}
            {@const fi = flatIndex(sIdx, cIdx)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              data-cmd-index={fi}
              class="flex items-center gap-2.5 mx-1.5 px-3 py-2 rounded-lg cursor-pointer
                     transition-colors
                     {fi === active
                       ? 'bg-blue-500/10 dark:bg-blue-500/15 text-foreground'
                       : 'text-foreground/80 hover:bg-accent'}"
              onclick={() => runCommand(cmd)}
              onmouseenter={() => (active = fi)}
              onkeydown={(e) => { if (e.key === 'Enter') runCommand(cmd); }}
              role="option"
              aria-selected={fi === active}
              tabindex="-1"
            >
              <span class="w-5 text-center text-[0.85rem] shrink-0">{cmd.icon}</span>
              <span class="flex-1 text-[0.78rem] font-medium truncate">{cmd.label}</span>
              {#if cmd.shortcut}
                <kbd class="text-[0.55rem] font-mono text-muted-foreground/50 border border-border
                            dark:border-white/[0.08] rounded px-1.5 py-0.5 shrink-0 whitespace-nowrap">
                  {cmd.shortcut}
                </kbd>
              {/if}
            </div>
          {/each}
        {/each}
      {/if}
    </div>

    <!-- Footer -->
    <div class="flex items-center gap-3 px-4 py-2 border-t border-border dark:border-white/[0.06]
                text-[0.55rem] text-muted-foreground/50">
      <span>↑↓ {t("cmdK.navigate")}</span>
      <span>↵ {t("cmdK.run")}</span>
      <span class="ml-auto">{t("cmdK.footerHint")}</span>
    </div>
  </div>
{/if}
