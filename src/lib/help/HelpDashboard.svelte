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

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- Community ──────────────────────────────────────────────────────────── -->
  <HelpSection title={t("helpDash.community")}
    description={t("helpDash.communityDesc")}>
    <a href="https://discord.gg/Rcvb8Cx4cZ"
       target="_blank" rel="noreferrer"
       class="inline-flex items-center gap-2 rounded-xl border border-border
              dark:border-white/[0.06] bg-white dark:bg-[#14141e] px-4 py-3
              text-[0.8rem] font-medium text-primary
              hover:border-primary/30 dark:hover:border-primary/30
              transition-colors w-fit">
      <svg class="w-4 h-4 shrink-0" viewBox="0 0 24 24" fill="currentColor">
        <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028 14.09 14.09 0 0 0 1.226-1.994.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.095 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.095 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z"/>
      </svg>
      {t("helpDash.discordLink")}
    </a>
  </HelpSection>

</div>
