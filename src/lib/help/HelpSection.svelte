<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Reusable help section: title + optional description + slotted children. -->

<script lang="ts">
import type { Snippet } from "svelte";

let {
  title,
  description = undefined,
  children = undefined,
}: {
  title: string;
  description?: string;
  children?: Snippet;
} = $props();
</script>

<section class="flex flex-col gap-3">
  <h2 class="text-ui-lg font-bold tracking-[0.04em] uppercase
             text-muted-foreground pl-0.5">
    {title}
  </h2>
  {#if description}
    <p class="text-ui-lg leading-relaxed text-foreground/75 px-0.5">
      {description}
    </p>
  {/if}
  {#if children}
    {@render children()}
  {/if}
</section>
