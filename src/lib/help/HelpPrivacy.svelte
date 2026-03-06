<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Help tab: Security & Privacy Policy -->

<script lang="ts">
  import HelpSection from "./HelpSection.svelte";
  import HelpItem    from "./HelpItem.svelte";
  import { Separator } from "$lib/components/ui/separator";
  import { t } from "$lib/i18n/index.svelte";
</script>

<div class="flex flex-col gap-6 pb-6">

  <!-- ── Overview ─────────────────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.overview")}
    description={t("helpPrivacy.overviewDesc")}>
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Data storage ─────────────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.dataStorage")}>
    <HelpItem id="helpPrivacy.allLocal" title={t("helpPrivacy.allLocal")} body={t("helpPrivacy.allLocalBody")} />
    <HelpItem id="helpPrivacy.noAccounts" title={t("helpPrivacy.noAccounts")} body={t("helpPrivacy.noAccountsBody")} />
    <HelpItem id="helpPrivacy.dataLocation" title={t("helpPrivacy.dataLocation")} body={t("helpPrivacy.dataLocationBody")} />
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Network ──────────────────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.network")}>
    <HelpItem id="helpPrivacy.noTelemetry" title={t("helpPrivacy.noTelemetry")} body={t("helpPrivacy.noTelemetryBody")} />
    <HelpItem id="helpPrivacy.localWs" title={t("helpPrivacy.localWs")} body={t("helpPrivacy.localWsBody")} />
    <HelpItem id="helpPrivacy.mdns" title={t("helpPrivacy.mdns")} body={t("helpPrivacy.mdnsBody")} />
    <HelpItem id="helpPrivacy.updateChecks" title={t("helpPrivacy.updateChecks")} body={t("helpPrivacy.updateChecksBody")} />
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Bluetooth ────────────────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.bluetooth")}>
    <HelpItem id="helpPrivacy.ble" title={t("helpPrivacy.ble")} body={t("helpPrivacy.bleBody")} />
    <HelpItem id="helpPrivacy.osPermissions" title={t("helpPrivacy.osPermissions")} body={t("helpPrivacy.osPermissionsBody")} />
    <HelpItem id="helpPrivacy.deviceIds" title={t("helpPrivacy.deviceIds")} body={t("helpPrivacy.deviceIdsBody")} />
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Activity Tracking ───────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.activityTracking")}>
    <HelpItem id="helpPrivacy.activityTracking" title={t("helpPrivacy.activityTracking")} body={t("helpPrivacy.activityTrackingBody")} />
    <HelpItem id="helpPrivacy.activityPermission" title={t("helpPrivacy.activityPermission")} body={t("helpPrivacy.activityPermissionBody")} />
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── On-device processing ─────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.onDevice")}>
    <HelpItem id="helpPrivacy.gpuLocal" title={t("helpPrivacy.gpuLocal")} body={t("helpPrivacy.gpuLocalBody")} />
    <HelpItem id="helpPrivacy.filtering" title={t("helpPrivacy.filtering")} body={t("helpPrivacy.filteringBody")} />
    <HelpItem id="helpPrivacy.nnSearch" title={t("helpPrivacy.nnSearch")} body={t("helpPrivacy.nnSearchBody")} />
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Your rights ──────────────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.yourData")}>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-white dark:bg-[#14141e] divide-y divide-border dark:divide-white/[0.05]
                overflow-hidden text-[0.78rem]">
      {#each [
        { icon: "🗂", labelKey: "helpPrivacy.access",  descKey: "helpPrivacy.accessBody" },
        { icon: "🗑", labelKey: "helpPrivacy.delete",  descKey: "helpPrivacy.deleteBody" },
        { icon: "📦", labelKey: "helpPrivacy.export",  descKey: "helpPrivacy.exportBody" },
        { icon: "🔒", labelKey: "helpPrivacy.encrypt", descKey: "helpPrivacy.encryptBody" },
      ] as row}
        <div class="flex items-start gap-3 px-4 py-3">
          <span class="text-base shrink-0 mt-0.5">{row.icon}</span>
          <div class="flex flex-col gap-0.5">
            <span class="font-semibold text-foreground">{t(row.labelKey)}</span>
            <span class="text-muted-foreground leading-relaxed">{t(row.descKey)}</span>
          </div>
        </div>
      {/each}
    </div>
  </HelpSection>

  <Separator class="bg-border dark:bg-white/[0.06]" />

  <!-- ── Summary ──────────────────────────────────────────────────────────── -->
  <HelpSection title={t("helpPrivacy.summaryTitle")}>
    <div class="rounded-xl border border-border dark:border-white/[0.06]
                bg-muted/50 dark:bg-[#0f0f18] px-4 py-4 flex flex-col gap-2
                text-[0.78rem] text-muted-foreground leading-relaxed">
      <p><strong class="text-foreground">{t("helpPrivacy.summaryNoCloud")}</strong></p>
      <p><strong class="text-foreground">{t("helpPrivacy.summaryNoTelemetry")}</strong></p>
      <p><strong class="text-foreground">{t("helpPrivacy.summaryNoAccounts")}</strong></p>
      <p><strong class="text-foreground">{t("helpPrivacy.summaryOneReq")}</strong></p>
      <p><strong class="text-foreground">{t("helpPrivacy.summaryOnDevice")}</strong></p>
      <p><strong class="text-foreground">{t("helpPrivacy.summaryActivityLocal")}</strong></p>
    </div>
  </HelpSection>

</div>
