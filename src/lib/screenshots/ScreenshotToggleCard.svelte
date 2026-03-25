<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
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
  "flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full hover:bg-slate-50 dark:hover:bg-white/[0.02]";
const switchBase = "relative shrink-0 w-8 h-4 rounded-full transition-colors";
const knobBase = "absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform";
</script>

<Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
  <CardContent class="py-0 px-0">
    <button onclick={onToggleEnabled} class={rowClass}>
      <div class="{switchBase} {enabled ? 'bg-violet-500' : 'bg-muted dark:bg-white/[0.08]'}">
        <div class="{knobBase} {enabled ? 'translate-x-4' : 'translate-x-0.5'}"></div>
      </div>
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-[0.72rem] font-semibold text-foreground leading-tight">{t("screenshots.enableToggle")}</span>
        <span class="text-[0.58rem] text-muted-foreground leading-tight">{t("screenshots.enableDesc")}</span>
      </div>
      <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0 {enabled ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {enabled ? t("common.on") : t("common.off")}
      </span>
    </button>

    <Separator class="bg-border dark:bg-white/[0.05]" />

    <button onclick={onToggleSessionOnly} class={rowClass}>
      <div class="{switchBase} {sessionOnly ? 'bg-violet-500' : 'bg-muted dark:bg-white/[0.08]'}">
        <div class="{knobBase} {sessionOnly ? 'translate-x-4' : 'translate-x-0.5'}"></div>
      </div>
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-[0.72rem] font-semibold text-foreground leading-tight">{t("screenshots.sessionOnlyToggle")}</span>
        <span class="text-[0.58rem] text-muted-foreground leading-tight">{t("screenshots.sessionOnlyDesc")}</span>
      </div>
      <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0 {sessionOnly ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {sessionOnly ? t("common.on") : t("common.off")}
      </span>
    </button>

    <Separator class="bg-border dark:bg-white/[0.05]" />

    <button onclick={onToggleOcr} class={rowClass}>
      <div class="{switchBase} {ocrEnabled ? 'bg-violet-500' : 'bg-muted dark:bg-white/[0.08]'}">
        <div class="{knobBase} {ocrEnabled ? 'translate-x-4' : 'translate-x-0.5'}"></div>
      </div>
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-[0.72rem] font-semibold text-foreground leading-tight">{t("screenshots.ocrToggle")}</span>
        <span class="text-[0.58rem] text-muted-foreground leading-tight">{t("screenshots.ocrToggleDesc")}</span>
      </div>
      <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0 {ocrEnabled ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {ocrEnabled ? t("common.on") : t("common.off")}
      </span>
    </button>

    <Separator class="bg-border dark:bg-white/[0.05]" />

    <button onclick={onToggleGpu} class={rowClass}>
      <div class="{switchBase} {useGpu ? 'bg-violet-500' : 'bg-muted dark:bg-white/[0.08]'}">
        <div class="{knobBase} {useGpu ? 'translate-x-4' : 'translate-x-0.5'}"></div>
      </div>
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-[0.72rem] font-semibold text-foreground leading-tight">{t("screenshots.gpuToggle")}</span>
        <span class="text-[0.58rem] text-muted-foreground leading-tight">{t("screenshots.gpuToggleDesc")}</span>
      </div>
      <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0 {useGpu ? 'text-violet-500' : 'text-muted-foreground/50'}">
        {useGpu ? 'GPU' : 'CPU'}
      </span>
    </button>
  </CardContent>
</Card>
