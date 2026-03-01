<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Windows & Overlays — all text pulled from i18n -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import HelpItem    from "./HelpItem.svelte";
  import { t }       from "$lib/i18n/index.svelte";
</script>

<div class="flex flex-col gap-6 pb-6">

  <HelpSection title={t("helpWindows.title")}
    description={t("helpWindows.desc")}>

    <HelpItem id="helpWindows.labelTitle"    title={t("helpWindows.labelTitle")}    body={t("helpWindows.labelBody")} />

    <!-- Search window: overview + one sub-item per mode -->
    <div id="helpWindows.searchTitle"
         class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-3 scroll-mt-4">
      <div class="flex flex-col gap-1">
        <span class="text-[0.78rem] font-semibold text-foreground">{t("helpWindows.searchTitle")}</span>
        <span class="text-[0.75rem] leading-relaxed text-muted-foreground">{t("helpWindows.searchBody")}</span>
      </div>
      <div class="flex flex-col gap-2 pl-3 border-l-2 border-border dark:border-white/[0.07]">
        <div id="helpWindows.searchEegTitle" class="flex flex-col gap-0.5 scroll-mt-4">
          <span class="text-[0.74rem] font-semibold text-foreground/80">{t("helpWindows.searchEegTitle")}</span>
          <span class="text-[0.72rem] leading-relaxed text-muted-foreground">{t("helpWindows.searchEegBody")}</span>
        </div>
        <div id="helpWindows.searchTextTitle" class="flex flex-col gap-0.5 scroll-mt-4">
          <span class="text-[0.74rem] font-semibold text-foreground/80">{t("helpWindows.searchTextTitle")}</span>
          <span class="text-[0.72rem] leading-relaxed text-muted-foreground">{t("helpWindows.searchTextBody")}</span>
        </div>
        <div id="helpWindows.searchInteractiveTitle" class="flex flex-col gap-0.5 scroll-mt-4">
          <span class="text-[0.74rem] font-semibold text-foreground/80">{t("helpWindows.searchInteractiveTitle")}</span>
          <span class="text-[0.72rem] leading-relaxed text-muted-foreground">{t("helpWindows.searchInteractiveBody")}</span>
        </div>
      </div>
    </div>

    <HelpItem id="helpWindows.calTitle"            title={t("helpWindows.calTitle")}          body={t("helpWindows.calBody")} />
    <HelpItem id="helpWindows.settingsTitle"       title={t("helpWindows.settingsTitle")}     body={t("helpWindows.settingsBody")} />
    <HelpItem id="helpWindows.helpTitle"           title={t("helpWindows.helpTitle")}         body={t("helpWindows.helpBody")} />
    <HelpItem id="helpWindows.onboardingTitle"     title={t("helpWindows.onboardingTitle")}   body={t("helpWindows.onboardingBody")} />
    <HelpItem id="helpWindows.apiTitle"            title={t("helpWindows.apiTitle")}          body={t("helpWindows.apiBody")} />
    <HelpItem id="helpWindows.sleepTitle"          title={t("helpWindows.sleepTitle")}        body={t("helpWindows.sleepBody")} />
    <HelpItem id="helpWindows.compareTitle"        title={t("helpWindows.compareTitle")}      body={t("helpWindows.compareBody")} />

  </HelpSection>

  <HelpSection title={t("helpWindows.overlaysTitle")}
    description={t("helpWindows.overlaysDesc")}>

    <HelpItem id="helpWindows.cmdPaletteTitle"       title={t("helpWindows.cmdPaletteTitle")}       body={t("helpWindows.cmdPaletteBody")} />
    <HelpItem id="helpWindows.shortcutsOverlayTitle" title={t("helpWindows.shortcutsOverlayTitle")} body={t("helpWindows.shortcutsOverlayBody")} />

  </HelpSection>

</div>
