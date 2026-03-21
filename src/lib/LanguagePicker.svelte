<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Language picker dropdown — shows current flag, opens locale list on click.
     The menu is teleported to <body> so it escapes all overflow/stacking ancestors. -->
<script lang="ts">
  import { t, getLocale, setLocale, SUPPORTED_LOCALES } from "$lib/i18n/index.svelte";
  import { onMount, onDestroy, tick } from "svelte";

  let open = $state(false);
  let btnEl = $state<HTMLButtonElement>();
  let portalEl: HTMLDivElement | undefined;
  let menuStyle = $state("");

  onMount(() => {
    portalEl = document.createElement("div");
    document.body.appendChild(portalEl);
    document.addEventListener("pointerdown", handleOutsidePointerDown, true);
  });
  onDestroy(() => {
    document.removeEventListener("pointerdown", handleOutsidePointerDown, true);
    portalEl?.remove();
  });

  async function toggle() {
    if (open) { close(); return; }
    if (!btnEl) return;
    const r = btnEl.getBoundingClientRect();
    const menuW = 160;
    let left = r.right - menuW;
    if (left < 4) left = r.left;
    if (left + menuW > window.innerWidth - 4) left = window.innerWidth - menuW - 4;
    let top = r.bottom + 4;
    // If it would overflow below, show above
    const menuH = SUPPORTED_LOCALES.length * 32 + 8;
    if (top + menuH > window.innerHeight - 4) {
      top = r.top - menuH - 4;
    }
    menuStyle = `position:fixed; top:${top}px; left:${left}px; width:${menuW}px; z-index:2147483647;`;
    open = true;
    await tick();
    renderMenu();
  }

  function pick(code: string) {
    setLocale(code);
    // Keep dropdown open so user sees the updated checkmark; it closes on outside click
    renderMenu();
  }

  function close() {
    if (open) { open = false; renderMenu(); }
  }

  /** Render the dropdown into the portal element (imperative DOM to escape Svelte's mount point). */
  function renderMenu() {
    if (!portalEl) return;
    if (!open) { portalEl.innerHTML = ""; return; }

    const locale = getLocale();
    const menu = document.createElement("div");
    menu.style.cssText = menuStyle;
    menu.className =
      "rounded-lg border border-neutral-200 dark:border-white/10 " +
      "bg-white dark:bg-[#1a1a28] shadow-xl py-1";

    // Prevent closing when clicking/pressing inside menu
    menu.addEventListener("pointerdown", (e) => e.stopPropagation());

    for (const loc of SUPPORTED_LOCALES) {
      const btn = document.createElement("button");
      const isActive = locale === loc.code;
      btn.className =
        "flex items-center gap-2 w-full px-3 py-1.5 text-[0.72rem] font-medium transition-colors " +
        (isActive
          ? "bg-primary/10 text-primary"
          : "text-neutral-700 dark:text-neutral-300 hover:bg-neutral-100 dark:hover:bg-white/5");
      btn.innerHTML =
        `<span class="text-[0.9rem] leading-none">${loc.flag}</span>` +
        `<span>${loc.name}</span>` +
        (isActive ? `<span class="ml-auto text-[0.6rem] text-primary">✓</span>` : "");
      btn.addEventListener("click", () => pick(loc.code));
      menu.appendChild(btn);
    }

    portalEl.innerHTML = "";
    portalEl.appendChild(menu);
  }

  function handleOutsidePointerDown(e: PointerEvent) {
    if (!open) return;
    // Ignore clicks inside the portal menu or the toggle button
    const target = e.target as Node;
    if (portalEl?.contains(target)) return;
    if (btnEl?.contains(target)) return;
    close();
  }

  const currentFlag = $derived(
    SUPPORTED_LOCALES.find(l => l.code === getLocale())?.flag ?? "🇺🇸"
  );
</script>

<svelte:window onkeydown={(e) => { if (e.key === "Escape" && open) close(); }} />

<div class="lang-picker-anchor">
  <button
    bind:this={btnEl}
    onclick={toggle}
    title={t("settings.language")}
    aria-label={t("settings.language")}
    aria-haspopup="listbox"
    aria-expanded={open}
    class="flex items-center justify-center w-6 h-6 mx-1 rounded-md
           text-muted-foreground hover:text-foreground hover:bg-accent
           transition-colors select-none text-[0.95rem] leading-none">
    {currentFlag}
  </button>
</div>
