<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Focus Timer / Pomodoro Mode
     Configurable work/break timer with automatic EEG labelling.
-->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { listen }             from "@tauri-apps/api/event";
  import { t }           from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import { Button }             from "$lib/components/ui/button";
  import { fmtDuration, fmtTimeShort as fmtTime, dateToCompactKey, fmtCountdown } from "$lib/format";
  import { openLabel } from "$lib/navigation";

  // ── Presets ────────────────────────────────────────────────────────────────
  type Preset = "pomodoro" | "deepWork" | "shortFocus" | "custom";

  interface PresetConfig {
    workMins:      number;
    breakMins:     number;
    longBreakMins: number;
    longBreakEvery:number;
  }

  const PRESETS: Record<Exclude<Preset, "custom">, PresetConfig> = {
    pomodoro:   { workMins: 25, breakMins: 5,  longBreakMins: 15, longBreakEvery: 4 },
    deepWork:   { workMins: 50, breakMins: 10, longBreakMins: 30, longBreakEvery: 2 },
    shortFocus: { workMins: 15, breakMins: 5,  longBreakMins: 15, longBreakEvery: 4 },
  };

  // ── Config state ────────────────────────────────────────────────────────────
  const LS_KEY = "skill.focusTimer.config";

  interface TimerConfig {
    preset:        Preset;
    workMins:      number;
    breakMins:     number;
    longBreakMins: number;
    longBreakEvery: number;
    autoLabel:     boolean;
    ttsEnabled:    boolean;
  }

  function loadConfig(): TimerConfig {
    try {
      const raw = localStorage.getItem(LS_KEY);
      if (raw) return { ...defaultConfig(), ...JSON.parse(raw) };
    } catch { /* ignore */ }
    return defaultConfig();
  }

  function defaultConfig(): TimerConfig {
    return { preset: "pomodoro", workMins: 25, breakMins: 5, longBreakMins: 15, longBreakEvery: 4, autoLabel: true, ttsEnabled: false };
  }

  const _init = loadConfig();
  let selectedPreset  = $state<Preset>(_init.preset);
  let workMins        = $state(_init.workMins);
  let breakMins       = $state(_init.breakMins);
  let longBreakMins   = $state(_init.longBreakMins);
  let longBreakEvery  = $state(_init.longBreakEvery);
  let autoLabel       = $state(_init.autoLabel);
  let ttsEnabled      = $state(_init.ttsEnabled);

  // Persist config whenever it changes (idle state only — don't interrupt a running timer)
  $effect(() => {
    const cfg: TimerConfig = { preset: selectedPreset, workMins, breakMins, longBreakMins, longBreakEvery, autoLabel, ttsEnabled };
    try { localStorage.setItem(LS_KEY, JSON.stringify(cfg)); } catch { /* private/full */ }
  });

  // ── TTS helpers ─────────────────────────────────────────────────────────────
  // Fire-and-forget: queued on the Rust TTS worker, never blocks the JS timer.
  function speakAsync(text: string) {
    if (!ttsEnabled) return;
    invoke("tts_speak", { text, voice: null }).catch(e => console.warn("[focus-timer] tts_speak failed:", e));
  }

  // Announcement spoken at the start of each phase.
  function phaseAnnouncement(p: Phase): string {
    const mins = p === "work"      ? workMins      :
                 p === "break"     ? breakMins     :
                 p === "longBreak" ? longBreakMins : 0;
    const label = p === "work"      ? "Focus time"   :
                  p === "break"     ? "Break time"   :
                  p === "longBreak" ? "Long break"   : "";
    return `${label}. ${mins} minute${mins !== 1 ? "s" : ""}.`;
  }

  function applyPreset(p: Exclude<Preset, "custom">) {
    selectedPreset = p;
    const c = PRESETS[p];
    workMins       = c.workMins;
    breakMins      = c.breakMins;
    longBreakMins  = c.longBreakMins;
    longBreakEvery = c.longBreakEvery;
  }

  // ── Session log ──────────────────────────────────────────────────────────────

  interface LogEntry {
    type:        "work" | "break" | "longBreak";
    startUtc:    number;   // unix seconds
    durationSecs: number;  // planned duration (what was configured)
    completedAt: number;   // unix seconds
  }

  const LS_LOG_PREFIX = "skill.focusTimer.log.";

  /** YYYYMMDD string for today (local time). */
  function todayKey(): string {
    return dateToCompactKey(new Date());
  }

  function loadLog(): LogEntry[] {
    try {
      const raw = localStorage.getItem(LS_LOG_PREFIX + todayKey());
      if (raw) return JSON.parse(raw) as LogEntry[];
    } catch { /* ignore */ }
    return [];
  }

  function saveLog(entries: LogEntry[]) {
    try { localStorage.setItem(LS_LOG_PREFIX + todayKey(), JSON.stringify(entries)); } catch { /* full/private */ }
  }

  let sessionLog = $state<LogEntry[]>(loadLog());

  function pushLog(entry: LogEntry) {
    sessionLog = [...sessionLog, entry];
    saveLog(sessionLog);
  }

  function clearLog() {
    sessionLog = [];
    try { localStorage.removeItem(LS_LOG_PREFIX + todayKey()); } catch { /* ignore */ }
  }

  // Derived totals
  const focusSecs  = $derived(sessionLog.filter(e => e.type === "work").reduce((s, e) => s + e.durationSecs, 0));
  const breakSecs  = $derived(sessionLog.filter(e => e.type !== "work").reduce((s, e) => s + e.durationSecs, 0));
  const logTotalSecs = $derived(focusSecs + breakSecs);
  const cyclesDone = $derived(sessionLog.filter(e => e.type === "work").length);

  // Whether the log panel is expanded
  let logOpen = $state(true);

  // ── Timer state ─────────────────────────────────────────────────────────────
  type Phase = "idle" | "work" | "break" | "longBreak";

  let phase         = $state<Phase>("idle");
  let paused        = $state(false);
  let secondsLeft   = $state(0);
  let sessionsDone  = $state(0);
  let phaseStartUtc = $state(0);

  let intervalId: ReturnType<typeof setInterval> | null = null;

  // Total seconds for current phase (for progress ring)
  let totalSecs = $derived(
    phase === "work"      ? workMins * 60       :
    phase === "break"     ? breakMins * 60      :
    phase === "longBreak" ? longBreakMins * 60  : 0
  );

  // SVG progress ring
  const RING_R   = 80;
  const RING_CX  = 96;
  const RING_CY  = 96;
  const CIRCUM   = 2 * Math.PI * RING_R;
  let ringOffset = $derived(
    totalSecs > 0
      ? CIRCUM * (1 - secondsLeft / totalSecs)
      : 0
  );

  // Formatted time MM:SS
  let timeDisplay = $derived(() => fmtCountdown(secondsLeft));

  // Phase label
  let phaseLabel = $derived(
    phase === "work"      ? t("focusTimer.workPhase")  :
    phase === "break"     ? t("focusTimer.breakPhase") :
    phase === "longBreak" ? t("focusTimer.longBreak")  :
    t("focusTimer.title")
  );

  // Ring color per phase
  let ringColor = $derived(
    phase === "work"      ? "#3b82f6" :   // blue
    phase === "break"     ? "#22c55e" :   // green
    phase === "longBreak" ? "var(--color-violet-500)" :
    "#64748b"                             // slate
  );

  // ── Tick ───────────────────────────────────────────────────────────────────
  function tick() {
    if (paused) return;
    if (secondsLeft <= 0) {
      onPhaseComplete();
      return;
    }
    secondsLeft--;
    // Speak countdown digits for the final 5 seconds of any phase.
    if (secondsLeft > 0 && secondsLeft <= 5) {
      speakAsync(String(secondsLeft));
    }
  }

  async function onPhaseComplete() {
    clearInterval(intervalId!);
    intervalId = null;

    const completedAt = Math.floor(Date.now() / 1000);

    if (phase === "work") {
      speakAsync("Time's up. Great work!");
      sessionsDone++;

      // Log the completed focus session
      pushLog({
        type:         "work",
        startUtc:     phaseStartUtc,
        durationSecs: workMins * 60,
        completedAt,
      });

      // Auto-label the completed focus session
      if (autoLabel) {
        try {
          const label = `${t("focusTimer.workPhase")} — ${workMins}min`;
          await invoke("submit_label", {
            labelStartUtc: phaseStartUtc,
            text: label,
          });
        } catch (e) { console.warn("[focus-timer] submit_label failed:", e); }
      }

      // Decide next phase
      const nextPhase = sessionsDone % longBreakEvery === 0 ? "longBreak" : "break";
      startPhase(nextPhase);
    } else {
      // Log the completed break
      pushLog({
        type:         phase as "break" | "longBreak",
        startUtc:     phaseStartUtc,
        durationSecs: phase === "longBreak" ? longBreakMins * 60 : breakMins * 60,
        completedAt,
      });

      speakAsync("Break over. Ready to focus?");
      // After break → go back to idle (don't auto-start, notify instead)
      phase       = "idle";
      secondsLeft = 0;
      // Show a toast (best-effort)
      try {
        await invoke("show_toast_from_frontend", {
          level: "info",
          title: t("focusTimer.breakComplete"),
          message: "",
        });
      } catch (e) { console.warn("[focus-timer] show_toast_from_frontend failed:", e); }
    }
  }

  function startPhase(p: Phase) {
    phase = p;
    paused = false;
    phaseStartUtc = Math.floor(Date.now() / 1000);
    secondsLeft =
      p === "work"      ? workMins * 60       :
      p === "break"     ? breakMins * 60      :
      p === "longBreak" ? longBreakMins * 60  : 0;
    clearInterval(intervalId!);
    intervalId = setInterval(tick, 1000);
    speakAsync(phaseAnnouncement(p));
  }

  function handleStart() {
    if (phase === "idle") {
      startPhase("work");
    } else if (paused) {
      paused = false;
    }
  }

  function handlePause() {
    paused = !paused;
  }

  function handleStop() {
    clearInterval(intervalId!);
    intervalId = null;
    phase       = "idle";
    paused      = false;
    secondsLeft = 0;
  }

  function handleReset() {
    handleStop();
    sessionsDone = 0;
  }



  function skipPhase() {
    clearInterval(intervalId!);
    intervalId = null;
    onPhaseComplete();
  }

  onMount(() => {
    // Auto-start when the window is opened via `timer` WS command
    // (URL param for fresh windows; event for already-open windows)
    const params = new URLSearchParams(window.location.search);
    if (params.get("autostart") === "1" && phase === "idle") {
      handleStart();
    }

    const unlisten = listen("focus-timer-start", () => {
      if (phase === "idle") handleStart();
    });

    return () => { unlisten.then(fn => fn()); };
  });

  onDestroy(() => {
    if (intervalId) clearInterval(intervalId);
  });

  useWindowTitle("window.title.focusTimer");
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden">

  <!-- ── Main content ─────────────────────────────────────────────────────── -->
  <div class="min-h-0 flex-1 flex flex-col items-center justify-start overflow-y-auto px-5 py-4 gap-5">

    <!-- Phase label -->
    <div class="text-[0.68rem] font-semibold tracking-widest uppercase
                text-muted-foreground/70 mt-1">
      {phaseLabel}
    </div>

    <!-- Progress ring + time -->
    <div role="progressbar"
         aria-label={phaseLabel}
         aria-valuenow={totalSecs > 0 ? totalSecs - secondsLeft : 0}
         aria-valuemin={0}
         aria-valuemax={totalSecs}>
      <svg width="192" height="192" viewBox="0 0 192 192" aria-hidden="true">
        <!-- Background ring -->
        <circle
          cx={RING_CX} cy={RING_CY} r={RING_R}
          fill="none" stroke="currentColor"
          stroke-width="8"
          class="text-muted/30"
        />
        <!-- Progress ring -->
        <circle
          cx={RING_CX} cy={RING_CY} r={RING_R}
          fill="none"
          stroke={ringColor}
          stroke-width="8"
          stroke-linecap="round"
          stroke-dasharray={CIRCUM}
          stroke-dashoffset={ringOffset}
          transform="rotate(-90 96 96)"
          style="transition: stroke-dashoffset 0.9s linear, stroke 0.4s ease;"
        />
        <!--
          Time text lives inside the SVG so its size is always in SVG user-units
          (proportional to the 192×192 viewBox), not CSS rem. This prevents the
          digits from escaping the ring when system display scale or browser zoom
          is set above 100%.
        -->
        <text
          x="96"
          y={phase !== "idle" ? "91" : "96"}
          text-anchor="middle"
          dominant-baseline="middle"
          font-family="ui-monospace, 'JetBrains Mono', monospace"
          font-weight="700"
          font-size="36"
          letter-spacing="-0.5"
          fill={ringColor}
        >{timeDisplay()}</text>
        {#if phase !== "idle"}
          <text
            x="96"
            y="116"
            text-anchor="middle"
            dominant-baseline="middle"
            font-family="ui-sans-serif, system-ui, sans-serif"
            font-size="10"
            letter-spacing="1.5"
            fill="currentColor"
            opacity="0.45"
          >{paused ? "PAUSED" : "RUNNING"}</text>
        {/if}
      </svg>
    </div>

    <!-- Control buttons -->
    <div class="flex items-center gap-3">
      {#if phase === "idle"}
        <Button onclick={handleStart} class="px-8 h-9 text-[0.82rem]">
          {t("focusTimer.start")}
        </Button>
      {:else}
        <Button variant="outline" onclick={handleStop} class="h-8 px-4 text-[0.75rem]">
          {t("focusTimer.stop")}
        </Button>
        <Button
          onclick={handlePause}
          class="h-8 px-5 text-[0.78rem]"
          style="background-color:{paused ? ringColor : 'transparent'}; border:1px solid {ringColor}; color:{paused ? '#fff' : ringColor}"
        >
          {paused ? t("focusTimer.resume") : t("focusTimer.pause")}
        </Button>
        <Button variant="ghost" onclick={skipPhase} class="h-8 px-3 text-[0.72rem] text-muted-foreground">
          Skip →
        </Button>
      {/if}
    </div>

    <!-- Label button — always accessible so users can annotate at any moment -->
    <button
      onclick={openLabel}
      class="flex items-center gap-1.5 rounded-lg border border-border
             dark:border-white/[0.08] bg-muted/20 hover:bg-muted/40
             px-3.5 py-1.5 text-[0.72rem] font-medium text-muted-foreground
             hover:text-foreground transition-colors"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
           stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
           class="w-3.5 h-3.5 shrink-0">
        <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
        <line x1="7" y1="7" x2="7.01" y2="7"/>
      </svg>
      {t("dashboard.addLabel")}
    </button>

    <!-- Session dots -->
    {#if sessionsDone > 0 || phase !== "idle"}
      <div class="flex items-center gap-1.5">
        {#each Array(Math.max(sessionsDone + (phase === "work" ? 1 : 0), 1)) as _, i}
          <div
            class="w-2.5 h-2.5 rounded-full transition-colors"
            style="background-color: {i < sessionsDone ? ringColor : (phase === 'work' ? ringColor + '50' : '#64748b30')}"
          ></div>
        {/each}
      </div>
    {/if}

    <!-- Presets -->
    <div class="w-full flex flex-col gap-2">
      <p class="text-[0.62rem] font-semibold tracking-widest uppercase text-muted-foreground/60">
        Presets
      </p>
      <div class="grid grid-cols-3 gap-2">
        {#each (["pomodoro", "deepWork", "shortFocus"] as const) as p}
          <button
            onclick={() => applyPreset(p)}
            disabled={phase !== "idle"}
            class="rounded-lg border px-2.5 py-2 text-[0.68rem] font-medium text-center
                   transition-colors focus:outline-none
                   {selectedPreset === p
                     ? 'border-primary/50 bg-primary/10 text-primary'
                     : 'border-border bg-muted/20 text-muted-foreground hover:bg-muted/40'}
                   disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t(`focusTimer.preset.${p}`)}
          </button>
        {/each}
      </div>
    </div>

    <!-- Manual config -->
    <div class="w-full flex flex-col gap-2">
      <p class="text-[0.62rem] font-semibold tracking-widest uppercase text-muted-foreground/60">
        {t("focusTimer.preset.custom")}
      </p>
      <div class="grid grid-cols-2 gap-3">
        <!-- Work -->
        <div class="flex flex-col gap-1">
          <label for="ft-work-mins" class="text-[0.65rem] text-muted-foreground/70">{t("focusTimer.workMins")}</label>
          <input id="ft-work-mins" type="number" min="1" max="120" bind:value={workMins}
                 disabled={phase !== "idle"}
                 oninput={() => selectedPreset = "custom"}
                 class="w-full px-2 py-1 text-[0.78rem] rounded-md
                        border border-border dark:border-white/[0.08]
                        bg-background text-foreground
                     focus:outline-none focus:ring-1 focus:ring-ring/40
                        disabled:opacity-40" />
        </div>
        <!-- Break -->
        <div class="flex flex-col gap-1">
          <label for="ft-break-mins" class="text-[0.65rem] text-muted-foreground/70">{t("focusTimer.breakMins")}</label>
          <input id="ft-break-mins" type="number" min="1" max="60" bind:value={breakMins}
                 disabled={phase !== "idle"}
                 oninput={() => selectedPreset = "custom"}
                 class="w-full px-2 py-1 text-[0.78rem] rounded-md
                        border border-border dark:border-white/[0.08]
                        bg-background text-foreground
                     focus:outline-none focus:ring-1 focus:ring-ring/40
                        disabled:opacity-40" />
        </div>
        <!-- Long break -->
        <div class="flex flex-col gap-1">
          <label for="ft-long-break-mins" class="text-[0.65rem] text-muted-foreground/70">{t("focusTimer.longBreakMins")}</label>
          <input id="ft-long-break-mins" type="number" min="1" max="120" bind:value={longBreakMins}
                 disabled={phase !== "idle"}
                 oninput={() => selectedPreset = "custom"}
                 class="w-full px-2 py-1 text-[0.78rem] rounded-md
                        border border-border dark:border-white/[0.08]
                        bg-background text-foreground
                     focus:outline-none focus:ring-1 focus:ring-ring/40
                        disabled:opacity-40" />
        </div>
        <!-- Long break every -->
        <div class="flex flex-col gap-1">
          <label for="ft-long-break-every" class="text-[0.65rem] text-muted-foreground/70">
            {t("focusTimer.longBreakEvery")}
          </label>
          <input id="ft-long-break-every" type="number" min="1" max="10" bind:value={longBreakEvery}
                 disabled={phase !== "idle"}
                 oninput={() => selectedPreset = "custom"}
                 class="w-full px-2 py-1 text-[0.78rem] rounded-md
                        border border-border dark:border-white/[0.08]
                        bg-background text-foreground
                     focus:outline-none focus:ring-1 focus:ring-ring/40
                        disabled:opacity-40" />
        </div>
      </div>
    </div>

    <!-- Auto-label toggle -->
    <div class="w-full flex items-start gap-3 rounded-xl border border-border
                dark:border-white/[0.07] bg-muted/20 px-3 py-2.5">
      <input
        type="checkbox"
        id="auto-label"
        bind:checked={autoLabel}
        class="mt-0.5 w-3.5 h-3.5 rounded accent-violet-500"
      />
      <div class="flex flex-col gap-0.5">
        <label for="auto-label" class="text-[0.75rem] font-medium cursor-pointer">
          {t("focusTimer.autoLabel")}
        </label>
        <p class="text-[0.62rem] text-muted-foreground/60 leading-snug">
          {t("focusTimer.autoLabelDesc")}
        </p>
      </div>
    </div>

    <!-- TTS toggle -->
    <div class="w-full flex items-start gap-3 rounded-xl border border-border
                dark:border-white/[0.07] bg-muted/20 px-3 py-2.5">
      <input
        type="checkbox"
        id="ft-tts"
        bind:checked={ttsEnabled}
        class="mt-0.5 w-3.5 h-3.5 rounded accent-violet-500"
      />
      <div class="flex flex-col gap-0.5">
        <label for="ft-tts" class="text-[0.75rem] font-medium cursor-pointer">
          {t("focusTimer.tts")}
        </label>
        <p class="text-[0.62rem] text-muted-foreground/60 leading-snug">
          {t("focusTimer.ttsDesc")}
        </p>
      </div>
    </div>

    <!-- Reset -->
    {#if sessionsDone > 0 && phase === "idle"}
      <button
        onclick={handleReset}
        class="text-[0.62rem] text-muted-foreground/40 hover:text-muted-foreground/70
               underline underline-offset-2 transition-colors"
      >
        {t("focusTimer.reset")} (reset counter)
      </button>
    {/if}

    <!-- ── Session Log ──────────────────────────────────────────────────── -->
    <div class="w-full rounded-xl border border-border dark:border-white/[0.07]
                bg-muted/10 overflow-hidden">

      <!-- Collapsible header -->
      <button
        onclick={() => logOpen = !logOpen}
        class="w-full flex items-center justify-between px-3.5 py-2.5
               hover:bg-muted/30 transition-colors cursor-pointer text-left"
      >
        <div class="flex items-center gap-2">
          <!-- Calendar icon -->
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"
               stroke-linecap="round" stroke-linejoin="round"
               class="w-3.5 h-3.5 text-muted-foreground/60 shrink-0">
            <rect x="1" y="3" width="14" height="12" rx="1.5"/>
            <path d="M1 7h14M5 1v4M11 1v4"/>
          </svg>
          <span class="text-[0.68rem] font-semibold tracking-wide text-foreground/80">
            {t("focusTimer.log.title")}
          </span>
          {#if cyclesDone > 0}
            <span class="text-[0.58rem] font-medium px-1.5 py-0.5 rounded-full
                         bg-primary/12 text-primary">
              {cyclesDone === 1
                ? t("focusTimer.log.cycles",      { n: cyclesDone })
                : t("focusTimer.log.cyclesPlural", { n: cyclesDone })}
            </span>
          {/if}
        </div>
        <!-- Chevron -->
        <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
             stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 text-muted-foreground/40 shrink-0 transition-transform
                    {logOpen ? 'rotate-180' : ''}">
          <polyline points="2 4 6 8 10 4"/>
        </svg>
      </button>

      {#if logOpen}
        <!-- Summary stats strip -->
        {#if cyclesDone > 0 || sessionLog.length > 0}
          <div class="grid grid-cols-3 divide-x divide-border dark:divide-white/[0.06]
                      border-t border-border dark:border-white/[0.07]">
            <!-- Focus total -->
            <div class="flex flex-col items-center py-2.5 gap-0.5">
              <span class="text-[0.48rem] font-semibold uppercase tracking-widest
                           text-primary/70">
                {t("focusTimer.log.focusTime")}
              </span>
              <span class="text-[0.82rem] font-bold tabular-nums text-primary">
                {fmtDuration(focusSecs)}
              </span>
            </div>
            <!-- Break total -->
            <div class="flex flex-col items-center py-2.5 gap-0.5">
              <span class="text-[0.48rem] font-semibold uppercase tracking-widest
                           text-green-500/70">
                {t("focusTimer.log.breakTime")}
              </span>
              <span class="text-[0.82rem] font-bold tabular-nums text-green-600 dark:text-green-400">
                {fmtDuration(breakSecs)}
              </span>
            </div>
            <!-- Combined total -->
            <div class="flex flex-col items-center py-2.5 gap-0.5">
              <span class="text-[0.48rem] font-semibold uppercase tracking-widest
                           text-muted-foreground/50">
                {t("focusTimer.log.totalTime")}
              </span>
              <span class="text-[0.82rem] font-bold tabular-nums text-foreground/70">
                {fmtDuration(logTotalSecs)}
              </span>
            </div>
          </div>
        {/if}

        <!-- Entry list -->
        <div class="border-t border-border dark:border-white/[0.06]
                    max-h-52 overflow-y-auto flex flex-col">
          {#if sessionLog.length === 0}
            <p class="text-center text-[0.62rem] text-muted-foreground/40
                      italic py-4 px-3">
              {t("focusTimer.log.empty")}
            </p>
          {:else}
            <!-- Reverse so newest appears first -->
            {#each [...sessionLog].reverse() as entry, ri}
              {@const isWork      = entry.type === "work"}
              {@const isLongBreak = entry.type === "longBreak"}
              <div class="flex items-center gap-2.5 px-3.5 py-2
                          {ri < sessionLog.length - 1
                            ? 'border-b border-border/50 dark:border-white/[0.04]'
                            : ''}">
                <!-- Phase dot -->
                <span class="w-2 h-2 rounded-full shrink-0
                             {isWork      ? 'bg-blue-500'
                             : isLongBreak ? 'bg-violet-500'
                             :               'bg-green-500'}">
                </span>
                <!-- Phase name -->
                <span class="text-[0.65rem] font-medium flex-1
                             {isWork      ? 'text-blue-700 dark:text-blue-300'
                             : isLongBreak ? 'text-violet-700 dark:text-violet-300'
                             :               'text-green-700 dark:text-green-300'}">
                  {isWork
                    ? t("focusTimer.log.work")
                    : isLongBreak
                      ? t("focusTimer.log.longBreak")
                      : t("focusTimer.log.break")}
                </span>
                <!-- Duration -->
                <span class="text-[0.6rem] tabular-nums font-semibold
                             text-foreground/60">
                  {fmtDuration(entry.durationSecs)}
                </span>
                <!-- Time range: start → end -->
                <span class="text-[0.56rem] tabular-nums text-muted-foreground/40
                             whitespace-nowrap">
                  {fmtTime(entry.startUtc)} → {fmtTime(entry.completedAt)}
                </span>
              </div>
            {/each}
          {/if}
        </div>

        <!-- Footer: Clear button -->
        {#if sessionLog.length > 0}
          <div class="border-t border-border dark:border-white/[0.06] px-3.5 py-2
                      flex justify-end">
            <button
              onclick={clearLog}
              class="text-[0.58rem] text-muted-foreground/40
                     hover:text-red-500/70 transition-colors underline
                     underline-offset-2 cursor-pointer">
              {t("focusTimer.log.clearDay")}
            </button>
          </div>
        {/if}
      {/if}
    </div>

  </div>
</main>
