<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Root error boundary — catches unhandled errors in any route. -->
<script lang="ts">
  import { page } from "$app/stores";
  import { t } from "$lib/i18n/index.svelte";
</script>

<div class="flex flex-col items-center justify-center min-h-[60vh] gap-4 px-6 text-center">
  <div class="text-5xl opacity-30 select-none">⚠</div>
  <h1 class="text-lg font-semibold text-foreground">
    {$page.error?.message ?? t("common.error")}
  </h1>
  <p class="text-sm text-muted-foreground max-w-md leading-relaxed">
    {t("error.description")}
  </p>
  <div class="flex gap-2 mt-2">
    <a href="/"
       class="px-4 py-2 rounded-lg bg-primary text-primary-foreground text-sm font-medium
              hover:opacity-90 transition-opacity">
      {t("error.goHome")}
    </a>
    <button
      onclick={() => window.location.reload()}
      class="px-4 py-2 rounded-lg border border-border text-sm font-medium
             text-foreground hover:bg-accent transition-colors">
      {t("error.reload")}
    </button>
  </div>
  {#if $page.status && $page.status !== 500}
    <p class="text-xs text-muted-foreground/50 mt-4 font-mono">
      HTTP {$page.status}
    </p>
  {/if}
</div>
