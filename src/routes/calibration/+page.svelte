<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount, onDestroy }  from "svelte";
  import { invoke }              from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
  import { Button }              from "$lib/components/ui/button";
  import { Badge }               from "$lib/components/ui/badge";
  import { Progress }            from "$lib/components/ui/progress";
  import { t }                   from "$lib/i18n/index.svelte";
  import { useWindowTitle }      from "$lib/stores/window-title.svelte";
  import DisclaimerFooter        from "$lib/DisclaimerFooter.svelte";
  import type { DeviceStatus }    from "$lib/types";
  import { MUSE_CHANNELS, MUSE_POSITIONS } from "$lib/types";
  import { fmtDateTimeLocale }  from "$lib/format";

  // ── Electrode quality ──────────────────────────────────────────────────────
  type ElecTab = "muse" | "10-20" | "10-10";
  const ELEC_TABS: { id: ElecTab; label: string; count: string }[] = [
    { id: "muse",  label: "Muse",  count: "4"  },
    { id: "10-20", label: "10-20", count: "21" },
    { id: "10-10", label: "10-10", count: "64" },
  ];

  let elecQuality   = $state<string[]>(["no_signal","no_signal","no_signal","no_signal"]);
  let museConnected = $state(false);
  let elecTab       = $state<ElecTab>("muse");

  // ── TTS readiness ──────────────────────────────────────────────────────────
  let ttsReady        = $state(false);
  let ttsDlLabel      = $state("");
  let unlistenTtsFn:  UnlistenFn | null = null;

  function elecQualityColor(label: string): string {
    switch (label) {
      case "good": return "#22c55e";
      case "fair": return "#f59e0b";
      case "poor": return "#ef4444";
      default:     return "#94a3b8";
    }
  }
  function elecQualityBg(label: string): string {
    switch (label) {
      case "good": return "bg-green-500/10 border-green-500/20";
      case "fair": return "bg-amber-500/10 border-amber-500/20";
      case "poor": return "bg-red-500/10 border-red-500/20";
      default:     return "bg-muted/30 border-border dark:border-white/[0.06]";
    }
  }
  function elecQualityText(label: string): string {
    switch (label) {
      case "good": return "Good";
      case "fair": return "Fair";
      case "poor": return "Poor";
      default:     return "No Signal";
    }
  }

  // ── Types ──────────────────────────────────────────────────────────────────
  interface CalibrationAction { label: string; duration_secs: number; }
  interface CalibrationProfile {
    id: string; name: string;
    actions: CalibrationAction[];
    break_duration_secs: number;
    loop_count: number;
    auto_start: boolean;
    last_calibration_utc: number | null;
  }

  type PhaseKind = "idle" | "action" | "break" | "done";
  interface Phase {
    kind:        PhaseKind;
    actionIndex: number;   // index into profile.actions (for "action" kind)
    loop:        number;   // 1-based
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let profiles     = $state<CalibrationProfile[]>([]);
  let profile      = $state<CalibrationProfile | null>(null);
  let phase        = $state<Phase>({ kind: "idle", actionIndex: 0, loop: 1 });
  let countdown    = $state(0);
  let totalSecs    = $state(0);
  let running      = $state(false);
  let unlisten: UnlistenFn | null = null;
  let unlistenQualityFn: UnlistenFn | null = null;
  let notifGranted = false;

  // ── Helpers ────────────────────────────────────────────────────────────────
  async function closeWindow() {
    if (running) {
      running = false;                           // prevent double-emit in onDestroy
      await emitEvent("calibration-cancelled", { loop: phase.loop });
    }
    await invoke("close_calibration_window");
  }

  async function emitEvent(event: string, payload: Record<string, unknown> = {}) {
    await invoke("emit_calibration_event", { event, payload });
  }

  function sleep(ms: number) { return new Promise<void>(r => setTimeout(r, ms)); }



  function timeAgo(utc: number): string {
    const diff = Math.floor(Date.now() / 1000) - utc;
    if (diff < 60)    return t("common.justNow");
    if (diff < 3600)  return t("common.minutesAgo", { n: Math.floor(diff / 60) });
    if (diff < 86400) return t("common.hoursAgo",   { n: Math.floor(diff / 3600) });
    return t("common.daysAgo", { n: Math.floor(diff / 86400) });
  }

  async function notify(title: string, body: string) {
    if (!notifGranted) return;
    try { sendNotification({ title, body }); } catch (e) { console.warn("[calibration] sendNotification failed:", e); }
  }

  async function runCountdown(secs: number): Promise<boolean> {
    totalSecs = secs;
    countdown = secs;
    const endTime = Date.now() + secs * 1000;
    while (countdown > 0) {
      // Sleep until the next whole-second boundary to avoid cumulative drift.
      const remaining = endTime - Date.now();
      if (remaining <= 0) { countdown = 0; break; }
      const nextTick = remaining % 1000 || 1000;
      await sleep(nextTick);
      if (!running) return false;
      countdown = Math.max(0, Math.round((endTime - Date.now()) / 1000));
    }
    return true;
  }

  // ── Load profiles ──────────────────────────────────────────────────────────
  async function loadProfiles() {
    profiles = await invoke<CalibrationProfile[]>("list_calibration_profiles");
  }

  async function selectProfile(p: CalibrationProfile) {
    profile = p;
    await invoke("set_active_calibration", { id: p.id });
  }

  // ── TTS helpers ────────────────────────────────────────────────────────────

  /**
   * Await TTS playback to completion before returning.
   * Use before starting a countdown so the timer only ticks after the user
   * has heard the full announcement.  Swallows all errors so TTS failures
   * never block calibration.
   */
  async function ttsSpeakWait(text: string): Promise<void> {
    try { await invoke("tts_speak", { text }); } catch { /* non-fatal */ }
  }

  /** Fire-and-forget TTS — never throws; failures are silently ignored. */
  function ttsSpeak(text: string): void {
    invoke("tts_speak", { text }).catch(e => console.warn("[calibration] tts_speak failed:", e));
  }

  // ── Calibration loop ───────────────────────────────────────────────────────
  async function startCalibration() {
    if (!profile) return;
    running = true;
    const p = profile;

    // Announce calibration start and wait for it to finish so the first action
    // announcement doesn't overlap.
    await ttsSpeakWait(`Calibration starting. ${p.actions.length} actions, ${p.loop_count} loops.`);
    if (!running) return;   // user cancelled during the announcement

    await emitEvent("calibration-started", {
      profile_id: p.id, profile_name: p.name,
      actions: p.actions.map(a => a.label), loop_count: p.loop_count,
    });

    for (let loop = 1; loop <= p.loop_count; loop++) {
      if (!running) break;

      for (let ai = 0; ai < p.actions.length; ai++) {
        if (!running) break;
        const action = p.actions[ai];

        // ACTION phase — speak the action label and wait for it to finish
        // before the countdown begins, so the user hears the full cue first.
        phase = { kind: "action", actionIndex: ai, loop };
        await ttsSpeakWait(action.label);
        if (!running) break;   // cancelled during speech
        await notify(action.label,
          t("calibration.notifActionBody", { loop: String(loop), total: String(p.loop_count) }));
        await emitEvent("calibration-action", {
          action: action.label, action_index: ai, loop, phase: `action_${ai}`,
        });
        const actionStart = Math.floor(Date.now() / 1000);
        if (!(await runCountdown(action.duration_secs))) break;
        try { await invoke("submit_label", { labelStartUtc: actionStart, text: action.label }); } catch (e) { console.warn("[calibration] submit_label failed:", e); }

        // BREAK phase — skip only after the very last action of the very last loop
        const isLast = loop === p.loop_count && ai === p.actions.length - 1;
        if (!isLast && running) {
          const nextAction = p.actions[(ai + 1) % p.actions.length];
          phase = { kind: "break", actionIndex: ai, loop };

          // Announce both parts sequentially so TTS is fully done before the
          // countdown starts.  This prevents "Next: …" from bleeding into the
          // next action phase and delaying its countdown.
          await ttsSpeakWait("Break.");
          if (!running) break;
          await sleep(300);
          await ttsSpeakWait(`Next: ${nextAction.label}.`);
          if (!running) break;

          await notify(t("calibration.break"),
            t("calibration.notifBreakBody", { next: nextAction.label }));
          await emitEvent("calibration-break", { after_action: action.label, loop });
          if (!(await runCountdown(p.break_duration_secs))) break;
        }
      }
    }

    if (running) {
      // Completed all loops normally.
      phase = { kind: "done", actionIndex: 0, loop: p.loop_count };
      running = false;
      ttsSpeak(`Calibration complete. ${p.loop_count} loops recorded.`);
      await notify(t("calibration.complete"),
        t("calibration.notifDoneBody", { n: String(p.loop_count) }));
      await emitEvent("calibration-completed", { loop_count: p.loop_count });
      await invoke("record_calibration_completed", { profileId: p.id });
      await loadProfiles();
      profile = profiles.find(x => x.id === p.id) ?? profile;
    } else if (phase.kind !== "idle") {
      // cancelCalibration() set running=false but the loop was mid-sleep when
      // it returned — ensure we end up in idle in case phase wasn't reset yet.
      phase = { kind: "idle", actionIndex: 0, loop: 1 };
    }
  }

  async function cancelCalibration() {
    if (!running) return;
    running = false;
    ttsSpeak("Calibration cancelled.");
    await emitEvent("calibration-cancelled", { loop: phase.loop });
    // Reset immediately so the UI returns to the start screen without waiting
    // for the background countdown loop to finish its current sleep(1000).
    phase = { kind: "idle", actionIndex: 0, loop: 1 };
  }

  // ── Derived ────────────────────────────────────────────────────────────────
  const progressPct = $derived(totalSecs > 0 ? ((totalSecs - countdown) / totalSecs) * 100 : 0);

  const phaseLabel = $derived.by(() => {
    if (phase.kind === "action" && profile) {
      return profile.actions[phase.actionIndex]?.label ?? "";
    }
    if (phase.kind === "break") return t("calibration.break");
    if (phase.kind === "done")  return t("calibration.complete");
    return t("calibration.ready");
  });

  const phaseColor = $derived.by(() => {
    const COLORS = [
      "text-blue-600 dark:text-blue-400",
      "text-violet-600 dark:text-violet-400",
      "text-emerald-600 dark:text-emerald-400",
      "text-amber-600 dark:text-amber-400",
      "text-rose-600 dark:text-rose-400",
      "text-cyan-600 dark:text-cyan-400",
    ];
    if (phase.kind === "action") return COLORS[phase.actionIndex % COLORS.length];
    if (phase.kind === "break")  return "text-amber-600 dark:text-amber-400";
    if (phase.kind === "done")   return "text-emerald-600 dark:text-emerald-400";
    return "text-muted-foreground";
  });

  const phaseBg = $derived.by(() => {
    const BG = ["bg-blue-500","bg-violet-500","bg-emerald-500","bg-amber-500","bg-rose-500","bg-cyan-500"];
    if (phase.kind === "action") return BG[phase.actionIndex % BG.length];
    if (phase.kind === "break")  return "bg-amber-500";
    return "bg-emerald-500";
  });

  // ── Lifecycle ──────────────────────────────────────────────────────────────
  onMount(async () => {
    // Notification permission
    try {
      notifGranted = await isPermissionGranted();
      if (!notifGranted) {
        const perm = await requestPermission();
        notifGranted = perm === "granted";
      }
    } catch (e) { console.warn("[calibration] requestPermission failed:", e); }

    await loadProfiles();

    // Parse URL params for profile pre-selection and autostart
    const params = new URLSearchParams(window.location.search);
    const paramId = params.get("profile");
    const autostart = params.get("autostart") === "1";

    if (paramId) {
      profile = profiles.find(p => p.id === paramId) ?? profiles[0] ?? null;
    } else {
      profile = (await invoke<CalibrationProfile | null>("get_active_calibration")) ?? profiles[0] ?? null;
    }

    if (autostart && profile) startCalibration();

    // Listen for run events emitted when window is already open
    unlisten = await listen<{ profile_id?: string; autostart?: boolean }>("calibration-run", async ev => {
      if (running) return;
      const pid = ev.payload.profile_id;
      if (pid) profile = profiles.find(p => p.id === pid) ?? profile;
      if (ev.payload.autostart) startCalibration();
    });

    // Pre-warm TTS engine — listen for progress events BEFORE calling init
    // so we never miss the first "step" event.
    unlistenTtsFn = await listen<{ phase: string; label: string }>(
      "tts-progress",
      (ev) => {
        if (ev.payload.phase === "ready") {
          ttsReady   = true;
          ttsDlLabel = "";
        } else {
          ttsReady   = false;
          ttsDlLabel = ev.payload.label ?? "";
        }
      }
    );
    invoke("tts_init").catch(e => console.warn("[calibration] tts_init failed:", e));

    // Electrode signal quality
    try {
      const s = await invoke<DeviceStatus>("get_status");
      elecQuality   = s.channel_quality;
      museConnected = s.state === "connected";
    } catch (e) { console.warn("[calibration] get_status failed:", e); }
    unlistenQualityFn = await listen<DeviceStatus>("status", (ev) => {
      elecQuality   = ev.payload.channel_quality;
      museConnected = ev.payload.state === "connected";
    });
  });

  onDestroy(async () => {
    unlisten?.();
    unlistenQualityFn?.();
    unlistenTtsFn?.();
    if (running) {
      running = false;
      await emitEvent("calibration-cancelled", { loop: phase.loop });
    }
  });

  useWindowTitle("window.title.calibration");
</script>

<svelte:window onkeydown={(e) => { if (e.key === "Escape") closeWindow(); }} />

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden select-none">

  <!-- ── Title bar ─────────────────────────────────────────────────────────── -->
  <div class="flex items-center gap-2.5 px-4 pt-4 pb-3
              border-b border-border dark:border-white/[0.07] shrink-0">
    {#if running}
      <Badge variant="outline"
        class="text-[0.52rem] tracking-wide uppercase py-0 px-1.5
               bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20">
        {t("calibration.recording")}
      </Badge>
    {/if}

    <!-- TTS engine readiness indicator -->
    {#if !ttsReady}
      <span class="flex items-center gap-1 text-[0.52rem] text-amber-600 dark:text-amber-400
                   font-medium animate-pulse" title={ttsDlLabel || "Preparing voice engine…"}>
        <!-- speaker-with-dots icon -->
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 shrink-0">
          <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
          <path d="M15.54 8.46a5 5 0 0 1 0 7.07"/>
        </svg>
        {#if ttsDlLabel}
          {ttsDlLabel}
        {:else}
          Voice loading…
        {/if}
      </span>
    {/if}

    <span class="ml-auto text-[0.62rem] text-muted-foreground/60 tabular-nums">
      {#if profile?.last_calibration_utc}
        {t("calibration.lastAgo", { ago: timeAgo(profile.last_calibration_utc) })}
      {:else}
        {t("calibration.neverCalibrated")}
      {/if}
    </span>
  </div>

  <!-- ── Profile selector (when idle and multiple profiles exist) ───────────── -->
  {#if phase.kind === "idle" && profiles.length > 1}
    <div class="flex items-center gap-2 px-4 pt-3 pb-1 flex-wrap shrink-0">
      <span class="text-[0.6rem] font-semibold uppercase tracking-wider text-muted-foreground/60 shrink-0">
        {t("calibration.selectProfile")}
      </span>
      {#each profiles as p}
        <button
          onclick={() => selectProfile(p)}
          class="rounded-lg border px-2.5 py-1 text-[0.62rem] font-semibold transition-all
                 {profile?.id === p.id
                   ? 'border-primary/50 bg-primary/10 text-primary'
                   : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
          {p.name}
        </button>
      {/each}
    </div>
  {/if}

  <!-- ── Electrode signal quality strip ──────────────────────────────────── -->
  <div class="px-4 pt-2 pb-0 shrink-0">
    <!-- Tab selector -->
    <div class="flex items-center gap-1 mb-2">
      {#each ELEC_TABS as etab}
        <button
          onclick={() => elecTab = etab.id}
          class="flex items-center gap-1 rounded-md px-2 py-0.5 text-[0.58rem] font-semibold
                 transition-all border
                 {elecTab === etab.id
                   ? 'bg-foreground text-background border-transparent'
                   : 'text-muted-foreground border-border dark:border-white/[0.07] hover:text-foreground hover:border-foreground/30'}"
        >
          {etab.label}
          <span class="text-[0.45rem] opacity-60 tabular-nums">{etab.count}</span>
        </button>
      {/each}
      {#if !museConnected}
        <span class="ml-2 text-[0.52rem] text-amber-600 dark:text-amber-400 opacity-70">⚠ not connected</span>
      {/if}
    </div>

    {#if elecTab === "muse"}
      <!-- 4 Muse channel quality cards -->
      <div class="grid grid-cols-4 gap-1.5">
        {#each MUSE_CHANNELS as name, idx}
          {@const label = elecQuality[idx] ?? "no_signal"}
          <div class="flex flex-col items-center gap-0.5 p-1.5 rounded-lg border transition-all
                      {elecQualityBg(label)}">
            <div class="relative">
              <span class="w-2.5 h-2.5 rounded-full block"
                    style="background:{elecQualityColor(label)}"></span>
              {#if label === "no_signal"}
                <span class="absolute inset-0 w-2.5 h-2.5 rounded-full animate-ping"
                      style="background:{elecQualityColor(label)}; opacity:0.3"></span>
              {/if}
            </div>
            <span class="text-[0.62rem] font-bold font-mono"
                  style="color:{elecQualityColor(label)}">{name}</span>
            <span class="text-[0.42rem] font-semibold uppercase tracking-wider"
                  style="color:{elecQualityColor(label)}; opacity:0.8">
              {elecQualityText(label)}
            </span>
            <span class="text-[0.38rem] text-muted-foreground/50">{MUSE_POSITIONS[idx]}</span>
          </div>
        {/each}
      </div>
    {:else}
      <!-- Informational grid for 10-20 / 10-10 electrode systems -->
      <div class="rounded-lg border border-border dark:border-white/[0.07] bg-muted/20 p-2.5">
        <div class="flex items-start gap-3">
          <!-- Compact Muse quality indicator strip -->
          <div class="flex flex-col gap-1 shrink-0">
            <span class="text-[0.48rem] font-semibold text-muted-foreground/60 uppercase tracking-wider">
              Muse signal
            </span>
            {#each MUSE_CHANNELS as name, idx}
              {@const label = elecQuality[idx] ?? "no_signal"}
              <div class="flex items-center gap-1.5">
                <span class="w-2 h-2 rounded-full shrink-0"
                      style="background:{elecQualityColor(label)}"></span>
                <span class="text-[0.52rem] font-mono font-bold"
                      style="color:{elecQualityColor(label)}">{name}</span>
                <span class="text-[0.44rem] text-muted-foreground/60">
                  {elecQualityText(label)}
                </span>
              </div>
            {/each}
          </div>
          <!-- System description -->
          <div class="flex-1 min-w-0">
            <span class="text-[0.48rem] font-semibold text-muted-foreground/60 uppercase tracking-wider block mb-1">
              {elecTab} system
            </span>
            {#if elecTab === "10-20"}
              <p class="text-[0.55rem] text-muted-foreground/70 leading-relaxed">
                19 electrodes + 2 reference points. Standard clinical system covering all major brain regions:
                frontal (Fp1/2, F3/4/7/8, Fz), central (C3/4, Cz), temporal (T3–6), parietal (P3/4, Pz), and occipital (O1/2).
              </p>
              <p class="text-[0.5rem] text-muted-foreground/50 mt-1">
                Muse uses TP9, AF7, AF8, TP10 — a subset optimized for frontal asymmetry and temporal reference.
              </p>
            {:else}
              <p class="text-[0.55rem] text-muted-foreground/70 leading-relaxed">
                High-density system with ~64 electrodes adding intermediate positions between 10-20 sites.
                Provides finer spatial resolution for source localization and BCI applications.
              </p>
              <p class="text-[0.5rem] text-muted-foreground/50 mt-1">
                Muse uses 4 of these sites: TP9 (left mastoid), AF7 (left frontal), AF8 (right frontal), TP10 (right mastoid).
              </p>
            {/if}
          </div>
        </div>
      </div>
    {/if}
  </div>

  <!-- ── Main content ──────────────────────────────────────────────────────── -->
  <div class="flex-1 flex flex-col items-center justify-center gap-6 px-6 py-4 overflow-y-auto">

    {#if phase.kind === "idle" && profile}
      <!-- IDLE / Start screen -->
      <div class="flex flex-col items-center gap-4 text-center max-w-[380px]">
        <div class="w-16 h-16 rounded-full bg-muted dark:bg-white/[0.06]
                    flex items-center justify-center text-2xl">🎯</div>

        <h2 class="text-[1rem] font-bold">{profile.name}</h2>

        <p class="text-[0.75rem] text-muted-foreground leading-relaxed">
          {@html t("calibration.descriptionN", {
            actions: profile.actions.map(a =>
              `<strong class="text-foreground">${a.label}</strong>`).join(" → "),
            count: String(profile.loop_count),
          })}
        </p>

        <!-- Action chips -->
        <div class="flex flex-wrap gap-1.5 justify-center">
          {#each profile.actions as action, i}
            {@const colors = [
              "border-primary/30 bg-primary/10 text-primary",
              "border-violet-500/30 bg-violet-500/10 text-violet-600 dark:text-violet-400",
              "border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400",
              "border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400",
              "border-rose-500/30 bg-rose-500/10 text-rose-600 dark:text-rose-400",
              "border-cyan-500/30 bg-cyan-500/10 text-cyan-600 dark:text-cyan-400",
            ]}
            <span class="rounded-full border px-2.5 py-0.5 text-[0.62rem] font-medium {colors[i % colors.length]}">
              {action.label} · {action.duration_secs}s
            </span>
          {/each}
          <span class="rounded-full border border-amber-500/30 bg-amber-500/10
                       text-amber-600 dark:text-amber-400 px-2.5 py-0.5 text-[0.62rem] font-medium">
            {t("calibration.break")} · {profile.break_duration_secs}s
          </span>
        </div>

        <p class="text-[0.63rem] text-muted-foreground/60">
          {t("calibration.timingDescN", {
            loops: String(profile.loop_count),
            actions: String(profile.actions.length),
            breakSecs: String(profile.break_duration_secs),
          })}
        </p>

        {#if profile.last_calibration_utc}
          <div class="flex items-center gap-2 rounded-lg border border-border dark:border-white/[0.06]
                      bg-muted dark:bg-[#1a1a28] px-3 py-2">
            <span class="text-[0.6rem] font-semibold text-muted-foreground/70">{t("calibration.lastCalibrated")}</span>
            <span class="text-[0.65rem] font-medium text-foreground/80">{fmtDateTimeLocale(profile.last_calibration_utc)}</span>
            <span class="text-[0.58rem] text-muted-foreground/50">({timeAgo(profile.last_calibration_utc)})</span>
          </div>
        {:else}
          <div class="flex items-center gap-2 rounded-lg border border-amber-500/20 bg-amber-500/5 px-3 py-2">
            <span class="text-[0.65rem] text-amber-600 dark:text-amber-400 font-medium">{t("calibration.noPrevious")}</span>
          </div>
        {/if}

        <Button class="mt-2 px-8" onclick={startCalibration}>
          {t("calibration.startCalibration")}
        </Button>
      </div>

    {:else if phase.kind === "done" && profile}
      <!-- DONE screen -->
      <div class="flex flex-col items-center gap-4 text-center max-w-[360px]">
        <div class="w-16 h-16 rounded-full bg-emerald-500/10 flex items-center justify-center text-2xl">✅</div>
        <h2 class="text-[1rem] font-bold text-emerald-600 dark:text-emerald-400">{t("calibration.complete")}</h2>
        <p class="text-[0.75rem] text-muted-foreground leading-relaxed">
          {t("calibration.completeDesc", { n: String(profile.loop_count) })}
        </p>
        <div class="flex gap-3 mt-2">
          <Button variant="outline" onclick={closeWindow}>{t("common.close")}</Button>
          <Button onclick={() => { phase = { kind: "idle", actionIndex: 0, loop: 1 }; }}>
            {t("calibration.runAgain")}
          </Button>
        </div>
      </div>

    {:else if phase.kind !== "idle" && profile}
      <!-- ACTIVE PHASE -->
      <div class="flex flex-col items-center gap-5 w-full max-w-[400px]">

        <!-- Profile name + loop indicator -->
        <div class="flex flex-col items-center gap-1.5">
          <span class="text-[0.6rem] font-semibold uppercase tracking-widest text-muted-foreground/60">
            {profile.name}
          </span>
          <div class="flex items-center gap-2">
            <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
              {t("calibration.iteration")}
            </span>
            <div class="flex gap-1">
              {#each Array(profile.loop_count) as _, i}
                <div class="w-3 h-3 rounded-full transition-colors
                            {i < phase.loop - 1 ? 'bg-emerald-500' :
                             i === phase.loop - 1 ? phaseBg :
                             'bg-muted dark:bg-white/[0.08]'}"></div>
              {/each}
            </div>
            <span class="text-[0.65rem] text-muted-foreground tabular-nums">
              {phase.loop}/{profile.loop_count}
            </span>
          </div>
        </div>

        <!-- Action progress dots -->
        {#if phase.kind === "action" && profile.actions.length > 1}
          <div class="flex items-center gap-2">
            {#each profile.actions as _, i}
              <div class="flex items-center gap-1">
                <div class="w-2 h-2 rounded-full transition-colors
                            {i < phase.actionIndex ? 'bg-emerald-500' :
                             i === phase.actionIndex ? 'bg-blue-500' :
                             'bg-muted dark:bg-white/[0.08]'}"></div>
                {#if i < profile.actions.length - 1}
                  <div class="w-3 h-px bg-muted dark:bg-white/[0.08]"></div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <!-- Phase label -->
        <div class="flex flex-col items-center gap-1">
          <span class="text-[2rem] font-bold tracking-tight {phaseColor}">{phaseLabel}</span>
          {#if phase.kind === "break" && profile}
            {@const nextIdx = (phase.actionIndex + 1) % profile.actions.length}
            <span class="text-[0.72rem] text-muted-foreground">
              {t("calibration.nextAction", { action: profile.actions[nextIdx]?.label ?? "" })}
            </span>
          {/if}
        </div>

        <!-- Countdown -->
        <div class="flex flex-col items-center gap-3 w-full">
          <span class="text-[3rem] font-bold tabular-nums text-foreground leading-none">{countdown}</span>
          <span class="text-[0.62rem] text-muted-foreground/50">{t("calibration.secondsRemaining")}</span>
          <div class="w-full"><Progress value={progressPct} class="h-2" /></div>
        </div>

        <Button variant="outline" size="sm" class="mt-2" onclick={cancelCalibration}>
          {t("common.cancel")}
        </Button>
      </div>
    {/if}

  </div>

  <!-- ── Footer ────────────────────────────────────────────────────────────── -->
  <div class="flex items-center justify-between px-4 pb-3 pt-2
              border-t border-border dark:border-white/[0.07] shrink-0">
    <span class="text-[0.6rem] text-muted-foreground/40">{t("calibration.footer")}</span>
    {#if phase.kind === "idle" || phase.kind === "done"}
      <Button variant="ghost" size="sm" class="text-[0.65rem] h-6 px-2 text-muted-foreground"
              onclick={closeWindow}>{t("common.close")}</Button>
    {/if}
  </div>

  <DisclaimerFooter />
</main>
