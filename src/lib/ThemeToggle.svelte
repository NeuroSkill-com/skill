<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Theme toggle — cycles system → light → dark.
     Shows: ◐ system, ☀ light, ● dark.
     Teleports tooltip to body to escape overflow clipping. -->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { getTheme, cycleTheme } from "$lib/stores/theme.svelte";
  import { t } from "$lib/i18n/index.svelte";

  const icon = $derived(
    getTheme() === "light" ? "sun" :
    getTheme() === "dark"  ? "moon" : "auto"
  );

  const label = $derived(
    getTheme() === "light" ? t("common.themeLight") :
    getTheme() === "dark"  ? t("common.themeDark") :
                             t("common.themeSystem")
  );

  let btnEl = $state<HTMLButtonElement>();
  let tipEl: HTMLDivElement | undefined;
  let tipTimer: ReturnType<typeof setTimeout> | undefined;

  function onEnter() {
    tipTimer = setTimeout(() => {
      if (!btnEl) return;
      const r = btnEl.getBoundingClientRect();
      const tipW = 120;
      let left = r.left + r.width / 2 - tipW / 2;
      if (left < 4) left = 4;
      if (left + tipW > window.innerWidth - 4) left = window.innerWidth - tipW - 4;

      tipEl = document.createElement("div");
      tipEl.className =
        "pointer-events-none rounded-md border border-neutral-200 dark:border-white/10 " +
        "bg-white dark:bg-[#1a1a28] text-neutral-700 dark:text-neutral-200 shadow-md " +
        "px-2.5 py-1.5 text-center font-medium";
      tipEl.style.cssText =
        `position:fixed; top:${r.bottom + 6}px; left:${left}px; width:${tipW}px; z-index:2147483647; font-size:0.65rem;`;
      tipEl.textContent = label;
      document.body.appendChild(tipEl);
    }, 500);
  }

  function onLeave() {
    clearTimeout(tipTimer);
    tipEl?.remove();
    tipEl = undefined;
  }

  onDestroy(() => {
    clearTimeout(tipTimer);
    tipEl?.remove();
  });
</script>

<button
  bind:this={btnEl}
  onclick={cycleTheme}
  onmouseenter={onEnter}
  onmouseleave={onLeave}
  aria-label={label}
  class="flex items-center justify-center w-6 h-6 rounded-md
         text-muted-foreground hover:text-foreground hover:bg-accent
         transition-colors select-none">
  {#if icon === "sun"}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
         class="w-3 h-3">
      <circle cx="12" cy="12" r="5"/>
      <line x1="12" y1="1" x2="12" y2="3"/>
      <line x1="12" y1="21" x2="12" y2="23"/>
      <line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
      <line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
      <line x1="1" y1="12" x2="3" y2="12"/>
      <line x1="21" y1="12" x2="23" y2="12"/>
      <line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
      <line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
    </svg>
  {:else if icon === "moon"}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
         class="w-3 h-3">
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
    </svg>
  {:else}
    <!-- Half-circle: system/auto -->
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
         class="w-3 h-3">
      <circle cx="12" cy="12" r="9"/>
      <path d="M12 3a9 9 0 0 1 0 18" fill="currentColor" stroke="none"/>
    </svg>
  {/if}
</button>

