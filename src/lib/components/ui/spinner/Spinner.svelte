<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  /** Animated arc spinner. Inherits text color from parent. */
  interface Props {
    /** Tailwind size class, e.g. "w-3.5 h-3.5" */
    size?: string;
    /** Extra CSS classes */
    class?: string;
  }
  let { size = "w-3.5 h-3.5", class: cls = "" }: Props = $props();
</script>

<svg class="{size} animate-spin {cls}" viewBox="0 0 24 24"
     fill="none" stroke="currentColor" stroke-width="2.5">
  <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
</svg>
