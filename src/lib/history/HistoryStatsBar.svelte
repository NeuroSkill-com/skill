<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- History stats bar — recording streak, session counts, week trend. -->
<script lang="ts">
import type { HistoryStatsData } from "$lib/history/history-helpers";
import { t } from "$lib/i18n/index.svelte";

interface WeekTrend {
  thisWeek: number;
  lastWeek: number;
  pctChange: number;
}

interface Props {
  daysCount: number;
  totalHours: number;
  recordingStreak: number;
  historyStats: HistoryStatsData | null;
  weekTrend: WeekTrend | null;
}

let { daysCount, totalHours, recordingStreak, historyStats, weekTrend }: Props = $props();
</script>

{#if recordingStreak > 0}
  <div class="rounded-2xl border border-border dark:border-white/[0.06]
              bg-gradient-to-r from-orange-500/10 via-amber-500/10 to-yellow-500/10
              dark:from-orange-500/15 dark:via-amber-500/15 dark:to-yellow-500/15
              px-5 py-4 flex items-center gap-4">
    <div class="flex items-center justify-center w-12 h-12 rounded-xl
                bg-gradient-to-br from-orange-500 to-amber-400 shadow-lg shadow-orange-500/25
                text-white text-xl shrink-0">🔥</div>
    <div class="flex flex-col gap-0.5 flex-1">
      <span class="text-ui-md font-bold text-foreground">
        {t("history.streakDays", { n: recordingStreak })}
      </span>
      <span class="text-ui-sm text-muted-foreground/70">
        {recordingStreak >= 7 ? t("history.streakAmazing") :
         recordingStreak >= 3 ? t("history.streakKeepGoing") :
         t("history.streakGreatStart")}
      </span>
    </div>
    <div class="flex items-center gap-5">
      <div class="flex flex-col items-center">
        <span class="text-ui-lg font-bold tabular-nums">{daysCount}</span>
        <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.days")}</span>
      </div>
      {#if historyStats}
        <div class="flex flex-col items-center">
          <span class="text-ui-lg font-bold tabular-nums">{totalHours.toFixed(1)}</span>
          <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.hours")}</span>
        </div>
        <div class="flex flex-col items-center">
          <span class="text-ui-lg font-bold tabular-nums">{historyStats.total_sessions}</span>
          <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.sessions")}</span>
        </div>
        {#if weekTrend && (weekTrend.thisWeek > 0 || weekTrend.lastWeek > 0)}
          <div class="flex flex-col items-center">
            <div class="flex items-center gap-0.5">
              <span class="text-ui-lg font-bold tabular-nums">{weekTrend.thisWeek.toFixed(1)}</span>
              {#if weekTrend.lastWeek > 0}
                <span class="text-ui-xs font-semibold
                             {weekTrend.pctChange > 0 ? 'text-emerald-500' : weekTrend.pctChange < -10 ? 'text-red-400' : 'text-muted-foreground/60'}">
                  {weekTrend.pctChange > 0 ? "↑" : weekTrend.pctChange < 0 ? "↓" : "→"}{Math.abs(weekTrend.pctChange).toFixed(0)}%
                </span>
              {/if}
            </div>
            <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">{t("history.thisWeek")}</span>
          </div>
        {/if}
      {/if}
    </div>
  </div>
{:else}
  <!-- Flat stats row (no streak) -->
  <div class="flex items-center gap-4 mb-1 px-1">
    <div class="flex items-center gap-1">
      <span class="text-ui-sm font-bold tabular-nums">{daysCount}</span>
      <span class="text-ui-xs text-muted-foreground">{t("history.days")}</span>
    </div>
    {#if historyStats}
      <div class="flex items-center gap-1">
        <span class="text-ui-sm font-bold tabular-nums">{totalHours.toFixed(1)}</span>
        <span class="text-ui-xs text-muted-foreground">{t("history.hoursTotal")}</span>
      </div>
      <div class="flex items-center gap-1">
        <span class="text-ui-sm font-bold tabular-nums">{historyStats.total_sessions}</span>
        <span class="text-ui-xs text-muted-foreground">{t("history.sessions")}</span>
      </div>
    {/if}
  </div>
{/if}
