<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  SettingsCard — the standard card wrapper used across all settings tabs.
  Provides consistent border, background, padding, and overflow styling.
-->
<script lang="ts">
import { Card } from "$lib/components/ui/card";
import { cn } from "$lib/utils.js";

let {
  class: className,
  children,
  ...restProps
}: { class?: string; children?: import("svelte").Snippet; [key: string]: unknown } = $props();
</script>

<Card
  class={cn(
    "border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden",
    className
  )}
  {...restProps}
>
  {@render children?.()}
</Card>
