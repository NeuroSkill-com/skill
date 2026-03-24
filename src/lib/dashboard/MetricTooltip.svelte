<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import type { Snippet } from "svelte";

interface Props {
  text: string;
  children: Snippet;
}
let { text, children }: Props = $props();
</script>

<div class="metric-tip group relative">
  {@render children()}
  {#if text}
    <div role="tooltip"
         class="pointer-events-none absolute z-50 bottom-full left-1/2 -translate-x-1/2 mb-1.5
                max-w-52 px-2.5 py-1.5 rounded-lg
                bg-popover text-popover-foreground dark:bg-[#222238] dark:text-neutral-200
                border border-border dark:border-white/10
                text-[0.55rem] leading-snug text-center shadow-lg
                opacity-0 scale-95 transition-all duration-150
                group-hover:opacity-100 group-hover:scale-100">
      {text}
      <div class="absolute top-full left-1/2 -translate-x-1/2 -mt-px
                  border-4 border-transparent border-t-popover dark:border-t-[#222238]"></div>
    </div>
  {/if}
</div>
