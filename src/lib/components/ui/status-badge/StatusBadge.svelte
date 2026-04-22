<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!--
  StatusBadge — a small pill that shows a coloured dot + label to convey
  granted / denied / unknown / not_required state. Extracted from the
  PermissionsTab where this pattern was repeated for every permission row.
-->
<script lang="ts" module>
export type StatusLevel = "granted" | "denied" | "unknown" | "not_required";

const DOT: Record<StatusLevel, string> = {
  granted: "bg-green-500",
  denied: "bg-red-500",
  unknown: "bg-amber-400",
  not_required: "bg-muted-foreground/40",
};

const PILL: Record<StatusLevel, string> = {
  granted: "bg-green-500/15 text-green-700 dark:text-green-400 border-green-500/30",
  denied: "bg-red-500/15 text-red-700 dark:text-red-400 border-red-500/30",
  unknown: "bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/30",
  not_required: "bg-muted/60 text-muted-foreground border-border",
};
</script>

<script lang="ts">
let {
  status,
  label,
}: {
  status: StatusLevel;
  label: string;
} = $props();
</script>

<span
  class="inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-ui-base font-semibold {PILL[status]}"
>
  <span class="w-1.5 h-1.5 rounded-full {DOT[status]}"></span>
  {label}
</span>
