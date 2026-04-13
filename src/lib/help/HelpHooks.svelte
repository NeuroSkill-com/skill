<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Proactive Hooks — EEG-driven automatic triggers -->

<script lang="ts">
import { Separator } from "$lib/components/ui/separator";
import { getLocale } from "$lib/i18n/index.svelte";
import HelpItem from "./HelpItem.svelte";
import HelpSection from "./HelpSection.svelte";
import { getHelpContent } from "./help-loader";

const sections = $derived(getHelpContent("hooks", getLocale()));
</script>

<div class="flex flex-col gap-6 pb-6">

  {#if sections[0]}
  <HelpSection title={sections[0].title}
    description={sections[0].description}>
    {#each sections[0].items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}
  </HelpSection>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  {#if sections[1]}
  <HelpSection title={sections[1].title}
    description={sections[1].description}>
    {#each sections[1].items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}
  </HelpSection>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  {#if sections[2]}
  <HelpSection title={sections[2].title}
    description={sections[2].description}>
    {#each sections[2].items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}
  </HelpSection>
  {/if}

</div>
