<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Activity dashboard — daily stats, top files/projects, focus sessions, meetings. -->
<script lang="ts">
import { onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { CardContent } from "$lib/components/ui/card";
import SectionHeader from "$lib/components/ui/section-header/SectionHeader.svelte";
import SettingsCard from "$lib/components/ui/settings-card/SettingsCard.svelte";
import { Spinner } from "$lib/components/ui/spinner";
import { daemonGet, daemonPost } from "$lib/daemon/http";
import {
  type DailySummaryRow,
  type FocusSessionRow,
  getDailySummary,
  getFilesInRange,
  getFocusSessions,
  getHourlyHeatmap,
  getLanguageBreakdown,
  getMeetingsInRange,
  getProductivityScore,
  getStaleFiles,
  getTopFiles,
  getTopProjects,
  getWeeklyDigest,
  type HourlyEditRow,
  type LanguageBreakdownRow,
  type MeetingEventRow,
  type ProductivityScore,
  type ProjectUsageRow,
  type SessionFileActivity,
  type StaleFileRow,
  type WeeklyDigest,
} from "$lib/daemon/settings";
import { t } from "$lib/i18n/index.svelte";
import {
  getFatigue as brainFatigue,
  getFlow as brainFlow,
  getStreak as brainStreak,
  startBrainPolling,
} from "$lib/stores/brain.svelte";

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

// Timeline
let timeline = $state<{ kind: string; title: string; detail: string; ts: number; eeg_focus: number | null }[]>([]);

// Fusion insights
let taskType = $state<{ task_type: string; confidence: number; signals: string[] } | null>(null);
let codeEeg = $state<{
  by_language: { key: string; avg_focus: number; total_mins: number; interactions: number }[];
  best_files: { key: string; avg_focus: number }[];
  worst_files: { key: string; avg_focus: number }[];
} | null>(null);

// ── Focus session replay ──────────────────────────────────────────────────
let expandedSession = $state<number | null>(null);
let sessionFiles = $state<SessionFileActivity | null>(null);
let sessionLoading = $state(false);

async function toggleSession(idx: number) {
  if (expandedSession === idx) {
    expandedSession = null;
    sessionFiles = null;
    return;
  }
  expandedSession = idx;
  sessionLoading = true;
  sessionFiles = null;
  const sess = sessions[idx];
  if (sess) {
    try {
      sessionFiles = await getFilesInRange(sess.start_at, sess.end_at);
    } catch {
      sessionFiles = { files: [], focus_sessions: [], meetings: [] };
    }
  }
  sessionLoading = false;
}

// ── Weekly digest + CSV export ────────────────────────────────────────────
let weeklyDigest = $state<WeeklyDigest | null>(null);
let weeklyLoading = $state(false);

async function loadWeeklyDigest() {
  weeklyLoading = true;
  try {
    const weekStart = todayStart - 7 * 86400;
    weeklyDigest = await getWeeklyDigest(weekStart);
  } catch {}
  weeklyLoading = false;
}

function exportCsv() {
  if (!weeklyDigest) return;
  const rows = [
    ["Day", "Files", "Edits", "Lines Added", "Lines Removed", "Projects", "Avg Focus"].join(","),
    ...weeklyDigest.days.map((d) => {
      const date = new Date(d.day_start * 1000).toISOString().slice(0, 10);
      return [
        date,
        d.distinct_files,
        d.edits,
        d.lines_added,
        d.lines_removed,
        d.distinct_projects,
        d.avg_eeg_focus?.toFixed(1) ?? "",
      ].join(",");
    }),
    "",
    `Total,${weeklyDigest.total_edits} edits,${weeklyDigest.total_lines_added} added,${weeklyDigest.total_lines_removed} removed,${weeklyDigest.focus_session_count} sessions,${weeklyDigest.meeting_count} meetings`,
  ];
  const blob = new Blob([rows.join("\n")], { type: "text/csv" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `activity-report-${new Date().toISOString().slice(0, 10)}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

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
  startBrainPolling();
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
      daemonPost("/v1/brain/task-type", { windowSecs: 300 }),
      daemonPost("/v1/brain/code-eeg", { since: weekAgo }),
      daemonPost("/v1/activity/timeline", { since: weekAgo, limit: 30 }),
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
    if (results[9].status === "fulfilled") taskType = results[9].value as typeof taskType;
    if (results[10].status === "fulfilled") codeEeg = results[10].value as typeof codeEeg;
    if (results[11].status === "fulfilled") timeline = (results[11].value as typeof timeline) ?? [];
  } catch {}
  loading = false;
});

// Max value in heatmap for scaling.
$effect(() => {
  heatmapMax = Math.max(1, ...heatmap.map((h) => h.total_churn));
});
let heatmapMax = $state(1);
</script>

<!-- ── Brain State (real-time) ──────────────────────────────────────────── -->
{#if true}
{@const flow = brainFlow()}
{@const fatigue = brainFatigue()}
{@const streak = brainStreak()}
{#if flow || fatigue || streak}
  <section class="flex gap-2">
    {#if flow}
      <div class="flex-1 rounded-lg border px-3 py-2
        {flow.in_flow
          ? 'border-emerald-500/30 bg-emerald-500/5'
          : 'border-border dark:border-white/[0.06] bg-muted/30 dark:bg-white/[0.02]'}">
        <div class="flex items-center gap-1.5">
          {#if flow.in_flow}
            <span class="relative flex h-2 w-2">
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
              <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
            </span>
            <span class="text-ui-sm font-bold text-emerald-500">FLOW</span>
            <span class="text-ui-xs text-emerald-500/60 tabular-nums">{Math.floor(flow.duration_secs / 60)}m</span>
          {:else}
            <span class="w-2 h-2 rounded-full bg-muted-foreground/20"></span>
            <span class="text-ui-sm text-muted-foreground/50">{t("activity.noFlow")}</span>
          {/if}
        </div>
        {#if flow.avg_focus != null}
          <div class="mt-1 text-ui-2xs text-muted-foreground/40">
            {t("activity.eegFocus")}: {flow.avg_focus.toFixed(0)} | {flow.edit_velocity.toFixed(1)} ln/min
          </div>
        {/if}
      </div>
    {/if}
    {#if fatigue?.fatigued}
      <div class="flex-1 rounded-lg border border-amber-500/30 bg-amber-500/5 px-3 py-2">
        <div class="text-ui-sm font-bold text-amber-500">{t("activity.fatiguedTitle")}</div>
        <div class="text-ui-2xs text-amber-500/60">{fatigue.suggestion}</div>
      </div>
    {/if}
    {#if streak && streak.current_streak_days > 0}
      <div class="flex-1 rounded-lg border border-violet-500/30 bg-violet-500/5 px-3 py-2">
        <div class="flex items-baseline gap-1">
          <span class="text-ui-lg font-bold text-violet-500 tabular-nums">{streak.current_streak_days}</span>
          <span class="text-ui-xs text-violet-500/60">{t("activity.streakDays")}</span>
        </div>
        <div class="text-ui-2xs text-muted-foreground/40">{streak.today_deep_mins}m {t("activity.deepWork")} {t("activity.todayLabel")}</div>
      </div>
    {/if}
  </section>
{/if}
{/if}

{#if loading}
  <div class="flex items-center justify-center py-12">
    <Spinner size="w-5 h-5" class="text-muted-foreground/40" />
  </div>
{:else}
  <!-- ── Activity Timeline ──────────────────────────────────────────────── -->
  {#if timeline.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.timeline")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-0 px-0 max-h-64 overflow-y-auto">
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each timeline as ev}
              {@const kindColors: Record<string, string> = {
                file: "text-sky-500", build: ev.detail === "fail" ? "text-red-400" : "text-emerald-500",
                meeting: "text-amber-500", ai: "text-violet-500", clipboard: "text-muted-foreground/40", label: "text-violet-400",
              }}
              {@const kindIcons: Record<string, string> = {
                file: "~", build: ev.detail === "fail" ? "x" : "+", meeting: "@", ai: "*", clipboard: "=", label: "#",
              }}
              <div class="flex items-center gap-2 px-2.5 py-1 text-ui-2xs">
                <span class="w-3 font-mono font-bold {kindColors[ev.kind] ?? 'text-muted-foreground/30'} shrink-0">{kindIcons[ev.kind] ?? "."}</span>
                <span class="text-muted-foreground/40 tabular-nums shrink-0 w-12">{fmtTime(ev.ts)}</span>
                <span class="truncate min-w-0 flex-1 {ev.kind === 'file' ? 'font-mono' : ''} text-foreground/80">
                  {ev.kind === "file" ? ev.title.split("/").pop() : ev.title}
                </span>
                {#if ev.detail}
                  <span class="text-muted-foreground/30 truncate shrink-0 max-w-24">{ev.detail}</span>
                {/if}
                {#if ev.eeg_focus != null}
                  <span class="w-1.5 h-1.5 rounded-full shrink-0
                    {ev.eeg_focus >= 70 ? 'bg-emerald-500' : ev.eeg_focus >= 40 ? 'bg-amber-400' : 'bg-red-400'}"></span>
                {/if}
              </div>
            {/each}
          </div>
        </CardContent>
      </SettingsCard>
    </section>
  {/if}

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

  <!-- ── Focus Sessions (expandable replay) ──────────────────────────────── -->
  {#if sessions.length > 0}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.focusSessions")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-0 px-0">
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each sessions.slice(0, 12) as sess, idx}
              {@const isExpanded = expandedSession === idx}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div>
                <div
                  class="flex items-center gap-2 px-3 py-2 text-ui-sm cursor-pointer hover:bg-muted/20 transition-colors"
                  role="button" tabindex="0"
                  onclick={() => toggleSession(idx)}
                  onkeydown={(e) => e.key === "Enter" && toggleSession(idx)}>
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
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       class="w-3 h-3 text-muted-foreground/30 transition-transform {isExpanded ? 'rotate-180' : ''} shrink-0">
                    <path d="M6 9l6 6 6-6"/>
                  </svg>
                </div>

                <!-- Expanded: session replay detail -->
                {#if isExpanded}
                  <div class="px-3 pb-3 pt-1 flex flex-col gap-2 bg-muted/10 dark:bg-white/[0.01]">
                    {#if sessionLoading}
                      <div class="flex items-center gap-2 py-2">
                        <Spinner size="w-3 h-3" class="text-muted-foreground/40" />
                        <span class="text-ui-xs text-muted-foreground/40">{t("activity.replayLoading")}</span>
                      </div>
                    {:else if sessionFiles}
                      <!-- EEG focus overlay bar -->
                      {#if sessionFiles.files.some((f) => f.eeg_focus != null)}
                        <div class="flex flex-col gap-0.5">
                          <span class="text-ui-2xs text-muted-foreground/40">{t("activity.eegOverlay")}</span>
                          <div class="flex items-end gap-px h-8">
                            {#each sessionFiles.files as fi}
                              {@const focus = fi.eeg_focus ?? 0}
                              <div class="flex-1 rounded-t-sm"
                                   style="height:{Math.max(8, focus)}%;background:rgba({focus >= 70 ? '16,185,129' : focus >= 40 ? '245,158,11' : '239,68,68'},{0.3 + 0.7 * (focus / 100)})"
                                   title="{fi.file_path.split('/').pop()} — focus: {focus.toFixed(0)}"></div>
                            {/each}
                          </div>
                        </div>
                      {/if}

                      <!-- File list -->
                      {#if sessionFiles.files.length > 0}
                        <div class="rounded-lg border border-border dark:border-white/[0.06] overflow-hidden divide-y divide-border dark:divide-white/[0.04]">
                          {#each sessionFiles.files as fi (fi.id)}
                            {@const basename = fi.file_path.split("/").pop() ?? fi.file_path}
                            <div class="flex items-center gap-2 px-2.5 py-1.5 text-ui-sm">
                              <span class="w-1.5 h-1.5 rounded-full shrink-0
                                {fi.eeg_focus != null && fi.eeg_focus >= 70 ? 'bg-emerald-500' :
                                 fi.eeg_focus != null && fi.eeg_focus >= 40 ? 'bg-amber-400' :
                                 fi.eeg_focus != null ? 'bg-red-400' : 'bg-muted-foreground/20'}"></span>
                              <span class="text-ui-2xs text-muted-foreground/40 tabular-nums shrink-0 w-10">{fmtTime(fi.seen_at)}</span>
                              <span class="font-mono text-foreground truncate min-w-0 flex-1" title={fi.file_path}>{basename}</span>
                              {#if fi.language}
                                <span class="text-ui-2xs text-muted-foreground/40 shrink-0">{fi.language}</span>
                              {/if}
                              {#if fi.was_modified}
                                <span class="text-ui-2xs tabular-nums shrink-0">
                                  <span class="text-emerald-500">+{fi.lines_added}</span>
                                  <span class="text-red-400">-{fi.lines_removed}</span>
                                </span>
                              {/if}
                            </div>
                          {/each}
                        </div>
                      {:else}
                        <span class="text-ui-xs text-muted-foreground/40 italic">{t("activity.noFilesInSession")}</span>
                      {/if}

                      <!-- Meeting interruptions during this session -->
                      {#if sessionFiles.meetings.length > 0}
                        <div class="flex flex-wrap gap-1">
                          {#each sessionFiles.meetings as mtg (mtg.id)}
                            <span class="inline-flex items-center gap-1 rounded-full bg-amber-500/10 border border-amber-500/20
                                         px-2 py-0.5 text-ui-2xs text-amber-600 dark:text-amber-400">
                              {mtg.platform}
                              {#if mtg.end_at}
                                <span class="text-amber-500/60 tabular-nums">{Math.round((mtg.end_at - mtg.start_at) / 60)}m</span>
                              {/if}
                            </span>
                          {/each}
                        </div>
                      {/if}
                    {/if}
                  </div>
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

  <!-- ── Code-Brain Correlation ──────────────────────────────────────────── -->
  {#if codeEeg && (codeEeg.by_language.length > 0 || codeEeg.best_files.length > 0)}
    <section class="flex flex-col gap-2">
      <SectionHeader>{t("activity.codeBrain")}</SectionHeader>
      <SettingsCard>
        <CardContent class="py-3">
          {#if codeEeg.by_language.length > 0}
            <div class="flex flex-col gap-1.5 mb-3">
              <span class="text-ui-2xs text-muted-foreground/40">{t("activity.brainByLanguage")}</span>
              {#each codeEeg.by_language.slice(0, 6) as lang}
                <div class="flex items-center gap-2 text-ui-sm">
                  <span class="w-16 truncate text-ui-xs text-muted-foreground/60 shrink-0">{lang.key}</span>
                  <div class="flex-1 h-1.5 rounded-full bg-muted/40 overflow-hidden">
                    <div class="h-full rounded-full {lang.avg_focus >= 70 ? 'bg-emerald-500/60' : lang.avg_focus >= 40 ? 'bg-amber-400/60' : 'bg-red-400/60'}"
                         style="width:{lang.avg_focus}%"></div>
                  </div>
                  <span class="text-ui-2xs tabular-nums text-muted-foreground/40 w-6 text-right shrink-0">{lang.avg_focus.toFixed(0)}</span>
                </div>
              {/each}
            </div>
          {/if}
          {#if codeEeg.best_files.length > 0 || codeEeg.worst_files.length > 0}
            <div class="grid grid-cols-2 gap-3 text-ui-2xs">
              {#if codeEeg.best_files.length > 0}
                <div>
                  <span class="text-emerald-500/60 font-semibold">{t("activity.bestFiles")}</span>
                  {#each codeEeg.best_files.slice(0, 3) as f}
                    <div class="truncate text-muted-foreground/50" title={f.key}>{f.key.split("/").pop()} ({f.avg_focus.toFixed(0)})</div>
                  {/each}
                </div>
              {/if}
              {#if codeEeg.worst_files.length > 0}
                <div>
                  <span class="text-red-400/60 font-semibold">{t("activity.worstFiles")}</span>
                  {#each codeEeg.worst_files.slice(0, 3) as f}
                    <div class="truncate text-muted-foreground/50" title={f.key}>{f.key.split("/").pop()} ({f.avg_focus.toFixed(0)})</div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
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

  <!-- ── Weekly Report ────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2">
    <div class="flex items-center gap-2 px-0.5">
      <SectionHeader>{t("activity.weeklyReport")}</SectionHeader>
      <span class="flex-1"></span>
      {#if weeklyDigest}
        <Button size="sm" variant="ghost" class="text-ui-xs h-6 px-2" onclick={exportCsv}>
          {t("activity.exportCsv")}
        </Button>
      {/if}
    </div>
    <SettingsCard>
      <CardContent class="py-3">
        {#if !weeklyDigest && !weeklyLoading}
          <Button size="sm" variant="outline" class="w-full" onclick={loadWeeklyDigest}>
            {t("activity.loadWeeklyReport")}
          </Button>
        {:else if weeklyLoading}
          <div class="flex items-center justify-center py-4">
            <Spinner size="w-4 h-4" class="text-muted-foreground/40" />
          </div>
        {:else if weeklyDigest}
          {@const weekStats: [string, number][] = [
            [t("activity.edits"), weeklyDigest.total_edits],
            [t("activity.linesChanged"), weeklyDigest.total_lines_added + weeklyDigest.total_lines_removed],
            [t("activity.focusSessions"), weeklyDigest.focus_session_count],
            [t("activity.meetings"), weeklyDigest.meeting_count],
          ]}
          <!-- Weekly summary stats -->
          <div class="grid grid-cols-4 gap-3 mb-3">
            {#each weekStats as [label, value]}
              <div class="flex flex-col items-center gap-0.5">
                <span class="text-ui-base font-bold tabular-nums text-foreground">{value.toLocaleString()}</span>
                <span class="text-ui-2xs text-muted-foreground/50">{label}</span>
              </div>
            {/each}
          </div>
          <!-- Daily breakdown -->
          <div class="flex flex-col gap-1">
            {#each weeklyDigest.days as day, i}
              {@const date = new Date(day.day_start * 1000)}
              {@const dayName = date.toLocaleDateString(undefined, { weekday: "short" })}
              {@const isPeak = i === weeklyDigest.peak_day_idx && day.edits > 0}
              {@const maxEdits = Math.max(1, ...weeklyDigest.days.map((d) => d.edits))}
              <div class="flex items-center gap-2 text-ui-sm {isPeak ? 'text-violet-500 font-semibold' : ''}">
                <span class="w-8 text-ui-xs text-muted-foreground/50 shrink-0">{dayName}</span>
                <div class="flex-1 h-1.5 rounded-full bg-muted/40 overflow-hidden">
                  <div class="h-full rounded-full {isPeak ? 'bg-violet-500/70' : 'bg-sky-500/50'}"
                       style="width:{(day.edits / maxEdits * 100).toFixed(0)}%"></div>
                </div>
                <span class="text-ui-2xs tabular-nums text-muted-foreground/40 w-8 text-right shrink-0">{day.edits}</span>
              </div>
            {/each}
          </div>
          {#if weeklyDigest.avg_eeg_focus != null}
            <div class="mt-2 text-ui-xs text-muted-foreground/50 text-center">
              {t("activity.weeklyAvgFocus")}: {weeklyDigest.avg_eeg_focus.toFixed(0)}
              · {t("activity.peakHour")}: {weeklyDigest.peak_hour}:00
            </div>
          {/if}
        {/if}
      </CardContent>
    </SettingsCard>
  </section>
{/if}
