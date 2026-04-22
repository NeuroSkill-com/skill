<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  FormRow — a label + optional value + optional description layout used
  inside settings cards. Renders a consistent header row with an optional
  description paragraph below it.
-->
<script lang="ts">
import { cn } from "$lib/utils.js";

let {
  label,
  value,
  description,
  class: className,
  children,
}: {
  /** Primary label text. */
  label: string;
  /** Optional right-aligned value display. */
  value?: string;
  /** Optional description shown below the label. */
  description?: string;
  class?: string;
  children?: import("svelte").Snippet;
} = $props();
</script>

<div class={cn("flex flex-col gap-2 px-4 py-3.5", className)}>
  <div class="flex items-center justify-between">
    <span class="text-ui-md font-semibold text-foreground">{label}</span>
    {#if value}
      <span class="text-ui-base tabular-nums text-muted-foreground">{value}</span>
    {/if}
  </div>
  {#if description}
    <p class="text-ui-sm text-muted-foreground leading-relaxed -mt-0.5">
      {description}
    </p>
  {/if}
  {#if children}
    {@render children()}
  {/if}
</div>
