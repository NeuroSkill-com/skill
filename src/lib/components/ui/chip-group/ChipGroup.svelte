<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  ChipGroup — renders a horizontal wrap of selectable chip/pill buttons.
  The active chip is highlighted with accent colours; inactive chips use
  the standard muted style. This replaces the copy-pasted preset-button
  markup found across every settings Tab.
-->
<script lang="ts" generics="T">
import { cn } from "$lib/utils.js";

let {
  items,
  selected,
  onselect,
  labelFn = (item: T) => String(item),
  class: className,
}: {
  /** Array of items to display as chips. */
  items: readonly T[] | T[];
  /** The currently selected item (compared by strict equality). */
  selected: T;
  /** Called when a chip is clicked. */
  onselect: (item: T) => void;
  /** Extracts a display label from each item. Defaults to String(item). */
  labelFn?: (item: T) => string;
  class?: string;
} = $props();
</script>

<div class={cn("flex items-center gap-1.5 flex-wrap", className)}>
  {#each items as item}
    {@const active = selected === item}
    <button
      onclick={() => onselect(item)}
      class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold
             transition-all cursor-pointer select-none
             {active
               ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
               : 'border-border dark:border-white/[0.08] bg-muted dark:bg-surface-2 text-muted-foreground hover:text-foreground hover:bg-accent dark:hover:bg-white/[0.04]'}"
    >
      {labelFn(item)}
    </button>
  {/each}
</div>
