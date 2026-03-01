<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Main Window / Dashboard -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import HelpItem    from "./HelpItem.svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { t } from "$lib/i18n/index.svelte";

  const sectionKeys = [
    ["helpDash.statusHero",      "helpDash.statusHeroBody"],
    ["helpDash.battery",         "helpDash.batteryBody"],
    ["helpDash.signalQuality",   "helpDash.signalQualityBody"],
    ["helpDash.eegChannelGrid",  "helpDash.eegChannelGridBody"],
    ["helpDash.uptimeSamples",   "helpDash.uptimeSamplesBody"],
    ["helpDash.csvRecording",    "helpDash.csvRecordingBody"],
    ["helpDash.bandPowers",      "helpDash.bandPowersBody"],
    ["helpDash.faa",             "helpDash.faaBody"],
    ["helpDash.eegWaveforms",    "helpDash.eegWaveformsBody"],
    ["helpDash.gpuUtilisation",  "helpDash.gpuUtilisationBody"],
  ] as const;

  const trayStates = [
    { dot: "bg-slate-400",  labelKey: "helpDash.trayGrey",  descKey: "helpDash.trayGreyDesc" },
    { dot: "bg-yellow-400", labelKey: "helpDash.trayAmber", descKey: "helpDash.trayAmberDesc" },
    { dot: "bg-green-500",  labelKey: "helpDash.trayGreen", descKey: "helpDash.trayGreenDesc" },
    { dot: "bg-red-500",    labelKey: "helpDash.trayRed",   descKey: "helpDash.trayRedDesc" },
  ] as const;
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={t("helpDash.mainWindow")}
    description={t("helpDash.mainWindowDesc")}>
    {#each sectionKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpDash.trayIconStates")}>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] divide-y divide-border dark:divide-white/[0.05]
                overflow-hidden text-[0.78rem]">
      {#each trayStates as row}
        <div class="flex items-start gap-3 px-4 py-3">
          <span class="mt-0.5 w-3 h-3 rounded-full shrink-0 {row.dot}"></span>
          <div class="flex flex-col gap-0.5">
            <span class="font-semibold text-foreground">{t(row.labelKey)}</span>
            <span class="text-muted-foreground leading-relaxed">{t(row.descKey)}</span>
          </div>
        </div>
      {/each}
    </div>
  </HelpSection>

</div>
