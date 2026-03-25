<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { t } from "$lib/i18n/index.svelte";
</script>

<div class="rounded-xl border border-violet-500/20 bg-violet-500/5 dark:bg-violet-500/10 px-4 py-3 flex gap-3">
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
       stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 shrink-0 text-violet-500 mt-0.5">
    <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
    <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
  </svg>
  <div class="flex flex-col gap-0.5">
    <p class="text-[0.62rem] text-violet-600 dark:text-violet-400 leading-relaxed">{t("screenshots.privacyNote")}</p>
    <p class="text-[0.54rem] text-violet-600/60 dark:text-violet-400/60 font-mono">{t("screenshots.storagePath")}</p>
  </div>
</div>
