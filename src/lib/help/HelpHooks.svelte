<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Proactive Hooks — EEG-driven automatic triggers -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import HelpItem    from "./HelpItem.svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { t }         from "$lib/i18n/index.svelte";

  const overviewKeys = [
    ["helpHooks.whatIsTitle",        "helpHooks.whatIsBody"],
    ["helpHooks.howItWorksTitle",    "helpHooks.howItWorksBody"],
    ["helpHooks.scenariosTitle",     "helpHooks.scenariosBody"],
  ] as const;

  const configKeys = [
    ["helpHooks.nameTitle",          "helpHooks.nameBody"],
    ["helpHooks.keywordsTitle",      "helpHooks.keywordsBody"],
    ["helpHooks.keywordSugTitle",    "helpHooks.keywordSugBody"],
    ["helpHooks.distanceTitle",      "helpHooks.distanceBody"],
    ["helpHooks.distanceSugTitle",   "helpHooks.distanceSugBody"],
    ["helpHooks.recentLimitTitle",   "helpHooks.recentLimitBody"],
    ["helpHooks.commandTitle",       "helpHooks.commandBody"],
    ["helpHooks.textTitle",          "helpHooks.textBody"],
  ] as const;

  const advancedKeys = [
    ["helpHooks.examplesTitle",      "helpHooks.examplesBody"],
    ["helpHooks.historyTitle",       "helpHooks.historyBody"],
    ["helpHooks.wsEventsTitle",      "helpHooks.wsEventsBody"],
    ["helpHooks.tipsTitle",          "helpHooks.tipsBody"],
  ] as const;
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={t("helpHooks.overviewSection")}
    description={t("helpHooks.overviewSectionDesc")}>
    {#each overviewKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpHooks.configSection")}
    description={t("helpHooks.configSectionDesc")}>
    {#each configKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpHooks.advancedSection")}
    description={t("helpHooks.advancedSectionDesc")}>
    {#each advancedKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

</div>
