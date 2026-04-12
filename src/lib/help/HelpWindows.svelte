<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Windows & Overlays — all text pulled from markdown -->

<script lang="ts">
import { getLocale } from "$lib/i18n/index.svelte";
import HelpItem from "./HelpItem.svelte";
import HelpSection from "./HelpSection.svelte";
import { getHelpContent } from "./help-loader";

const sections = $derived(getHelpContent("windows", getLocale()));
</script>

<div class="flex flex-col gap-6 pb-6">

  {#each sections as section, si}
    <HelpSection title={section.title}
      description={section.description}>

      {#each section.items as item, ii}
        {#if si === 0 && ii === 1}
          <!-- Search window: overview + nested sub-items for search modes -->
          <div id={item.id}
               class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-3 scroll-mt-4">
            <div class="flex flex-col gap-1">
              <span class="text-[0.78rem] font-semibold text-foreground">{item.title}</span>
              <span class="text-[0.75rem] leading-relaxed text-muted-foreground">{item.body}</span>
            </div>
            {#if section.items.length > ii + 1}
              <div class="flex flex-col gap-2 pl-3 border-l-2 border-border dark:border-white/[0.07]">
                {#each section.items.slice(ii + 1, ii + 4) as sub}
                  <div id={sub.id} class="flex flex-col gap-0.5 scroll-mt-4">
                    <span class="text-[0.74rem] font-semibold text-foreground/80">{sub.title}</span>
                    <span class="text-[0.72rem] leading-relaxed text-muted-foreground">{sub.body}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {:else if si === 0 && ii >= 2 && ii <= 4}
          <!-- skip: rendered as sub-items of Search above -->
        {:else}
          <HelpItem id={item.id} title={item.title} body={item.body} />
        {/if}
      {/each}

    </HelpSection>
  {/each}

</div>
