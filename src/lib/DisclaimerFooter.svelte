<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Compact one-line research disclaimer. Click to open full text. -->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";

let open = $state(false);
</script>

<!-- One-liner -->
<div class="mt-2 px-2 pb-1.5 text-center">
  <button
    onclick={() => open = true}
    class="text-[0.56rem] text-muted-foreground/50 hover:text-muted-foreground/80
           transition-colors cursor-pointer leading-relaxed select-none">
    {t("disclaimer.footer")}
  </button>
  <p class="mt-1 text-[0.5rem] text-muted-foreground/35 select-none">
    {t("disclaimer.copyright", { year: new Date().getFullYear() })}
  </p>
</div>

<!-- Full disclaimer overlay -->
{#if open}
  <div
    class="fixed inset-0 z-[9999] flex items-center justify-center bg-black/50 backdrop-blur-sm p-4"
    role="presentation"
    onclick={(e) => { if (e.target === e.currentTarget) open = false; }}
    onkeydown={(e) => { if (e.key === "Escape") open = false; }}>
    <div class="bg-white dark:bg-[#14141e] border border-border dark:border-white/[0.08]
                rounded-2xl shadow-2xl max-w-md w-full p-5 flex flex-col gap-3 animate-in">
      <div class="flex items-center gap-2">
        <span class="text-lg">⚠️</span>
        <span class="text-[0.82rem] font-bold uppercase tracking-widest text-amber-700 dark:text-amber-400">
          {t("disclaimer.title")}
        </span>
      </div>
      <p class="text-[0.78rem] text-foreground/80 leading-relaxed">
        {t("disclaimer.body")}
      </p>
      <p class="text-[0.72rem] font-semibold text-amber-700/70 dark:text-amber-400/50 uppercase tracking-wider">
        {t("disclaimer.nonCommercial")}
      </p>
      <button
        onclick={() => open = false}
        class="mt-1 self-end rounded-lg border border-border dark:border-white/[0.08]
               px-4 py-1.5 text-[0.72rem] font-semibold text-muted-foreground
               hover:bg-muted dark:hover:bg-white/[0.04] transition-colors cursor-pointer">
        {t("common.close")}
      </button>
    </div>
  </div>
{/if}

<style>
  .animate-in {
    animation: disclaimer-pop 0.15s ease-out;
  }
  @keyframes disclaimer-pop {
    from { opacity: 0; transform: scale(0.95) translateY(8px); }
    to   { opacity: 1; transform: scale(1) translateY(0); }
  }
</style>
