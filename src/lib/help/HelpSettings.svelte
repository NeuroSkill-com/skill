<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Settings & EEG Model -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import HelpItem    from "./HelpItem.svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { t }         from "$lib/i18n/index.svelte";

  const settingsKeys = [
    ["helpSettings.pairedDevices",    "helpSettings.pairedDevicesBody"],
    ["helpSettings.signalProcessing", "helpSettings.signalProcessingBody"],
    ["helpSettings.eegEmbedding",     "helpSettings.eegEmbeddingBody"],
    ["helpSettings.calibration",      "helpSettings.calibrationBody"],
    ["helpSettings.globalShortcuts",  "helpSettings.globalShortcutsBody"],
    ["helpSettings.debugLogging",     "helpSettings.debugLoggingBody"],
    ["helpSettings.updates",          "helpSettings.updatesBody"],
    ["helpSettings.appearanceTab",    "helpSettings.appearanceTabBody"],
    ["helpSettings.goalsTab",         "helpSettings.goalsTabBody"],
    ["helpSettings.embeddingsTab",    "helpSettings.embeddingsTabBody"],
    ["helpSettings.shortcutsTab",     "helpSettings.shortcutsTabBody"],
    ["helpSettings.umapTab",          "helpSettings.umapTabBody"],
  ] as const;

  const openbciKeys = [
    ["helpSettings.openbciBoard",    "helpSettings.openbciBoardBody"],
    ["helpSettings.openbciGanglion", "helpSettings.openbciGanglionBody"],
    ["helpSettings.openbciSerial",   "helpSettings.openbciSerialBody"],
    ["helpSettings.openbciWifi",     "helpSettings.openbciWifiBody"],
    ["helpSettings.openbciGalea",    "helpSettings.openbciGaleaBody"],
    ["helpSettings.openbciChannels", "helpSettings.openbciChannelsBody"],
  ] as const;

  const eegModelKeys = [
    ["helpSettings.encoderStatus",   "helpSettings.encoderStatusBody"],
    ["helpSettings.embeddingsToday", "helpSettings.embeddingsTodayBody"],
    ["helpSettings.hnswParams",      "helpSettings.hnswParamsBody"],
    ["helpSettings.dataNorm",        "helpSettings.dataNormBody"],
  ] as const;
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={t("helpSettings.settingsTab")}
    description={t("helpSettings.settingsTabDesc")}>
    {#each settingsKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpSettings.openbciSection")}
    description={t("helpSettings.openbciSectionDesc")}>
    {#each openbciKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpSettings.eegModelTab")}
    description={t("helpSettings.eegModelTabDesc")}>
    {#each eegModelKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

</div>
