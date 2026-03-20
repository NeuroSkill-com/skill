<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- About window — app identity, credits, licence. -->
<script lang="ts">
  import { onMount }        from "svelte";
  import { invoke }         from "@tauri-apps/api/core";
  import { t }              from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/stores/window-title.svelte";

  useWindowTitle("window.title.about");

  interface AboutInfo {
    name:             string;
    version:          string;
    tagline:          string;
    website:          string;
    websiteLabel:     string;
    repoUrl:          string;
    discordUrl:       string;
    license:          string;
    licenseName:      string;
    licenseUrl:       string;
    copyright:        string;
    /** [name, role] pairs */
    authors:          [string, string][];
    acknowledgements: string;
    /** PNG data URL of the Tauri app icon; null if unavailable. */
    iconDataUrl:      string | null;
  }

  let info = $state<AboutInfo | null>(null);

  onMount(async () => {
    info = await invoke<AboutInfo>("get_about_info");
  });
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden select-none">

  <!-- ── Content ───────────────────────────────────────────────────────────── -->
  {#if info}
    <div class="min-h-0 flex-1 overflow-y-auto px-7 py-5 flex flex-col gap-4">

      <!-- Hero ---------------------------------------------------------------->
      <div class="flex flex-col items-center gap-2 text-center">
        <img src={info.iconDataUrl ?? "/icon.png"} alt={info.name}
             class="w-14 h-14 shrink-0 object-contain" />
        <div class="flex flex-col items-center gap-0.5">
          <h1 class="text-[1.1rem] font-bold tracking-tight leading-tight">{info.name}</h1>
          <span class="text-[0.68rem] font-mono text-muted-foreground/45">v{info.version}</span>
        </div>
        <p class="text-[0.75rem] text-muted-foreground/75 max-w-sm leading-snug">
          {info.tagline}
        </p>
      </div>

      <hr class="border-border dark:border-white/[0.07] shrink-0" />

      <!-- Links --------------------------------------------------------------->
      <section class="flex flex-col gap-2">
        <p class="text-[0.6rem] font-semibold tracking-widest uppercase
                  text-muted-foreground/45">
          {t("about.links")}
        </p>
        <div class="flex gap-4">
          <a href={info.website}
             target="_blank" rel="noreferrer"
             class="flex items-center gap-1.5 text-[0.8rem] text-blue-500 dark:text-blue-400
                    hover:underline underline-offset-2 transition-colors">
            <span class="text-[0.7rem] opacity-60">🌐</span>{info.websiteLabel}
          </a>
          <a href={info.repoUrl}
             target="_blank" rel="noreferrer"
             class="flex items-center gap-1.5 text-[0.8rem] text-blue-500 dark:text-blue-400
                    hover:underline underline-offset-2 transition-colors">
            <span class="text-[0.7rem] opacity-60">📦</span>{t("about.sourceCode")}
          </a>
          <a href={info.discordUrl}
             target="_blank" rel="noreferrer"
             class="flex items-center gap-1.5 text-[0.8rem] text-blue-500 dark:text-blue-400
                    hover:underline underline-offset-2 transition-colors">
            <span class="text-[0.7rem] opacity-60">💬</span>{t("about.discord")}
          </a>
        </div>
      </section>

      <!-- Authors ------------------------------------------------------------->
      <section class="flex flex-col gap-2">
        <p class="text-[0.6rem] font-semibold tracking-widest uppercase
                  text-muted-foreground/45">
          {t("about.authors")}
        </p>
        <!-- Two-column grid: name left, role right — both can wrap freely -->
        <div class="grid gap-y-2" style="grid-template-columns: auto 1fr;">
          {#each info.authors as [name, role]}
            <span class="text-[0.82rem] font-medium pr-4 leading-snug">{name}</span>
            <span class="text-[0.75rem] text-muted-foreground/65 leading-snug">{role}</span>
          {/each}
        </div>
      </section>

      <!-- Licence ------------------------------------------------------------->
      <section class="flex flex-col gap-2">
        <p class="text-[0.6rem] font-semibold tracking-widest uppercase
                  text-muted-foreground/45">
          {t("about.license")}
        </p>
        <div class="flex items-center gap-2 flex-wrap">
          <span class="text-[0.82rem]">{info.licenseName}</span>
          <a href={info.licenseUrl}
             target="_blank" rel="noreferrer"
             class="text-[0.7rem] font-mono px-1.5 py-0.5 rounded
                    bg-muted/50 text-muted-foreground/60
                    hover:text-blue-500 dark:hover:text-blue-400
                    hover:bg-blue-500/10 transition-colors">
            {info.license}
          </a>
        </div>
      </section>

      <!-- Acknowledgements --------------------------------------------------->
      <section class="flex flex-col gap-2">
        <p class="text-[0.6rem] font-semibold tracking-widest uppercase
                  text-muted-foreground/45">
          {t("about.acknowledgements")}
        </p>
        <p class="text-[0.76rem] text-muted-foreground/65 leading-relaxed">
          {info.acknowledgements}
        </p>
      </section>

      <!-- Copyright footer ---------------------------------------------------->
      <div class="mt-auto pt-3 border-t border-border/50 dark:border-white/[0.05]
                  text-center shrink-0">
        <p class="text-[0.65rem] text-muted-foreground/35">{info.copyright}</p>
      </div>

    </div>
  {:else}
    <!-- Loading -->
    <div class="flex-1 flex items-center justify-center">
      <div class="w-5 h-5 rounded-full border-2 border-blue-500/30
                  border-t-blue-500 animate-spin"></div>
    </div>
  {/if}

</main>
