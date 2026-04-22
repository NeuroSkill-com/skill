<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  Collapsible dashboard card with a chevron toggle, section title, and live-blink dot.
  Extracts the repeated header + wrapper pattern used across all dashboard metric cards.
-->
<script lang="ts">
import type { Snippet } from "svelte";

interface Props {
  /** Section title (already translated). */
  title: string;
  /** Tailwind text-color class for the live-blink dot, e.g. "text-emerald-500". */
  dotColor?: string;
  /** Whether the section is initially expanded. */
  expanded?: boolean;
  /** Optional extra attributes forwarded to the root div (e.g. role, aria-*). */
  rootAttrs?: Record<string, unknown>;
  /** Slot content rendered when expanded. */
  children: Snippet;
}

let {
  title,
  dotColor = "text-muted-foreground",
  expanded = $bindable(true),
  rootAttrs = {},
  children,
}: Props = $props();
</script>

<div
  class="rounded-xl border border-border dark:border-white/[0.04]
         bg-muted dark:bg-surface-2 px-3 py-2 flex flex-col gap-1.5"
  {...rootAttrs}
>
  <button class="flex items-center gap-1.5 w-full group" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
         stroke-linecap="round" stroke-linejoin="round"
         class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                transition-transform duration-150 shrink-0 {expanded ? 'rotate-90' : ''}">
      <path d="M9 18l6-6-6-6"/>
    </svg>
    <span class="text-ui-2xs font-semibold tracking-widest uppercase text-muted-foreground
                 group-hover:text-foreground transition-colors">{title}</span>
    <span class="text-[0.45rem] {dotColor} live-blink shrink-0" aria-hidden="true">●</span>
  </button>

  {#if expanded}
    {@render children()}
  {/if}
</div>
