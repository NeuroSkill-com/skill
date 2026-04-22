<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Settings & EEG Model -->

<script lang="ts">
import { Separator } from "$lib/components/ui/separator";
import { getLocale } from "$lib/i18n/index.svelte";
import HelpItem from "./HelpItem.svelte";
import HelpSection from "./HelpSection.svelte";
import { getHelpContent } from "./help-loader";

const sections = $derived(getHelpContent("settings", getLocale()));
</script>

<div class="flex flex-col gap-6 pb-6">

  <!-- Settings Tab (section 0) -->
  {#if sections[0]}
  <HelpSection title={sections[0].title}
    description={sections[0].description}>
    {#each sections[0].items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}
  </HelpSection>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- OpenBCI Boards (section 4) -->
  {#if sections[4]}
  <HelpSection title={sections[4].title}
    description={sections[4].description}>
    {#each sections[4].items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}
  </HelpSection>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- EEG Model Tab (section 3) -->
  {#if sections[3]}
  <HelpSection title={sections[3].title}
    description={sections[3].description}>
    {#each sections[3].items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}
  </HelpSection>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Activity Tracking (section 1) ────────────────────────────────────── -->
  {#if sections[1]}
  <HelpSection title={sections[1].title}
    description={sections[1].description}>

    {#each sections[1].items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}

    <!-- Platform permission matrix -->
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-surface-1 overflow-hidden">
      <div class="px-4 pt-3 pb-2">
        <span class="text-ui-lg font-semibold text-foreground">
          Permission matrix
        </span>
      </div>
      <table class="w-full text-ui-md border-t border-border dark:border-white/[0.06]">
        <thead>
          <tr class="divide-x divide-border dark:divide-white/[0.05]
                     bg-muted/40 dark:bg-white/[0.02]">
            <th class="px-3 py-2 text-left font-semibold text-muted-foreground">Feature</th>
            <th class="px-3 py-2 text-center font-semibold text-muted-foreground">macOS</th>
            <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Linux</th>
            <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Windows</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-border dark:divide-white/[0.05]">
          {#each [
            ["Active window (name/path)", "✅ None", "✅ xdotool", "✅ None"],
            ["Window title", "⚠️ May be empty (sandbox)", "✅ xprop", "✅ None"],
            ["Keyboard & mouse timestamps", "🔑 Accessibility", "✅ libxtst", "✅ None"],
          ] as [feat, mac, linux, win]}
            <tr class="divide-x divide-border dark:divide-white/[0.05]">
              <td class="px-3 py-2 text-foreground/80">{feat}</td>
              <td class="px-3 py-2 text-center text-muted-foreground">{mac}</td>
              <td class="px-3 py-2 text-center text-muted-foreground">{linux}</td>
              <td class="px-3 py-2 text-center text-muted-foreground">{win}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="px-4 py-2.5 border-t border-border dark:border-white/[0.06]
                  text-ui-base text-muted-foreground/60 flex gap-3">
        <span>✅ No permission needed</span>
        <span>⚠️ Best-effort</span>
        <span>🔑 OS permission required — degrades silently if absent</span>
      </div>
    </div>

  </HelpSection>
  {/if}

</div>
