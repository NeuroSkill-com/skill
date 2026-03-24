<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: WebSocket / LAN API -->

<script lang="ts">
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";
import HelpItem from "./HelpItem.svelte";
import HelpSection from "./HelpSection.svelte";

const overviewKeys = [
  ["helpApi.liveStreaming", "helpApi.liveStreamingBody"],
  ["helpApi.commands", "helpApi.commandsBody"],
] as const;

const commandKeys = [
  ["helpApi.cmdStatus", "helpApi.cmdStatusParams", "helpApi.cmdStatusDesc"],
  ["helpApi.cmdSay", "helpApi.cmdSayParams", "helpApi.cmdSayDesc"],
  ["helpApi.cmdCalibrate", "helpApi.cmdCalibrateParams", "helpApi.cmdCalibrateDesc"],
  ["helpApi.cmdLabel", "helpApi.cmdLabelParams", "helpApi.cmdLabelDesc"],
  ["helpApi.cmdSearch", "helpApi.cmdSearchParams", "helpApi.cmdSearchDesc"],
  ["helpApi.cmdSessions", "helpApi.cmdSessionsParams", "helpApi.cmdSessionsDesc"],
  ["helpApi.cmdCompare", "helpApi.cmdCompareParams", "helpApi.cmdCompareDesc"],
  ["helpApi.cmdSleep", "helpApi.cmdSleepParams", "helpApi.cmdSleepDesc"],
  ["helpApi.cmdUmap", "helpApi.cmdUmapParams", "helpApi.cmdUmapDesc"],
  ["helpApi.cmdUmapPoll", "helpApi.cmdUmapPollParams", "helpApi.cmdUmapPollDesc"],
] as const;
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={t("helpApi.overview")}>
    {#each overviewKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpApi.commandReference")}>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] divide-y divide-border dark:divide-white/[0.05]
                overflow-hidden text-[0.78rem]">
      {#each commandKeys as [nameKey, paramsKey, descKey]}
        <div class="px-4 py-3 flex flex-col gap-1">
          <span class="font-mono font-semibold text-foreground">{t(nameKey)}</span>
          <span class="text-[0.7rem] text-muted-foreground/70">{t(paramsKey)}</span>
          <span class="text-muted-foreground leading-relaxed">{t(descKey)}</span>
        </div>
      {/each}
    </div>
  </HelpSection>

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
