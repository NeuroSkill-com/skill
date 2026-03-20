<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { toasts, dismissToast, type Toast, type ToastLevel } from "$lib/stores/toast.svelte";

  const ICONS: Record<ToastLevel, string> = {
    info:    "ℹ",
    success: "✓",
    warning: "⚠",
    error:   "✕",
  };

  const LEVEL_CLASSES: Record<ToastLevel, string> = {
    info:    "border-blue-400/40   bg-blue-50   dark:bg-blue-950/30   text-blue-800   dark:text-blue-300",
    success: "border-green-400/40  bg-green-50  dark:bg-green-950/30  text-green-800  dark:text-green-300",
    warning: "border-amber-400/40  bg-amber-50  dark:bg-amber-950/30  text-amber-800  dark:text-amber-300",
    error:   "border-red-400/40    bg-red-50    dark:bg-red-950/30    text-red-800    dark:text-red-300",
  };

  const ICON_CLASSES: Record<ToastLevel, string> = {
    info:    "bg-blue-100  dark:bg-blue-900/40  text-blue-600  dark:text-blue-400",
    success: "bg-green-100 dark:bg-green-900/40 text-green-600 dark:text-green-400",
    warning: "bg-amber-100 dark:bg-amber-900/40 text-amber-600 dark:text-amber-400",
    error:   "bg-red-100   dark:bg-red-900/40   text-red-600   dark:text-red-400",
  };
</script>

{#if toasts.length > 0}
  <div class="fixed top-3 right-3 z-[9999] flex flex-col gap-2 pointer-events-none max-w-[360px] w-full"
       role="log" aria-live="polite" aria-label="Notifications">
    {#each toasts as toast (toast.id)}
      <div
        class="pointer-events-auto rounded-xl border px-3.5 py-2.5 shadow-lg backdrop-blur-sm
               flex items-start gap-2.5 {LEVEL_CLASSES[toast.level]}"
        transition:fly={{ x: 80, duration: 250 }}
        animate:flip={{ duration: 200 }}
        role="alert"
      >
        <!-- Icon -->
        <div class="flex-shrink-0 w-6 h-6 rounded-full flex items-center justify-center
                    text-[0.7rem] font-bold {ICON_CLASSES[toast.level]}">
          {ICONS[toast.level]}
        </div>

        <!-- Content -->
        <div class="flex-1 min-w-0 pt-0.5">
          <p class="text-[0.75rem] font-semibold leading-tight">{toast.title}</p>
          {#if toast.message}
            <p class="text-[0.67rem] opacity-80 leading-snug mt-0.5">{toast.message}</p>
          {/if}
        </div>

        <!-- Dismiss -->
        <button
          onclick={() => dismissToast(toast.id)}
          class="flex-shrink-0 w-5 h-5 rounded-md flex items-center justify-center
                 opacity-50 hover:opacity-100 transition-opacity text-[0.65rem]"
          aria-label="Dismiss"
        >
          ✕
        </button>
      </div>
    {/each}
  </div>
{/if}
