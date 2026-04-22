<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { cn } from "$lib/utils.js";

let {
  class: className,
  children,
  ...restProps
}: { class?: string; children?: import("svelte").Snippet; [key: string]: unknown } = $props();
</script>

<span
  class={cn(
    "text-ui-xs font-semibold tracking-widest uppercase text-muted-foreground px-0.5",
    className
  )}
  {...restProps}
>
  {@render children?.()}
</span>
