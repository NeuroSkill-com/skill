<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Primary shell destinations — Live / Find / Ask / History.
  Shown on main + primary utility windows so users can jump without hunting
  the tray or command palette. Opens/focuses dedicated windows (preserves
  per-surface sizing) rather than SPA-routing the main autofit window.
-->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onMount } from "svelte";
import { t } from "$lib/i18n/index.svelte";
import { openChat, openHistory, openLive, openSearch } from "$lib/navigation";
import { prettyAccelerator } from "$lib/utils/accelerator";

type Dest = "live" | "find" | "ask" | "history";

let windowLabel = $state("main");
let searchShortcut = $state("");
let chatShortcut = $state("");
let historyShortcut = $state("");

const active = $derived<Dest>(
  windowLabel === "search" ? "find" : windowLabel === "chat" ? "ask" : windowLabel === "history" ? "history" : "live",
);

const ITEMS: { id: Dest; label: () => string; shortcut: () => string }[] = [
  { id: "live", label: () => t("shell.live"), shortcut: () => "" },
  { id: "find", label: () => t("shell.find"), shortcut: () => prettyAccelerator(searchShortcut) },
  { id: "ask", label: () => t("shell.ask"), shortcut: () => prettyAccelerator(chatShortcut) },
  {
    id: "history",
    label: () => t("shell.history"),
    shortcut: () => prettyAccelerator(historyShortcut),
  },
];

const SHELL_WINDOWS = new Set(["main", "search", "chat", "history"]);

onMount(async () => {
  try {
    windowLabel = getCurrentWindow().label;
  } catch {
    windowLabel = "main";
  }
  try {
    [searchShortcut, chatShortcut, historyShortcut] = await Promise.all([
      invoke<string>("get_search_shortcut"),
      invoke<string>("get_chat_shortcut"),
      invoke<string>("get_history_shortcut"),
    ]);
  } catch {
    // Shortcuts are optional discoverability — nav still works without them.
  }
});

async function go(dest: Dest) {
  if (dest === active) return;
  switch (dest) {
    case "live":
      await openLive();
      break;
    case "find":
      await openSearch();
      break;
    case "ask":
      await openChat();
      break;
    case "history":
      await openHistory();
      break;
  }
}

function buttonTitle(item: (typeof ITEMS)[number]): string {
  const sc = item.shortcut();
  if (!sc || sc === "—") return item.label();
  return `${item.label()} (${sc})`;
}

const visible = $derived(SHELL_WINDOWS.has(windowLabel));
</script>

{#if visible}
  <nav
    class="shell-nav relative z-10 shrink-0 flex items-center justify-center gap-0.5 px-3 py-1.5
           border-b border-border dark:border-white/[0.06]
           bg-muted/30 dark:bg-white/[0.02]"
    aria-label={t("shell.navLabel")}
    data-tauri-drag-region="false"
  >
    {#each ITEMS as item (item.id)}
      {@const isActive = active === item.id}
      {@const sc = item.shortcut()}
      <button
        type="button"
        onclick={() => go(item.id)}
        title={buttonTitle(item)}
        aria-current={isActive ? "page" : undefined}
        aria-keyshortcuts={sc && sc !== "—" ? sc : undefined}
        data-tauri-drag-region="false"
        class="px-3 py-1 rounded-md text-ui-sm font-semibold tracking-wide transition-colors
               inline-flex items-center gap-1.5 pointer-events-auto
               {isActive
                 ? 'bg-foreground/[0.08] dark:bg-white/[0.1] text-foreground'
                 : 'text-muted-foreground hover:text-foreground hover:bg-foreground/[0.04] dark:hover:bg-white/[0.04]'}"
      >
        {item.label()}
        {#if sc && sc !== "—" && !isActive}
          <kbd
            class="hidden sm:inline font-mono text-[10px] font-medium opacity-50
                   border border-current/20 rounded px-1 py-px leading-none"
          >{sc}</kbd>
        {/if}
      </button>
    {/each}
  </nav>
{/if}
