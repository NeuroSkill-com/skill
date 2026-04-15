<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Keyboard Shortcuts Overlay
  Press `?` anywhere to open; press `?` or `Esc` to close.
  Shows all global shortcuts (fetched from the Rust backend) plus
  in-app keyboard shortcuts hardcoded in the frontend.
-->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";
import { fade } from "svelte/transition";
import { t } from "$lib/i18n/index.svelte";

let open = $state(false);

// Global shortcuts fetched from the backend
let labelShortcut = $state("");
let searchShortcut = $state("");
let settingsShortcut = $state("");
let calibrationShortcut = $state("");
let focusTimerShortcut = $state("");
let chatShortcut = $state("");
let compareShortcut = $state("");
let helpShortcut = $state("");
let historyShortcut = $state("");
let apiShortcut = $state("");
let themeShortcut = $state("");

/** Pretty-print a Tauri accelerator string for display.
 *  "CmdOrCtrl+Shift+L" → "⌘⇧L" on Mac, "Ctrl+Shift+L" elsewhere. */
function pretty(accel: string): string {
  if (!accel) return "—";
  const isMac = navigator.platform?.startsWith("Mac") || navigator.userAgent.includes("Mac");
  let s = accel;
  if (isMac) {
    s = s
      .replace(/CmdOrCtrl/gi, "⌘")
      .replace(/CommandOrControl/gi, "⌘")
      .replace(/Ctrl/gi, "⌃")
      .replace(/Cmd/gi, "⌘")
      .replace(/Alt/gi, "⌥")
      .replace(/Shift/gi, "⇧")
      .replace(/\+/g, "");
  } else {
    s = s.replace(/CmdOrCtrl/gi, "Ctrl").replace(/CommandOrControl/gi, "Ctrl");
  }
  return s;
}

interface ShortcutEntry {
  keys: string;
  label: string;
  section: string;
}

const mod = navigator.platform?.startsWith("Mac") || navigator.userAgent.includes("Mac") ? "⌘" : "Ctrl+";

const appShortcuts: ShortcutEntry[] = [
  { keys: "?", label: "shortcuts.showOverlay", section: "shortcuts.sectionApp" },
  { keys: `${mod}K`, label: "shortcutsTab.cmdKTitle", section: "shortcuts.sectionApp" },
  { keys: "Esc", label: "shortcuts.closeOverlay", section: "shortcuts.sectionApp" },
];

let globalShortcuts = $derived<ShortcutEntry[]>([
  { keys: pretty(labelShortcut), label: "settings.shortcutAddLabel", section: "shortcuts.sectionGlobal" },
  { keys: pretty(searchShortcut), label: "settings.shortcutSearch", section: "shortcuts.sectionGlobal" },
  { keys: pretty(settingsShortcut), label: "settings.shortcutSettings", section: "shortcuts.sectionGlobal" },
  { keys: pretty(calibrationShortcut), label: "shortcuts.openCalibration", section: "shortcuts.sectionGlobal" },
  { keys: pretty(focusTimerShortcut), label: "settings.shortcutFocusTimer", section: "shortcuts.sectionGlobal" },
  { keys: pretty(chatShortcut), label: "settings.shortcutChat", section: "shortcuts.sectionGlobal" },
  { keys: pretty(compareShortcut), label: "settings.shortcutCompare", section: "shortcuts.sectionGlobal" },
  { keys: pretty(helpShortcut), label: "settings.shortcutHelp", section: "shortcuts.sectionGlobal" },
  { keys: pretty(historyShortcut), label: "settings.shortcutHistory", section: "shortcuts.sectionGlobal" },
  { keys: pretty(apiShortcut), label: "settings.shortcutApi", section: "shortcuts.sectionGlobal" },
  { keys: pretty(themeShortcut), label: "settings.shortcutTheme", section: "shortcuts.sectionGlobal" },
]);

const windowShortcuts: ShortcutEntry[] = [
  { keys: "⌘/Ctrl + ↵", label: "shortcuts.submitLabel", section: "shortcuts.sectionWindows" },
  { keys: "Esc", label: "shortcuts.closeWindow", section: "shortcuts.sectionWindows" },
];

let allShortcuts = $derived([...appShortcuts, ...globalShortcuts, ...windowShortcuts]);

// Group by section for rendering
let sections = $derived(() => {
  const map = new Map<string, ShortcutEntry[]>();
  for (const s of allShortcuts) {
    if (!map.has(s.section)) map.set(s.section, []);
    map.get(s.section)?.push(s);
  }
  return [...map.entries()];
});

async function fetchShortcuts() {
  try {
    [labelShortcut, searchShortcut, settingsShortcut, calibrationShortcut, focusTimerShortcut,
     chatShortcut, compareShortcut, helpShortcut, historyShortcut, apiShortcut, themeShortcut] = await Promise.all([
      invoke<string>("get_label_shortcut"),
      invoke<string>("get_search_shortcut"),
      invoke<string>("get_settings_shortcut"),
      invoke<string>("get_calibration_shortcut"),
      invoke<string>("get_focus_timer_shortcut"),
      invoke<string>("get_chat_shortcut"),
      invoke<string>("get_compare_shortcut"),
      invoke<string>("get_help_shortcut"),
      invoke<string>("get_history_shortcut"),
      invoke<string>("get_api_shortcut"),
      invoke<string>("get_theme_shortcut"),
    ]);
  } catch (_) {
    /* backend unavailable — keep defaults */
  }
}

function handleKeydown(e: KeyboardEvent) {
  // Don't trigger when typing in inputs/textareas
  const tag = (e.target as HTMLElement)?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

  if (e.key === "?" && !e.ctrlKey && !e.metaKey && !e.altKey) {
    e.preventDefault();
    if (open) {
      open = false;
    } else {
      fetchShortcuts();
      open = true;
    }
  }
  if (e.key === "Escape" && open) {
    e.preventDefault();
    e.stopPropagation();
    open = false;
  }
}

// ── Focus trap ─────────────────────────────────────────────────────────────
function focusTrap(node: HTMLElement) {
  const FOCUSABLE = 'button:not([disabled]),input,[tabindex]:not([tabindex="-1"])';
  const first = () => node.querySelectorAll<HTMLElement>(FOCUSABLE)[0];
  const last = () => {
    const els = node.querySelectorAll<HTMLElement>(FOCUSABLE);
    return els[els.length - 1];
  };
  first()?.focus();
  function trap(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    if (e.shiftKey) {
      if (document.activeElement === first()) {
        e.preventDefault();
        last()?.focus();
      }
    } else {
      if (document.activeElement === last()) {
        e.preventDefault();
        first()?.focus();
      }
    }
  }
  node.addEventListener("keydown", trap);
  return {
    destroy() {
      node.removeEventListener("keydown", trap);
    },
  };
}

onMount(() => {
  window.addEventListener("keydown", handleKeydown, true);
});
onDestroy(() => {
  window.removeEventListener("keydown", handleKeydown, true);
});
</script>

{#if open}
  <!-- Backdrop -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-[9998] bg-black/50 backdrop-blur-sm"
    transition:fade={{ duration: 150 }}
    onkeydown={handleKeydown}
    onclick={() => (open = false)}
  ></div>

  <!-- Modal -->
  <div
    class="fixed top-[50%] left-[50%] z-[9999] w-full max-w-[420px]
           translate-x-[-50%] translate-y-[-50%]
           rounded-2xl border border-border dark:border-white/[0.08]
           bg-white dark:bg-[#16161e] shadow-2xl
           p-5 flex flex-col gap-4"
    transition:fade={{ duration: 150 }}
    role="dialog"
    aria-modal="true"
    aria-labelledby="kbd-shortcuts-title"
    use:focusTrap
  >
    <!-- Header -->
    <div class="flex items-center justify-between">
      <h2 id="kbd-shortcuts-title" class="text-[0.95rem] font-bold tracking-tight text-foreground">
        {t("shortcuts.title")}
      </h2>
      <button
        onclick={() => (open = false)}
        class="w-7 h-7 rounded-md flex items-center justify-center
               text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
        aria-label={t("common.close")}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-4 h-4">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>

    <!-- Shortcut sections -->
    <div class="flex flex-col gap-4 max-h-[60vh] overflow-y-auto -mx-1 px-1">
      {#each sections() as [sectionKey, entries]}
        <div class="flex flex-col gap-1.5">
          <p class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground/70">
            {t(sectionKey)}
          </p>
          {#each entries as entry}
            <div class="flex items-center justify-between gap-3 py-1">
              <span class="text-[0.75rem] text-foreground/80">{t(entry.label)}</span>
              <kbd class="inline-flex items-center gap-0.5 rounded-md border border-border
                          dark:border-white/[0.1] bg-muted dark:bg-white/[0.05]
                          px-2 py-0.5 font-mono text-[0.65rem] font-medium
                          text-muted-foreground whitespace-nowrap shrink-0">
                {entry.keys}
              </kbd>
            </div>
          {/each}
        </div>
      {/each}
    </div>

    <!-- Footer hint -->
    <p class="text-[0.6rem] text-muted-foreground/50 text-center pt-1">
      {t("shortcuts.footer")}
    </p>
  </div>
{/if}
