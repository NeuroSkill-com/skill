<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  ToggleRow — a full-width clickable row with a toggle switch, label,
  description, and optional ON/OFF badge. Used inside SettingsCard for
  boolean settings. Combines the ToggleSwitch with the surrounding
  layout that was previously duplicated across every Tab.
-->
<script lang="ts">
import { cn } from "$lib/utils.js";

let {
  checked,
  label,
  description,
  ontoggle,
  activeColor = "bg-violet-500",
  showBadge = true,
  badgeOnLabel = "ON",
  badgeOffLabel = "OFF",
  class: className,
}: {
  checked: boolean;
  label: string;
  description?: string;
  ontoggle: () => void;
  activeColor?: string;
  showBadge?: boolean;
  badgeOnLabel?: string;
  badgeOffLabel?: string;
  class?: string;
} = $props();
</script>

<button
  role="switch"
  aria-checked={checked}
  class={cn(
    "flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full hover:bg-accent/50 dark:hover:bg-white/[0.02]",
    className
  )}
  onclick={ontoggle}
>
  <!-- Toggle switch -->
  <div
    class="relative shrink-0 w-8 h-4 rounded-full transition-colors
           {checked ? activeColor : 'bg-muted dark:bg-white/[0.08]'}"
  >
    <div
      class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
             {checked ? 'translate-x-4' : 'translate-x-0.5'}"
    ></div>
  </div>

  <!-- Label + description -->
  <div class="flex flex-col gap-0.5 min-w-0">
    <span class="text-ui-md font-semibold text-foreground leading-tight">{label}</span>
    {#if description}
      <span class="text-ui-sm text-muted-foreground leading-tight">{description}</span>
    {/if}
  </div>

  <!-- ON/OFF badge -->
  {#if showBadge}
    <span
      class="ml-auto text-ui-xs font-bold tracking-widest uppercase shrink-0
             {checked ? 'text-violet-500' : 'text-muted-foreground/50'}"
    >
      {checked ? badgeOnLabel : badgeOffLabel}
    </span>
  {/if}
</button>
