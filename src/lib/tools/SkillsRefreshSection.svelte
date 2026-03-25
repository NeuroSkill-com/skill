<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Card, CardContent } from "$lib/components/ui/card";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  skillsRefreshInterval: number;
  skillsSyncOnLaunch: boolean;
  skillsSyncing: boolean;
  skillsLastSync: number | null;
  formatLastSync: (ts: number | null) => string;
  onSetSkillsInterval: (secs: number) => void;
  onToggleSyncOnLaunch: () => void | Promise<void>;
  onSyncNow: () => void | Promise<void>;
}

let {
  skillsRefreshInterval,
  skillsSyncOnLaunch,
  skillsSyncing,
  skillsLastSync,
  formatLastSync,
  onSetSkillsInterval,
  onToggleSyncOnLaunch,
  onSyncNow,
}: Props = $props();
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("llm.tools.skillsRefresh")}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col py-0 px-0">
      <div class="px-4 pt-3.5 pb-2">
        <p class="text-[0.65rem] text-muted-foreground leading-relaxed">
          {t("llm.tools.skillsRefreshDesc")}
        </p>
      </div>

      <div class="flex flex-col gap-3 px-4 pb-3">
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground">{t("llm.tools.skillsRefresh")}</span>
          </div>
          <div class="flex items-center gap-1">
            {#each [
              { secs: 0,      label: t("llm.tools.skillsRefreshOff") },
              { secs: 43200,  label: t("llm.tools.skillsRefresh12h") },
              { secs: 86400,  label: t("llm.tools.skillsRefresh24h") },
              { secs: 604800, label: t("llm.tools.skillsRefresh7d") },
            ] as opt}
              <button
                onclick={() => onSetSkillsInterval(opt.secs)}
                class="rounded-lg border px-2 py-1 text-[0.64rem] font-semibold transition-all cursor-pointer
                       {skillsRefreshInterval === opt.secs
                         ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                         : 'border-border bg-background text-muted-foreground hover:text-foreground'}">
                {opt.label}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.72rem] font-semibold text-foreground">{t("llm.tools.skillsSyncOnLaunch")}</span>
            <span class="text-[0.6rem] text-muted-foreground">{t("llm.tools.skillsSyncOnLaunchDesc")}</span>
          </div>
          <button role="switch" aria-checked={skillsSyncOnLaunch} aria-label={t("llm.tools.skillsSyncOnLaunch")}
            onclick={onToggleSyncOnLaunch}
            class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                   border-transparent transition-colors duration-200
                   {skillsSyncOnLaunch ? 'bg-violet-500' : 'bg-muted dark:bg-white/10'}">
            <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                          transform transition-transform duration-200
                          {skillsSyncOnLaunch ? 'translate-x-4' : 'translate-x-0'}"></span>
          </button>
        </div>

        <div class="flex items-center justify-between gap-4 pt-1">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.62rem] text-muted-foreground">
              {t("llm.tools.skillsLastSync")}: {formatLastSync(skillsLastSync)}
            </span>
          </div>
          <button
            onclick={onSyncNow}
            disabled={skillsSyncing}
            class="rounded-lg border border-border px-3 py-1.5 text-[0.64rem] font-semibold
                   transition-all cursor-pointer bg-background text-foreground
                   hover:bg-muted disabled:opacity-50 disabled:cursor-not-allowed">
            {skillsSyncing ? t("llm.tools.skillsSyncing") : t("llm.tools.skillsSyncNow")}
          </button>
        </div>
      </div>
    </CardContent>
  </Card>
</section>
