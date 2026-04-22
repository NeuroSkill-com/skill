<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Button } from "$lib/components/ui/button";
import { IconExternalLink } from "$lib/components/ui/icons";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  isMac: boolean;
  screenPermission: boolean | null;
  onOpenSettings: () => void | Promise<void>;
}

let { isMac, screenPermission, onOpenSettings }: Props = $props();
</script>

{#if isMac && screenPermission === false}
  <div class="rounded-xl border border-red-500/30 bg-red-500/5 dark:bg-red-500/10 px-4 py-3 flex flex-col gap-2">
    <div class="flex items-center gap-2">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 shrink-0 text-red-500">
        <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/>
        <line x1="12" y1="9" x2="12" y2="13"/>
        <line x1="12" y1="17" x2="12.01" y2="17"/>
      </svg>
      <span class="text-ui-md font-semibold text-red-600 dark:text-red-400">{t("screenshots.permissionRequired")}</span>
    </div>
    <p class="text-ui-sm text-red-600/80 dark:text-red-400/80 leading-relaxed">{t("screenshots.permissionDesc")}</p>
    <div class="flex gap-2 mt-1">
      <Button size="sm" variant="outline" class="text-ui-sm h-7 px-3" onclick={onOpenSettings}>
        {t("screenshots.openPermissionSettings")}
        <IconExternalLink class="w-3 h-3 ml-1 shrink-0" />
      </Button>
    </div>
  </div>
{:else if isMac && screenPermission === true}
  <div class="rounded-xl border border-green-500/20 bg-green-500/5 dark:bg-green-500/10 px-3 py-2 flex items-center gap-2">
    <span class="w-1.5 h-1.5 rounded-full bg-green-500 shrink-0"></span>
    <span class="text-ui-sm text-green-700 dark:text-green-400">{t("screenshots.permissionGranted")}</span>
  </div>
{/if}
