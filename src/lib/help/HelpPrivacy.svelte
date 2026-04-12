<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Security & Privacy Policy -->

<script lang="ts">
import { Separator } from "$lib/components/ui/separator";
import { getLocale } from "$lib/i18n/index.svelte";
import HelpItem from "./HelpItem.svelte";
import HelpSection from "./HelpSection.svelte";
import { getHelpContent } from "./help-loader";

const sections = $derived(getHelpContent("privacy", getLocale()));

/** Emoji icons for the "Your Data, Your Control" rights items. */
const rightsEmojis = ["🗂", "🗑", "📦", "🔒"];
</script>

<div class="flex flex-col gap-6 pb-6">

  {#each sections as section, si}
    {#if si > 0}
      <Separator class="bg-border dark:bg-white/[0.06]" />
    {/if}

    {#if section.title === "Your Data, Your Control"}
      <!-- ── Your rights ──────────────────────────────────────────────────── -->
      <HelpSection title={section.title}>
        <div class="rounded-xl border border-border dark:border-white/[0.06]
                    bg-white dark:bg-[#14141e] divide-y divide-border dark:divide-white/[0.05]
                    overflow-hidden text-[0.78rem]">
          {#each section.items as item, ii}
            <div class="flex items-start gap-3 px-4 py-3">
              <span class="text-base shrink-0 mt-0.5">{rightsEmojis[ii] ?? ""}</span>
              <div class="flex flex-col gap-0.5">
                <span class="font-semibold text-foreground">{item.title}</span>
                <span class="text-muted-foreground leading-relaxed">{item.body}</span>
              </div>
            </div>
          {/each}
        </div>
      </HelpSection>

    {:else if section.title === "Summary"}
      <!-- ── Summary ──────────────────────────────────────────────────────── -->
      <HelpSection title={section.title}>
        <div class="rounded-xl border border-border dark:border-white/[0.06]
                    bg-muted/50 dark:bg-[#0f0f18] px-4 py-4 flex flex-col gap-2
                    text-[0.78rem] text-muted-foreground leading-relaxed">
          {#each section.items as item}
            <p><strong class="text-foreground">{item.title}</strong></p>
          {/each}
        </div>
      </HelpSection>

    {:else if section.items.length === 0}
      <!-- Section with description only (e.g. Overview) -->
      <HelpSection title={section.title}
        description={section.description}>
      </HelpSection>

    {:else}
      <!-- Standard section with HelpItem cards -->
      <HelpSection title={section.title}
        description={section.description}>
        {#each section.items as item}
          <HelpItem id={item.id} title={item.title} body={item.body} />
        {/each}
      </HelpSection>
    {/if}
  {/each}

</div>
