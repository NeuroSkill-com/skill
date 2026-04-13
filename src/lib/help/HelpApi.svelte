<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: WebSocket / LAN API -->

<script lang="ts">
import { Separator } from "$lib/components/ui/separator";
import { getLocale, t } from "$lib/i18n/index.svelte";
import HelpItem from "./HelpItem.svelte";
import HelpSection from "./HelpSection.svelte";
import { getHelpContent } from "./help-loader";

const sections = $derived(getHelpContent("api", getLocale()));
const overviewSection = $derived(sections[0]);
const commandSection = $derived(sections[1]);
</script>

<div class="flex flex-col gap-6 pb-6">

  {#if overviewSection}
  <HelpSection title={overviewSection.title}>
    {#each overviewSection.items as item}
      <HelpItem id={item.id} title={item.title} body={item.body} />
    {/each}
  </HelpSection>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  {#if commandSection}
  <HelpSection title={commandSection.title}>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] divide-y divide-border dark:divide-white/[0.05]
                overflow-hidden text-[0.78rem]">
      {#each commandSection.items as item}
        {@const parts = item.body.split("\n\n")}
        <div class="px-4 py-3 flex flex-col gap-1">
          <span class="font-mono font-semibold text-foreground">{item.title}</span>
          <span class="text-[0.7rem] text-muted-foreground/70">{parts[0] ?? ""}</span>
          <span class="text-muted-foreground leading-relaxed">{parts.slice(1).join("\n\n")}</span>
        </div>
      {/each}
    </div>
  </HelpSection>
  {/if}

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpApi.discoveryWireFormat")}>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-muted/50 dark:bg-[#0f0f18] px-4 py-3 flex flex-col gap-2">
      <p class="text-[0.72rem] font-semibold text-muted-foreground uppercase tracking-widest">
        {t("helpApi.discoverService")}
      </p>
      <pre class="text-[0.72rem] font-mono text-foreground/80 whitespace-pre-wrap"># macOS
dns-sd -B _skill._tcp

# Linux
avahi-browse _skill._tcp</pre>
      <p class="text-[0.72rem] font-semibold text-muted-foreground uppercase tracking-widest mt-1">
        {t("helpApi.outboundEvents")}
      </p>
      <pre class="text-[0.72rem] font-mono text-foreground/80 whitespace-pre-wrap">{`{ "event": "eeg-bands" | "status" | "label-created", "payload": { … } }`}</pre>
      <p class="text-[0.72rem] font-semibold text-muted-foreground uppercase tracking-widest mt-1">
        {t("helpApi.inboundCommands")}
      </p>
      <pre class="text-[0.72rem] font-mono text-foreground/80 whitespace-pre-wrap">{`{ "command": "status" }
{ "command": "label", "text": "eyes closed meditation" }
{ "command": "search", "start_utc": 1700000000, "end_utc": 1700000300 }
{ "command": "compare", "a_start_utc": …, "a_end_utc": …, "b_start_utc": …, "b_end_utc": … }`}</pre>
      <p class="text-[0.72rem] font-semibold text-muted-foreground uppercase tracking-widest mt-1">
        {t("helpApi.response")}
      </p>
      <pre class="text-[0.72rem] font-mono text-foreground/80 whitespace-pre-wrap">{`{ "command": "status", "ok": true, "device": { … }, … }
{ "command": "…", "ok": false, "error": "description" }`}</pre>
    </div>
  </HelpSection>

</div>
