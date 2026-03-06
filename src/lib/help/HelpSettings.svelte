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

  const activityKeys = [
    ["helpSettings.activeWindowHelp",       "helpSettings.activeWindowHelpBody"],
    ["helpSettings.inputActivityHelp",      "helpSettings.inputActivityHelpBody"],
    ["helpSettings.activityStorageHelp",    "helpSettings.activityStorageHelpBody"],
    ["helpSettings.activityPermissionsHelp","helpSettings.activityPermissionsHelpBody"],
    ["helpSettings.activityDisablingHelp",  "helpSettings.activityDisablingHelpBody"],
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

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Activity Tracking ─────────────────────────────────────────────────── -->
  <HelpSection title={t("helpSettings.activitySection")}
    description={t("helpSettings.activitySectionDesc")}>

    {#each activityKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}

    <!-- Platform permission matrix -->
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] overflow-hidden">
      <div class="px-4 pt-3 pb-2">
        <span class="text-[0.78rem] font-semibold text-foreground">
          Permission matrix
        </span>
      </div>
      <table class="w-full text-[0.72rem] border-t border-border dark:border-white/[0.05]">
        <thead>
          <tr class="divide-x divide-border dark:divide-white/[0.05]
                     bg-muted/40 dark:bg-white/[0.02]">
            <th class="px-3 py-2 text-left font-semibold text-muted-foreground">Feature</th>
            <th class="px-3 py-2 text-center font-semibold text-muted-foreground">macOS</th>
            <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Linux</th>
            <th class="px-3 py-2 text-center font-semibold text-muted-foreground">Windows</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-border dark:divide-white/[0.04]">
          {#each [
            ["Active window (name/path)", "✅ None", "✅ xdotool", "✅ None"],
            ["Window title", "⚠️ May be empty (sandbox)", "✅ xprop", "✅ None"],
            ["Keyboard & mouse timestamps", "🔑 Accessibility", "✅ libxtst", "✅ None"],
          ] as [feat, mac, linux, win]}
            <tr class="divide-x divide-border dark:divide-white/[0.04]">
              <td class="px-3 py-2 text-foreground/80">{feat}</td>
              <td class="px-3 py-2 text-center text-muted-foreground">{mac}</td>
              <td class="px-3 py-2 text-center text-muted-foreground">{linux}</td>
              <td class="px-3 py-2 text-center text-muted-foreground">{win}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="px-4 py-2.5 border-t border-border dark:border-white/[0.05]
                  text-[0.64rem] text-muted-foreground/60 flex gap-3">
        <span>✅ No permission needed</span>
        <span>⚠️ Best-effort</span>
        <span>🔑 OS permission required — degrades silently if absent</span>
      </div>
    </div>

  </HelpSection>

</div>
