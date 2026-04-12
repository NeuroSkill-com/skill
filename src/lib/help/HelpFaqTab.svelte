<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: FAQ -->

<script lang="ts">
import { getLocale } from "$lib/i18n/index.svelte";
import { getFaqContent } from "./help-loader";
import HelpSection from "./HelpSection.svelte";

const entries = $derived(getFaqContent(getLocale()));
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={"Frequently Asked Questions"}>
    <div class="flex flex-col divide-y divide-border dark:divide-white/[0.05]
                rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] overflow-hidden">
      {#each entries as entry}
        <details id={entry.id} class="group px-4 py-3 cursor-pointer scroll-mt-4">
          <summary class="flex items-center justify-between gap-3 text-[0.78rem]
                          font-semibold text-foreground list-none select-none">
            {entry.question}
            <svg class="w-4 h-4 shrink-0 text-muted-foreground transition-transform
                        group-open:rotate-180"
                 viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M6 9l6 6 6-6"/>
            </svg>
          </summary>
          <p class="mt-2 text-[0.75rem] text-muted-foreground leading-relaxed">
            {entry.answer}
          </p>
        </details>
      {/each}
    </div>
  </HelpSection>

</div>

<style>
  details > summary::-webkit-details-marker { display: none; }
</style>
