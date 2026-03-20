<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Session Compare — side-by-side band-power & score comparison of two sessions. -->

<script lang="ts">
  import { onMount }       from "svelte";
  import { invoke }        from "@tauri-apps/api/core";
  import { Button }        from "$lib/components/ui/button";
  import { Separator }     from "$lib/components/ui/separator";
  import { t }             from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/stores/window-title.svelte";
  import DisclaimerFooter  from "$lib/DisclaimerFooter.svelte";
  import Hypnogram         from "$lib/Hypnogram.svelte";
  import UmapViewer3D      from "$lib/UmapViewer3D.svelte";
  import { TimeSeriesChart } from "$lib/dashboard";
  import type { Series }   from "$lib/dashboard/TimeSeriesChart.svelte";

  import { Spinner }       from "$lib/components/ui/spinner";
  import { getResolved }   from "$lib/stores/theme.svelte";
  import type { SessionMetrics, EpochRow } from "$lib/dashboard/SessionDetail.svelte";
  import type { SleepEpoch, SleepSummary, SleepStages } from "$lib/types";
  import { analyzeSleep, type SleepAnalysis } from "$lib/sleep-analysis";
  import {
    fmtSecs, fmtTime, fmtDateTime, fmtDuration, pad,
    fmtDateIso, fmtDayKey, fmtDateTimeLocalInput, parseDateTimeLocalInput,
    dateToLocalKey, fromUnix,
  } from "$lib/format";
  import type { UmapPoint, UmapResult, UmapProgress } from "$lib/types";

  // ── Extracted modules ──────────────────────────────────────────────────────
  import {
    type EmbeddingSession, type InsightDelta, type ClusterAnalysis,
    SESSION_COLORS, bandKeys, bandMeta, scoreKeys, radarMetrics,
    advancedMetrics, insightMetrics,
    HIGHER_IS_BETTER, LOWER_IS_BETTER,
    bv, pct, diff, scoreDiff, dc, sdc,
    analyzeUmapClusters, generateUmapPlaceholder, computeInsightDeltas,
  } from "$lib/compare-types";
  import {
    HEATMAP_ROW_H, HEATMAP_LABEL_W,
    drawSpectrum, drawDiffChart, drawRadar,
    drawBandHeatmap, drawScoreHeatmap, drawBandDiffHeatmap,
  } from "$lib/compare-canvas";
  import type { JobTicket, JobPollResult } from "$lib/search-types";

  // ── State ──────────────────────────────────────────────────────────────────
  let sessions    = $state<EmbeddingSession[]>([]);
  let loading     = $state(true);

  // Timeline range-picker state — one per side (A / B)
  let aAnchorUtc  = $state<number | null>(null);   // UTC seconds — local midnight of anchor day (start of 48h window)
  let bAnchorUtc  = $state<number | null>(null);
  let aRangeStart = $state<number | null>(null);   // UTC seconds — selection start
  let aRangeEnd   = $state<number | null>(null);   // UTC seconds — selection end
  let bRangeStart = $state<number | null>(null);
  let bRangeEnd   = $state<number | null>(null);

  // Pointer-drag state
  let dragSide   = $state<"A" | "B" | null>(null);
  let dragAnchor = $state(0);  // UTC seconds where drag started

  let metricsA    = $state<SessionMetrics | null>(null);
  let metricsB    = $state<SessionMetrics | null>(null);
  let sleepA      = $state<SleepStages | null>(null);
  let sleepB      = $state<SleepStages | null>(null);
  let comparing   = $state(false);
  let tsA         = $state<EpochRow[]>([]);
  let tsB         = $state<EpochRow[]>([]);
  let showCharts      = $state(false);
  let advExpanded     = $state(false);
  let sleepExpanded   = $state(false);

  // UMAP state
  let umapResult    = $state<UmapResult | null>(null);
  let umapLoading   = $state(false);
  let umapRequested = $state(false);               // true once the user clicks "Calculate UMAP"
  let umapEta       = $state<string | null>(null); // short status line
  let umapReadyUtc  = $state<number | null>(null); // unix-sec when result expected
  let umapElapsed   = $state(0);                   // seconds elapsed since start
  let umapCountdown = $state<number | null>(null); // live seconds remaining
  let umapComputeMs = $state<number | null>(null); // actual compute time from backend
  let umapProgress  = $state<UmapProgress | null>(null); // live epoch progress
  let umapTimerHandle: ReturnType<typeof setInterval> | null = null;
  let umapStartMs   = 0;                           // performance.now() at fire

  // Queue-aware status — updated from poll responses
  /** Current 0-indexed position in the pending queue (0 = running now, >0 = waiting). */
  let umapQueuePosition = $state<number | null>(null);
  /** Estimated seconds this job spends actively computing (excludes queue wait). */
  let umapOwnEstimateSecs = $state<number>(3);
  /** Estimated seconds remaining until my job STARTS (decreases as jobs ahead finish). */
  let umapWaitSecs = $state<number | null>(null);

  // Placeholder UMAP data — shown while real UMAP computes
  let umapPlaceholder = $state<UmapResult | null>(null);



  // Job queue types — imported from search-types.ts
  /** Start the wall-clock timer that drives elapsed / countdown display. */
  function startUmapTimer(estimatedSecs: number | null) {
    stopUmapTimer();
    umapStartMs = performance.now();
    umapElapsed = 0;
    umapCountdown = estimatedSecs;
    umapTimerHandle = setInterval(() => {
      const elapsed = Math.floor((performance.now() - umapStartMs) / 1000);
      umapElapsed = elapsed;
      if (umapReadyUtc) {
        const nowUtc = Math.floor(Date.now() / 1000);
        umapCountdown = Math.max(0, umapReadyUtc - nowUtc);
      } else if (estimatedSecs != null) {
        umapCountdown = Math.max(0, estimatedSecs - elapsed);
      }
    }, 250);
  }

  function stopUmapTimer() {
    if (umapTimerHandle != null) { clearInterval(umapTimerHandle); umapTimerHandle = null; }
    umapCountdown = null;
  }



  // Canvas refs for spectrum charts
  let specCanvasA = $state<HTMLCanvasElement | null>(null);
  let specCanvasB = $state<HTMLCanvasElement | null>(null);
  let diffCanvas  = $state<HTMLCanvasElement | null>(null);

  // Reactive dark-mode flag — used by canvas draw functions to pick colours.
  const isDark = $derived(getResolved() === "dark");

  // ── Timeline / range-picker helpers ─────────────────────────────────────────



  /**
   * Svelte action: attach a non-passive wheel listener so we can call
   * preventDefault() and use horizontal (or vertical) scroll to navigate
   * the 48h timeline between days.
   * Scroll right / down → older day; scroll left / up → newer day.
   */
  function timelineWheel(node: HTMLElement, side: "A" | "B") {
    let accum = 0;
    function onWheel(e: WheelEvent) {
      // Only intercept when there's meaningful horizontal intent, or pure
      // vertical mouse-wheel with no modifier keys held.
      const dx = e.deltaX, dy = e.deltaY;
      const dominant = Math.abs(dx) >= Math.abs(dy) ? dx : dy;
      if (dominant === 0) return;
      e.preventDefault();
      accum += dominant;
      const THRESHOLD = 80; // ~80px of scroll before jumping one day
      if (accum >  THRESHOLD) { accum = 0; navigateDay(side, +1); } // older
      if (accum < -THRESHOLD) { accum = 0; navigateDay(side, -1); } // newer
    }
    node.addEventListener("wheel", onWheel, { passive: false });
    return { destroy() { node.removeEventListener("wheel", onWheel); } };
  }

  /** Local date string "YYYY-MM-DD" from a UTC unix-second timestamp. */
  function localDateFromUtc(utc: number): string {
    return dateToLocalKey(fromUnix(utc));
  }

  /** UTC seconds for local midnight of a "YYYY-MM-DD" date string. */
  function localMidnight(dateStr: string): number {
    const [y, mo, d] = dateStr.split("-").map(Number);
    return Math.floor(new Date(y, mo - 1, d, 0, 0, 0).getTime() / 1000);
  }

  /** "HH:MM" from a UTC unix-second timestamp (local time). */
  function utcToTimeStr(utc: number): string {
    const d = new Date(utc * 1000);
    return `${String(d.getHours()).padStart(2,"0")}:${String(d.getMinutes()).padStart(2,"0")}`;
  }

  /** UTC seconds from a "YYYY-MM-DD" date and "HH:MM" time (local). */
  function timeStrToUtc(dateStr: string, timeStr: string): number {
    const [y, mo, d] = dateStr.split("-").map(Number);
    const [h,  mi]   = timeStr.split(":").map(Number);
    return Math.floor(new Date(y, mo - 1, d, h, mi, 0).getTime() / 1000);
  }

  /** Human-readable date label for a "YYYY-MM-DD" string. */
  function dayLabel(dateStr: string): string {
    return fmtDayKey(dateStr);
  }

  /** Sessions that overlap the half-open interval [startUtc, endUtc). */
  function sessionsInRange(startUtc: number, endUtc: number): EmbeddingSession[] {
    return sessions.filter(s => s.end_utc > startUtc && s.start_utc < endUtc);
  }

  /** Sorted unique day strings (newest first) that have recorded sessions. */
  const sortedDays = $derived.by(() => {
    const s = new Set<string>();
    for (const sess of sessions) {
      let d = new Date(fromUnix(sess.start_utc).getFullYear(), fromUnix(sess.start_utc).getMonth(), fromUnix(sess.start_utc).getDate());
      const endD = fromUnix(sess.end_utc);
      const endMid = new Date(endD.getFullYear(), endD.getMonth(), endD.getDate());
      while (d <= endMid) {
        s.add(dateToLocalKey(d));
        d.setDate(d.getDate() + 1);
      }
    }
    return [...s].sort().reverse(); // newest first
  });

  /**
   * Select an anchor day for one side and auto-set the range to cover all
   * sessions within the 48-hour window starting at that day's midnight.
   */
  function selectDay(side: "A" | "B", dateStr: string) {
    const anchorUtc = localMidnight(dateStr);
    const windowSess = sessions.filter(s => s.end_utc > anchorUtc && s.start_utc < anchorUtc + 172800);
    const rangeS = windowSess.length > 0 ? Math.min(...windowSess.map(s => s.start_utc)) : anchorUtc;
    const rangeE = Math.min(
      windowSess.length > 0 ? Math.max(...windowSess.map(s => s.end_utc)) : anchorUtc + 86400,
      rangeS + 86400
    );
    if (side === "A") { aAnchorUtc = anchorUtc; aRangeStart = rangeS; aRangeEnd = rangeE; }
    else              { bAnchorUtc = anchorUtc; bRangeStart = rangeS; bRangeEnd = rangeE; }
  }

  /** Navigate a side to the next/previous day (by sortedDays list). */
  function navigateDay(side: "A" | "B", direction: -1 | 1) {
    const current = side === "A" ? aDayStr : bDayStr;
    if (!current) return;
    const idx  = sortedDays.indexOf(current);
    const next = sortedDays[idx + direction];
    if (next) selectDay(side, next);
  }

  /** Snap a session onto a side's selection (click on a session bar). */
  function pickSession(side: "A" | "B", sess: EmbeddingSession) {
    if (side === "A") { aRangeStart = sess.start_utc; aRangeEnd = sess.end_utc; }
    else              { bRangeStart = sess.start_utc; bRangeEnd = sess.end_utc; }
  }

  /** "YYYY-MM-DDThh:mm:ss" from a UTC unix-second timestamp (for datetime-local inputs). */
  function utcToDateTimeLocal(utc: number): string {
    return fmtDateTimeLocalInput(utc);
  }

  /** UTC seconds from a "YYYY-MM-DDThh:mm" datetime-local string. */
  function dateTimeLocalToUtc(dt: string): number {
    return parseDateTimeLocalInput(dt);
  }

  /** Update range start from a datetime-local input. */
  function setRangeStart(side: "A" | "B", val: string) {
    const u = dateTimeLocalToUtc(val);
    if (side === "A") aRangeStart = u; else bRangeStart = u;
  }

  /** Update range end from a datetime-local input. */
  function setRangeEnd(side: "A" | "B", val: string) {
    const u = dateTimeLocalToUtc(val);
    if (side === "A") aRangeEnd = u; else bRangeEnd = u;
  }

  // ── Pointer drag handlers for the 48h timeline ─────────────────────────────

  function ptrDown(e: PointerEvent, side: "A" | "B") {
    const el     = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect   = el.getBoundingClientRect();
    const pct    = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const anchor = side === "A" ? aAnchorUtc : bAnchorUtc;
    if (!anchor) return;
    const utc  = Math.round(anchor + pct * 172800);
    dragSide   = side;
    dragAnchor = utc;
    if (side === "A") { aRangeStart = utc; aRangeEnd = utc; }
    else              { bRangeStart = utc; bRangeEnd = utc; }
  }

  function ptrMove(e: PointerEvent, side: "A" | "B") {
    if (dragSide !== side) return;
    const el     = e.currentTarget as HTMLElement;
    const rect   = el.getBoundingClientRect();
    const pct    = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const anchor = side === "A" ? aAnchorUtc : bAnchorUtc;
    if (!anchor) return;
    const utc = Math.round(anchor + pct * 172800);
    const lo  = Math.min(dragAnchor, utc);
    const hi  = Math.min(Math.max(dragAnchor, utc), lo + 86400); // cap 24 h
    if (side === "A") { aRangeStart = lo; aRangeEnd = hi; }
    else              { bRangeStart = lo; bRangeEnd = hi; }
  }

  function ptrUp(side: "A" | "B") {
    if (dragSide !== side) return;
    dragSide = null;
    // Snap to nearest minute
    if (side === "A" && aRangeStart !== null && aRangeEnd !== null) {
      aRangeStart = Math.round(aRangeStart / 60) * 60;
      aRangeEnd   = Math.round(aRangeEnd   / 60) * 60;
      if (aRangeEnd <= aRangeStart) aRangeEnd = aRangeStart + 60;
    } else if (side === "B" && bRangeStart !== null && bRangeEnd !== null) {
      bRangeStart = Math.round(bRangeStart / 60) * 60;
      bRangeEnd   = Math.round(bRangeEnd   / 60) * 60;
      if (bRangeEnd <= bRangeStart) bRangeEnd = bRangeStart + 60;
    }
  }

  // ── Derived UTC ranges (consumed by compare / UMAP / insights) ─────────────
  // Day string derived from anchor — used for nav dropdown + navigateDay
  const aDayStr      = $derived(aAnchorUtc !== null ? localDateFromUtc(aAnchorUtc + 43200) : null);
  const bDayStr      = $derived(bAnchorUtc !== null ? localDateFromUtc(bAnchorUtc + 43200) : null);
  // Sessions visible in the 48h window
  const aDaySessions = $derived(aAnchorUtc !== null ? sessions.filter(s => s.end_utc > aAnchorUtc! && s.start_utc < aAnchorUtc! + 172800) : []);
  const bDaySessions = $derived(bAnchorUtc !== null ? sessions.filter(s => s.end_utc > bAnchorUtc! && s.start_utc < bAnchorUtc! + 172800) : []);

  const aStartUtc = $derived(aRangeStart ?? 0);
  const aEndUtc   = $derived(aRangeEnd   ?? 0);
  const bStartUtc = $derived(bRangeStart ?? 0);
  const bEndUtc   = $derived(bRangeEnd   ?? 0);

  const aDurSecs  = $derived(aEndUtc - aStartUtc);
  const bDurSecs  = $derived(bEndUtc - bStartUtc);
  const aSessions = $derived(aRangeStart !== null && aRangeEnd !== null ? sessionsInRange(aStartUtc, aEndUtc) : []);
  const bSessions = $derived(bRangeStart !== null && bRangeEnd !== null ? sessionsInRange(bStartUtc, bEndUtc) : []);

  const aValid = $derived(aRangeStart !== null && aRangeEnd !== null && aDurSecs > 0 && aDurSecs <= 86400 && aSessions.length > 0);
  const bValid = $derived(bRangeStart !== null && bRangeEnd !== null && bDurSecs > 0 && bDurSecs <= 86400 && bSessions.length > 0);

  // ── Helpers ────────────────────────────────────────────────────────────────

  function sessionLabel(s: EmbeddingSession): string {
    const dt  = fmtDateTime(s.start_utc);
    const dur = fmtDuration(s.end_utc - s.start_utc);
    return `${dt}  (${dur}, ${s.n_epochs} ep)`;
  }

  // ── Load sessions & auto-select ────────────────────────────────────────────
  let refreshing = $state(false);

  async function loadSessions(autoSelect = false) {
    // Primary source: sessions that have embeddings computed.
    const embSessions = await invoke<EmbeddingSession[]>("list_embedding_sessions");

    // Fallback: also pull from the raw session list so that sessions recorded
    // today (whose embeddings haven't been computed yet) still appear in the
    // compare timeline.  We fetch only the most recent days to keep it fast.
    try {
      const days = await invoke<string[]>("list_session_days");
      const recentDays = days.slice(0, Math.min(days.length, 7));
      const dayResults = await Promise.allSettled(
        recentDays.map(day =>
          invoke<{ session_start_utc: number | null; session_end_utc: number | null }[]>(
            "list_sessions_for_day", { day }
          )
        )
      );

      // Build a lookup of already-covered ranges (rounded to nearest minute to
      // avoid floating-point / rounding mismatches between the two sources).
      const covered = new Set(
        embSessions.map(s => `${Math.round(s.start_utc / 60)}-${Math.round(s.end_utc / 60)}`)
      );

      for (const result of dayResults) {
        if (result.status !== "fulfilled") continue;
        for (const sess of result.value) {
          if (!sess.session_start_utc || !sess.session_end_utc) continue;
          const key = `${Math.round(sess.session_start_utc / 60)}-${Math.round(sess.session_end_utc / 60)}`;
          if (!covered.has(key)) {
            covered.add(key);
            const dur = sess.session_end_utc - sess.session_start_utc;
            embSessions.push({
              start_utc: sess.session_start_utc,
              end_utc:   sess.session_end_utc,
              n_epochs:  Math.floor(dur / 5),   // assume 5-second epochs
              day:       localDateFromUtc(sess.session_start_utc),
            });
          }
        }
      }
    } catch (e) {
      // Non-fatal — the primary embedding list is still usable.
      console.warn("[compare] fallback session load failed:", e);
    }

    sessions = embSessions;
    if (!autoSelect || sessions.length === 0) return;

    // Check for quick-compare URL params (from open_compare_window_with_sessions)
    const params = new URLSearchParams(window.location.search);
    const qStartA = params.get("startA");
    const qEndA   = params.get("endA");
    const qStartB = params.get("startB");
    const qEndB   = params.get("endB");
    if (qStartA && qEndA && qStartB && qEndB) {
      const sA = parseInt(qStartA, 10), eA = parseInt(qEndA, 10);
      const sB = parseInt(qStartB, 10), eB = parseInt(qEndB, 10);
      aAnchorUtc = localMidnight(localDateFromUtc(sA)); aRangeStart = sA; aRangeEnd = eA;
      bAnchorUtc = localMidnight(localDateFromUtc(sB)); bRangeStart = sB; bRangeEnd = eB;
      await compare();
      return;
    }

    // Build sorted unique local-date list (newest first) from all sessions
    const daySet = new Set<string>();
    for (const sess of sessions) daySet.add(localDateFromUtc(sess.start_utc));
    const allDays = [...daySet].sort().reverse();

    if (allDays.length >= 1) selectDay("A", allDays[0]);
    if (allDays.length >= 2) {
      selectDay("B", allDays[1]);
    } else if (allDays.length === 1) {
      // ── Only one calendar day available ────────────────────────────────
      // Mirror the same day into both pickers so the user can pick two
      // different time windows from the same recording day.
      const onlyDay = allDays[0];
      bAnchorUtc = localMidnight(onlyDay);

      if (sessions.length >= 2) {
        // Multiple sessions on same day: A = newest, B = second newest.
        aRangeStart = sessions[0].start_utc; aRangeEnd = sessions[0].end_utc;
        bRangeStart = sessions[1].start_utc; bRangeEnd = sessions[1].end_utc;
      } else if (sessions.length === 1) {
        // Single session: split into first half (A) vs second half (B)
        // so the user has a sensible default and can adjust by dragging.
        const sess   = sessions[0];
        const midUtc = Math.round((sess.start_utc + sess.end_utc) / 2);
        aRangeStart  = sess.start_utc; aRangeEnd  = midUtc;
        bRangeStart  = midUtc;        bRangeEnd  = sess.end_utc;
      }
    }

    // Let derived values settle before comparing
    await new Promise(r => setTimeout(r, 0));
    if (aValid && bValid && (aStartUtc !== bStartUtc || aEndUtc !== bEndUtc)) await compare();
  }

  async function refreshSessions() {
    refreshing = true;
    await loadSessions(false);
    refreshing = false;
  }

  // Scroll-container ref used by the ResizeObserver
  let scrollContainer = $state<HTMLElement | null>(null);

  onMount(() => {
    // Async session load — fire and forget (sets `loading = false` on resolve)
    loadSessions(true).then(() => { loading = false; });

    // ResizeObserver: redraws all canvases after the layout settles at the new
    // size.  A plain window "resize" listener fires before CSS has updated, so
    // the canvas.clientWidth reads the OLD value — causing the stretched look.
    const ro = new ResizeObserver(() => redrawAllCanvases());
    if (scrollContainer) ro.observe(scrollContainer);
    return () => ro.disconnect();
  });

  // ── Compare ────────────────────────────────────────────────────────────────
  async function compare() {
    if (aRangeStart === null || aRangeEnd === null || bRangeStart === null || bRangeEnd === null) return;

    // Snapshot the derived UTC values at call time
    const sA = aStartUtc, eA = aEndUtc;
    const sB = bStartUtc, eB = bEndUtc;

    comparing = true;
    metricsA = null;
    metricsB = null;
    sleepA   = null;
    sleepB   = null;

    const [ma, mb, sa, sb] = await Promise.all([
      invoke<SessionMetrics>("get_session_metrics", { startUtc: sA, endUtc: eA }),
      invoke<SessionMetrics>("get_session_metrics", { startUtc: sB, endUtc: eB }),
      invoke<SleepStages>("get_sleep_stages",       { startUtc: sA, endUtc: eA }),
      invoke<SleepStages>("get_sleep_stages",       { startUtc: sB, endUtc: eB }),
    ]);

    metricsA  = ma;
    metricsB  = mb;
    sleepA    = sa.epochs.length > 0 ? sa : null;
    sleepB    = sb.epochs.length > 0 ? sb : null;
    comparing = false;

    // Reset any previously computed UMAP — the user must request it again.
    umapResult      = null;
    umapPlaceholder = null;
    umapRequested   = false;
    umapLoading     = false;
    umapEta         = null;
    umapReadyUtc    = null;
    umapComputeMs   = null;
    umapProgress    = null;
    stopUmapTimer();

    // Load time-series for charts (non-blocking).
    Promise.all([
      invoke<EpochRow[]>("get_session_timeseries", { startUtc: sA, endUtc: eA }),
      invoke<EpochRow[]>("get_session_timeseries", { startUtc: sB, endUtc: eB }),
    ]).then(([ta, tb]) => { tsA = ta; tsB = tb; }).catch(e => console.warn("[compare] get_session_timeseries failed:", e));
  }

  // ── Calculate UMAP (triggered explicitly by the user) ───────────────────
  async function calculateUmap() {
    if (aRangeStart === null || aRangeEnd === null || bRangeStart === null || bRangeEnd === null) return;

    umapRequested        = true;
    umapResult           = null;
    umapLoading          = true;
    umapEta              = null;
    umapReadyUtc         = null;
    umapComputeMs        = null;
    umapProgress         = null;
    umapQueuePosition    = null;
    umapWaitSecs         = null;
    umapOwnEstimateSecs  = 3;          // updated from ticket below
    umapPlaceholder = generateUmapPlaceholder(150, 150);
    startUmapTimer(null);

    const umapArgs = {
      aStartUtc, aEndUtc, bStartUtc, bEndUtc,
    };

    // Try the job queue first (enqueue_umap_compare); if the backend doesn't
    // support it yet, fall back to the synchronous compute_umap_compare.
    invoke<JobTicket>("enqueue_umap_compare", umapArgs)
      .then(ticket => {
        umapReadyUtc = ticket.estimated_ready_utc;
        startUmapTimer(ticket.estimated_secs);

        // Derive this job's own compute estimate.
        // ticket.estimated_secs = (all pending jobs + this job) / 1000.
        // When queue_position == 0 there are no pending jobs, so
        // estimated_secs ≈ this job's own time.  When queued, the backend
        // uses 3 s per UMAP job as the fixed estimate, so we can compute:
        //   wait = estimated_secs − own_estimate
        //   own  = estimated_secs / (queue_position + 1)  — uniform estimate
        umapOwnEstimateSecs = ticket.queue_position > 0
          ? Math.round(ticket.estimated_secs / (ticket.queue_position + 1))
          : ticket.estimated_secs;

        umapQueuePosition = ticket.queue_position;
        umapWaitSecs      = Math.max(0, ticket.estimated_secs - umapOwnEstimateSecs);

        if (ticket.queue_position > 0) {
          umapEta = `queued #${ticket.queue_position + 1}`;
        } else {
          umapEta = "computing 3D projection…";
        }
        pollUmapJob(ticket.job_id);
      })
      .catch(() => {
        // Job queue not available — fall back to direct (blocking) call
        umapEta             = "computing 3D projection…";
        umapQueuePosition   = 0;
        umapWaitSecs        = 0;
        startUmapTimer(10); // rough estimate for direct call
        invoke<UmapResult>("compute_umap_compare", umapArgs)
          .then(r => {
            umapResult = r && r.points && r.points.length > 0 ? r : null;
            umapComputeMs = r?.elapsed_ms ?? null;
            finishUmap();
          })
          .catch(() => { finishUmap(); });
      });
  }

  function finishUmap() {
    umapLoading       = false;
    umapEta           = null;
    umapReadyUtc      = null;
    umapProgress      = null;
    umapQueuePosition = null;
    umapWaitSecs      = null;
    stopUmapTimer();
  }

  // ── Job queue polling ────────────────────────────────────────────────────
  async function pollUmapJob(jobId: number) {
    const poll = async () => {
      try {
        const r = await invoke<JobPollResult>("poll_job", { jobId });
        if (r.status === "complete") {
          const res = r.result as UmapResult | undefined;
          console.log("[umap] poll complete, raw r:", JSON.stringify(r).slice(0, 500));
          console.log("[umap] res type:", typeof res, "keys:", res ? Object.keys(res) : "null");
          console.log("[umap] res?.points?.length:", res?.points?.length, "n_a:", res?.n_a, "n_b:", res?.n_b);
          umapResult = res && res.points && res.points.length > 0 ? res : null;
          umapComputeMs = res?.elapsed_ms ?? (r.elapsed_ms ?? null);
          console.log("[umap] umapResult set:", umapResult != null, "points:", umapResult?.points?.length);
          finishUmap();
        } else if (r.status === "error") {
          console.error("[umap] job failed:", r.error);
          finishUmap();
        } else if (r.status === "pending") {
          // Update queue-aware state on every poll so the UI stays current.
          umapQueuePosition = r.queue_position!;
          // estimated_secs from poll = (jobs ahead + this job) estimate.
          // Subtract this job's own estimate to get remaining wait time.
          umapWaitSecs = Math.max(0, (r.estimated_secs ?? 0) - umapOwnEstimateSecs);

          if (r.queue_position! > 0) {
            umapEta = `queued #${r.queue_position! + 1}`;
          } else if (r.progress) {
            const p = r.progress as unknown as UmapProgress;
            umapProgress = p;
            const pct = p.total_epochs > 0 ? Math.round(p.epoch / p.total_epochs * 100) : 0;
            const remaining = p.epoch_ms > 0 ? ((p.total_epochs - p.epoch) * p.epoch_ms / 1000).toFixed(0) : "?";
            umapEta = `epoch ${p.epoch}/${p.total_epochs} (${pct}%) · ${p.epoch_ms.toFixed(0)}ms/ep · ~${remaining}s left`;
          } else {
            umapEta = "computing 3D projection…";
          }
          setTimeout(poll, 500);
        } else {
          finishUmap();
        }
      } catch {
        finishUmap();
      }
    };
    poll();
  }

  const canCompare = $derived(
    aValid && bValid &&
    (aStartUtc !== bStartUtc || aEndUtc !== bEndUtc)
  );

  // ── Copy summary to clipboard ─────────────────────────────────────────────
  let copied = $state(false);
  async function copySummary() {
    if (!metricsA || !metricsB) return;
    const lines: string[] = [
      `Compare`,
      `A: ${aDayStr ?? "?"} ${aRangeStart ? utcToTimeStr(aRangeStart) : "?"}–${aRangeEnd ? utcToTimeStr(aRangeEnd) : "?"} (${metricsA.n_epochs} epochs)`,
      `B: ${bDayStr ?? "?"} ${bRangeStart ? utcToTimeStr(bRangeStart) : "?"}–${bRangeEnd ? utcToTimeStr(bRangeEnd) : "?"} (${metricsB.n_epochs} epochs)`,
      ``,
      `Band Powers (A → B):`,
      ...bandKeys.map((k, i) => `  ${bandMeta[i].name}: ${pct(bv(metricsA!, k))}% → ${pct(bv(metricsB!, k))}% (${diff(bv(metricsA!, k), bv(metricsB!, k))})`),
      ``,
      `Scores (A → B):`,
      ...scoreKeys.map(sk => `  ${sk.key}: ${metricsA![sk.key].toFixed(1)} → ${metricsB![sk.key].toFixed(1)} (${scoreDiff(metricsA![sk.key], metricsB![sk.key])})`),
      `  FAA: ${metricsA!.faa.toFixed(3)} → ${metricsB!.faa.toFixed(3)}`,
    ];
    if (improved.length > 0) {
      lines.push(``, `Improved: ${improved.map(d => `${d.label} (${d.pctChange > 0 ? "+" : ""}${d.pctChange.toFixed(0)}%)`).join(", ")}`);
    }
    if (declined.length > 0) {
      lines.push(`Declined: ${declined.map(d => `${d.label} (${d.pctChange > 0 ? "+" : ""}${d.pctChange.toFixed(0)}%)`).join(", ")}`);
    }
    if (sleepAnalysisA) {
      lines.push(``, `Sleep A: ${sleepAnalysisA.efficiency.toFixed(0)}% eff, onset ${sleepAnalysisA.onsetLatencyMin.toFixed(0)}m, ${sleepAnalysisA.awakenings} awakenings`);
    }
    if (sleepAnalysisB) {
      lines.push(`Sleep B: ${sleepAnalysisB.efficiency.toFixed(0)}% eff, onset ${sleepAnalysisB.onsetLatencyMin.toFixed(0)}m, ${sleepAnalysisB.awakenings} awakenings`);
    }
    if (umapAnalysis) {
      lines.push(``, `UMAP separation: ${umapAnalysis.separationScore.toFixed(2)}`);
    }
    await navigator.clipboard.writeText(lines.join("\n"));
    copied = true;
    setTimeout(() => copied = false, 2000);
  }

  // ── Client-side insights (types + logic in compare-types.ts) ──────────────
  const insightDeltas = $derived.by(() => {
    if (!metricsA || !metricsB) return [];
    return computeInsightDeltas(metricsA, metricsB);
  });
  const improved = $derived(insightDeltas.filter(d => d.direction === "improved"));
  const declined = $derived(insightDeltas.filter(d => d.direction === "declined"));
  const sleepAnalysisA = $derived(sleepA ? analyzeSleep(sleepA) : null);
  const sleepAnalysisB = $derived(sleepB ? analyzeSleep(sleepB) : null);

  const umapAnalysis = $derived(umapResult ? analyzeUmapClusters(umapResult) : null);



  // Re-render radar when metrics change
  $effect(() => {
    if (metricsA && metricsB && radarCanvas) drawRadar(radarCanvas, metricsA, metricsB);
  });

  // ── Heatmap state ─────────────────────────────────────────────────────────
  let radarCanvas = $state<HTMLCanvasElement | null>(null);
  let hmBandCanvasA    = $state<HTMLCanvasElement | null>(null);
  let hmBandCanvasB    = $state<HTMLCanvasElement | null>(null);
  let hmBandDiffCanvas = $state<HTMLCanvasElement | null>(null);
  let hmScoreCanvasA   = $state<HTMLCanvasElement | null>(null);
  let hmScoreCanvasB   = $state<HTMLCanvasElement | null>(null);
  let showHeatmaps     = $state(false);
  // Re-render heatmaps when time-series data, visibility, or theme changes
  $effect(() => {
    if (!showHeatmaps) return;
    const dark = isDark; // track theme as a reactive dependency
    if (hmBandCanvasA   && tsA.length > 2) drawBandHeatmap(hmBandCanvasA, tsA, dark);
    if (hmBandCanvasB   && tsB.length > 2) drawBandHeatmap(hmBandCanvasB, tsB, dark);
    if (hmBandDiffCanvas && tsA.length > 2 && tsB.length > 2)
      drawBandDiffHeatmap(hmBandDiffCanvas, tsA, tsB, dark);
    if (hmScoreCanvasA  && tsA.length > 2) drawScoreHeatmap(hmScoreCanvasA, tsA, dark);
    if (hmScoreCanvasB  && tsB.length > 2) drawScoreHeatmap(hmScoreCanvasB, tsB, dark);
  });


  // Re-render canvases when metrics or theme changes
  $effect(() => {
    void isDark; // track theme changes
    if (metricsA && specCanvasA) drawSpectrum(specCanvasA, metricsA, "A");
    if (metricsB && specCanvasB) drawSpectrum(specCanvasB, metricsB, "B");
    if (metricsA && metricsB && diffCanvas)  drawDiffChart(diffCanvas, metricsA, metricsB);
  });

  // ── Resize observer — redraws all canvases when the window is resized ─────
  // Without this the browser stretches the old pixel buffer to fit the new
  // CSS size, making numbers and chart lines look distorted.
  function redrawAllCanvases() {
    const dark = isDark;
    if (metricsA && specCanvasA) drawSpectrum(specCanvasA, metricsA, "A");
    if (metricsB && specCanvasB) drawSpectrum(specCanvasB, metricsB, "B");
    if (metricsA && metricsB && diffCanvas)  drawDiffChart(diffCanvas, metricsA, metricsB);
    if (metricsA && metricsB && radarCanvas) drawRadar(radarCanvas, metricsA, metricsB);
    if (showHeatmaps) {
      if (hmBandCanvasA   && tsA.length > 2) drawBandHeatmap(hmBandCanvasA,   tsA, dark);
      if (hmBandCanvasB   && tsB.length > 2) drawBandHeatmap(hmBandCanvasB,   tsB, dark);
      if (hmBandDiffCanvas && tsA.length > 2 && tsB.length > 2)
        drawBandDiffHeatmap(hmBandDiffCanvas, tsA, tsB, dark);
      if (hmScoreCanvasA  && tsA.length > 2) drawScoreHeatmap(hmScoreCanvasA, tsA, dark);
      if (hmScoreCanvasB  && tsB.length > 2) drawScoreHeatmap(hmScoreCanvasB, tsB, dark);
    }
  }

  useWindowTitle("window.title.compare");
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden">

  <!-- ── Content ──────────────────────────────────────────────────────────── -->
    <div bind:this={scrollContainer}
      class="min-h-0 flex-1 overflow-y-auto px-4 py-4 flex flex-col gap-4">

    {#if loading}
      <div class="flex flex-col items-center justify-center gap-3 py-12">
        <svg class="w-6 h-6 text-muted-foreground spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
        </svg>
        <p class="text-[0.73rem] text-muted-foreground">{t("common.loading")}</p>
      </div>

    {:else if sessions.length === 0}
      <div class="flex flex-col items-center justify-center gap-3 py-16">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
             stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
             class="w-10 h-10 text-muted-foreground/30">
          <line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/>
          <line x1="6" y1="20" x2="6" y2="14"/>
        </svg>
        <p class="text-[0.78rem] text-muted-foreground">{t("compare.needSessions")}</p>
        <p class="text-[0.65rem] text-muted-foreground/60 max-w-[260px] text-center leading-relaxed">
          {t("compare.needSessionsHint")}
        </p>
        <button
          onclick={refreshSessions}
          disabled={refreshing}
          class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg
                 border border-border dark:border-white/[0.08]
                 bg-white dark:bg-[#14141e]
                 text-[0.65rem] text-muted-foreground hover:text-foreground
                 hover:border-foreground/20 disabled:opacity-40
                 transition-colors">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 {refreshing ? 'spin' : ''}">
            <polyline points="1 4 1 10 7 10"/>
            <path d="M3.51 15a9 9 0 1 0 .49-4.5"/>
          </svg>
          {t("compare.refresh")}
        </button>
      </div>

    {:else}
      <!-- ── Timeline range pickers (stacked, full-width, 48h window) ────── -->

      {#snippet timelinePicker(
        side: "A"|"B",
        label: string,
        anchorUtc: number|null,
        dayStr: string|null,
        windowSessions: EmbeddingSession[],
        rangeStart: number|null,
        rangeEnd: number|null,
        durSecs: number,
        sessCount: number,
        valid: boolean,
        accent: string,
        accentBg: string,
      )}
        {@const anchor = anchorUtc ?? 0}
        {@const day2Utc = anchor + 86400}
        {@const day2Str = anchor > 0 ? localDateFromUtc(day2Utc + 43200) : null}

        <div class="flex flex-col gap-1.5">

          <!-- Header row: label · status · day navigation -->
          <div class="flex items-center gap-2">
            <!-- Side badge -->
            <span class="text-[0.62rem] font-bold tracking-widest uppercase shrink-0" style="color:{accent}">{label}</span>

            <!-- Status -->
            {#if durSecs > 86400}
              <span class="text-[0.5rem] text-red-500">{t("compare.rangeExceedsLimit")}</span>
            {:else if valid}
              <span class="text-[0.5rem] text-muted-foreground/50 tabular-nums">
                {sessCount} {sessCount === 1 ? "session" : "sessions"} · {fmtDuration(durSecs)}
                <span class="text-emerald-500 ml-0.5">✓</span>
              </span>
            {:else if anchorUtc !== null && sessCount === 0}
              <span class="text-[0.5rem] text-amber-500">{t("compare.noSessionsInRange")}</span>
            {/if}

            <!-- Spacer -->
            <div class="flex-1"></div>

            <!-- Day navigation -->
            <div class="flex items-center gap-1 shrink-0">
              <button
                disabled={!dayStr || sortedDays.indexOf(dayStr) <= 0}
                onclick={() => navigateDay(side, -1)}
                title={t("common.newer")}
                class="w-5 h-5 rounded flex items-center justify-center text-muted-foreground
                       hover:text-foreground hover:bg-muted/60 disabled:opacity-25
                       disabled:pointer-events-none transition-colors">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                     stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                  <polyline points="15 18 9 12 15 6"/>
                </svg>
              </button>

              <select
                value={dayStr ?? ""}
                onchange={(e) => { const v = (e.target as HTMLSelectElement).value; if (v) selectDay(side, v); }}
                class="text-[0.62rem] font-medium text-foreground/80 bg-transparent
                       border-none outline-none cursor-pointer">
                {#if !dayStr}
                  <option value="" disabled>— pick a day —</option>
                {/if}
                {#each sortedDays as d}
                  <option value={d}>{dayLabel(d)}</option>
                {/each}
              </select>

              <button
                disabled={!dayStr || sortedDays.indexOf(dayStr) >= sortedDays.length - 1}
                onclick={() => navigateDay(side, +1)}
                title={t("common.older")}
                class="w-5 h-5 rounded flex items-center justify-center text-muted-foreground
                       hover:text-foreground hover:bg-muted/60 disabled:opacity-25
                       disabled:pointer-events-none transition-colors">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                     stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3">
                  <polyline points="9 18 15 12 9 6"/>
                </svg>
              </button>
            </div>
          </div>

          <!-- 48-hour timeline (wheel-scroll navigates days) -->
          <div class="rounded-lg border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] overflow-hidden w-full"
               role="application"
               aria-label="48-hour session timeline"
               style="touch-action:none; user-select:none"
               use:timelineWheel={side}
               onpointerdown={(e) => ptrDown(e, side)}
               onpointermove={(e) => ptrMove(e, side)}
               onpointerup={() => ptrUp(side)}
               onpointercancel={() => { dragSide = null; }}>

            <!-- Tick / label row -->
            <div class="relative h-5 bg-muted/30 dark:bg-white/[0.02] select-none">
              <!-- Day 1 label at far left -->
              {#if dayStr}
                <span class="absolute top-0.5 left-1 text-[5.5px] font-semibold text-muted-foreground/50
                             pointer-events-none uppercase tracking-wide">
                  {fromUnix(anchor + 43200).toLocaleDateString("default",{weekday:"short",month:"short",day:"numeric"})}
                </span>
              {/if}
              <!-- Day 2 label at 50% -->
              {#if day2Str}
                <span class="absolute top-0.5 text-[5.5px] font-semibold text-muted-foreground/50
                             pointer-events-none uppercase tracking-wide"
                      style="left:50%; transform:translateX(2px)">
                  {fromUnix(day2Utc + 43200).toLocaleDateString("default",{weekday:"short",month:"short",day:"numeric"})}
                </span>
              {/if}
              <!-- Hour ticks every 3h across 48h — labels read from local clock -->
              {#each [3,6,9,12,15,18,21,24,27,30,33,36,39,42,45] as hOff}
                {@const pct = hOff / 48 * 100}
                {@const tickUtc = anchor + hOff * 3600}
                {@const localH = anchor > 0 ? new Date(tickUtc * 1000).getHours() : hOff % 24}
                {@const isMidnight = localH === 0}
                <span class="absolute bottom-0 pointer-events-none"
                      style="left:{pct}%; width:{isMidnight?'2px':'1px'}; top:{isMidnight?'0':'40%'};
                             background:{isMidnight?'rgba(148,163,184,0.5)':'rgba(148,163,184,0.2)'}">
                </span>
                {#if hOff % 6 === 0}
                  <span class="absolute bottom-0.5 text-[5px] tabular-nums pointer-events-none
                               {isMidnight ? 'text-muted-foreground/55 font-semibold' : 'text-muted-foreground/35'}"
                        style="left:{pct}%; transform:translateX(-50%)">
                    {String(localH).padStart(2,"0")}
                  </span>
                {/if}
              {/each}
            </div>

            <!-- Session bars + range overlay row -->
            <div class="relative h-12 cursor-crosshair">
              <!-- Subtle 6h grid lines -->
              {#each [6,12,18,30,36,42] as hOff}
                <span class="absolute inset-y-0 w-px bg-border/20 dark:bg-white/[0.03] pointer-events-none"
                      style="left:{hOff/48*100}%"></span>
              {/each}
              <!-- Day boundary line at 50% -->
              <span class="absolute inset-y-0 w-0.5 bg-border/40 dark:bg-white/[0.07] pointer-events-none"
                    style="left:50%"></span>

              <!-- Session segments -->
              {#if anchor > 0}
                {#each windowSessions as sess, i}
                  {@const lp = Math.max(0,   (Math.max(sess.start_utc, anchor)         - anchor) / 172800 * 100)}
                  {@const rp = Math.min(100,  (Math.min(sess.end_utc,  anchor+172800)   - anchor) / 172800 * 100)}
                  {@const wp = Math.max(0.4,  rp - lp)}
                  {@const clr = SESSION_COLORS[i % SESSION_COLORS.length]}
                  {@const dur = fmtDuration(sess.end_utc - sess.start_utc)}
                  <button
                    onpointerdown={(e) => e.stopPropagation()}
                    onclick={(e) => { e.stopPropagation(); pickSession(side, sess); }}
                    title="{utcToTimeStr(sess.start_utc)} – {utcToTimeStr(sess.end_utc)} · {dur} · click to select"
                    class="absolute top-2 bottom-2 rounded overflow-hidden flex items-center justify-center
                           transition-all duration-100 hover:brightness-110 hover:scale-y-105 pointer-events-auto
                           ring-0 hover:ring-2 hover:ring-white/60 hover:ring-offset-0"
                    style="left:{lp}%; width:{wp}%; background:{clr}; opacity:0.72; z-index:2">
                    {#if wp > 5}
                      <span class="text-[6px] font-semibold text-white drop-shadow-sm pointer-events-none truncate px-0.5">{dur}</span>
                    {/if}
                  </button>
                {/each}
              {/if}

              <!-- Range selection overlay -->
              {#if rangeStart !== null && rangeEnd !== null && rangeEnd > rangeStart && anchor > 0}
                {@const rl = Math.max(0,   (rangeStart - anchor) / 172800 * 100)}
                {@const rr = Math.min(100, (rangeEnd   - anchor) / 172800 * 100)}
                {@const rw = Math.max(0.3, rr - rl)}
                <div class="absolute inset-y-0 pointer-events-none z-10 rounded-sm border-2"
                     style="left:{rl}%; width:{rw}%; background:{accentBg}; border-color:{accent}">
                  {#if rw > 8}
                    <span class="absolute left-0.5 bottom-0.5 text-[5px] font-bold pointer-events-none leading-none"
                          style="color:{accent}">{utcToTimeStr(rangeStart)}</span>
                    <span class="absolute right-0.5 bottom-0.5 text-[5px] font-bold pointer-events-none leading-none"
                          style="color:{accent}">{utcToTimeStr(rangeEnd)}</span>
                  {/if}
                </div>
              {/if}
            </div>
          </div>

          <!-- Precision datetime inputs -->
          {#if anchorUtc !== null}
            <div class="flex items-center gap-1.5">
              <span class="text-[0.5rem] text-muted-foreground/50 shrink-0">{t("compare.timeFrom")}</span>
              <input type="datetime-local"
                value={rangeStart !== null ? utcToDateTimeLocal(rangeStart) : utcToDateTimeLocal(anchor)}
                min={utcToDateTimeLocal(anchor)}
                max={utcToDateTimeLocal(anchor + 172800)}
                oninput={(e) => setRangeStart(side, (e.target as HTMLInputElement).value)}
                class="flex-1 min-w-0 rounded border border-border dark:border-white/[0.08]
                       bg-background dark:bg-[#14141e] px-1.5 py-0.5
                       text-[0.6rem] text-foreground focus:outline-none focus:ring-1"
                style="--tw-ring-color:{accent}50"/>
              <span class="text-[0.5rem] text-muted-foreground/50 shrink-0">–</span>
              <input type="datetime-local"
                value={rangeEnd !== null ? utcToDateTimeLocal(rangeEnd) : utcToDateTimeLocal(anchor + 86400)}
                min={utcToDateTimeLocal(anchor)}
                max={utcToDateTimeLocal(anchor + 172800)}
                oninput={(e) => setRangeEnd(side, (e.target as HTMLInputElement).value)}
                class="flex-1 min-w-0 rounded border border-border dark:border-white/[0.08]
                       bg-background dark:bg-[#14141e] px-1.5 py-0.5
                       text-[0.6rem] text-foreground focus:outline-none focus:ring-1"
                style="--tw-ring-color:{accent}50"/>
            </div>
          {/if}

        </div>
      {/snippet}

      <!-- Single-day notice -->
      {#if aDayStr !== null && bDayStr !== null && aDayStr === bDayStr}
        <div class="flex items-center gap-2 rounded-lg border border-amber-500/20 bg-amber-500/5
                    px-3 py-2 text-[0.62rem] text-amber-600 dark:text-amber-400">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
               stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 shrink-0">
            <circle cx="12" cy="12" r="10"/>
            <line x1="12" y1="8" x2="12" y2="12"/>
            <line x1="12" y1="16" x2="12.01" y2="16"/>
          </svg>
          <span>Only one day of data. Both ranges show the same day — drag the timeline or click a session bar to select different windows for A and B.</span>
        </div>
      {/if}

      <div class="flex flex-col gap-4">
        {@render timelinePicker("A", t("compare.rangeA"), aAnchorUtc, aDayStr, aDaySessions, aRangeStart, aRangeEnd, aDurSecs, aSessions.length, aValid, "#3b82f6", "rgba(59,130,246,0.15)")}
        {@render timelinePicker("B", t("compare.rangeB"), bAnchorUtc, bDayStr, bDaySessions, bRangeStart, bRangeEnd, bDurSecs, bSessions.length, bValid, "#f59e0b", "rgba(245,158,11,0.15)")}
      </div>

      <!-- Compare button + status -->
      <div class="flex items-center gap-3">
        <Button size="sm"
                class="text-[0.7rem] h-8 px-4"
                disabled={!canCompare || comparing}
                onclick={compare}>
          {#if comparing}
            <svg class="w-3.5 h-3.5 mr-1.5 spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"/>
            </svg>
            {t("compare.comparing")}
          {:else}
            {t("compare.compareBtn")}
          {/if}
        </Button>
        {#if metricsA && metricsB}
          <Button size="sm" variant="outline"
                  class="text-[0.65rem] h-8 px-3 gap-1"
                  onclick={copySummary}>
            {#if copied}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3 h-3 text-emerald-500">
                <polyline points="20 6 9 17 4 12"/>
              </svg>
              {t("compare.copied")}
            {:else}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3 h-3">
                <rect x="9" y="9" width="13" height="13" rx="2" ry="2"/>
                <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>
              </svg>
              {t("compare.copySummary")}
            {/if}
          </Button>
          <span class="text-[0.58rem] text-muted-foreground/50">
            {t("compare.epochsA", { n: metricsA.n_epochs })} · {t("compare.epochsB", { n: metricsB.n_epochs })}
          </span>
        {/if}
        {#if false}<!-- urlParamMismatch removed -->
          <div>
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                 stroke-linecap="round" class="w-3 h-3 shrink-0">
              <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>
              <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
            </svg>
          </div>
        {/if}
      </div>

      <!-- ── Results ──────────────────────────────────────────────────────── -->
      {#if metricsA && metricsB}
        <Separator class="bg-border dark:bg-white/[0.06]" />

        <!-- ── Band Spectrum (stacked bars) ────────────────────────────── -->
        <div class="flex flex-col gap-2">
          <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("compare.spectrum")}
          </span>

          <!-- Band legend -->
          <div class="flex items-center gap-3 px-1">
            {#each bandMeta as band, i}
              <div class="flex items-center gap-1">
                <span class="w-2 h-2 rounded-full shrink-0" style="background:{band.color}"></span>
                <span class="text-[0.55rem] text-muted-foreground">{band.sym} {band.name}</span>
              </div>
            {/each}
          </div>

          <!-- Spectrum A -->
          <div class="flex flex-col gap-1">
            <div class="flex items-center gap-2">
              <span class="text-[0.55rem] font-bold text-muted-foreground w-4 shrink-0">A</span>
              <div class="flex-1 h-[34px]">
                <canvas bind:this={specCanvasA} class="w-full h-full block"></canvas>
              </div>
            </div>
            {#if metricsA.n_epochs === 0}
              <span class="text-[0.55rem] text-red-400 ml-6">{t("compare.noEpochs")}</span>
            {/if}
          </div>

          <!-- Spectrum B -->
          <div class="flex flex-col gap-1">
            <div class="flex items-center gap-2">
              <span class="text-[0.55rem] font-bold text-muted-foreground w-4 shrink-0">B</span>
              <div class="flex-1 h-[34px]">
                <canvas bind:this={specCanvasB} class="w-full h-full block"></canvas>
              </div>
            </div>
            {#if metricsB.n_epochs === 0}
              <span class="text-[0.55rem] text-red-400 ml-6">{t("compare.noEpochs")}</span>
            {/if}
          </div>
        </div>

        <!-- ── Diff Chart (grouped vertical bars) ─────────────────────── -->
        <div class="flex flex-col gap-2">
          <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("compare.bandPowers")}
          </span>
          <div class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] overflow-hidden p-2">
            <canvas bind:this={diffCanvas}
                    class="w-full block text-muted-foreground"
                    style="height:140px"></canvas>
          </div>

          <!-- Numeric table -->
          <div class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] overflow-hidden">
            <!-- Header -->
            <div class="grid grid-cols-[1fr_72px_72px_56px] gap-2 px-3.5 py-2
                        border-b border-border dark:border-white/[0.05]
                        bg-muted/30 dark:bg-white/[0.02]">
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground">{t("compare.band")}</span>
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">A</span>
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">B</span>
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">{t("compare.diff")}</span>
            </div>
            {#each bandKeys as key, i}
              {@const a = bv(metricsA, key)}
              {@const b = bv(metricsB, key)}
              <div class="grid grid-cols-[1fr_72px_72px_56px] gap-2 px-3.5 py-1.5
                          border-b border-border/50 dark:border-white/[0.03] last:border-b-0
                          items-center">
                <div class="flex items-center gap-2">
                  <span class="w-2 h-2 rounded-full shrink-0" style="background:{bandMeta[i].color}"></span>
                  <span class="text-[0.68rem] font-semibold text-foreground">{bandMeta[i].name}</span>
                  <span class="text-[0.58rem] text-muted-foreground/40">{bandMeta[i].sym}</span>
                </div>
                <span class="text-[0.68rem] tabular-nums text-foreground text-right">{pct(a)}%</span>
                <span class="text-[0.68rem] tabular-nums text-foreground text-right">{pct(b)}%</span>
                <span class="text-[0.6rem] tabular-nums text-right font-semibold {dc(a, b)}">{diff(a, b)}</span>
              </div>
            {/each}
          </div>
        </div>

        <!-- ── Scores ─────────────────────────────────────────────────── -->
        <div class="flex flex-col gap-2">
          <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("compare.scores")}
          </span>
          <div class="grid grid-cols-3 gap-2">
            {#each scoreKeys as sk}
              {@const a = metricsA[sk.key]}
              {@const b = metricsB[sk.key]}
              <div class="rounded-xl border border-border dark:border-white/[0.06]
                          bg-white dark:bg-[#14141e] px-3 py-2.5 flex flex-col gap-2">
                <span class="text-[0.5rem] font-semibold tracking-widest uppercase text-muted-foreground">
                  {t(sk.label)}
                </span>
                <!-- A bar -->
                <div class="flex items-center gap-1.5">
                  <span class="text-[0.48rem] text-muted-foreground/50 w-3 shrink-0">A</span>
                  <div class="flex-1 h-2.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
                    <div class="h-full rounded-full transition-all duration-500"
                         style="width:{Math.min(100, a)}%; background:{sk.color}"></div>
                  </div>
                  <span class="text-[0.62rem] tabular-nums font-bold w-7 text-right" style="color:{sk.color}">{a.toFixed(0)}</span>
                </div>
                <!-- B bar -->
                <div class="flex items-center gap-1.5">
                  <span class="text-[0.48rem] text-muted-foreground/50 w-3 shrink-0">B</span>
                  <div class="flex-1 h-2.5 rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
                    <div class="h-full rounded-full transition-all duration-500"
                         style="width:{Math.min(100, b)}%; background:{sk.color}; opacity:0.55"></div>
                  </div>
                  <span class="text-[0.62rem] tabular-nums font-bold w-7 text-right" style="color:{sk.color}; opacity:0.65">{b.toFixed(0)}</span>
                </div>
                <!-- Diff -->
                <span class="text-[0.55rem] tabular-nums font-semibold text-center {sdc(a, b)}">
                  {scoreDiff(a, b)}
                </span>
              </div>
            {/each}
          </div>
        </div>

        <!-- ── Radar Chart ─────────────────────────────────────────── -->
        <div class="flex flex-col gap-2">
          <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("compare.radarChart")}
          </span>
          <div class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] overflow-hidden p-2">
            <canvas bind:this={radarCanvas}
                    class="w-full block text-muted-foreground"
                    style="height:220px"></canvas>
            <div class="flex items-center justify-center gap-4 text-[0.48rem] text-muted-foreground/60 pt-1">
              <div class="flex items-center gap-1">
                <span class="inline-block w-2.5 h-0.5 rounded bg-blue-500"></span>
                <span>A</span>
              </div>
              <div class="flex items-center gap-1">
                <span class="inline-block w-2.5 h-0.5 rounded bg-amber-500 opacity-65"></span>
                <span>B</span>
              </div>
            </div>
          </div>
        </div>

        <!-- ── Insights Summary ─────────────────────────────────────── -->
        {#if improved.length > 0 || declined.length > 0}
          <div class="flex flex-col gap-2">
            <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
              {t("compare.insights")}
            </span>
            <div class="rounded-xl border border-border dark:border-white/[0.06]
                        bg-white dark:bg-[#14141e] px-3.5 py-3 flex flex-col gap-2.5">
              {#if improved.length > 0}
                <div class="flex flex-wrap items-center gap-1.5">
                  <span class="text-[0.55rem] font-semibold text-emerald-500 shrink-0">▲ Improved</span>
                  {#each improved as d}
                    <span class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md
                                 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400
                                 text-[0.55rem] font-medium">
                      {d.label}
                      <span class="text-[0.45rem] opacity-70">
                        {d.pctChange > 0 ? "+" : ""}{d.pctChange.toFixed(0)}%
                      </span>
                    </span>
                  {/each}
                </div>
              {/if}
              {#if declined.length > 0}
                <div class="flex flex-wrap items-center gap-1.5">
                  <span class="text-[0.55rem] font-semibold text-red-400 shrink-0">▼ Declined</span>
                  {#each declined as d}
                    <span class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md
                                 bg-red-500/10 text-red-500 dark:text-red-400
                                 text-[0.55rem] font-medium">
                      {d.label}
                      <span class="text-[0.45rem] opacity-70">
                        {d.pctChange > 0 ? "+" : ""}{d.pctChange.toFixed(0)}%
                      </span>
                    </span>
                  {/each}
                </div>
              {/if}
              <p class="text-[0.42rem] text-muted-foreground/40 italic">
                Comparing session B vs A. Changes &gt;3% shown.
              </p>
            </div>
          </div>
        {/if}

        <!-- ── FAA ────────────────────────────────────────────────────── -->
        <div class="flex flex-col gap-2">
          <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
            {t("dashboard.faa")}
          </span>
          <div class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] px-3.5 py-3 flex flex-col gap-3">
            <!-- Centre-anchored gauge for each -->
            {#each [
              { label: "A", val: metricsA.faa },
              { label: "B", val: metricsB.faa },
            ] as item}
              <div class="flex flex-col gap-1">
                <div class="flex items-center gap-2">
                  <span class="text-[0.5rem] font-bold text-muted-foreground/60 w-3">{item.label}</span>
                  <div class="flex-1 h-2 rounded-full bg-black/6 dark:bg-white/8 relative overflow-hidden">
                    <!-- Centre line -->
                    <div class="absolute top-0 bottom-0 left-1/2 w-px bg-muted-foreground/20"></div>
                    <!-- Bar from centre -->
                    {#if item.val >= 0}
                      <div class="absolute top-0 bottom-0 left-1/2 rounded-r-full bg-violet-500/70"
                           style="width:{Math.min(50, Math.abs(item.val) * 50)}%"></div>
                    {:else}
                      <div class="absolute top-0 bottom-0 rounded-l-full bg-violet-500/70"
                           style="right:50%; width:{Math.min(50, Math.abs(item.val) * 50)}%"></div>
                    {/if}
                  </div>
                  <span class="text-[0.68rem] font-bold tabular-nums w-12 text-right"
                        style="color:{Math.abs(item.val) > 0.3 ? 'var(--color-violet-500)' : 'inherit'}">
                    {item.val >= 0 ? "+" : ""}{item.val.toFixed(3)}
                  </span>
                </div>
              </div>
            {/each}
            <!-- Diff -->
            <div class="flex items-center justify-between pt-1 border-t border-border/50 dark:border-white/[0.04]">
              <div class="flex justify-between text-[0.42rem] text-muted-foreground/30 flex-1">
                <span>{t("dashboard.faaWithdrawal")}</span>
                <span>{t("dashboard.faaFormula")}</span>
                <span>{t("dashboard.faaApproach")}</span>
              </div>
              <span class="text-[0.6rem] tabular-nums font-semibold ml-3 w-12 text-right {sdc(metricsA.faa, metricsB.faa)}">
                {(metricsA.faa - metricsB.faa) >= 0 ? "+" : ""}{(metricsA.faa - metricsB.faa).toFixed(3)}
              </span>
            </div>
          </div>
        </div>
        <!-- ── Advanced Metrics Table ──────────────────────────────── -->
        <div class="flex flex-col gap-2">
          <button
            onclick={() => advExpanded = !advExpanded}
            class="flex items-center gap-1.5 group w-fit">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                 stroke-linecap="round" stroke-linejoin="round"
                 class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                        transition-transform duration-150 shrink-0 {advExpanded ? 'rotate-90' : ''}">
              <path d="M9 18l6-6-6-6"/>
            </svg>
            <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground
                         group-hover:text-foreground transition-colors">
              {t("compare.advancedMetrics")}
            </span>
          </button>
          {#if advExpanded}
          <div class="rounded-xl border border-border dark:border-white/[0.06]
                      bg-white dark:bg-[#14141e] overflow-hidden">
            <!-- Header -->
            <div class="grid grid-cols-[1fr_72px_72px_56px] gap-2 px-3.5 py-2
                        border-b border-border dark:border-white/[0.05]
                        bg-muted/30 dark:bg-white/[0.02]">
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground">{t("compare.metric")}</span>
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">A</span>
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">B</span>
              <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">{t("compare.diff")}</span>
            </div>
            {#each advancedMetrics as mr}
              {@const a = Number(metricsA[mr.key]) || 0}
              {@const b = Number(metricsB[mr.key]) || 0}
              {@const d = a - b}
              <div class="grid grid-cols-[1fr_72px_72px_56px] gap-2 px-3.5 py-1.5
                          border-b border-border/50 dark:border-white/[0.03] last:border-b-0
                          items-center">
                <span class="text-[0.68rem] font-medium">
                  {t(mr.label)}{#if mr.unit} <span class="text-[0.5rem] text-muted-foreground/50">({mr.unit})</span>{/if}
                </span>
                <span class="text-[0.68rem] tabular-nums text-foreground text-right">{mr.fmt(a)}</span>
                <span class="text-[0.68rem] tabular-nums text-foreground text-right">{mr.fmt(b)}</span>
                <span class="text-[0.6rem] tabular-nums text-right font-semibold {Math.abs(d) < 0.001 ? 'text-muted-foreground/40' : d > 0 ? 'text-emerald-500' : 'text-red-400'}">
                  {Math.abs(d) < 0.001 ? "—" : `${d > 0 ? "+" : ""}${mr.fmt(d)}`}
                </span>
              </div>
            {/each}
          </div>
          {/if}
        </div>

        <!-- ── Heatmap Charts ─────────────────────────────────────── -->
        {#if tsA.length > 2 || tsB.length > 2}
          <div class="flex flex-col gap-2">
            <div class="flex items-center gap-2">
              <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
                {t("compare.heatmap")}
              </span>
              <Button size="sm" variant="ghost" class="text-[0.6rem] h-6 px-2"
                      onclick={() => showHeatmaps = !showHeatmaps}>
                {showHeatmaps ? "▲ Hide" : "▼ Show"}
              </Button>
            </div>

            {#if showHeatmaps}
              <!-- Band Power Heatmap — Session A -->
              {#if tsA.length > 2}
                <div class="flex flex-col gap-1">
                  <div class="flex items-center gap-2">
                    <span class="text-[0.48rem] font-bold text-muted-foreground/60 w-4">A</span>
                    <span class="text-[0.48rem] text-muted-foreground/40">{t("compare.heatmapBands")}</span>
                  </div>
                  <div class="rounded-xl border border-border dark:border-white/[0.06]
                              bg-[#f5f5fa] dark:bg-[#0e0e1a] overflow-hidden">
                    <canvas bind:this={hmBandCanvasA}
                            class="w-full block"
                            style="height:{HEATMAP_ROW_H * 5}px; display:block"></canvas>
                  </div>
                  <span class="text-[0.42rem] text-muted-foreground/30 pl-1 italic">
                    {t("compare.heatmapRowNorm")} · {tsA.length} epochs
                  </span>
                </div>
              {/if}

              <!-- Band Power Heatmap — Session B -->
              {#if tsB.length > 2}
                <div class="flex flex-col gap-1">
                  <div class="flex items-center gap-2">
                    <span class="text-[0.48rem] font-bold text-muted-foreground/60 w-4">B</span>
                    <span class="text-[0.48rem] text-muted-foreground/40">{t("compare.heatmapBands")}</span>
                  </div>
                  <div class="rounded-xl border border-border dark:border-white/[0.06]
                              bg-[#f5f5fa] dark:bg-[#0e0e1a] overflow-hidden">
                    <canvas bind:this={hmBandCanvasB}
                            class="w-full block"
                            style="height:{HEATMAP_ROW_H * 5}px; display:block"></canvas>
                  </div>
                  <span class="text-[0.42rem] text-muted-foreground/30 pl-1 italic">
                    {t("compare.heatmapRowNorm")} · {tsB.length} epochs
                  </span>
                </div>
              {/if}

              <!-- Band Power Diff Heatmap (B − A) -->
              {#if tsA.length > 2 && tsB.length > 2}
                <div class="flex flex-col gap-1">
                  <span class="text-[0.48rem] text-muted-foreground/40">{t("compare.heatmapDiff")}</span>
                  <div class="rounded-xl border border-border dark:border-white/[0.06]
                              bg-[#f5f5fa] dark:bg-[#0e0e1a] overflow-hidden">
                    <canvas bind:this={hmBandDiffCanvas}
                            class="w-full block"
                            style="height:{HEATMAP_ROW_H * 5 + 12}px; display:block"></canvas>
                  </div>
                  <span class="text-[0.42rem] text-muted-foreground/30 pl-1 italic">
                    {t("compare.heatmapDiffLegend")} · time-proportionally aligned
                  </span>
                </div>
              {/if}

              <!-- Score Heatmap — Session A -->
              {#if tsA.length > 2}
                <div class="flex flex-col gap-1">
                  <div class="flex items-center gap-2">
                    <span class="text-[0.48rem] font-bold text-muted-foreground/60 w-4">A</span>
                    <span class="text-[0.48rem] text-muted-foreground/40">{t("compare.heatmapScores")}</span>
                  </div>
                  <div class="rounded-xl border border-border dark:border-white/[0.06]
                              bg-[#f5f5fa] dark:bg-[#0e0e1a] overflow-hidden">
                    <canvas bind:this={hmScoreCanvasA}
                            class="w-full block"
                            style="height:{HEATMAP_ROW_H * 5}px; display:block"></canvas>
                  </div>
                </div>
              {/if}

              <!-- Score Heatmap — Session B -->
              {#if tsB.length > 2}
                <div class="flex flex-col gap-1">
                  <div class="flex items-center gap-2">
                    <span class="text-[0.48rem] font-bold text-muted-foreground/60 w-4">B</span>
                    <span class="text-[0.48rem] text-muted-foreground/40">{t("compare.heatmapScores")}</span>
                  </div>
                  <div class="rounded-xl border border-border dark:border-white/[0.06]
                              bg-[#f5f5fa] dark:bg-[#0e0e1a] overflow-hidden">
                    <canvas bind:this={hmScoreCanvasB}
                            class="w-full block"
                            style="height:{HEATMAP_ROW_H * 5}px; display:block"></canvas>
                  </div>
                  <span class="text-[0.42rem] text-muted-foreground/30 pl-1 italic">
                    {t("compare.heatmapRowNorm")}
                  </span>
                </div>
              {/if}
            {/if}
          </div>
        {/if}

        <!-- ── Time-Series Charts ──────────────────────────────── -->
        {#if tsA.length > 2 || tsB.length > 2}
          <div class="flex flex-col gap-2">
            <div class="flex items-center gap-2">
              <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
                {t("compare.timeSeries")}
              </span>
              <Button size="sm" variant="ghost" class="text-[0.6rem] h-6 px-2"
                      onclick={() => showCharts = !showCharts}>
                {showCharts ? "▲ Hide" : "▼ Show"}
              </Button>
            </div>

            {#if showCharts}
              {@const tsTimesA = tsA.map(r => r.t)}
              {@const tsTimesB = tsB.map(r => r.t)}
              <!-- Shared time range across all A/B charts -->
              {@const allTimes = [...tsTimesA, ...tsTimesB]}
              {@const sharedXMin = allTimes.length ? Math.min(...allTimes) : 0}
              {@const sharedXMax = allTimes.length ? Math.max(...allTimes) : 1}
              <!-- Shared Y ranges for auto-scaled chart pairs -->
              {@const hrAll = [...tsA.map(r => r.hr), ...tsB.map(r => r.hr)].filter(v => v > 0)}
              {@const rmssdAll = [...tsA.map(r => r.rmssd), ...tsB.map(r => r.rmssd)].filter(v => v > 0)}
              {@const ppgYMin = hrAll.length ? Math.min(...hrAll, ...rmssdAll) * 0.9 : 0}
              {@const ppgYMax = hrAll.length ? Math.max(...hrAll, ...rmssdAll) * 1.1 : 100}
              {@const blinkAll = [...tsA.map(r => r.blink_r), ...tsB.map(r => r.blink_r)]}
              {@const artYMax = Math.max(...blinkAll, 1) * 1.1}
              {@const pitchAll = [...tsA.map(r => r.pitch), ...tsB.map(r => r.pitch)]}
              {@const rollAll = [...tsA.map(r => r.roll), ...tsB.map(r => r.roll)]}
              {@const stillAll = [...tsA.map(r => r.still), ...tsB.map(r => r.still)]}
              {@const poseMin = Math.min(...pitchAll, ...rollAll, ...stillAll) * 1.1}
              {@const poseMax = Math.max(...pitchAll, ...rollAll, ...stillAll, 1) * 1.1}

              <!-- Band Powers overlay -->
              {#if tsA.length > 2}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">A — {t("compare.chartBands")}</span>
                <TimeSeriesChart height={100} yMin={0} yMax={1} xMin={sharedXMin} xMax={sharedXMax}
                  timestamps={tsTimesA}
                  series={[
                    { key: "delta", label: "δ", color: "#6366f1", data: tsA.map(r => r.rd) },
                    { key: "theta", label: "θ", color: "#22c55e", data: tsA.map(r => r.rt) },
                    { key: "alpha", label: "α", color: "#3b82f6", data: tsA.map(r => r.ra) },
                    { key: "beta",  label: "β", color: "#f59e0b", data: tsA.map(r => r.rb) },
                    { key: "gamma", label: "γ", color: "#ef4444", data: tsA.map(r => r.rg) },
                  ]} />
              {/if}
              {#if tsB.length > 2}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">B — {t("compare.chartBands")}</span>
                <TimeSeriesChart height={100} yMin={0} yMax={1} xMin={sharedXMin} xMax={sharedXMax}
                  timestamps={tsTimesB}
                  series={[
                    { key: "delta", label: "δ", color: "#6366f1", data: tsB.map(r => r.rd) },
                    { key: "theta", label: "θ", color: "#22c55e", data: tsB.map(r => r.rt) },
                    { key: "alpha", label: "α", color: "#3b82f6", data: tsB.map(r => r.ra) },
                    { key: "beta",  label: "β", color: "#f59e0b", data: tsB.map(r => r.rb) },
                    { key: "gamma", label: "γ", color: "#ef4444", data: tsB.map(r => r.rg) },
                  ]} />
              {/if}

              <!-- Brain Scores -->
              {#if tsA.length > 2}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">A — {t("compare.chartScores")}</span>
                <TimeSeriesChart height={100} yMin={0} yMax={100} xMin={sharedXMin} xMax={sharedXMax}
                  timestamps={tsTimesA}
                  series={[
                    { key: "relax",   label: "Relax",   color: "#10b981", data: tsA.map(r => r.relaxation) },
                    { key: "engage",  label: "Engage",  color: "#f59e0b", data: tsA.map(r => r.engagement) },
                  ]} />
              {/if}
              {#if tsB.length > 2}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">B — {t("compare.chartScores")}</span>
                <TimeSeriesChart height={100} yMin={0} yMax={100} xMin={sharedXMin} xMax={sharedXMax}
                  timestamps={tsTimesB}
                  series={[
                    { key: "relax",   label: "Relax",   color: "#10b981", data: tsB.map(r => r.relaxation) },
                    { key: "engage",  label: "Engage",  color: "#f59e0b", data: tsB.map(r => r.engagement) },
                  ]} />
              {/if}

              <!-- Composite Scores -->
              {#if tsA.length > 2}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">A — {t("compare.chartComposite")}</span>
                <TimeSeriesChart height={100} yMin={0} yMax={100} xMin={sharedXMin} xMax={sharedXMax}
                  timestamps={tsTimesA}
                  series={[
                    { key: "med",  label: "Meditation",  color: "#8b5cf6", data: tsA.map(r => r.med) },
                    { key: "cog",  label: "Cog. Load",   color: "#3b82f6", data: tsA.map(r => r.cog) },
                    { key: "drow", label: "Drowsiness",  color: "#f59e0b", data: tsA.map(r => r.drow) },
                  ]} />
              {/if}
              {#if tsB.length > 2}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">B — {t("compare.chartComposite")}</span>
                <TimeSeriesChart height={100} yMin={0} yMax={100} xMin={sharedXMin} xMax={sharedXMax}
                  timestamps={tsTimesB}
                  series={[
                    { key: "med",  label: "Meditation",  color: "#8b5cf6", data: tsB.map(r => r.med) },
                    { key: "cog",  label: "Cog. Load",   color: "#3b82f6", data: tsB.map(r => r.cog) },
                    { key: "drow", label: "Drowsiness",  color: "#f59e0b", data: tsB.map(r => r.drow) },
                  ]} />
              {/if}

              <!-- PPG Vitals (if any non-zero HR data) -->
              {#if tsA.some(r => r.hr > 0)}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">A — {t("compare.chartPpg")}</span>
                <TimeSeriesChart height={100} xMin={sharedXMin} xMax={sharedXMax}
                  yMin={ppgYMin} yMax={ppgYMax}
                  timestamps={tsTimesA}
                  series={[
                    { key: "hr",    label: "HR (bpm)",    color: "#ef4444", data: tsA.map(r => r.hr) },
                    { key: "rmssd", label: "RMSSD (ms)",  color: "#10b981", data: tsA.map(r => r.rmssd) },
                  ]} />
              {/if}
              {#if tsB.some(r => r.hr > 0)}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">B — {t("compare.chartPpg")}</span>
                <TimeSeriesChart height={100} xMin={sharedXMin} xMax={sharedXMax}
                  yMin={ppgYMin} yMax={ppgYMax}
                  timestamps={tsTimesB}
                  series={[
                    { key: "hr",    label: "HR (bpm)",    color: "#ef4444", data: tsB.map(r => r.hr) },
                    { key: "rmssd", label: "RMSSD (ms)",  color: "#10b981", data: tsB.map(r => r.rmssd) },
                  ]} />
              {/if}

              <!-- Artifacts -->
              {#if tsA.some(r => r.blink_r > 0)}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">A — {t("compare.chartArtifacts")}</span>
                <TimeSeriesChart height={90} xMin={sharedXMin} xMax={sharedXMax}
                  yMin={0} yMax={artYMax}
                  timestamps={tsTimesA}
                  series={[
                    { key: "blink_r", label: "Blinks/min", color: "#ec4899", data: tsA.map(r => r.blink_r) },
                  ]} />
              {/if}
              {#if tsB.some(r => r.blink_r > 0)}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">B — {t("compare.chartArtifacts")}</span>
                <TimeSeriesChart height={90} xMin={sharedXMin} xMax={sharedXMax}
                  yMin={0} yMax={artYMax}
                  timestamps={tsTimesB}
                  series={[
                    { key: "blink_r", label: "Blinks/min", color: "#ec4899", data: tsB.map(r => r.blink_r) },
                  ]} />
              {/if}

              <!-- Head Pose -->
              {#if tsA.some(r => r.still > 0)}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">A — {t("compare.chartPose")}</span>
                <TimeSeriesChart height={90} xMin={sharedXMin} xMax={sharedXMax}
                  yMin={poseMin} yMax={poseMax}
                  timestamps={tsTimesA}
                  series={[
                    { key: "pitch", label: "Pitch °", color: "#0ea5e9", data: tsA.map(r => r.pitch) },
                    { key: "roll",  label: "Roll °",  color: "#6366f1", data: tsA.map(r => r.roll) },
                    { key: "still", label: "Stillness", color: "#22c55e", data: tsA.map(r => r.still) },
                  ]} />
              {/if}
              {#if tsB.some(r => r.still > 0)}
                <span class="text-[0.48rem] font-semibold text-muted-foreground/60">B — {t("compare.chartPose")}</span>
                <TimeSeriesChart height={90} xMin={sharedXMin} xMax={sharedXMax}
                  yMin={poseMin} yMax={poseMax}
                  timestamps={tsTimesB}
                  series={[
                    { key: "pitch", label: "Pitch °", color: "#0ea5e9", data: tsB.map(r => r.pitch) },
                    { key: "roll",  label: "Roll °",  color: "#6366f1", data: tsB.map(r => r.roll) },
                    { key: "still", label: "Stillness", color: "#22c55e", data: tsB.map(r => r.still) },
                  ]} />
              {/if}
            {/if}
          </div>
        {/if}

        <!-- ── UMAP 3D embedding distribution ──────────────────────── -->
        <!-- The viewer is only shown after the user explicitly requests it. -->
        {#if !umapRequested}
          <div class="flex flex-col gap-2">
            <span class="flex items-baseline gap-1.5 text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground">
              Brain Nebula™
              <span class="text-[0.45rem] font-normal normal-case tracking-normal text-muted-foreground/40">{t("compare.umap")}</span>
            </span>
            <div class="rounded-xl border border-border dark:border-white/[0.06]
                        bg-white dark:bg-[#14141e] px-4 py-5 flex flex-col items-center gap-3">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
                   class="w-8 h-8 text-muted-foreground/30">
                <circle cx="12" cy="12" r="10"/>
                <circle cx="7"  cy="10" r="2"/><circle cx="14" cy="7"  r="2"/>
                <circle cx="17" cy="15" r="2"/><circle cx="9"  cy="17" r="2"/>
                <line x1="7" y1="10" x2="14" y2="7"/>
                <line x1="14" y1="7" x2="17" y2="15"/>
                <line x1="17" y1="15" x2="9" y2="17"/>
              </svg>
              <p class="text-[0.65rem] text-muted-foreground text-center max-w-[240px] leading-relaxed">
                {t("compare.umapDesc")}
              </p>
              <Button size="sm"
                      class="text-[0.7rem] h-8 px-4"
                      onclick={calculateUmap}>
                {t("compare.calculateUmap")}
              </Button>
            </div>
          </div>
        {:else}
          <div class="rounded-xl border border-border bg-card text-card-foreground shadow-sm flex flex-col"
               style="min-height:calc(100vh - 8rem)">
            <!-- Header -->
            <div class="flex items-center gap-2 px-4 py-3 shrink-0 flex-wrap">
              <span class="text-[0.7rem] font-semibold">Brain Nebula™</span>
              <span class="text-[0.45rem] text-muted-foreground/40 font-normal">{t("compare.umap")}</span>
              {#if umapLoading}
                <!-- ── Queued: waiting for other tasks to finish ── -->
                {#if umapQueuePosition !== null && umapQueuePosition > 0}
                  <div class="flex items-center gap-1.5 ml-1
                              px-2 py-1 rounded-lg
                              border border-amber-400/30
                              bg-amber-50/80 dark:bg-amber-950/20">
                    <!-- Queue icon -->
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                         class="w-3 h-3 text-amber-500 shrink-0">
                      <line x1="8" y1="6" x2="21" y2="6"/>
                      <line x1="8" y1="12" x2="21" y2="12"/>
                      <line x1="8" y1="18" x2="21" y2="18"/>
                      <line x1="3" y1="6" x2="3.01" y2="6"/>
                      <line x1="3" y1="12" x2="3.01" y2="12"/>
                      <line x1="3" y1="18" x2="3.01" y2="18"/>
                    </svg>
                    <div class="flex flex-col gap-0 leading-tight">
                      <!-- "X tasks ahead" -->
                      <span class="text-[0.6rem] font-semibold text-amber-700 dark:text-amber-300 tabular-nums">
                        {umapQueuePosition} task{umapQueuePosition === 1 ? "" : "s"} running before yours
                      </span>
                      <!-- Wait + compute breakdown -->
                      <span class="text-[0.52rem] text-amber-600/70 dark:text-amber-400/60 tabular-nums">
                        {#if umapWaitSecs !== null && umapWaitSecs > 0}
                          starts in ~{fmtSecs(umapWaitSecs)}
                        {:else}
                          starting soon
                        {/if}
                        &nbsp;·&nbsp;
                        your task ~{fmtSecs(umapOwnEstimateSecs)}
                      </span>
                    </div>
                    <!-- Live elapsed badge -->
                    <span class="ml-1 text-[0.46rem] text-amber-500/60 tabular-nums shrink-0">
                      {fmtSecs(umapElapsed)} elapsed
                    </span>
                  </div>

                <!-- ── Computing: job is running now ── -->
                {:else}
                  <Spinner size="w-3 h-3" class="text-blue-400 shrink-0" />
                  <span class="text-[0.45rem] text-muted-foreground italic tabular-nums">
                    computing 3D projection
                    {#if umapCountdown != null && umapCountdown > 0 && !umapProgress}
                      · ~{fmtSecs(umapCountdown)} remaining
                    {/if}
                    · {fmtSecs(umapElapsed)} elapsed
                  </span>
                  {#if umapProgress && umapProgress.total_epochs > 0}
                    {@const pct = Math.round(umapProgress.epoch / umapProgress.total_epochs * 100)}
                    {@const remEpochs = umapProgress.total_epochs - umapProgress.epoch}
                    {@const remSecs = umapProgress.epoch_ms > 0
                      ? Math.round(remEpochs * umapProgress.epoch_ms / 1000) : null}
                    <div class="flex-1 max-w-[200px] flex flex-col gap-0.5">
                      <div class="flex items-center gap-1.5">
                        <div class="flex-1 h-1.5 rounded-full bg-muted dark:bg-white/[0.06] overflow-hidden">
                          <div class="h-full rounded-full bg-blue-500 dark:bg-blue-400 transition-all duration-300"
                               style="width:{pct}%"></div>
                        </div>
                        <span class="text-[0.42rem] text-muted-foreground tabular-nums shrink-0 w-6 text-right">
                          {pct}%
                        </span>
                      </div>
                      {#if remSecs !== null}
                        <span class="text-[0.4rem] text-muted-foreground/50 tabular-nums">
                          epoch {umapProgress.epoch}/{umapProgress.total_epochs}
                          · {umapProgress.epoch_ms.toFixed(0)}ms/ep
                          · ~{fmtSecs(remSecs)} left
                        </span>
                      {/if}
                    </div>
                  {/if}
                {/if}

              {:else if umapResult}
                <span class="text-[0.45rem] text-muted-foreground tabular-nums">
                  {umapResult.n_a} + {umapResult.n_b} {t("compare.umapPoints")} · dim={umapResult.dim} · 3D
                  {#if umapComputeMs != null}
                    · {umapComputeMs < 1000 ? `${umapComputeMs}ms` : `${(umapComputeMs / 1000).toFixed(1)}s`} compute
                  {/if}
                </span>
                {#if umapAnalysis}
                  <span class="ml-1 inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[0.45rem] font-semibold tabular-nums
                               {umapAnalysis.separationScore >= 2 ? 'bg-emerald-500/10 text-emerald-500' :
                                umapAnalysis.separationScore >= 1 ? 'bg-yellow-500/10 text-yellow-600 dark:text-yellow-400' :
                                'bg-red-500/10 text-red-400'}">
                    sep: {umapAnalysis.separationScore.toFixed(2)}
                  </span>
                {/if}
              {/if}
            </div>

            <!-- 3D viewer — fills remaining vertical space -->
            <div class="flex-1" style="width:100%; min-height:400px">
              <UmapViewer3D data={umapResult ?? umapPlaceholder ?? { points: [], n_a: 0, n_b: 0, dim: 0 }} computing={umapLoading} progress={umapProgress} />
            </div>

            <!-- Footer legend -->
            <div class="flex items-center gap-4 text-[0.42rem] text-muted-foreground/60 px-4 py-3 shrink-0">
              <div class="flex items-center gap-1">
                <span class="inline-block w-2 h-2 rounded-full" style="background:#3b82f6"></span>
                <span>A ({(umapResult ?? umapPlaceholder)?.n_a ?? 0})</span>
              </div>
              <div class="flex items-center gap-1">
                <span class="inline-block w-2 h-2 rounded-full" style="background:#f59e0b"></span>
                <span>B ({(umapResult ?? umapPlaceholder)?.n_b ?? 0})</span>
              </div>
              <span class="ml-auto italic">{t("compare.umapDesc")}</span>
            </div>
          </div>
        {/if}

        {#if sleepA || sleepB}
          {@const sleepAllUtc = [
            ...(sleepA?.epochs.map(e => e.utc) ?? []),
            ...(sleepB?.epochs.map(e => e.utc) ?? []),
          ]}
          {@const sleepXMin = sleepAllUtc.length ? Math.min(...sleepAllUtc) : 0}
          {@const sleepXMax = sleepAllUtc.length ? Math.max(...sleepAllUtc) : 1}
          <div class="flex flex-col gap-2">
            <button
              onclick={() => sleepExpanded = !sleepExpanded}
              class="flex items-center gap-1.5 group w-fit">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                   stroke-linecap="round" stroke-linejoin="round"
                   class="w-2.5 h-2.5 text-muted-foreground/40 group-hover:text-muted-foreground/70
                          transition-transform duration-150 shrink-0 {sleepExpanded ? 'rotate-90' : ''}">
                <path d="M9 18l6-6-6-6"/>
              </svg>
              <span class="text-[0.6rem] font-semibold tracking-widest uppercase text-muted-foreground
                           group-hover:text-foreground transition-colors">
                {t("sleep.title")}
              </span>
            </button>
          {#if sleepExpanded}

            <!-- Sleep efficiency stats -->
            {#if sleepAnalysisA || sleepAnalysisB}
              <div class="grid grid-cols-2 gap-2">
                {#each [
                  { label: "A", sa: sleepAnalysisA },
                  { label: "B", sa: sleepAnalysisB },
                ] as item}
                  {#if item.sa}
                    <div class="rounded-lg border border-border dark:border-white/[0.06]
                                bg-white dark:bg-[#14141e] px-3 py-2 flex flex-col gap-1">
                      <span class="text-[0.48rem] font-bold text-muted-foreground/60">{item.label}</span>
                      <div class="flex items-center gap-3 flex-wrap">
                        <div class="flex flex-col">
                          <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">Efficiency</span>
                          <span class="text-[0.72rem] font-bold tabular-nums {item.sa.efficiency >= 85 ? 'text-emerald-500' : item.sa.efficiency >= 70 ? 'text-yellow-500' : 'text-red-400'}">
                            {item.sa.efficiency.toFixed(0)}%
                          </span>
                        </div>
                        <div class="flex flex-col">
                          <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">Onset</span>
                          <span class="text-[0.72rem] font-bold tabular-nums">{item.sa.onsetLatencyMin.toFixed(0)}m</span>
                        </div>
                        {#if item.sa.remLatencyMin >= 0}
                          <div class="flex flex-col">
                            <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">→ REM</span>
                            <span class="text-[0.72rem] font-bold tabular-nums">{item.sa.remLatencyMin.toFixed(0)}m</span>
                          </div>
                        {/if}
                        <div class="flex flex-col">
                          <span class="text-[0.42rem] text-muted-foreground/50 uppercase tracking-wider">Awakenings</span>
                          <span class="text-[0.72rem] font-bold tabular-nums">{item.sa.awakenings}</span>
                        </div>
                      </div>
                    </div>
                  {/if}
                {/each}
              </div>
            {/if}

            {#if sleepA}
              <div class="flex flex-col gap-1">
                <span class="text-[0.5rem] font-bold text-muted-foreground/60">A</span>
                <div class="rounded-xl border border-border dark:border-white/[0.06]
                            bg-white dark:bg-[#14141e] p-2">
                  <Hypnogram epochs={sleepA.epochs} summary={sleepA.summary}
                             xMin={sleepB ? sleepXMin : undefined} xMax={sleepB ? sleepXMax : undefined} />
                </div>
              </div>
            {/if}

            {#if sleepB}
              <div class="flex flex-col gap-1">
                <span class="text-[0.5rem] font-bold text-muted-foreground/60">B</span>
                <div class="rounded-xl border border-border dark:border-white/[0.06]
                            bg-white dark:bg-[#14141e] p-2">
                  <Hypnogram epochs={sleepB.epochs} summary={sleepB.summary}
                             xMin={sleepA ? sleepXMin : undefined} xMax={sleepA ? sleepXMax : undefined} />
                </div>
              </div>
            {/if}

            <!-- Side-by-side stage % table -->
            {#if sleepA && sleepB}
              {@const stages = [
                { key: "sleep.wake", a: sleepA.summary.wake_epochs, b: sleepB.summary.wake_epochs, color: "#f59e0b" },
                { key: "sleep.rem",  a: sleepA.summary.rem_epochs,  b: sleepB.summary.rem_epochs,  color: "#a855f7" },
                { key: "sleep.n1",   a: sleepA.summary.n1_epochs,   b: sleepB.summary.n1_epochs,   color: "#38bdf8" },
                { key: "sleep.n2",   a: sleepA.summary.n2_epochs,   b: sleepB.summary.n2_epochs,   color: "#3b82f6" },
                { key: "sleep.n3",   a: sleepA.summary.n3_epochs,   b: sleepB.summary.n3_epochs,   color: "#6366f1" },
              ]}
              <div class="rounded-xl border border-border dark:border-white/[0.06]
                          bg-white dark:bg-[#14141e] overflow-hidden">
                <div class="grid grid-cols-[1fr_60px_60px_50px] gap-2 px-3.5 py-2
                            border-b border-border dark:border-white/[0.05]
                            bg-muted/30 dark:bg-white/[0.02]">
                  <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground">{t("sleep.title")}</span>
                  <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">A</span>
                  <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">B</span>
                  <span class="text-[0.55rem] font-semibold tracking-widest uppercase text-muted-foreground text-right">{t("compare.diff")}</span>
                </div>
                {#each stages as st}
                  {@const aPct = sleepA.summary.total_epochs > 0 ? (st.a / sleepA.summary.total_epochs * 100) : 0}
                  {@const bPct = sleepB.summary.total_epochs > 0 ? (st.b / sleepB.summary.total_epochs * 100) : 0}
                  {@const d = aPct - bPct}
                  <div class="grid grid-cols-[1fr_60px_60px_50px] gap-2 px-3.5 py-1.5
                              border-b border-border/50 dark:border-white/[0.03] last:border-b-0
                              items-center">
                    <div class="flex items-center gap-2">
                      <span class="w-2 h-2 rounded-full shrink-0" style="background:{st.color}"></span>
                      <span class="text-[0.68rem] font-semibold" style="color:{st.color}">{t(st.key)}</span>
                    </div>
                    <span class="text-[0.68rem] tabular-nums text-foreground text-right">{aPct.toFixed(0)}%</span>
                    <span class="text-[0.68rem] tabular-nums text-foreground text-right">{bPct.toFixed(0)}%</span>
                    <span class="text-[0.6rem] tabular-nums text-right font-semibold {Math.abs(d) < 0.5 ? 'text-muted-foreground/40' : d > 0 ? 'text-emerald-500' : 'text-red-400'}">
                      {Math.abs(d) < 0.5 ? "—" : `${d > 0 ? "+" : ""}${d.toFixed(0)}`}
                    </span>
                  </div>
                {/each}
              </div>
            {/if}
          {/if}
          </div>
        {/if}
      {/if}
    {/if}
  </div>
  <DisclaimerFooter />
</main>


