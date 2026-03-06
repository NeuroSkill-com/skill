<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Goals tab — daily recording goal configuration + 30-day history chart. -->
<script lang="ts">
  import { onMount, onDestroy }  from "svelte";
  import { invoke }              from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Card, CardContent }   from "$lib/components/ui/card";
  import { t }                   from "$lib/i18n/index.svelte";

  // ── Daily Goal ─────────────────────────────────────────────────────────────
  let dailyGoalMin = $state(60);
  let saving       = $state(false);

  // ── Do Not Disturb Automation ──────────────────────────────────────────────
  interface DndConfig {
    enabled:               boolean;
    focus_threshold:       number;   // 0–100
    duration_secs:         number;
    focus_mode_identifier: string;   // modeIdentifier string, e.g. "com.apple.donotdisturb.mode.default"
  }

  interface FocusModeOption {
    identifier: string;
    name:       string;
  }

  const DND_DURATION_PRESETS: [string, number][] = [
    [t("dnd.durationPreset30"),  30],
    [t("dnd.durationPreset60"),  60],
    [t("dnd.durationPreset120"), 120],
    [t("dnd.durationPreset300"), 300],
  ];

  const DND_DEFAULT_MODE = "com.apple.donotdisturb.mode.default";

  let dndConfig        = $state<DndConfig>({ enabled: false, focus_threshold: 60, duration_secs: 60, focus_mode_identifier: DND_DEFAULT_MODE });
  let dndActive        = $state(false);
  let dndSaving        = $state(false);
  let dndTesting       = $state(false);
  let focusModes       = $state<FocusModeOption[]>([]);
  let focusModesLoaded = $state(false);

  async function testDnd() {
    dndTesting = true;
    try { await invoke("test_dnd", { enabled: !dndActive }); } catch {}
    dndTesting = false;
  }

  async function saveDnd() {
    dndSaving = true;
    try { await invoke("set_dnd_config", { config: dndConfig }); } catch {}
    dndSaving = false;
  }

  async function toggleDnd() {
    dndConfig = { ...dndConfig, enabled: !dndConfig.enabled };
    await saveDnd();
  }

  async function setDndThreshold(v: number) {
    dndConfig = { ...dndConfig, focus_threshold: v };
    await saveDnd();
  }

  async function setDndDuration(secs: number) {
    dndConfig = { ...dndConfig, duration_secs: secs };
    await saveDnd();
  }

  async function setFocusMode(identifier: string) {
    dndConfig = { ...dndConfig, focus_mode_identifier: identifier };
    await saveDnd();
  }

  let dndUnlisten: UnlistenFn | null = null;

  onMount(async () => {
    try {
      const v = await invoke<number>("get_daily_goal");
      if (v > 0) dailyGoalMin = v;
    } catch {}
    await loadChart();

    // Load DND config + current active state
    try {
      dndConfig = await invoke<DndConfig>("get_dnd_config");
      dndActive = await invoke<boolean>("get_dnd_active");
    } catch {}

    // Load available Focus modes from the OS (macOS only; empty on other platforms)
    try {
      focusModes = await invoke<FocusModeOption[]>("list_focus_modes");
    } catch {}
    focusModesLoaded = true;

    // Listen for live DND state changes (from the EEG band monitor)
    dndUnlisten = await listen<boolean>("dnd-state-changed", (ev) => {
      dndActive = ev.payload;
    });
  });

  onDestroy(() => {
    dndUnlisten?.();
  });

  async function save() {
    saving = true;
    try { await invoke("set_daily_goal", { minutes: dailyGoalMin }); } catch {}
    saving = false;
    await loadChart();          // refresh chart after goal change
  }

  // Quick presets
  const PRESETS: [string, number][] = [
    ["15m",  15],
    ["30m",  30],
    ["1h",   60],
    ["2h",  120],
    ["4h",  240],
    ["8h",  480],
  ];

  const goalHours = $derived(dailyGoalMin / 60);

  // ── 30-day chart ───────────────────────────────────────────────────────────
  interface DayEntry { date: string; minutes: number; label: string }

  let chartDays   = $state<DayEntry[]>([]);
  let chartMax    = $state(1);
  let loading     = $state(false);

  async function loadChart() {
    loading = true;
    try {
      const raw = await invoke<[string, number][]>("get_daily_recording_mins", { days: 30 });
      const days: DayEntry[] = raw.map(([iso, mins]) => {
        const d = new Date(iso + "T00:00:00Z");
        const label = d.toLocaleDateString(undefined, { month: "short", day: "numeric", timeZone: "UTC" });
        return { date: iso, minutes: mins, label };
      });
      chartDays = days;
      chartMax  = Math.max(dailyGoalMin * 1.25, ...days.map(d => d.minutes), 1);
    } catch {}
    loading = false;
  }

  // Bar colours
  function barColor(mins: number): string {
    if (mins >= dailyGoalMin) return "#22c55e";   // green — goal met
    if (mins >= dailyGoalMin * 0.5) return "#3b82f6"; // blue — halfway+
    if (mins === 0) return "transparent";
    return "#6366f1";                             // indigo — some progress
  }

  // Format minutes → "1h 23m" or "45m"
  function fmtMins(m: number): string {
    if (m === 0) return "—";
    if (m < 60) return `${m}m`;
    return `${Math.floor(m / 60)}h ${m % 60 > 0 ? `${m % 60}m` : ""}`.trim();
  }

  // Goal line Y position (% from top)
  const goalY = $derived((1 - dailyGoalMin / chartMax) * 100);

  // Streak: consecutive days (from today backwards) that hit the goal
  const streak = $derived.by(() => {
    if (!chartDays.length || dailyGoalMin === 0) return 0;
    let s = 0;
    for (let i = chartDays.length - 1; i >= 0; i--) {
      if (chartDays[i].minutes >= dailyGoalMin) s++;
      else break;
    }
    return s;
  });
</script>

<section class="flex flex-col gap-4">

  <!-- ── Hero ───────────────────────────────────────────────────────────────── -->
  <div class="rounded-2xl border border-border dark:border-white/[0.06]
              bg-gradient-to-r from-blue-500/10 via-indigo-500/10 to-violet-500/10
              dark:from-blue-500/15 dark:via-indigo-500/15 dark:to-violet-500/15
              px-5 py-4 flex items-center gap-4">
    <div class="flex items-center justify-center w-11 h-11 rounded-xl
                bg-gradient-to-br from-blue-500 to-indigo-500
                shadow-lg shadow-blue-500/25 dark:shadow-blue-500/40 shrink-0">
      <span class="text-xl leading-none">🎯</span>
    </div>
    <div class="flex flex-col gap-0.5">
      <span class="text-[0.82rem] font-bold">{t("goals.title")}</span>
      <span class="text-[0.55rem] text-muted-foreground/70">
        {t("goals.subtitle")}
      </span>
    </div>
    <span class="flex-1"></span>
    <div class="flex flex-col items-end gap-0.5">
      <span class="text-2xl font-extrabold tabular-nums tracking-tight
                   bg-gradient-to-r from-blue-500 to-indigo-500
                   bg-clip-text text-transparent">
        {dailyGoalMin}m
      </span>
      <span class="text-[0.45rem] text-muted-foreground/50">
        {goalHours >= 1 ? `${goalHours.toFixed(1)} hours` : `${dailyGoalMin} minutes`} / day
      </span>
      {#if streak > 0}
        <span class="text-[0.55rem] font-semibold text-amber-500">
          🔥 {streak}d streak
        </span>
      {/if}
    </div>
  </div>

  <!-- ── Slider ─────────────────────────────────────────────────────────────── -->
  <Card class="gap-0 py-0 border-border dark:border-white/[0.06]">
    <CardContent class="py-4 px-4 flex flex-col gap-4">

      <div class="flex flex-col gap-2">
        <div class="flex items-center justify-between">
          <span class="text-[0.72rem] font-semibold text-foreground">{t("goals.targetMinutes")}</span>
          <span class="text-[0.75rem] font-bold tabular-nums text-foreground">{dailyGoalMin}m</span>
        </div>
        <input type="range" min="5" max="480" step="5"
               bind:value={dailyGoalMin}
               oninput={save}
               class="w-full accent-blue-500 h-2" />
        <div class="flex justify-between text-[0.42rem] text-muted-foreground/40 tabular-nums px-0.5">
          <span>5m</span>
          <span>1h</span>
          <span>2h</span>
          <span>4h</span>
          <span>8h</span>
        </div>
      </div>

      <!-- Quick presets -->
      <div class="flex flex-col gap-1.5">
        <span class="text-[0.55rem] font-semibold text-muted-foreground/60 uppercase tracking-wider">
          {t("goals.presets")}
        </span>
        <div class="flex items-center gap-1.5 flex-wrap">
          {#each PRESETS as [label, val]}
            <button
              onclick={() => { dailyGoalMin = val; save(); }}
              class="rounded-lg border px-3 py-1.5 text-[0.66rem] font-semibold
                     transition-all cursor-pointer select-none
                     {dailyGoalMin === val
                       ? 'border-blue-500/50 bg-blue-500/10 dark:bg-blue-500/15 text-blue-600 dark:text-blue-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              {label}
            </button>
          {/each}
        </div>
      </div>

    </CardContent>
  </Card>

  <!-- ── 30-day chart ───────────────────────────────────────────────────────── -->
  <Card class="gap-0 py-0 border-border dark:border-white/[0.06]">
    <CardContent class="py-4 px-4 flex flex-col gap-3">

      <div class="flex items-center justify-between">
        <span class="text-[0.72rem] font-semibold">{t("goals.chartTitle")}</span>
        {#if loading}
          <span class="text-[0.55rem] text-muted-foreground animate-pulse">{t("common.loading")}</span>
        {/if}
      </div>

      {#if chartDays.length}
        <!-- Bar chart -->
        <div class="relative" style="height:96px">
          <!-- Goal line -->
          <div class="absolute inset-x-0 border-t border-dashed border-emerald-500/50 z-10 pointer-events-none"
               style="top:{goalY}%">
            <span class="absolute right-0 -top-3.5 text-[0.42rem] text-emerald-500/70 font-medium pr-0.5">
              {fmtMins(dailyGoalMin)}
            </span>
          </div>

          <!-- Bars -->
          <div class="absolute inset-0 flex items-end gap-px overflow-hidden rounded-md">
            {#each chartDays as day, i}
              {@const pct   = Math.min(100, (day.minutes / chartMax) * 100)}
              {@const color = barColor(day.minutes)}
              {@const isToday = i === chartDays.length - 1}
              <div class="group relative flex-1 flex flex-col justify-end h-full"
                   title="{day.label}: {fmtMins(day.minutes)}">
                <!-- bar fill -->
                {#if day.minutes > 0}
                  <div class="w-full rounded-t-[2px] transition-all duration-300 relative"
                       style="height:{pct}%; background:{color}; opacity:{isToday ? 1 : 0.7}">
                    <!-- today indicator dot -->
                    {#if isToday}
                      <div class="absolute -top-1 left-1/2 -translate-x-1/2 w-1 h-1 rounded-full bg-white/80"></div>
                    {/if}
                  </div>
                {:else}
                  <div class="w-full rounded-t-[2px]" style="height:2px; background:#334155; opacity:0.3"></div>
                {/if}
                <!-- tooltip on hover -->
                <div class="absolute bottom-full mb-1 left-1/2 -translate-x-1/2
                            bg-popover border border-border rounded px-1.5 py-0.5
                            text-[0.5rem] whitespace-nowrap z-20 pointer-events-none
                            opacity-0 group-hover:opacity-100 transition-opacity shadow-md">
                  <span class="font-semibold">{day.label}</span>
                  <br>{fmtMins(day.minutes)}
                  {#if day.minutes >= dailyGoalMin}<span class="text-emerald-500"> ✓</span>{/if}
                </div>
              </div>
            {/each}
          </div>
        </div>

        <!-- X-axis labels: only show first, middle, last -->
        <div class="flex justify-between text-[0.42rem] text-muted-foreground/40 tabular-nums px-0.5 -mt-1">
          <span>{chartDays[0]?.label ?? ""}</span>
          <span>{chartDays[Math.floor(chartDays.length / 2)]?.label ?? ""}</span>
          <span class="text-foreground/60 font-medium">{t("goals.today")}</span>
        </div>

        <!-- Legend -->
        <div class="flex items-center gap-3 flex-wrap text-[0.5rem] text-muted-foreground/60">
          <span class="flex items-center gap-1">
            <span class="inline-block w-2 h-2 rounded-sm" style="background:#22c55e"></span>
            {t("goals.legendGoalMet")}
          </span>
          <span class="flex items-center gap-1">
            <span class="inline-block w-2 h-2 rounded-sm" style="background:#3b82f6"></span>
            {t("goals.legendHalfway")}
          </span>
          <span class="flex items-center gap-1">
            <span class="inline-block w-2 h-2 rounded-sm" style="background:#6366f1"></span>
            {t("goals.legendSomeProgress")}
          </span>
        </div>
      {:else if !loading}
        <p class="text-[0.6rem] text-muted-foreground/50 text-center py-4">
          {t("goals.noData")}
        </p>
      {/if}

    </CardContent>
  </Card>

  <!-- ── Info ───────────────────────────────────────────────────────────────── -->
  <div class="rounded-xl border border-border dark:border-white/[0.06]
              bg-white dark:bg-[#14141e] px-4 py-3 flex flex-col gap-2">
    <span class="text-[0.6rem] font-semibold text-muted-foreground uppercase tracking-wider">
      {t("goals.howItWorks")}
    </span>
    <ul class="flex flex-col gap-1.5 text-[0.62rem] text-muted-foreground/70 leading-relaxed">
      <li class="flex items-start gap-2">
        <span class="shrink-0 mt-0.5">📊</span>
        <span>{t("goals.info1")}</span>
      </li>
      <li class="flex items-start gap-2">
        <span class="shrink-0 mt-0.5">🔔</span>
        <span>{t("goals.info2")}</span>
      </li>
      <li class="flex items-start gap-2">
        <span class="shrink-0 mt-0.5">🔥</span>
        <span>{t("goals.info3")}</span>
      </li>
    </ul>
  </div>

  <!-- ── Do Not Disturb Automation ──────────────────────────────────────────── -->
  <div class="flex flex-col gap-2">
    <!-- Section header -->
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("dnd.section")}
      </span>
      <!-- Live status badge -->
      {#if dndConfig.enabled}
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {dndActive ? 'text-violet-500' : 'text-muted-foreground/50'}">
          {dndActive ? t("dnd.statusActive") : t("dnd.statusInactive")}
        </span>
      {/if}
      {#if dndSaving}
        <span class="text-[0.52rem] text-muted-foreground">saving…</span>
      {/if}
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <div class="flex flex-col divide-y divide-border dark:divide-white/[0.05]">

        <!-- ── Enable toggle ──────────────────────────────────────────────── -->
        <button
          onclick={toggleDnd}
          class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
                 hover:bg-slate-50 dark:hover:bg-white/[0.02]">
          <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                      {dndConfig.enabled ? 'bg-violet-500' : 'bg-muted dark:bg-white/[0.08]'}">
            <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                        {dndConfig.enabled ? 'translate-x-4' : 'translate-x-0.5'}"></div>
          </div>
          <div class="flex flex-col gap-0.5 min-w-0">
            <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
              {t("dnd.enabled")}
            </span>
            <span class="text-[0.58rem] text-muted-foreground leading-tight">
              {t("dnd.enabledDesc")}
            </span>
          </div>
          <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                       {dndConfig.enabled ? 'text-violet-500' : 'text-muted-foreground/50'}">
            {dndConfig.enabled ? "ON" : "OFF"}
          </span>
        </button>

        {#if dndConfig.enabled}
          <!-- ── Focus threshold ────────────────────────────────────────── -->
          <div class="flex flex-col gap-2 px-4 py-3.5">
            <div class="flex items-center justify-between">
              <span class="text-[0.72rem] font-semibold text-foreground">
                {t("dnd.threshold")} <span class="text-[0.62rem] font-normal text-muted-foreground">(engagement)</span>
              </span>
              <span class="text-[0.75rem] font-bold tabular-nums text-foreground">
                {Math.round(dndConfig.focus_threshold)}
              </span>
            </div>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed -mt-0.5">
              {t("dnd.thresholdDesc")}
            </p>
            <input type="range" min="10" max="95" step="5"
                   value={dndConfig.focus_threshold}
                   oninput={(e) => setDndThreshold(Number((e.currentTarget as HTMLInputElement).value))}
                   class="w-full accent-violet-500 h-2" />
            <div class="flex justify-between text-[0.42rem] text-muted-foreground/40 tabular-nums px-0.5">
              <span>10</span>
              <span>40</span>
              <span>60</span>
              <span>80</span>
              <span>95</span>
            </div>
          </div>

          <!-- ── Sustained duration ─────────────────────────────────────── -->
          <div class="flex flex-col gap-2 px-4 py-3.5">
            <div class="flex items-center justify-between">
              <span class="text-[0.72rem] font-semibold text-foreground">
                {t("dnd.duration")}
              </span>
              <span class="text-[0.68rem] tabular-nums text-muted-foreground">
                {dndConfig.duration_secs}s
              </span>
            </div>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed -mt-0.5">
              {t("dnd.durationDesc")}
            </p>
            <div class="flex items-center gap-1.5 flex-wrap">
              {#each DND_DURATION_PRESETS as [label, secs]}
                <button
                  onclick={() => setDndDuration(secs)}
                  class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                         transition-all cursor-pointer select-none
                         {dndConfig.duration_secs === secs
                           ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                           : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                  {label}
                </button>
              {/each}
            </div>
          </div>

          <!-- ── Focus mode picker ──────────────────────────────────────── -->
          {#if focusModes.length > 0}
            <div class="flex flex-col gap-2 px-4 py-3.5">
              <div class="flex items-center justify-between">
                <span class="text-[0.72rem] font-semibold text-foreground">
                  {t("dnd.focusMode")}
                </span>
                {#if !focusModesLoaded}
                  <span class="text-[0.6rem] text-muted-foreground/60">{t("dnd.focusModeLoading")}</span>
                {/if}
              </div>
              <p class="text-[0.62rem] text-muted-foreground leading-relaxed -mt-0.5">
                {t("dnd.focusModeDesc")}
              </p>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each focusModes as mode}
                  <button
                    onclick={() => setFocusMode(mode.identifier)}
                    class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                           transition-all cursor-pointer select-none
                           {dndConfig.focus_mode_identifier === mode.identifier
                             ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                             : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                    {mode.name}
                  </button>
                {/each}
              </div>
            </div>
          {/if}

          <!-- ── Active state indicator ─────────────────────────────────── -->
          <div class="flex items-center gap-3 px-4 py-3
                      bg-slate-50 dark:bg-[#111118]">
            <span class="relative flex h-2.5 w-2.5 shrink-0">
              {#if dndActive}
                <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-violet-400 opacity-75"></span>
                <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-violet-500"></span>
              {:else}
                <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-muted-foreground/20"></span>
              {/if}
            </span>
            <span class="text-[0.62rem] {dndActive ? 'text-violet-600 dark:text-violet-400 font-semibold' : 'text-muted-foreground/60'}">
              {dndActive ? t("dnd.statusActive") : t("dnd.statusInactive")}
            </span>
            <span class="ml-auto text-[0.52rem] text-muted-foreground/40">
              {focusModes.find(m => m.identifier === dndConfig.focus_mode_identifier)?.name ?? "Do Not Disturb"}
              · engagement ≥ {Math.round(dndConfig.focus_threshold)} for {dndConfig.duration_secs}s
            </span>
          </div>
        {/if}

        <!-- ── Manual test row (always visible) ─────────────────────────── -->
        <div class="flex items-center gap-3 px-4 py-3 border-t border-border dark:border-white/[0.05]">
          <span class="text-[0.62rem] text-muted-foreground/60">Test DND</span>
          <button
            onclick={testDnd}
            disabled={dndTesting}
            class="ml-auto shrink-0 text-[0.6rem] font-medium px-2.5 py-1 rounded-md border
                   transition-colors cursor-pointer select-none
                   {dndActive
                     ? 'border-violet-400/40 bg-violet-500/10 text-violet-600 dark:text-violet-400 hover:bg-violet-500/20'
                     : 'border-border dark:border-white/[0.08] bg-background text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}
                   disabled:opacity-40 disabled:cursor-not-allowed">
            {dndTesting ? "…" : dndActive ? "Turn Off" : "Turn On"}
          </button>
        </div>

      </div>
    </Card>

    <!-- macOS requirement note -->
    <p class="text-[0.58rem] text-muted-foreground/50 px-0.5 flex items-center gap-1">
      <span>🍎</span>
      <span>{t("dnd.requiresMacOS")}</span>
    </p>
  </div>

</section>
