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
    exit_duration_secs:    number;   // seconds below threshold before DND clears (default 300)
    focus_lookback_secs:   number;   // lookback window — recent focus delays exit (default 60)
    focus_mode_identifier: string;   // modeIdentifier string, e.g. "com.apple.donotdisturb.mode.default"
    exit_notification:     boolean;  // whether to send a notification when focus mode exits
    snr_exit_db:           number;   // SNR threshold (dB) below which DND is forcibly deactivated (default 0)
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

  // Exit delay presets: 1 / 2 / 5 / 10 / 15 / 30 / 60 minutes
  const DND_EXIT_PRESETS: [string, number][] = [
    [t("dnd.exitDurationValue", { min: "1"  }),   60],
    [t("dnd.exitDurationValue", { min: "2"  }),  120],
    [t("dnd.exitDurationValue", { min: "5"  }),  300],
    [t("dnd.exitDurationValue", { min: "10" }),  600],
    [t("dnd.exitDurationValue", { min: "15" }),  900],
    [t("dnd.exitDurationValue", { min: "30" }), 1800],
    [t("dnd.exitDurationValue", { min: "60" }), 3600],
  ];

  // Lookback presets: 30s / 1 min / 2 min / 5 min / 10 min
  const DND_LOOKBACK_PRESETS: [string, number][] = [
    [t("dnd.focusLookbackValue",     { secs: "30" }),  30],
    [t("dnd.focusLookbackValue_min", { min:  "1"  }),  60],
    [t("dnd.focusLookbackValue_min", { min:  "2"  }), 120],
    [t("dnd.focusLookbackValue_min", { min:  "5"  }), 300],
    [t("dnd.focusLookbackValue_min", { min:  "10" }), 600],
  ];

  const DND_DEFAULT_MODE = "com.apple.donotdisturb.mode.default";

  let dndConfig              = $state<DndConfig>({ enabled: false, focus_threshold: 60, duration_secs: 60, exit_duration_secs: 300, focus_lookback_secs: 60, focus_mode_identifier: DND_DEFAULT_MODE, exit_notification: true, snr_exit_db: 0 });
  let dndActive              = $state(false);
  let dndOsActive            = $state<boolean | null>(null); // real system-level state
  let dndExitSecsRemain      = $state(0);    // >0 while exit countdown is running
  let dndExitHeldByLookback  = $state(false);// true when lookback is resetting countdown
  // Activation-progress fields — populated by dnd-eligibility events
  let dndAvgScore            = $state(0);
  let dndSampleCount         = $state(0);
  let dndWindowSize          = $state(0);
  let dndThresholdLive       = $state(60);   // mirrors dndConfig.focus_threshold
  let dndSaving              = $state(false);
  let dndTesting             = $state(false);
  let focusModes             = $state<FocusModeOption[]>([]);
  let focusModesLoaded       = $state(false);

  async function testDnd() {
    dndTesting = true;
    // Only ever sends enabled=false — activation is data-only.
    try { await invoke("test_dnd", { enabled: false }); } catch (e) { console.warn("[goals] test_dnd failed:", e); }
    dndTesting = false;
  }

  async function saveDnd() {
    dndSaving = true;
    try { await invoke("set_dnd_config", { config: dndConfig }); } catch (e) { console.warn("[goals] set_dnd_config failed:", e); }
    dndSaving = false;
    // Mark "set a DND threshold" onboarding step when the user saves with DND enabled.
    if (dndConfig.enabled) {
      try {
        const ob = JSON.parse(localStorage.getItem("onboardDone") ?? "{}");
        if (!ob.dndConfigured) { ob.dndConfigured = true; localStorage.setItem("onboardDone", JSON.stringify(ob)); }
      } catch (e) { console.warn("[goals] onboarding localStorage update failed:", e); }
    }
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

  async function setDndExitDuration(secs: number) {
    dndConfig = { ...dndConfig, exit_duration_secs: secs };
    await saveDnd();
  }

  async function setDndLookback(secs: number) {
    dndConfig = { ...dndConfig, focus_lookback_secs: secs };
    await saveDnd();
  }

  async function setFocusMode(identifier: string) {
    dndConfig = { ...dndConfig, focus_mode_identifier: identifier };
    await saveDnd();
  }

  async function toggleExitNotification() {
    dndConfig = { ...dndConfig, exit_notification: !dndConfig.exit_notification };
    await saveDnd();
  }

  const DND_SNR_EXIT_PRESETS: [string, number][] = [
    ["0 dB",  0],
    ["3 dB",  3],
    ["5 dB",  5],
    ["10 dB", 10],
    ["15 dB", 15],
  ];

  async function setSnrExitDb(db: number) {
    dndConfig = { ...dndConfig, snr_exit_db: db };
    await saveDnd();
  }

  let dndUnlisten: UnlistenFn | null = null;

  // Re-fetch the authoritative DND state from the backend.  Called on mount
  // and whenever the window regains visibility (e.g. user switches back to the
  // app after changing Focus settings in System Settings).
  async function refreshDndState() {
    try {
      // get_dnd_active  → app-controlled flag
      // get_dnd_status  → full pipeline snapshot (os_active, avg_score, …)
      const [appActive, status] = await Promise.all([
        invoke<boolean>("get_dnd_active"),
        invoke<{
          dnd_active:   boolean;
          os_active:    boolean | null;
          avg_score:    number;
          sample_count: number;
          window_size:  number;
          threshold:    number;
        }>("get_dnd_status"),
      ]);
      dndActive        = appActive;
      dndOsActive      = status.os_active      ?? null;
      dndAvgScore      = status.avg_score      ?? 0;
      dndSampleCount   = status.sample_count   ?? 0;
      dndWindowSize    = status.window_size    ?? 0;
      dndThresholdLive = status.threshold      ?? dndConfig.focus_threshold;
    } catch (e) { console.warn("[goals] refreshDndState failed:", e); }
  }

  onMount(async () => {
    try {
      const v = await invoke<number>("get_daily_goal");
      if (v > 0) dailyGoalMin = v;
    } catch (e) { console.warn("[goals] get_daily_goal failed:", e); }
    await loadChart();

    // Load DND config + current active state
    try {
      dndConfig = await invoke<DndConfig>("get_dnd_config");
    } catch (e) { console.warn("[goals] get_dnd_config failed:", e); }
    await refreshDndState();

    // Load available Focus modes from the OS (macOS full list, Linux/Windows default DND option).
    try {
      focusModes = await invoke<FocusModeOption[]>("list_focus_modes");
    } catch (e) { console.warn("[goals] list_focus_modes failed:", e); }
    focusModesLoaded = true;

    // Re-sync when the user switches back to the app window after making changes
    // in System Settings or another app that may have toggled Focus mode.
    const onVisible = () => { if (document.visibilityState === "visible") refreshDndState(); };
    document.addEventListener("visibilitychange", onVisible);

    // Listen for live DND state changes (from the EEG band monitor)
    const stateUnlisten = await listen<boolean>("dnd-state-changed", (ev) => {
      dndActive = ev.payload;
      if (!ev.payload) { dndExitSecsRemain = 0; dndExitHeldByLookback = false; }
    });

    // Keep exit countdown, lookback state, and activation progress fresh from
    // the ~4 Hz eligibility event.
    // os_active is read from the 5-second OS-poll cache (not live file).
    const eligibilityUnlisten = await listen<{
      dnd_active:            boolean;
      exit_secs_remaining:   number;
      exit_held_by_lookback: boolean;
      os_active:             boolean | null;
      avg_score:             number;
      sample_count:          number;
      window_size:           number;
      threshold:             number;
    }>("dnd-eligibility", (ev) => {
      dndActive             = ev.payload.dnd_active;
      dndOsActive           = ev.payload.os_active          ?? null;
      dndExitSecsRemain     = Math.ceil(ev.payload.exit_secs_remaining ?? 0);
      dndExitHeldByLookback = ev.payload.exit_held_by_lookback ?? false;
      dndAvgScore           = ev.payload.avg_score           ?? 0;
      dndSampleCount        = ev.payload.sample_count        ?? 0;
      dndWindowSize         = ev.payload.window_size         ?? 0;
      dndThresholdLive      = ev.payload.threshold           ?? dndConfig.focus_threshold;
    });

    // Background OS poll fires when system DND state changes externally
    // (user toggled in System Settings, Shortcuts automation, lock screen, etc.)
    const osChangedUnlisten = await listen<{ os_active: boolean | null }>("dnd-os-changed", (ev) => {
      dndOsActive = ev.payload.os_active ?? null;
      // If the OS cleared DND without the app doing it, the backend already
      // reconciles dnd_active and emits dnd-state-changed — no extra action needed here.
    });

    dndUnlisten = () => {
      document.removeEventListener("visibilitychange", onVisible);
      stateUnlisten();
      eligibilityUnlisten();
      osChangedUnlisten();
    };
  });

  onDestroy(() => {
    dndUnlisten?.();
  });

  async function save() {
    saving = true;
    try { await invoke("set_daily_goal", { minutes: dailyGoalMin }); } catch (e) { console.warn("[goals] set_daily_goal failed:", e); }
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
    } catch (e) { console.warn("[goals] loadChart failed:", e); }
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

  // Format exit-countdown seconds → "5m 12s" style
  function fmtExitCountdown(secs: number): string {
    if (secs <= 0) return "";
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    if (m === 0) return t("dnd.exitCountdown", { secs: String(s) });
    return t("dnd.exitCountdownLong", { min: String(m), secs: String(s) });
  }

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
           class="w-full accent-violet-500 h-2" />
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
                       ? 'border-primary/50 bg-primary/10 text-primary'
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
      <!-- Live status badge (app-controlled) + OS indicator -->
      {#if dndConfig.enabled}
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {dndActive ? 'text-violet-500' : 'text-muted-foreground/50'}">
          {dndActive ? t("dnd.statusActive") : t("dnd.statusInactive")}
        </span>
      {/if}
      <!-- System-level OS DND badge: shown when the OS state is known and
           either diverges from app state or there is no automation enabled. -->
      {#if dndOsActive !== null}
        <span class="text-[0.50rem] font-semibold tracking-wide shrink-0 px-1.5 py-0.5
                     rounded-full border
                     {dndOsActive
                       ? 'border-violet-400/40 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border dark:border-white/[0.06] text-muted-foreground/40'}">
          {dndOsActive ? "System: ON" : "System: OFF"}
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

          <!-- ── DND Exit Delay ─────────────────────────────────────────── -->
          <div class="flex flex-col gap-2 px-4 py-3.5">
            <div class="flex items-center justify-between">
              <span class="text-[0.72rem] font-semibold text-foreground">
                {t("dnd.exitDuration")}
              </span>
              <span class="text-[0.68rem] tabular-nums text-muted-foreground">
                {t("dnd.exitDurationValue", { min: String(Math.round(dndConfig.exit_duration_secs / 60)) })}
              </span>
            </div>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed -mt-0.5">
              {t("dnd.exitDurationDesc")}
            </p>
            <div class="flex items-center gap-1.5 flex-wrap">
              {#each DND_EXIT_PRESETS as [label, secs]}
                <button
                  onclick={() => setDndExitDuration(secs)}
                  class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                         transition-all cursor-pointer select-none
                         {dndConfig.exit_duration_secs === secs
                           ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                           : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                  {label}
                </button>
              {/each}
            </div>
          </div>

          <!-- ── Focus Lookback ─────────────────────────────────────────── -->
          <div class="flex flex-col gap-2 px-4 py-3.5">
            <div class="flex items-center justify-between">
              <span class="text-[0.72rem] font-semibold text-foreground">
                {t("dnd.focusLookback")}
              </span>
              <span class="text-[0.68rem] tabular-nums text-muted-foreground">
                {dndConfig.focus_lookback_secs >= 60
                  ? t("dnd.focusLookbackValue_min", { min: String(Math.round(dndConfig.focus_lookback_secs / 60)) })
                  : t("dnd.focusLookbackValue",     { secs: String(dndConfig.focus_lookback_secs) })}
              </span>
            </div>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed -mt-0.5">
              {t("dnd.focusLookbackDesc")}
            </p>
            <div class="flex items-center gap-1.5 flex-wrap">
              {#each DND_LOOKBACK_PRESETS as [label, secs]}
                <button
                  onclick={() => setDndLookback(secs)}
                  class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                         transition-all cursor-pointer select-none
                         {dndConfig.focus_lookback_secs === secs
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

          <!-- ── Exit notification toggle ──────────────────────────────── -->
          <button
            onclick={toggleExitNotification}
            class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
                   hover:bg-slate-50 dark:hover:bg-white/[0.02]">
            <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                        {dndConfig.exit_notification ? 'bg-violet-500' : 'bg-muted dark:bg-white/[0.08]'}">
              <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                          {dndConfig.exit_notification ? 'translate-x-4' : 'translate-x-0.5'}"></div>
            </div>
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
                {t("dnd.exitNotification")}
              </span>
              <span class="text-[0.58rem] text-muted-foreground leading-tight">
                {t("dnd.exitNotificationDesc")}
              </span>
            </div>
            <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                         {dndConfig.exit_notification ? 'text-violet-500' : 'text-muted-foreground/50'}">
              {dndConfig.exit_notification ? "ON" : "OFF"}
            </span>
          </button>

          <!-- ── SNR exit threshold ──────────────────────────────────────── -->
          <div class="px-4 py-3.5 space-y-2">
            <div class="flex items-center justify-between">
              <span class="text-[0.72rem] font-semibold text-foreground">
                {t("dnd.snrExitThreshold")}
              </span>
              <span class="text-[0.62rem] font-bold text-violet-500">
                {dndConfig.snr_exit_db} dB
              </span>
            </div>
            <p class="text-[0.62rem] text-muted-foreground leading-relaxed -mt-0.5">
              {t("dnd.snrExitThresholdDesc")}
            </p>
            <div class="flex items-center gap-1.5 flex-wrap">
              {#each DND_SNR_EXIT_PRESETS as [label, db]}
                <button
                  onclick={() => setSnrExitDb(db)}
                  class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold
                         transition-all cursor-pointer select-none
                         {dndConfig.snr_exit_db === db
                           ? 'border-violet-500/50 bg-violet-500/10 dark:bg-violet-500/15 text-violet-600 dark:text-violet-400'
                           : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
                  {label}
                </button>
              {/each}
            </div>
          </div>

          <!-- ── Active state indicator + exit countdown timer ───────────── -->
          <!--
            States:
            1. violet  — DND active, score above threshold (focused, no countdown)
            2. sky     — DND active, score below threshold, lookback delaying exit
            3. amber   — DND active, score below threshold, exit countdown running
            4. grey    — DND not active
          -->
          {@const isHeld     = dndActive && dndExitHeldByLookback}
          {@const isCounting = dndActive && !dndExitHeldByLookback && dndExitSecsRemain > 0}
          {@const isActive   = dndActive && !dndExitHeldByLookback && dndExitSecsRemain === 0}

          <!-- Status row: dot · label · meta/OS badge -->
          <div class="flex items-center gap-3 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
            <!-- Pulsing dot -->
            <span class="relative flex h-2.5 w-2.5 shrink-0">
              {#if isHeld}
                <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-sky-400 opacity-75"></span>
                <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-sky-500"></span>
              {:else if isCounting}
                <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-75"></span>
                <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-amber-500"></span>
              {:else if isActive}
                <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-violet-400 opacity-75"></span>
                <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-violet-500"></span>
              {:else}
                <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-muted-foreground/20"></span>
              {/if}
            </span>

            <!-- Status label -->
            <span class="text-[0.62rem] font-semibold
                         {isHeld     ? 'text-sky-600 dark:text-sky-400'
                        : isCounting ? 'text-amber-600 dark:text-amber-400'
                        : isActive   ? 'text-violet-600 dark:text-violet-400'
                                     : 'text-muted-foreground/60'}">
              {#if isHeld}
                {t("dnd.exitHeld", { ago: String(dndConfig.focus_lookback_secs) })}
              {:else if isCounting}
                {t("dnd.exitingFocusMode")}
              {:else}
                {dndActive ? t("dnd.statusActive") : t("dnd.statusInactive")}
              {/if}
            </span>

            <!-- Right-side meta -->
            <div class="ml-auto flex flex-col items-end gap-0.5">
              <span class="text-[0.50rem] text-muted-foreground/40 text-right leading-relaxed">
                {focusModes.find(m => m.identifier === dndConfig.focus_mode_identifier)?.name ?? "Do Not Disturb"}
                · ≥{Math.round(dndConfig.focus_threshold)} for {dndConfig.duration_secs}s
              </span>
              {#if dndOsActive !== null}
                <span class="text-[0.50rem] font-medium
                             {dndOsActive && !dndActive
                               ? 'text-amber-500 dark:text-amber-400'
                               : 'text-muted-foreground/35'}">
                  {#if dndOsActive && !dndActive}
                    ⚠ System Focus active (set externally)
                  {:else}
                    System: {dndOsActive ? "ON" : "OFF"}
                  {/if}
                </span>
              {/if}
            </div>
          </div>

          <!-- ── Activation countdown bar ───────────────────────────────── -->
          <!--
            Two phases, unified into one bar:

            A) Score below threshold → score-fill bar (how close you are to
               the threshold).  No countdown yet — we don't know when the
               score will cross the line.

            B) Score at/above threshold, window still accumulating → window-
               fill bar with MM:SS countdown.  Once the window is full the
               backend activates DND in the same tick.
          -->
          {#if !dndActive && dndSampleCount > 0}
            {@const scorePct      = dndThresholdLive > 0
              ? Math.min(100, (dndAvgScore / dndThresholdLive) * 100) : 0}
            {@const windowFillPct = dndWindowSize > 0
              ? Math.min(100, (dndSampleCount / dndWindowSize) * 100) : 0}
            {@const windowRemSecs = Math.max(0, Math.round((dndWindowSize - dndSampleCount) / 4))}
            {@const scoreAbove    = dndAvgScore >= dndThresholdLive}
            {@const countingDown  = scoreAbove && windowRemSecs > 0}
            {@const amm           = Math.floor(windowRemSecs / 60)}
            {@const ass           = windowRemSecs % 60}

            <div class="px-4 pb-3.5 pt-2 flex flex-col gap-2 bg-slate-50 dark:bg-[#111118]
                        border-t border-border/50 dark:border-white/[0.04]">

              <!-- Header row -->
              <div class="flex items-center justify-between gap-2">
                <span class="text-[0.55rem] font-semibold tracking-widest uppercase
                             {scoreAbove ? 'text-violet-500/70 dark:text-violet-400/60'
                                         : 'text-muted-foreground/40'}">
                  {scoreAbove ? t("dnd.untilActivation") : t("dnd.buildingLabel")}
                </span>
                {#if countingDown}
                  <span class="text-[1.1rem] font-black tabular-nums leading-none
                               text-violet-500 dark:text-violet-400 ml-auto"
                        style="font-variant-numeric: tabular-nums;">
                    {String(amm).padStart(2, "0")}:{String(ass).padStart(2, "0")}
                  </span>
                {:else if scoreAbove}
                  <span class="text-[0.65rem] font-semibold text-violet-600 dark:text-violet-400 ml-auto">
                    {t("dnd.activating")}
                  </span>
                {:else}
                  <span class="text-[0.65rem] font-semibold tabular-nums text-muted-foreground/60 ml-auto">
                    {dndAvgScore.toFixed(0)} / {dndThresholdLive.toFixed(0)}
                  </span>
                {/if}
              </div>

              <!-- Single countdown / progress bar -->
              <div class="relative h-2.5 w-full rounded-full overflow-hidden
                          bg-muted/60 dark:bg-white/[0.06]">
                {#if scoreAbove}
                  <!-- Phase B: window-fill countdown bar (violet) -->
                  <div class="absolute inset-y-0 left-0 rounded-full
                              transition-[width] duration-1000 ease-linear
                              bg-violet-500 dark:bg-violet-400"
                       style="width:{windowFillPct}%"></div>
                {:else}
                  <!-- Phase A: score-to-threshold bar (blue) -->
                  <div class="absolute inset-y-0 left-0 rounded-full
                              transition-[width] duration-1000 ease-linear
                              {scorePct > 70
                                ? 'bg-blue-400 dark:bg-blue-500'
                                : 'bg-blue-500/60 dark:bg-blue-600/60'}"
                       style="width:{scorePct}%"></div>
                {/if}
              </div>

              <!-- Axis labels -->
              <div class="flex justify-between text-[0.42rem] text-muted-foreground/35
                          tabular-nums select-none -mt-0.5">
                {#if scoreAbove}
                  <span>0s</span>
                  <span class="text-violet-500/50 dark:text-violet-400/50">activates</span>
                  <span>{dndConfig.duration_secs}s</span>
                {:else}
                  <span>0</span>
                  <span class="text-blue-500/50 dark:text-blue-400/50">≥{dndThresholdLive.toFixed(0)} to start</span>
                  <span>100</span>
                {/if}
              </div>
            </div>
          {/if}

          <!-- ── Exit countdown bar ──────────────────────────────────────── -->
          <!--
            Counting: amber bar fills left → right over exit_duration_secs.
                      When full the backend clears DND.
            Held:     lookback found recent focus — pulsing sky bar, no timer.
          -->
          {#if isCounting || isHeld}
            {@const totalSecs   = dndConfig.exit_duration_secs}
            {@const elapsedSecs = isCounting ? totalSecs - dndExitSecsRemain : 0}
            {@const pct         = isCounting ? Math.min(100, (elapsedSecs / totalSecs) * 100) : 0}
            {@const mm          = isCounting ? Math.floor(dndExitSecsRemain / 60) : 0}
            {@const ss          = isCounting ? dndExitSecsRemain % 60             : 0}

            <div class="px-4 pb-3.5 pt-2 flex flex-col gap-2 bg-slate-50 dark:bg-[#111118]
                        border-t border-border/50 dark:border-white/[0.04]">

              <!-- Header row -->
              <div class="flex items-center justify-between gap-2">
                <span class="text-[0.55rem] font-semibold tracking-widest uppercase
                             {isCounting
                               ? 'text-amber-500/70 dark:text-amber-400/60'
                               : 'text-sky-500/70 dark:text-sky-400/60'}">
                  {isCounting ? t("dnd.untilExit") : t("dnd.exitHeld", { ago: String(dndConfig.focus_lookback_secs) })}
                </span>
                {#if isCounting}
                  <span class="text-[1.1rem] font-black tabular-nums leading-none
                               text-amber-500 dark:text-amber-400 ml-auto"
                        style="font-variant-numeric: tabular-nums;">
                    {String(mm).padStart(2, "0")}:{String(ss).padStart(2, "0")}
                  </span>
                {/if}
              </div>

              <!-- Progress bar -->
              <div class="relative h-2.5 w-full rounded-full overflow-hidden
                          bg-muted/60 dark:bg-white/[0.06]">
                {#if isCounting}
                  <div class="absolute inset-y-0 left-0 rounded-full
                              bg-amber-400 dark:bg-amber-500
                              transition-[width] duration-1000 ease-linear"
                       style="width:{pct}%"></div>
                {:else}
                  <!-- held: pulsing sky sliver at the left edge -->
                  <div class="absolute inset-y-0 left-0 w-5 rounded-full
                              bg-sky-400 dark:bg-sky-500 opacity-70 animate-pulse"></div>
                {/if}
              </div>

              <!-- Axis labels -->
              <div class="flex justify-between text-[0.42rem] text-muted-foreground/35
                          tabular-nums select-none -mt-0.5">
                {#if isCounting}
                  <span>0s</span>
                  <span class="text-amber-500/50 dark:text-amber-400/50">exits</span>
                  <span>{totalSecs >= 120 ? `${Math.round(totalSecs / 60)}m` : `${totalSecs}s`}</span>
                {:else}
                  <span class="text-sky-500/50 dark:text-sky-400/50 w-full text-center">
                    reset while focus was recent
                  </span>
                {/if}
              </div>
            </div>
          {/if}
        {/if}

        <!-- ── Force-off row — only shown while focus mode is active ──────── -->
        {#if dndActive}
          <div class="flex items-center gap-3 px-4 py-3 border-t border-border dark:border-white/[0.05]">
            <span class="text-[0.62rem] text-muted-foreground/60">{t("dnd.forceOff")}</span>
            <button
              onclick={testDnd}
              disabled={dndTesting}
              class="ml-auto shrink-0 text-[0.6rem] font-medium px-2.5 py-1 rounded-md border
                     transition-colors cursor-pointer select-none
                     border-violet-400/40 bg-violet-500/10 text-violet-600 dark:text-violet-400
                     hover:bg-violet-500/20
                     disabled:opacity-40 disabled:cursor-not-allowed">
              {dndTesting ? "…" : t("dnd.forceOffBtn")}
            </button>
          </div>
        {/if}

      </div>
    </Card>

    <!-- macOS requirement note -->
    <p class="text-[0.58rem] text-muted-foreground/50 px-0.5 flex items-center gap-1">
      <span>🍎</span>
      <span>{t("dnd.requiresMacOS")}</span>
    </p>
  </div>

</section>
