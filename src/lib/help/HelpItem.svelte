<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Reusable help item card: bold title + body text. -->

<script lang="ts">
let { title, body, id = undefined }: { title: string; body: string; id?: string } = $props();
</script>

<div {id}
     class="rounded-xl border border-border dark:border-white/[0.06]
            bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-1
            scroll-mt-4">
  <span class="text-[0.78rem] font-semibold text-foreground">{title}</span>
  <span class="text-[0.75rem] leading-relaxed text-muted-foreground">{body}</span>
</div>
