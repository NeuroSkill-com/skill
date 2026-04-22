<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { ToggleSwitch } from "$lib/components/ui/toggle-switch";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  enabled: boolean;
  sessionOnly: boolean;
  ocrEnabled: boolean;
  useGpu: boolean;
  onToggleEnabled: () => void | Promise<void>;
  onToggleSessionOnly: () => void | Promise<void>;
  onToggleOcr: () => void | Promise<void>;
  onToggleGpu: () => void | Promise<void>;
}

let {
  enabled,
  sessionOnly,
  ocrEnabled,
  useGpu,
  onToggleEnabled,
  onToggleSessionOnly,
  onToggleOcr,
  onToggleGpu,
}: Props = $props();

const rowClass =
  "flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full hover:bg-accent/50 dark:hover:bg-white/[0.02]";
</script>

<Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
  <CardContent class="py-0 px-0">
    <div class={rowClass}>
      <ToggleSwitch checked={enabled} label={t("screenshots.enableToggle")} ontoggle={onToggleEnabled} />
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-ui-md font-semibold text-foreground leading-tight">{t("screenshots.enableToggle")}</span>
        <span class="text-ui-sm text-muted-foreground leading-tight">{t("screenshots.enableDesc")}</span>
      </div>
      <span class="ml-auto text-ui-xs font-bold tracking-widest uppercase shrink-0 {enabled ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {enabled ? t("common.on") : t("common.off")}
      </span>
    </div>

    <Separator class="bg-border dark:bg-white/[0.04]" />

    <div class={rowClass}>
      <ToggleSwitch checked={sessionOnly} label={t("screenshots.sessionOnlyToggle")} ontoggle={onToggleSessionOnly} />
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-ui-md font-semibold text-foreground leading-tight">{t("screenshots.sessionOnlyToggle")}</span>
        <span class="text-ui-sm text-muted-foreground leading-tight">{t("screenshots.sessionOnlyDesc")}</span>
      </div>
      <span class="ml-auto text-ui-xs font-bold tracking-widest uppercase shrink-0 {sessionOnly ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {sessionOnly ? t("common.on") : t("common.off")}
      </span>
    </div>

    <Separator class="bg-border dark:bg-white/[0.04]" />

    <div class={rowClass}>
      <ToggleSwitch checked={ocrEnabled} label={t("screenshots.ocrToggle")} ontoggle={onToggleOcr} />
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-ui-md font-semibold text-foreground leading-tight">{t("screenshots.ocrToggle")}</span>
        <span class="text-ui-sm text-muted-foreground leading-tight">{t("screenshots.ocrToggleDesc")}</span>
      </div>
      <span class="ml-auto text-ui-xs font-bold tracking-widest uppercase shrink-0 {ocrEnabled ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {ocrEnabled ? t("common.on") : t("common.off")}
      </span>
    </div>

    <Separator class="bg-border dark:bg-white/[0.04]" />

    <div class={rowClass}>
      <ToggleSwitch checked={useGpu} label={t("screenshots.gpuToggle")} ontoggle={onToggleGpu} />
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-ui-md font-semibold text-foreground leading-tight">{t("screenshots.gpuToggle")}</span>
        <span class="text-ui-sm text-muted-foreground leading-tight">{t("screenshots.gpuToggleDesc")}</span>
      </div>
      <span class="ml-auto text-ui-xs font-bold tracking-widest uppercase shrink-0 {useGpu ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {useGpu ? 'GPU' : 'CPU'}
      </span>
    </div>
  </CardContent>
</Card>
