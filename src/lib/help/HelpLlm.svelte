<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: LLM — local inference server, model management, tools, chat -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import HelpItem    from "./HelpItem.svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { t }         from "$lib/i18n/index.svelte";

  const overviewKeys = [
    ["helpLlm.whatIsTitle",          "helpLlm.whatIsBody"],
    ["helpLlm.privacyTitle",        "helpLlm.privacyBody"],
    ["helpLlm.compatTitle",         "helpLlm.compatBody"],
  ] as const;

  const modelKeys = [
    ["helpLlm.catalogTitle",        "helpLlm.catalogBody"],
    ["helpLlm.quantsTitle",         "helpLlm.quantsBody"],
    ["helpLlm.hardwareFitTitle",    "helpLlm.hardwareFitBody"],
    ["helpLlm.visionTitle",         "helpLlm.visionBody"],
    ["helpLlm.downloadTitle",       "helpLlm.downloadBody"],
  ] as const;

  const inferenceKeys = [
    ["helpLlm.gpuLayersTitle",      "helpLlm.gpuLayersBody"],
    ["helpLlm.ctxSizeTitle",        "helpLlm.ctxSizeBody"],
    ["helpLlm.parallelTitle",       "helpLlm.parallelBody"],
    ["helpLlm.apiKeyTitle",         "helpLlm.apiKeyBody"],
  ] as const;

  const toolKeys = [
    ["helpLlm.toolsOverviewTitle",  "helpLlm.toolsOverviewBody"],
    ["helpLlm.toolsSafeTitle",      "helpLlm.toolsSafeBody"],
    ["helpLlm.toolsDangerTitle",    "helpLlm.toolsDangerBody"],
    ["helpLlm.toolsExecModeTitle",  "helpLlm.toolsExecModeBody"],
  ] as const;

  const chatKeys = [
    ["helpLlm.chatWindowTitle",     "helpLlm.chatWindowBody"],
    ["helpLlm.chatApiTitle",        "helpLlm.chatApiBody"],
    ["helpLlm.serverLogsTitle",     "helpLlm.serverLogsBody"],
  ] as const;
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={t("helpLlm.overviewSection")}
    description={t("helpLlm.overviewSectionDesc")}>
    {#each overviewKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpLlm.modelsSection")}
    description={t("helpLlm.modelsSectionDesc")}>
    {#each modelKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpLlm.inferenceSection")}
    description={t("helpLlm.inferenceSectionDesc")}>
    {#each inferenceKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpLlm.toolsSection")}
    description={t("helpLlm.toolsSectionDesc")}>
    {#each toolKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <HelpSection title={t("helpLlm.chatSection")}
    description={t("helpLlm.chatSectionDesc")}>
    {#each chatKeys as [titleKey, bodyKey]}
      <HelpItem id={titleKey} title={t(titleKey)} body={t(bodyKey)} />
    {/each}
  </HelpSection>

</div>
