<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Daemon background activity panel.

  Lists every recurring task the daemon runs in the background — what it
  does, why it has to, how often it wakes up, and when it last ticked.
  Surfaces this so users who notice CPU usage can see exactly which workers
  are active rather than guessing.

  Live updates come from the `activity-state` WebSocket event so the panel
  refreshes without polling. We also fetch `/v1/activity` on mount (and as
  a 30s safety-net poll) to pick up any task that hasn't ticked yet.
-->
<script lang="ts">
import { onDestroy, onMount } from "svelte";

import { CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { SettingsCard } from "$lib/components/ui/settings-card";
import { type DaemonBackgroundTask, getDaemonActivity } from "$lib/daemon/misc";
import { onDaemonEvent } from "$lib/daemon/ws";
import { t } from "$lib/i18n/index.svelte";

let tasks = $state<DaemonBackgroundTask[]>([]);
let loading = $state(true);
let error = $state<string | null>(null);
let now = $state(Date.now());
let pollTimer: ReturnType<typeof setInterval> | null = null;
let clockTimer: ReturnType<typeof setInterval> | null = null;
let unsubscribe: (() => void) | null = null;

async function refresh() {
  try {
    const resp = await getDaemonActivity();
    tasks = resp.tasks;
    error = null;
  } catch (e) {
    error = String(e);
  } finally {
    loading = false;
  }
}

onMount(() => {
  refresh();
  // Safety-net refresh in case we miss a WebSocket event (reconnect window,
  // tab woke from sleep, etc.). 30s is far less aggressive than the 5s poll
  // we used before — the WS event is the primary update path now.
  pollTimer = setInterval(refresh, 30_000);
  // Tick the "last ran Ns ago" clock once per second without re-fetching.
  clockTimer = setInterval(() => {
    now = Date.now();
  }, 1000);
  // Live heartbeat patches via WebSocket. The daemon throttles these to
  // every 5th tick per task, so we get a steady stream without flooding.
  unsubscribe = onDaemonEvent("activity-state", (ev) => {
    const p = ev.payload as {
      task_id?: string;
      last_tick_unix_ms?: number;
      last_duration_ms?: number;
      tick_count?: number;
    };
    if (!p.task_id) return;
    tasks = tasks.map((task) =>
      task.id === p.task_id
        ? {
            ...task,
            heartbeat: {
              lastTickUnixMs: p.last_tick_unix_ms ?? task.heartbeat.lastTickUnixMs,
              lastDurationMs: p.last_duration_ms ?? task.heartbeat.lastDurationMs,
              tickCount: p.tick_count ?? task.heartbeat.tickCount,
            },
          }
        : task,
    );
  });
});

onDestroy(() => {
  if (pollTimer) clearInterval(pollTimer);
  if (clockTimer) clearInterval(clockTimer);
  unsubscribe?.();
});

function costLabel(cost: "low" | "medium" | "high"): string {
  if (cost === "low") return t("daemonActivity.costLow");
  if (cost === "medium") return t("daemonActivity.costMedium");
  return t("daemonActivity.costHigh");
}

function intervalLabel(secs: number): string {
  if (secs === 0) return t("daemonActivity.eventDriven");
  if (secs < 60) return `${secs}s`;
  return `${Math.round(secs / 60)}m`;
}

function lastRanLabel(lastTickUnixMs: number): string {
  if (!lastTickUnixMs) return t("daemonActivity.never");
  const ageMs = Math.max(0, now - lastTickUnixMs);
  const secs = Math.round(ageMs / 1000);
  if (secs < 60) return t("daemonActivity.lastRanSecondsAgo", { n: secs });
  const mins = Math.round(secs / 60);
  if (mins < 60) return t("daemonActivity.lastRanMinutesAgo", { n: mins });
  const hours = Math.round(mins / 60);
  return t("daemonActivity.lastRanHoursAgo", { n: hours });
}
</script>

<section class="flex flex-col gap-2">
  <SectionHeader description={t("daemonActivity.intro")}>{t("daemonActivity.title")}</SectionHeader>
  <SettingsCard>
    <CardContent class="flex flex-col gap-3 p-4">
      {#if loading}
        <p class="text-ui-base text-muted-foreground animate-pulse">{t("daemonActivity.loading")}</p>
      {:else if error}
        <p class="text-ui-base text-red-600 dark:text-red-400">{error}</p>
      {:else}
        <ul class="flex flex-col divide-y divide-border">
          {#each tasks as task (task.id)}
            <li class="flex flex-col gap-1 py-3 first:pt-0 last:pb-0">
              <div class="flex items-baseline justify-between gap-3">
                <div class="flex items-center gap-2">
                  <span class="text-ui-md font-medium text-foreground">{task.name}</span>
                  {#if task.state?.running}
                    <span
                      class="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-2 py-0.5 text-ui-xs font-semibold text-emerald-700 dark:text-emerald-400"
                    >
                      <span class="h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
                      {t("daemonActivity.running")}
                    </span>
                  {:else if task.state && !task.state.running}
                    <span class="inline-flex rounded-full bg-muted px-2 py-0.5 text-ui-xs text-muted-foreground">
                      {t("daemonActivity.idle")}
                    </span>
                  {/if}
                </div>
                <div class="flex items-center gap-2 text-ui-xs text-muted-foreground">
                  <span>{intervalLabel(task.intervalSecs)}</span>
                  <span aria-hidden="true">·</span>
                  <span class="uppercase tracking-wider">{costLabel(task.cost)}</span>
                </div>
              </div>
              <p class="text-ui-base text-muted-foreground leading-snug">{task.does}</p>
              <p class="text-ui-xs text-muted-foreground/80 italic leading-snug">
                {t("daemonActivity.whyPrefix")} {task.why}
              </p>
              {#if task.state?.detail}
                <p class="text-ui-xs text-muted-foreground/80 leading-snug">{task.state.detail}</p>
              {/if}
              <p class="text-ui-xs text-muted-foreground/60 leading-snug">
                {lastRanLabel(task.heartbeat.lastTickUnixMs)}
                {#if task.heartbeat.lastDurationMs > 0}
                  <span aria-hidden="true">·</span>
                  {t("daemonActivity.tickDuration", { n: task.heartbeat.lastDurationMs })}
                {/if}
                {#if task.heartbeat.tickCount > 0}
                  <span aria-hidden="true">·</span>
                  {t("daemonActivity.tickCount", { n: task.heartbeat.tickCount })}
                {/if}
              </p>
            </li>
          {/each}
        </ul>
      {/if}
    </CardContent>
  </SettingsCard>
</section>
