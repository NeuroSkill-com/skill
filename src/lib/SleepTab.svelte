<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Sleep tab — configure sleeping hours with presets. -->
<script lang="ts">
import { onMount } from "svelte";
import { Card, CardContent } from "$lib/components/ui/card";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { t } from "$lib/i18n/index.svelte";

interface SleepConfig {
  bedtime: string; // "HH:MM"
  wake_time: string; // "HH:MM"
  preset: string; // snake_case preset id
}

interface Preset {
  id: string;
  label: () => string;
  desc: () => string;
  bedtime: string;
  wake: string;
}

const PRESETS: Preset[] = [
  {
    id: "default",
    label: () => t("sleepSettings.presetDefault"),
    desc: () => t("sleepSettings.presetDefaultDesc"),
    bedtime: "23:00",
    wake: "07:00",
  },
  {
    id: "early_bird",
    label: () => t("sleepSettings.presetEarlyBird"),
    desc: () => t("sleepSettings.presetEarlyBirdDesc"),
    bedtime: "21:30",
    wake: "05:30",
  },
  {
    id: "night_owl",
    label: () => t("sleepSettings.presetNightOwl"),
    desc: () => t("sleepSettings.presetNightOwlDesc"),
    bedtime: "01:00",
    wake: "09:00",
  },
  {
    id: "short_sleeper",
    label: () => t("sleepSettings.presetShortSleeper"),
    desc: () => t("sleepSettings.presetShortSleeperDesc"),
    bedtime: "00:00",
    wake: "06:00",
  },
  {
    id: "long_sleeper",
    label: () => t("sleepSettings.presetLongSleeper"),
    desc: () => t("sleepSettings.presetLongSleeperDesc"),
    bedtime: "22:00",
    wake: "08:00",
  },
];

let config = $state<SleepConfig>({ bedtime: "23:00", wake_time: "07:00", preset: "default" });
let saving = $state(false);

// Derived sleep duration in hours + minutes
const duration = $derived.by(() => {
  const [bh, bm] = config.bedtime.split(":").map(Number);
  const [wh, wm] = config.wake_time.split(":").map(Number);
  const bed = bh * 60 + bm;
  const wake = wh * 60 + wm;
  const mins = wake >= bed ? wake - bed : 24 * 60 - bed + wake;
  return { hours: Math.floor(mins / 60), minutes: mins % 60, total: mins };
});

function durationLabel(mins: number): string {
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (m === 0) return `${h}h`;
  return `${h}h ${m}m`;
}

// The 24-hour clock visualization
const bedAngle = $derived(timeToAngle(config.bedtime));
const wakeAngle = $derived(timeToAngle(config.wake_time));

function timeToAngle(hhmm: string): number {
  const [h, m] = hhmm.split(":").map(Number);
  return ((h + m / 60) / 24) * 360 - 90; // -90 to start at top
}

async function save() {
  saving = true;
  try {
    await daemonInvoke("set_sleep_config", { config });
  } catch (e) {}
  saving = false;
}

function applyPreset(p: Preset) {
  config = { bedtime: p.bedtime, wake_time: p.wake, preset: p.id };
  save();
}

function setBedtime(val: string) {
  config = { ...config, bedtime: val, preset: "custom" };
  save();
}

function setWakeTime(val: string) {
  config = { ...config, wake_time: val, preset: "custom" };
  save();
}

onMount(async () => {
  try {
    config = await daemonInvoke<SleepConfig>("get_sleep_config");
  } catch (e) {}
});

// Arc path for the sleep span on the clock
function arcPath(startAngle: number, endAngle: number, r: number): string {
  const toRad = (d: number) => (d * Math.PI) / 180;
  const cx = 60,
    cy = 60;
  let sweep = endAngle - startAngle;
  if (sweep < 0) sweep += 360;
  const largeArc = sweep > 180 ? 1 : 0;
  const sx = cx + r * Math.cos(toRad(startAngle));
  const sy = cy + r * Math.sin(toRad(startAngle));
  const ex = cx + r * Math.cos(toRad(endAngle));
  const ey = cy + r * Math.sin(toRad(endAngle));
  return `M ${sx} ${sy} A ${r} ${r} 0 ${largeArc} 1 ${ex} ${ey}`;
}
</script>

<section class="flex flex-col gap-4">

  <!-- ── Hero ───────────────────────────────────────────────────────────────── -->
  <div class="rounded-2xl border border-border dark:border-white/[0.06]
              bg-gradient-to-r from-indigo-500/10 via-purple-500/10 to-blue-500/10
              dark:from-indigo-500/15 dark:via-purple-500/15 dark:to-blue-500/15
              px-5 py-4 flex items-center gap-4">
    <div class="flex items-center justify-center w-11 h-11 rounded-xl
                bg-gradient-to-br from-indigo-500 to-purple-500
                shadow-lg shadow-indigo-500/25 dark:shadow-indigo-500/40 shrink-0">
      <span class="text-xl leading-none">🌙</span>
    </div>
    <div class="flex flex-col gap-0.5">
      <span class="text-[0.82rem] font-bold">{t("sleepSettings.title")}</span>
      <span class="text-[0.55rem] text-muted-foreground/70">
        {t("sleepSettings.subtitle")}
      </span>
    </div>
    <span class="flex-1"></span>
    <div class="flex flex-col items-end gap-0.5">
      <span class="text-2xl font-extrabold tabular-nums tracking-tight
                   bg-gradient-to-r from-indigo-500 to-purple-500
                   bg-clip-text text-transparent">
        {durationLabel(duration.total)}
      </span>
      <span class="text-[0.45rem] text-muted-foreground/50">
        {config.bedtime} — {config.wake_time}
      </span>
    </div>
  </div>

  <!-- ── Clock visualization + time pickers ─────────────────────────────────── -->
  <Card class="gap-0 py-0 border-border dark:border-white/[0.06]">
    <CardContent class="py-5 px-5 flex flex-col gap-5">

      <div class="flex items-center gap-6 flex-wrap">

        <!-- Mini clock -->
        <div class="shrink-0 flex items-center justify-center">
          <svg viewBox="0 0 120 120" class="w-28 h-28">
            <!-- Background circle -->
            <circle cx="60" cy="60" r="52" fill="none" stroke="currentColor"
                    class="text-border dark:text-white/[0.08]" stroke-width="3" />

            <!-- Sleep arc -->
            <path d={arcPath(bedAngle, wakeAngle, 52)}
                  fill="none" stroke="url(#sleepGrad)" stroke-width="5"
                  stroke-linecap="round" />

            <!-- Hour ticks -->
            {#each Array(24) as _, i}
              {@const angle = (i / 24) * 360 - 90}
              {@const rad   = (angle * Math.PI) / 180}
              {@const isMajor = i % 6 === 0}
              <line
                x1={60 + 45 * Math.cos(rad)} y1={60 + 45 * Math.sin(rad)}
                x2={60 + (isMajor ? 40 : 43) * Math.cos(rad)} y2={60 + (isMajor ? 40 : 43) * Math.sin(rad)}
                stroke="currentColor"
                class={isMajor ? "text-muted-foreground/40" : "text-muted-foreground/15"}
                stroke-width={isMajor ? 1.5 : 0.75} />
            {/each}

            <!-- Hour labels -->
            {#each [0, 6, 12, 18] as h}
              {@const angle = (h / 24) * 360 - 90}
              {@const rad   = (angle * Math.PI) / 180}
              <text x={60 + 34 * Math.cos(rad)} y={60 + 34 * Math.sin(rad) + 2.5}
                    text-anchor="middle" class="fill-muted-foreground/50"
                    font-size="7" font-weight="500">
                {h === 0 ? "0" : String(h)}
              </text>
            {/each}

            <!-- Bed icon (moon) -->
            {#if true}
              {@const bedRad = (bedAngle * Math.PI) / 180}
              <circle cx={60 + 52 * Math.cos(bedRad)} cy={60 + 52 * Math.sin(bedRad)}
                      r="5" class="fill-indigo-500" />
              <text x={60 + 52 * Math.cos(bedRad)} y={60 + 52 * Math.sin(bedRad) + 2}
                    text-anchor="middle" font-size="5">🌙</text>
            {/if}

            <!-- Wake icon (sun) -->
            {#if true}
              {@const wakeRad = (wakeAngle * Math.PI) / 180}
              <circle cx={60 + 52 * Math.cos(wakeRad)} cy={60 + 52 * Math.sin(wakeRad)}
                      r="5" class="fill-amber-400" />
              <text x={60 + 52 * Math.cos(wakeRad)} y={60 + 52 * Math.sin(wakeRad) + 2}
                    text-anchor="middle" font-size="5">☀️</text>
            {/if}

            <!-- Center label -->
            <text x="60" y="58" text-anchor="middle" class="fill-foreground"
                  font-size="11" font-weight="700">{durationLabel(duration.total)}</text>
            <text x="60" y="68" text-anchor="middle" class="fill-muted-foreground/50"
                  font-size="6">{t("sleepSettings.sleepDuration")}</text>

            <defs>
              <linearGradient id="sleepGrad" x1="0%" y1="0%" x2="100%" y2="0%">
                <stop offset="0%"   stop-color="#6366f1" />
                <stop offset="100%" stop-color="#a78bfa" />
              </linearGradient>
            </defs>
          </svg>
        </div>

        <!-- Time pickers -->
        <div class="flex-1 flex flex-col gap-4 min-w-[180px]">

          <!-- Bedtime -->
          <div class="flex flex-col gap-1.5">
            <label for="bedtime" class="text-[0.65rem] font-semibold text-muted-foreground/70 uppercase tracking-wider flex items-center gap-1.5">
              <span>🌙</span> {t("sleepSettings.bedtime")}
            </label>
            <input id="bedtime" type="time" value={config.bedtime}
                   onchange={(e) => setBedtime((e.currentTarget as HTMLInputElement).value)}
                   class="w-full rounded-lg border border-border dark:border-white/[0.08]
                          bg-muted dark:bg-[#1a1a28] px-3 py-2
                          text-[0.8rem] font-semibold tabular-nums text-foreground
                          focus:outline-none focus:ring-2 focus:ring-primary/30" />
          </div>

          <!-- Wake time -->
          <div class="flex flex-col gap-1.5">
            <label for="waketime" class="text-[0.65rem] font-semibold text-muted-foreground/70 uppercase tracking-wider flex items-center gap-1.5">
              <span>☀️</span> {t("sleepSettings.wakeTime")}
            </label>
            <input id="waketime" type="time" value={config.wake_time}
                   onchange={(e) => setWakeTime((e.currentTarget as HTMLInputElement).value)}
                   class="w-full rounded-lg border border-border dark:border-white/[0.08]
                          bg-muted dark:bg-[#1a1a28] px-3 py-2
                          text-[0.8rem] font-semibold tabular-nums text-foreground
                          focus:outline-none focus:ring-2 focus:ring-primary/30" />
          </div>

          <!-- Duration summary -->
          <div class="flex items-center gap-2 px-1">
            <span class="text-[0.55rem] text-muted-foreground/50">
              {t("sleepSettings.durationSummary", {
                hours: String(duration.hours),
                minutes: String(duration.minutes),
              })}
            </span>
            {#if saving}
              <span class="text-[0.52rem] text-muted-foreground animate-pulse ml-auto">{t("common.saving")}</span>
            {/if}
          </div>

        </div>
      </div>

    </CardContent>
  </Card>

  <!-- ── Presets ────────────────────────────────────────────────────────────── -->
  <Card class="gap-0 py-0 border-border dark:border-white/[0.06]">
    <CardContent class="py-4 px-4 flex flex-col gap-3">

      <span class="text-[0.55rem] font-semibold text-muted-foreground/60 uppercase tracking-wider">
        {t("sleepSettings.presets")}
      </span>

      <div class="flex flex-col gap-1.5">
        {#each PRESETS as p}
          {@const active = config.preset === p.id}
          <button
            onclick={() => applyPreset(p)}
            class="flex items-center gap-3 rounded-xl border px-4 py-3
                   transition-all cursor-pointer select-none text-left
                   {active
                     ? 'border-primary/50 bg-primary/[0.06] dark:bg-primary/[0.08]'
                     : 'border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] hover:bg-slate-50 dark:hover:bg-white/[0.02]'}">

            <!-- Active indicator -->
            <div class="shrink-0 w-4 h-4 rounded-full border-2 flex items-center justify-center
                        {active
                          ? 'border-primary'
                          : 'border-muted-foreground/25'}">
              {#if active}
                <div class="w-2 h-2 rounded-full bg-primary"></div>
              {/if}
            </div>

            <div class="flex-1 flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.72rem] font-semibold {active ? 'text-foreground' : 'text-foreground/80'}">
                {p.label()}
              </span>
              <span class="text-[0.58rem] text-muted-foreground/60">
                {p.desc()}
              </span>
            </div>

            <span class="shrink-0 text-[0.62rem] font-mono tabular-nums
                         {active ? 'text-primary' : 'text-muted-foreground/40'}">
              {p.bedtime} — {p.wake}
            </span>
          </button>
        {/each}
      </div>

    </CardContent>
  </Card>

  <!-- ── Info ───────────────────────────────────────────────────────────────── -->
  <div class="rounded-xl border border-border dark:border-white/[0.06]
              bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-2">
    <span class="text-[0.6rem] font-semibold text-muted-foreground uppercase tracking-wider">
      {t("sleepSettings.howItWorks")}
    </span>
    <ul class="flex flex-col gap-1.5 text-[0.62rem] text-muted-foreground/70 leading-relaxed">
      <li class="flex items-start gap-2">
        <span class="shrink-0 mt-0.5">📊</span>
        <span>{t("sleepSettings.info1")}</span>
      </li>
      <li class="flex items-start gap-2">
        <span class="shrink-0 mt-0.5">🧠</span>
        <span>{t("sleepSettings.info2")}</span>
      </li>
      <li class="flex items-start gap-2">
        <span class="shrink-0 mt-0.5">⏰</span>
        <span>{t("sleepSettings.info3")}</span>
      </li>
    </ul>
  </div>

</section>
