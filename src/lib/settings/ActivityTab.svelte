<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Activity dashboard — daily stats, top files/projects, focus sessions, meetings. -->
<script lang="ts">
import { onMount } from "svelte";
import { CardContent } from "$lib/components/ui/card";
import SectionHeader from "$lib/components/ui/section-header/SectionHeader.svelte";
import SettingsCard from "$lib/components/ui/settings-card/SettingsCard.svelte";
import { Spinner } from "$lib/components/ui/spinner";
import {
  type DailySummaryRow,
  type FileInteractionRow,
  type FocusSessionRow,
  getDailySummary,
  getFocusSessions,
  getHourlyHeatmap,
  getLanguageBreakdown,
  getMeetingsInRange,
  getProductivityScore,
  getStaleFiles,
  getTopFiles,
  getTopProjects,
  type HourlyEditRow,
  type LanguageBreakdownRow,
  type MeetingEventRow,
  type ProductivityScore,
  type ProjectUsageRow,
  type StaleFileRow,
} from "$lib/daemon/settings";
import { t } from "$lib/i18n/index.svelte";

// ── State ─────────────────────────────────────────────────────────────────
let loading = $state(true);
let todayStart = $state(0);

let summary = $state<DailySummaryRow | null>(null);
let score = $state<ProductivityScore | null>(null);
let topFiles = $state<{ file_path: string; interactions: number; total_secs: number }[]>([]);
let topProjects = $state<ProjectUsageRow[]>([]);
let languages = $state<LanguageBreakdownRow[]>([]);
let heatmap = $state<HourlyEditRow[]>([]);
let sessions = $state<FocusSessionRow[]>([]);
let meetings = $state<MeetingEventRow[]>([]);
let staleFiles = $state<StaleFileRow[]>([]);

function todayUnix(): number {
  const now = new Date();
  return Math.floor(new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() / 1000);
}

function fmtDuration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

function fmtTime(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

onMount(async () => {
  todayStart = todayUnix();
  const dayEnd = todayStart + 86400;
  const weekAgo = todayStart - 7 * 86400;

  try {
    const results = await Promise.allSettled([
      getDailySummary(todayStart),
      getProductivityScore(todayStart),
      getTopFiles(10, weekAgo),
      getTopProjects(10, weekAgo),
      getLanguageBreakdown(weekAgo),
      getHourlyHeatmap(weekAgo),
      getFocusSessions(20, weekAgo),
      getMeetingsInRange(weekAgo, dayEnd),
      getStaleFiles(weekAgo),
    ]);

    if (results[0].status === "fulfilled") summary = results[0].value;
    if (results[1].status === "fulfilled") score = results[1].value;
    if (results[2].status === "fulfilled") topFiles = results[2].value;
    if (results[3].status === "fulfilled") topProjects = results[3].value;
    if (results[4].status === "fulfilled") languages = results[4].value;
    if (results[5].status === "fulfilled") heatmap = results[5].value;
    if (results[6].status === "fulfilled") sessions = results[6].value;
    if (results[7].status === "fulfilled") meetings = results[7].value;
    if (results[8].status === "fulfilled") staleFiles = results[8].value;
  } catch {}
  loading = false;
});

// Max value in heatmap for scaling.
$effect(() => {
  heatmapMax = Math.max(1, ...heatmap.map((h) => h.total_churn));
});
let heatmapMax = $state(1);
</script>

{#if loading}
  <div class="flex items-center justify-center py-12">
    <Spinner size="w-5 h-5" class="text-muted-foreground/40" />
  </div>
{:else}
  <!-- ── Today's Summary ──────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("activity.todaySummary")}</SectionHeader>
    <SettingsCard>
      <CardContent class="py-3">
        {#if summary}
          {@const stats: [string, number][] = [
            [t("activity.filesWorked"), summary.distinct_files],
            [t("activity.edits"), summary.edits],
            [t("activity.linesChanged"), summary.lines_added + summary.lines_removed],
            [t("activity.projects"), summary.distinct_projects],
          ]}
          <div class="grid grid-cols-4 gap-3">
            {#each stats as [label, value]}
              <div class="flex flex-col items-center gap-0.5">
                <span class="text-ui-lg font-bold tabular-nums text-foreground">{value.toLocaleString()}</span>
                <span class="text-ui-2xs text-muted-foreground/50">{label}</span>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-ui-sm text-muted-foreground/50 text-center">{t("activity.noData")}</p>
        {/if}
      </CardContent>
    </SettingsCard>
  </section>

  <!-- ── Productivity Score ────────────────────────────────────────────────── -->
  {#if score && score.score > 0}
    {@const components: [string, number, number][] = [
      [t("activity.editVelocity"), score.edit_velocity, 25],
      [t("activity.deepWork"), score.deep_work, 25],
      [t("activity.contextStability"), score.context_stability, 25],
      [t("activity.eegFocus"), score.eeg_focus, 25],
    ]}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.productivityScore")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-3">
          <div class="flex items-center gap-4">
            <div class="flex flex-col items-center">
              <span class="text-2xl font-bold tabular-nums
                {score.score >= 70 ? 'text-emerald-500' : score.score >= 40 ? 'text-amber-500' : 'text-red-400'}">
                {score.score.toFixed(0)}
              </span>
              <span class="text-ui-2xs text-muted-foreground/50">/100</span>
            </div>
            <div class="flex-1 grid grid-cols-2 gap-x-4 gap-y-1.5 text-ui-sm">
              {#each components as [label, val, max]}
                <div class="flex items-center gap-2">
                  <span class="text-muted-foreground/60 text-ui-xs w-24 shrink-0 truncate">{label}</span>
                  <div class="flex-1 h-1.5 rounded-full bg-muted/40 overflow-hidden">
                    <div class="h-full rounded-full bg-violet-500/70" style="width:{(val / max * 100).toFixed(0)}%"></div>
                  </div>
                  <span class="text-ui-2xs tabular-nums text-muted-foreground/40 w-5 text-right">{val.toFixed(0)}</span>
                </div>
              {/each}
            </div>
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

  <!-- ── Hourly Heatmap ───────────────────────────────────────────────────── -->
  {#if heatmap.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.hourlyHeatmap")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-3">
          <div class="flex items-end gap-px h-16">
            {#each Array(24) as _, h}
              {@const entry = heatmap.find((e) => e.hour === h)}
              {@const val = entry?.total_churn ?? 0}
              {@const pct = (val / heatmapMax) * 100}
              <div class="flex-1 flex flex-col items-center gap-0.5">
                <div class="w-full rounded-t-sm transition-all"
                     style="height:{Math.max(2, pct)}%;background:rgba(139,92,246,{0.15 + 0.85 * (val / heatmapMax)})"
                     title="{h}:00 — {val} lines"></div>
              </div>
            {/each}
          </div>
          <div class="flex justify-between text-[0.4rem] text-muted-foreground/30 mt-1 px-0.5">
            <span>0</span><span>6</span><span>12</span><span>18</span><span>23</span>
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

  <!-- ── Top Files ────────────────────────────────────────────────────────── -->
  {#if topFiles.length > 0}
    <section class="flex flex-col gap-2">
      <div class="flex items-center gap-2 px-0.5">
        <SectionHeader>{t("activity.topFiles")}</SectionHeader>
        <span class="text-ui-2xs text-muted-foreground/40 ml-auto">{t("activity.last7days")}</span>
      </div>
      <SettingsCard>
        <CardContent class="py-0 px-0">
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each topFiles.slice(0, 8) as file}
              {@const basename = file.file_path.split("/").pop() ?? file.file_path}
              <div class="flex items-center gap-2 px-3 py-1.5 text-ui-sm">
                <span class="font-mono text-foreground truncate min-w-0 flex-1" title={file.file_path}>{basename}</span>
                <span class="text-ui-2xs tabular-nums text-muted-foreground/40 shrink-0">{file.interactions}x</span>
                <span class="text-ui-2xs tabular-nums text-muted-foreground/40 shrink-0">{fmtDuration(file.total_secs)}</span>
              </div>
            {/each}
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

  <!-- ── Top Projects ─────────────────────────────────────────────────────── -->
  {#if topProjects.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.topProjects")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-0 px-0">
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each topProjects.slice(0, 6) as proj}
              <div class="flex items-center gap-2 px-3 py-1.5 text-ui-sm">
                <span class="text-foreground truncate min-w-0 flex-1">{proj.project}</span>
                <span class="text-ui-2xs tabular-nums text-muted-foreground/40 shrink-0">{proj.interactions}x</span>
              </div>
            {/each}
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

  <!-- ── Language Breakdown ───────────────────────────────────────────────── -->
  {#if languages.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.languages")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-3">
          {@const maxLang = Math.max(1, ...languages.map((l) => l.interactions))}
          <div class="flex flex-col gap-1.5">
            {#each languages.slice(0, 8) as lang}
              <div class="flex items-center gap-2 text-ui-sm">
                <span class="w-16 text-muted-foreground/60 truncate text-ui-xs shrink-0">{lang.language}</span>
                <div class="flex-1 h-1.5 rounded-full bg-muted/40 overflow-hidden">
                  <div class="h-full rounded-full bg-sky-500/60" style="width:{(lang.interactions / maxLang * 100).toFixed(0)}%"></div>
                </div>
                <span class="text-ui-2xs tabular-nums text-muted-foreground/40 w-8 text-right shrink-0">{lang.interactions}</span>
              </div>
            {/each}
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

  <!-- ── Focus Sessions ───────────────────────────────────────────────────── -->
  {#if sessions.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.focusSessions")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-0 px-0">
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each sessions.slice(0, 8) as sess}
              <div class="flex items-center gap-2 px-3 py-2 text-ui-sm">
                <span class="text-foreground tabular-nums shrink-0">{fmtTime(sess.start_at)}</span>
                <span class="text-muted-foreground/30">-</span>
                <span class="text-foreground tabular-nums shrink-0">{fmtTime(sess.end_at)}</span>
                <span class="text-ui-xs text-muted-foreground/50 shrink-0">{fmtDuration(sess.end_at - sess.start_at)}</span>
                {#if sess.project}
                  <span class="text-ui-xs text-violet-500/70 truncate min-w-0 flex-1">{sess.project}</span>
                {:else}
                  <span class="flex-1"></span>
                {/if}
                <span class="text-ui-2xs tabular-nums text-muted-foreground/40 shrink-0">{sess.file_count} {t("activity.filesShort")}</span>
                {#if sess.avg_eeg_focus != null}
                  <span class="text-ui-2xs tabular-nums shrink-0
                    {sess.avg_eeg_focus >= 70 ? 'text-emerald-500' : sess.avg_eeg_focus >= 40 ? 'text-amber-400' : 'text-red-400'}">
                    {sess.avg_eeg_focus.toFixed(0)}
                  </span>
                {/if}
              </div>
            {/each}
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

  <!-- ── Meetings ─────────────────────────────────────────────────────────── -->
  {#if meetings.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.meetings")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-0 px-0">
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each meetings.slice(0, 8) as mtg}
              <div class="flex items-center gap-2 px-3 py-1.5 text-ui-sm">
                <span class="inline-flex items-center gap-1 rounded-full bg-amber-500/10 border border-amber-500/20
                             px-2 py-0.5 text-ui-2xs text-amber-600 dark:text-amber-400 shrink-0">
                  {mtg.platform}
                </span>
                <span class="text-foreground tabular-nums shrink-0">{fmtTime(mtg.start_at)}</span>
                {#if mtg.end_at}
                  <span class="text-muted-foreground/40 tabular-nums shrink-0">{fmtDuration(mtg.end_at - mtg.start_at)}</span>
                {:else}
                  <span class="text-amber-500/60 text-ui-xs">{t("activity.meetingOngoing")}</span>
                {/if}
                <span class="text-muted-foreground/40 text-ui-xs truncate min-w-0 flex-1">{mtg.title}</span>
              </div>
            {/each}
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

  <!-- ── Stale Files ──────────────────────────────────────────────────────── -->
  {#if staleFiles.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.staleFiles")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-0 px-0">
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each staleFiles.slice(0, 5) as sf}
              {@const basename = sf.file_path.split("/").pop() ?? sf.file_path}
              <div class="flex items-center gap-2 px-3 py-1.5 text-ui-sm">
                <span class="font-mono text-foreground truncate min-w-0 flex-1" title={sf.file_path}>{basename}</span>
                {#if sf.language}
                  <span class="text-ui-2xs text-muted-foreground/40 shrink-0">{sf.language}</span>
                {/if}
                <span class="text-ui-2xs text-amber-500/70 shrink-0">{sf.days_stale}d {t("activity.staleAgo")}</span>
              </div>
            {/each}
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}
{/if}
