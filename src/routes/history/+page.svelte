<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Session History — single-day view with prev/next pagination. -->

<script lang="ts">
  import { onMount }       from "svelte";
  import { onDestroy }     from "svelte";
  import { fade }          from "svelte/transition";
  import { invoke }        from "@tauri-apps/api/core";
  import { Button }        from "$lib/components/ui/button";
  import { Badge }         from "$lib/components/ui/badge";
  import { Separator }     from "$lib/components/ui/separator";
  import { t }             from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import { getResolved }   from "$lib/theme-store.svelte";
  import DisclaimerFooter  from "$lib/DisclaimerFooter.svelte";
  import Hypnogram         from "$lib/Hypnogram.svelte";
  import { SessionDetail } from "$lib/dashboard";
  import type { SessionMetrics, EpochRow, CsvMetricsResult } from "$lib/dashboard/SessionDetail.svelte";
  import { Spinner }       from "$lib/components/ui/spinner";
  import { hBar, hCbs, type HistoryViewMode } from "$lib/history-titlebar.svelte";

  // ── Types ───────────────────────────────────────────────────────────────
  interface LabelRow {
    id: number; eeg_start: number; eeg_end: number;
    text: string; created_at: number;
  }
  interface SessionEntry {
    csv_file: string; csv_path: string;
    session_start_utc: number | null; session_end_utc: number | null;
    device_name: string | null; serial_number: string | null;
    battery_pct: number | null; total_samples: number | null;
    sample_rate_hz: number | null; labels: LabelRow[];
    file_size_bytes: number;
  }
  interface SleepEpoch {
    utc: number; stage: number;
    rel_delta: number; rel_theta: number; rel_alpha: number; rel_beta: number;
  }
  interface SleepSummary {
    total_epochs: number; wake_epochs: number; n1_epochs: number;
    n2_epochs: number; n3_epochs: number; rem_epochs: number; epoch_secs: number;
  }
  interface SleepStages { epochs: SleepEpoch[]; summary: SleepSummary; }
  interface HistoryStatsData {
    total_sessions: number; total_secs: number;
    this_week_secs: number; last_week_secs: number;
  }

  // ── Pagination state ────────────────────────────────────────────────────
  /** All recording day keys (YYYYMMDD UTC), newest first — used only for fetching. */
  let allUtcDays    = $state<string[]>([]);
  let currentDayIdx = $state(0);
  let dayLoading   = $state(false);
  let daysLoading  = $state(true);

  /** Sessions for the currently displayed day. */
  let sessions     = $state<SessionEntry[]>([]);

  /** Aggregate stats loaded lazily in the background. */
  let historyStats = $state<HistoryStatsData | null>(null);

  // ── Per-session UI state ────────────────────────────────────────────────
  let expanded      = $state<Record<string, boolean>>({});
  let confirmDelete = $state<string | null>(null);
  let hoveredSession = $state<string | null>(null);

  // ── Chart visibility (IntersectionObserver per row) ─────────────────────
  /** csv_paths whose row has entered the viewport — chart is only mounted then. */
  let renderedRows  = $state(new Set<string>());

  /** Svelte action: fires onEnter once when the element scrolls into view
   *  (with a 120px look-ahead margin), then disconnects. */
  function inview(node: HTMLElement, onEnter: () => void) {
    const obs = new IntersectionObserver(
      ([entry]) => { if (entry?.isIntersecting) { onEnter(); obs.disconnect(); } },
      { rootMargin: "120px 0px" }
    );
    obs.observe(node);
    return { destroy: () => obs.disconnect() };
  }

  // ── Per-day localStorage cache ──────────────────────────────────────────
  const DAY_CACHE_PFX   = "skill.history.day.v1.";
  const METRICS_CACHE_PFX = "skill.metrics.v1.";

  function readDayCache(day: string): SessionEntry[] | null {
    try {
      const raw = localStorage.getItem(DAY_CACHE_PFX + day);
      return raw ? (JSON.parse(raw) as SessionEntry[]) : null;
    } catch { return null; }
  }
  function writeDayCache(day: string, data: SessionEntry[]) {
    try { localStorage.setItem(DAY_CACHE_PFX + day, JSON.stringify(data)); } catch {}
  }
  function readMetricsCache(csvPath: string): CsvMetricsResult | null {
    try {
      const raw = sessionStorage.getItem(METRICS_CACHE_PFX + csvPath);
      return raw ? (JSON.parse(raw) as CsvMetricsResult) : null;
    } catch { return null; }
  }
  function writeMetricsCache(csvPath: string, result: CsvMetricsResult) {
    try { sessionStorage.setItem(METRICS_CACHE_PFX + csvPath, JSON.stringify(result)); } catch {}
  }

  // ── Caches: sleep / metrics / timeseries ────────────────────────────────
  let sleepCache   = $state<Record<string, SleepStages | "loading" | "short">>({});
  let metricsCache = $state<Record<string, SessionMetrics | "loading" | "none">>({});
  let tsCache      = $state<Record<string, EpochRow[] | "loading">>({});

  /** Accumulates every SessionEntry we've ever fetched (current day + prefetched
   *  adjacent days).  Lets runMetrics / loadSleep look up timestamps for any
   *  session regardless of which day is currently displayed.              */
  const sessionRegistry = new Map<string, SessionEntry>();
  function registerSessions(list: SessionEntry[]) {
    for (const s of list) if (s.csv_path) sessionRegistry.set(s.csv_path, s);
  }

  function getSleepData(csvPath: string): SleepStages | null {
    const v = sleepCache[csvPath];
    if (!v || v === "loading" || v === "short") return null;
    return (v as SleepStages).epochs.length > 0 ? (v as SleepStages) : null;
  }
  function getMetrics(csvPath: string): SessionMetrics | null {
    const v = metricsCache[csvPath];
    if (!v || v === "loading" || v === "none") return null;
    return (v as SessionMetrics).n_epochs > 0 ? (v as SessionMetrics) : null;
  }
  function getTs(csvPath: string): EpochRow[] | null {
    const v = tsCache[csvPath];
    if (!v || v === "loading") return null;
    return (v as EpochRow[]).length > 2 ? (v as EpochRow[]) : null;
  }

  // ── Throttled metrics loader (max 4 concurrent) ─────────────────────────
  const METRICS_CONCURRENCY = 4;
  let metricsInFlight = 0;
  const metricsBacklog: string[] = [];

  function loadMetrics(csvPath: string) {
    if (csvPath in metricsCache) return;
    metricsCache[csvPath] = "loading";
    tsCache[csvPath]      = "loading";
    if (metricsInFlight < METRICS_CONCURRENCY) { void runMetrics(csvPath); }
    else { metricsBacklog.push(csvPath); }
  }
  function drainMetrics() {
    while (metricsInFlight < METRICS_CONCURRENCY && metricsBacklog.length > 0)
      void runMetrics(metricsBacklog.shift()!);
  }
  async function runMetrics(csvPath: string) {
    metricsInFlight++;
    try {
      try {
        const result = await invoke<CsvMetricsResult | null>("get_csv_metrics", { csvPath });
        if (result && result.n_rows > 0) {
          metricsCache[csvPath] = result.summary;
          tsCache[csvPath]      = result.timeseries;
          writeMetricsCache(csvPath, result);
          return;
        }
      } catch (e) { console.warn("[history] get_csv_metrics:", e); }
      const session = sessionRegistry.get(csvPath);
      if (!session?.session_start_utc || !session?.session_end_utc) {
        metricsCache[csvPath] = "none"; tsCache[csvPath] = []; return;
      }
      try {
        metricsCache[csvPath] = await invoke<SessionMetrics>("get_session_metrics", {
          startUtc: session.session_start_utc, endUtc: session.session_end_utc,
        });
      } catch { metricsCache[csvPath] = "none"; }
      try {
        tsCache[csvPath] = await invoke<EpochRow[]>("get_session_timeseries", {
          startUtc: session.session_start_utc, endUtc: session.session_end_utc,
        });
      } catch { tsCache[csvPath] = []; }
    } finally { metricsInFlight--; drainMetrics(); }
  }

  async function loadSleep(csvPath: string) {
    if (csvPath in sleepCache) return;
    const session = sessionRegistry.get(csvPath);
    if (!session || !session.session_start_utc || !session.session_end_utc) return;
    if ((session.session_end_utc - session.session_start_utc) < 1800) {
      sleepCache[csvPath] = "short"; return;
    }
    sleepCache[csvPath] = "loading";
    try {
      sleepCache[csvPath] = await invoke<SleepStages>("get_sleep_stages", {
        startUtc: session.session_start_utc, endUtc: session.session_end_utc,
      });
    } catch { delete sleepCache[csvPath]; }
  }

  // ── Local-day helpers ────────────────────────────────────────────────────
  /** Convert a UTC Unix-seconds value to its UTC YYYYMMDD directory name. */
  function secToUtcDir(sec: number): string {
    const d = new Date(sec * 1000);
    return `${d.getUTCFullYear()}${String(d.getUTCMonth()+1).padStart(2,"0")}${String(d.getUTCDate()).padStart(2,"0")}`;
  }

  /** Local [midnight, nextMidnight) in Unix seconds for a YYYY-MM-DD local key. */
  function localDayBounds(localKey: string): { startSec: number; endSec: number } {
    const [y, m, d] = localKey.split("-").map(Number);
    return {
      startSec: new Date(y, m - 1, d).getTime() / 1000,
      endSec:   new Date(y, m - 1, d + 1).getTime() / 1000,
    };
  }

  /** Build a sorted (newest-first) list of unique LOCAL YYYY-MM-DD day keys
   *  from the UTC YYYYMMDD directory names.
   *
   *  Each UTC dir covers 00:00–23:59:59 UTC.  Depending on the local
   *  timezone offset that window may straddle two local calendar days, so we
   *  emit both endpoints and de-duplicate.
   *
   *  We cap at today's local date: a UTC dir whose *end* converts to a local
   *  day that hasn't started yet (e.g. UTC Mar 2 00:00 = local Mar 1 19:00 in
   *  EST) must not generate a future "Mar 2" tab — no sessions can be recorded
   *  there yet and it would become the default first page with 0 sessions. */
  function buildLocalDays(utcDirs: string[]): string[] {
    const today = dateKey(Date.now() / 1000); // local today as YYYY-MM-DD
    const seen = new Set<string>();
    const result: string[] = [];
    for (const dir of utcDirs) {
      const startUtc = Date.UTC(
        +dir.slice(0,4), +dir.slice(4,6) - 1, +dir.slice(6,8)
      ) / 1000;
      const endUtc = startUtc + 86400 - 1;
      for (const lk of [dateKey(startUtc), dateKey(endUtc)]) {
        if (!seen.has(lk) && lk <= today) { seen.add(lk); result.push(lk); }
      }
    }
    result.sort((a, b) => b.localeCompare(a)); // newest first
    return result;
  }

  // ── Day navigation ──────────────────────────────────────────────────────
  /** Monotonically increasing counter — incremented on every loadDay call so
   *  that stale responses from rapid navigation are silently discarded.    */
  let loadSeq = 0;

  /** Fetch sessions for a local day key and return the filtered list.
   *  Pure data function — touches no reactive state.                    */
  async function fetchDaySessions(localKey: string): Promise<SessionEntry[]> {
    const { startSec, endSec } = localDayBounds(localKey);
    const dir1 = secToUtcDir(startSec);
    const dir2 = secToUtcDir(endSec - 1);
    const dirsToFetch = [...new Set([dir1, dir2])];

    // Fetch all overlapping UTC dirs in parallel.
    const results = await Promise.allSettled(
      dirsToFetch.map(dir => invoke<SessionEntry[]>("list_sessions_for_day", { day: dir }))
    );

    const seen = new Set<string>();
    const merged: SessionEntry[] = [];
    for (const r of results) {
      if (r.status !== "fulfilled") continue;
      for (const s of r.value) {
        if (seen.has(s.csv_path)) continue;
        seen.add(s.csv_path);
        merged.push(s);
      }
    }

    // Keep only sessions whose start time falls within the local calendar day.
    // Prefer session_start_utc for the comparison; fall back to session_end_utc only
    // when start is absent (genuinely orphaned CSV whose timestamp couldn't be parsed).
    // Sessions that have neither timestamp are excluded — they are corrupt/empty entries.
    const { startSec: s0, endSec: s1 } = localDayBounds(localKey);
    const filtered = merged.filter(s => {
      const t = s.session_start_utc ?? s.session_end_utc;
      if (!t) return false; // no usable timestamp — exclude rather than show a ghost row
      return t >= s0 && t < s1;
    });

    // Sort most-recent sessions first so the list reads newest → oldest.
    filtered.sort((a, b) => {
      const ta = a.session_start_utc ?? a.session_end_utc ?? 0;
      const tb = b.session_start_utc ?? b.session_end_utc ?? 0;
      return tb - ta;
    });

    return filtered;
  }

  /** Warm the localStorage + metrics caches for a day without touching any
   *  reactive display state.  Called speculatively for adjacent days.     */
  async function prefetchDay(localKey: string) {
    // Fetch session list (skip if already cached).
    let list = readDayCache(localKey);
    if (!list) {
      try {
        list = await fetchDaySessions(localKey);
        writeDayCache(localKey, list);
      } catch { return; /* silent — prefetch is best-effort */ }
    }

    // Register sessions so runMetrics can resolve timestamps for them.
    registerSessions(list);

    // Queue metrics for any session not already in the cache.
    for (const s of list) {
      if (!s.csv_path) continue;
      const mc = readMetricsCache(s.csv_path);
      if (mc) {
        // Restore from sessionStorage without triggering reactive updates.
        if (!(s.csv_path in metricsCache)) {
          metricsCache[s.csv_path] = mc.summary as SessionMetrics;
          tsCache[s.csv_path]      = mc.timeseries ?? [];
        }
      } else if (!(s.csv_path in metricsCache)) {
        loadMetrics(s.csv_path);
      }
    }
  }

  async function loadDay(idx: number) {
    if (idx < 0 || idx >= localDays.length) return;
    currentDayIdx = idx;
    const seq     = ++loadSeq;          // tag this navigation
    const localKey = localDays[idx];

    // Reset per-day UI state
    renderedRows  = new Set();
    expanded      = {};
    hoveredSession = null;
    confirmDelete  = null;

    // ① Show cached sessions immediately — zero-latency first paint
    const cached = readDayCache(localKey);
    if (cached && cached.length > 0) {
      sessions = cached;
      registerSessions(cached);
      for (const s of sessions) {
        if (s.csv_path && !(s.csv_path in tsCache)) {
          const mc = readMetricsCache(s.csv_path);
          if (mc) { tsCache[s.csv_path] = mc.timeseries ?? []; metricsCache[s.csv_path] = mc.summary; }
        }
      }
    } else {
      sessions = [];
    }

    // ② Load fresh data from the backend (both UTC dirs fetched in parallel).
    dayLoading = true;
    try {
      const fresh = await fetchDaySessions(localKey);
      if (loadSeq !== seq) return; // navigated away — discard stale response

      sessions = fresh;
      registerSessions(fresh);
      setTimeout(() => writeDayCache(localKey, fresh), 0); // defer serialisation
      for (const s of fresh) {
        if (!s.csv_path) continue;
        const mc = readMetricsCache(s.csv_path);
        if (mc) { tsCache[s.csv_path] = mc.timeseries ?? []; metricsCache[s.csv_path] = mc.summary; }
        else if (!(s.csv_path in metricsCache)) loadMetrics(s.csv_path);
      }
    } catch (e) {
      if (loadSeq === seq) console.error("[history] loadDay failed:", e);
    } finally {
      if (loadSeq === seq) dayLoading = false;
    }

    // ③ Speculatively warm adjacent days so the next navigation is instant.
    setTimeout(() => {
      if (idx > 0)                    void prefetchDay(localDays[idx - 1]);
      if (idx < localDays.length - 1) void prefetchDay(localDays[idx + 1]);
    }, 300);
  }

  // ── Session actions ─────────────────────────────────────────────────────
  async function deleteSession(csvPath: string) {
    await invoke("delete_session", { csvPath });
    confirmDelete = null;
    delete expanded[csvPath];
    sessions = sessions.filter(s => s.csv_path !== csvPath);
    setTimeout(() => writeDayCache(localDays[currentDayIdx], sessions), 0);
  }

  function toggleExpand(csvPath: string) {
    expanded[csvPath] = !expanded[csvPath];
    if (expanded[csvPath]) { loadSleep(csvPath); loadMetrics(csvPath); }
  }

  // ── Quick-compare ───────────────────────────────────────────────────────
  let compareMode     = $state(false);
  let compareSelected = $state<string[]>([]);

  function toggleCompareSelect(csvPath: string) {
    if (compareSelected.includes(csvPath))
      compareSelected = compareSelected.filter(p => p !== csvPath);
    else if (compareSelected.length < 2)
      compareSelected = [...compareSelected, csvPath];
  }
  async function openQuickCompare() {
    if (compareSelected.length < 2) return;
    const [a, b] = compareSelected;
    const sA = sessions.find(s => s.csv_path === a);
    const sB = sessions.find(s => s.csv_path === b);
    if (!sA?.session_start_utc || !sA?.session_end_utc ||
        !sB?.session_start_utc || !sB?.session_end_utc) return;
    try {
      await invoke("open_compare_window_with_sessions", {
        startA: sA.session_start_utc, endA: sA.session_end_utc,
        startB: sB.session_start_utc, endB: sB.session_end_utc,
      });
    } catch (e) { console.error("open_compare_window_with_sessions:", e); }
  }
  function exitCompareMode() { compareMode = false; compareSelected = []; }

  // ── Labels browser ──────────────────────────────────────────────────────
  let allLabels      = $state<any[]>([]);
  let showLabels     = $state(false);
  let labelSearchQuery = $state("");

  async function loadLabels() {
    try { allLabels = await invoke<any[]>("query_annotations", { startUtc: null, endUtc: null }); }
    catch { allLabels = []; }
  }
  async function removeLabel(id: number) {
    try { await invoke("delete_label", { labelId: id }); allLabels = allLabels.filter(l => l.id !== id); }
    catch {}
  }
  const filteredLabels = $derived.by(() => {
    const q = labelSearchQuery.toLowerCase().trim();
    return q ? allLabels.filter(l => l.text.toLowerCase().includes(q)) : allLabels;
  });

  // ── Calendar heatmap state ──────────────────────────────────────────────
  let viewMode = $state<HistoryViewMode>("month");
  /** Anchor date for calendar navigation. */
  let calendarAnchor = $state(new Date());

  function setViewMode(m: HistoryViewMode) {
    viewMode = m;
    if (m === "day" && localDays.length > 0) {
      loadDay(currentDayIdx);
    }
  }

  /** Navigate calendar by one unit in the given direction. */
  function calendarNav(dir: -1 | 1) {
    const d = new Date(calendarAnchor);
    switch (viewMode) {
      case "year":  d.setFullYear(d.getFullYear() + dir); break;
      case "month": d.setMonth(d.getMonth() + dir); break;
      case "week":  d.setDate(d.getDate() + dir * 7); break;
      case "day":   break;
    }
    calendarAnchor = d;
  }

  /** Navigate to a specific day from the calendar heatmap. */
  function navigateToDay(dayKey: string) {
    const idx = localDays.indexOf(dayKey);
    if (idx >= 0) {
      viewMode = "day";
      loadDay(idx);
    }
  }

  function heatColor(count: number, maxC: number): string {
    if (count === 0) return "";
    const intensity = Math.min(1, count / Math.max(1, maxC));
    if (intensity < 0.25) return "bg-emerald-200/60 dark:bg-emerald-900/40";
    if (intensity < 0.5)  return "bg-emerald-300/70 dark:bg-emerald-800/50";
    if (intensity < 0.75) return "bg-emerald-400/80 dark:bg-emerald-700/60";
    return "bg-emerald-500 dark:bg-emerald-600/80";
  }

  // ── Week/day epoch dot timeline ─────────────────────────────────────────

  /** Sessions loaded for each day key in the week view. */
  let weekSessions = $state<Map<string, SessionEntry[]>>(new Map());
  let weekLoading  = $state(false);

  /** Load sessions + timeseries for all days shown in the current week view. */
  async function loadWeekData() {
    if (viewMode !== "week") return;
    weekLoading = true;
    const dayKeys = calendarCells.map(c => c.dayKey).filter(k => k);
    const map = new Map<string, SessionEntry[]>();
    await Promise.all(dayKeys.map(async (dk) => {
      try {
        let list = readDayCache(dk);
        if (!list) {
          list = await fetchDaySessions(dk);
          writeDayCache(dk, list);
        }
        registerSessions(list);
        map.set(dk, list);
        // Trigger timeseries loading for each session
        for (const s of list) {
          if (s.csv_path && !(s.csv_path in metricsCache)) loadMetrics(s.csv_path);
        }
      } catch { map.set(dk, []); }
    }));
    weekSessions = map;
    weekLoading = false;
  }

  // Reload week data when anchor or mode changes
  $effect(() => {
    if (viewMode === "week") {
      // Reference calendarAnchor to re-run when it changes
      void calendarAnchor;
      void loadWeekData();
    }
  });

  /** Svelte action: draw epoch dots + labels on a 24h canvas timeline for a given day. */
  function drawDayDots(
    canvas: HTMLCanvasElement,
    data: { sessions: SessionEntry[]; dayStart: number; labels: LabelRow[] }
  ) {
    renderDayDots(canvas, data);
    return {
      update(d: { sessions: SessionEntry[]; dayStart: number; labels: LabelRow[] }) {
        renderDayDots(canvas, d);
      }
    };
  }

  function renderDayDots(
    canvas: HTMLCanvasElement,
    data: { sessions: SessionEntry[]; dayStart: number; labels: LabelRow[] }
  ) {
    const dpr = devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (w === 0 || h === 0) return;
    canvas.width  = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const { sessions, dayStart, labels } = data;
    const dayEnd = dayStart + 86400;

    // Draw hour grid lines
    ctx.strokeStyle = getComputedStyle(canvas).getPropertyValue("--dot-grid") || "rgba(128,128,128,0.08)";
    ctx.lineWidth = 0.5;
    for (let hr = 0; hr <= 24; hr += 3) {
      const x = (hr / 24) * w;
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, h); ctx.stroke();
    }

    // Draw hour labels at the top
    ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("--dot-hour-text") || "rgba(128,128,128,0.35)";
    ctx.font = `${Math.max(7, h * 0.14)}px system-ui, sans-serif`;
    ctx.textAlign = "center";
    for (let hr = 0; hr < 24; hr += 6) {
      const x = (hr / 24) * w;
      ctx.fillText(`${String(hr).padStart(2, "0")}`, x + 2, Math.max(8, h * 0.18));
    }

    // Compute vertical layout: split height into bands per session
    const LABEL_BAND = Math.max(6, h * 0.12);
    const dotAreaTop = Math.max(10, h * 0.22);
    const dotAreaH   = h - dotAreaTop - 2;
    const nSessions  = sessions.length;
    const bandH      = nSessions > 0 ? dotAreaH / nSessions : dotAreaH;

    // Draw epoch dots for each session
    sessions.forEach((session, sIdx) => {
      const color = SESSION_COLORS[sIdx % SESSION_COLORS.length];
      const ts = getTs(session.csv_path);
      if (!ts || ts.length === 0) return;

      const bandY = dotAreaTop + sIdx * bandH;
      const dotR  = Math.min(2.5, Math.max(1, bandH * 0.3));

      ctx.fillStyle = color;
      ctx.globalAlpha = 0.7;

      for (const row of ts) {
        if (row.t < dayStart || row.t >= dayEnd) continue;
        const x = ((row.t - dayStart) / 86400) * w;
        // Map relaxation (0–1) to Y within the band
        const valNorm = Math.max(0, Math.min(1, row.relaxation));
        const y = bandY + (1 - valNorm) * (bandH - dotR * 2) + dotR;
        ctx.beginPath();
        ctx.arc(x, y, dotR, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1.0;
    });

    // Draw labels as markers
    if (labels.length > 0) {
      ctx.globalAlpha = 0.9;
      for (const label of labels) {
        const t = label.eeg_start;
        if (t < dayStart || t >= dayEnd) continue;
        const x = ((t - dayStart) / 86400) * w;
        // Draw a small triangle marker at the bottom
        ctx.fillStyle = "#f59e0b";
        ctx.beginPath();
        ctx.moveTo(x, h);
        ctx.lineTo(x - 3, h - 5);
        ctx.lineTo(x + 3, h - 5);
        ctx.closePath();
        ctx.fill();
        // Draw label text
        ctx.fillStyle = getComputedStyle(canvas).getPropertyValue("--dot-label-text") || "rgba(245,158,11,0.8)";
        ctx.font = `bold ${Math.max(6, h * 0.1)}px system-ui, sans-serif`;
        ctx.textAlign = "left";
        const maxLabelW = w - x - 4;
        if (maxLabelW > 10) {
          ctx.fillText(label.text, x + 5, h - 1, maxLabelW);
        }
      }
      ctx.globalAlpha = 1.0;
    }
  }

  /** Collect all labels for a day from sessions. */
  function labelsForDay(dayKey: string, sessionsForDay: SessionEntry[]): LabelRow[] {
    const all: LabelRow[] = [];
    for (const s of sessionsForDay) all.push(...s.labels);
    return all;
  }

  /** Check if timeseries data is loaded for any session on a given day. */
  function hasTsForDay(sessionsForDay: SessionEntry[]): boolean {
    return sessionsForDay.some(s => {
      const ts = tsCache[s.csv_path];
      return ts && ts !== "loading" && (ts as EpochRow[]).length > 0;
    });
  }

  // ── Timeline bar ordering ────────────────────────────────────────────────
  /** Sessions paired with their original list index, sorted by duration
   *  descending for the 24h timeline bar.
   *
   *  Widest bars are drawn first (lower in DOM stacking order) so that
   *  narrower bars always appear on top and remain clickable even when their
   *  time-range overlaps visually with a longer adjacent session.          */
  const timelineSessions = $derived.by(() =>
    sessions
      .map((s, i) => ({ s, i }))
      .sort((a, b) => {
        const durA = (a.s.session_end_utc ?? 0) - (a.s.session_start_utc ?? 0);
        const durB = (b.s.session_end_utc ?? 0) - (b.s.session_start_utc ?? 0);
        return durB - durA; // longest first → drawn at bottom of stack
      })
  );

  // ── Derived stats ────────────────────────────────────────────────────────

  /** Sorted (newest-first) list of unique LOCAL YYYY-MM-DD day keys.
   *  Built from the raw UTC dirs by expanding each UTC dir to the local
   *  calendar days it overlaps (can be 1 or 2 depending on timezone offset). */
  const localDays = $derived(buildLocalDays(allUtcDays));

  /** The currently displayed local day key (YYYY-MM-DD). */
  const currentLocalKey = $derived(localDays[currentDayIdx] ?? null);

  /** Alias kept for backward-compat with template references. */
  const currentDayKey = $derived(currentLocalKey ?? "");

  /** Local midnight (Unix seconds) for the 24h timeline bar. */
  const currentDayStart = $derived.by(() => {
    if (!currentDayKey) return 0;
    return localDayBounds(currentDayKey).startSec;
  });

  // ── Calendar-derived state (depends on localDays) ────────────────────────

  /** Session counts per local day. */
  const daySessionCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const d of localDays) counts.set(d, 1);
    return counts;
  });

  /** Label for the calendar navigation header. */
  const calendarLabel = $derived.by(() => {
    const d = calendarAnchor;
    switch (viewMode) {
      case "year":
        return d.getFullYear().toString();
      case "month":
        return d.toLocaleDateString(undefined, { year: "numeric", month: "long" });
      case "week": {
        const start = new Date(d);
        start.setDate(start.getDate() - start.getDay());
        const end = new Date(start);
        end.setDate(end.getDate() + 6);
        return `${start.toLocaleDateString(undefined, { month: "short", day: "numeric" })} – ${end.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })}`;
      }
      default: return "";
    }
  });

  interface CalendarCell {
    dayKey: string;
    date: Date;
    count: number;
    inRange: boolean;
    isToday: boolean;
  }

  const calendarCells = $derived.by((): CalendarCell[] => {
    const today = dateKey(Date.now() / 1000);
    const cells: CalendarCell[] = [];

    if (viewMode === "month") {
      const y = calendarAnchor.getFullYear();
      const m = calendarAnchor.getMonth();
      const first = new Date(y, m, 1);
      const startDow = first.getDay();
      for (let i = startDow - 1; i >= 0; i--) {
        const d = new Date(y, m, -i);
        const dk = dateKey(d.getTime() / 1000);
        cells.push({ dayKey: dk, date: d, count: daySessionCounts.get(dk) ?? 0, inRange: false, isToday: dk === today });
      }
      const daysInMonth = new Date(y, m + 1, 0).getDate();
      for (let day = 1; day <= daysInMonth; day++) {
        const d = new Date(y, m, day);
        const dk = dateKey(d.getTime() / 1000);
        cells.push({ dayKey: dk, date: d, count: daySessionCounts.get(dk) ?? 0, inRange: true, isToday: dk === today });
      }
      const remaining = 7 - (cells.length % 7);
      if (remaining < 7) {
        for (let i = 1; i <= remaining; i++) {
          const d = new Date(y, m + 1, i);
          const dk = dateKey(d.getTime() / 1000);
          cells.push({ dayKey: dk, date: d, count: daySessionCounts.get(dk) ?? 0, inRange: false, isToday: dk === today });
        }
      }
    } else if (viewMode === "week") {
      const start = new Date(calendarAnchor);
      start.setDate(start.getDate() - start.getDay());
      for (let i = 0; i < 7; i++) {
        const d = new Date(start);
        d.setDate(d.getDate() + i);
        const dk = dateKey(d.getTime() / 1000);
        cells.push({ dayKey: dk, date: d, count: daySessionCounts.get(dk) ?? 0, inRange: true, isToday: dk === today });
      }
    } else if (viewMode === "year") {
      const y = calendarAnchor.getFullYear();
      const start = new Date(y, 0, 1);
      const end = new Date(y + 1, 0, 1);
      for (let d = new Date(start); d < end; d.setDate(d.getDate() + 1)) {
        const dk = dateKey(d.getTime() / 1000);
        cells.push({ dayKey: dk, date: new Date(d), count: daySessionCounts.get(dk) ?? 0, inRange: true, isToday: dk === today });
      }
    }
    return cells;
  });

  const maxCount = $derived(Math.max(1, ...calendarCells.map(c => c.count)));

  /** Group year cells by week for the GitHub-style year heatmap. */
  const yearWeeks = $derived.by(() => {
    if (viewMode !== "year") return [];
    const weeks: CalendarCell[][] = [];
    let currentWeek: CalendarCell[] = [];
    const firstDow = calendarCells[0]?.date.getDay() ?? 0;
    for (let i = 0; i < firstDow; i++) currentWeek.push({ dayKey: "", date: new Date(), count: 0, inRange: false, isToday: false });
    for (const cell of calendarCells) {
      if (cell.date.getDay() === 0 && currentWeek.length > 0) {
        weeks.push(currentWeek);
        currentWeek = [];
      }
      currentWeek.push(cell);
    }
    if (currentWeek.length > 0) weeks.push(currentWeek);
    return weeks;
  });

  /** Consecutive-day streak in LOCAL calendar days. */
  const recordingStreak = $derived.by((): number => {
    if (localDays.length === 0) return 0;
    const daySet = new Set(localDays);
    const today = dateKey(Date.now() / 1000);
    let streak = 0;
    const d = new Date();
    if (!daySet.has(today)) d.setDate(d.getDate() - 1);
    for (let i = 0; i < 365; i++) {
      const k = `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,"0")}-${String(d.getDate()).padStart(2,"0")}`;
      if (daySet.has(k)) { streak++; d.setDate(d.getDate() - 1); }
      else break;
    }
    return streak;
  });

  const totalHours = $derived((historyStats?.total_secs ?? 0) / 3600);
  const weekTrend  = $derived.by(() => {
    if (!historyStats) return null;
    const tw = historyStats.this_week_secs / 3600;
    const lw = historyStats.last_week_secs / 3600;
    if (tw === 0 && lw === 0) return null;
    return { thisWeek: tw, lastWeek: lw, pctChange: lw > 0 ? ((tw - lw) / lw) * 100 : 0 };
  });

  // ── Helpers ──────────────────────────────────────────────────────────────
  function dateKey(utc: number): string {
    const d = new Date(utc * 1000);
    return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,"0")}-${String(d.getDate()).padStart(2,"0")}`;
  }
  function dateLabel(key: string): string {
    const [y, m, d] = key.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(undefined, {
      weekday: "long", year: "numeric", month: "long", day: "numeric",
    });
  }
  /** Format a LOCAL YYYY-MM-DD key as a human-readable date string. */
  function fmtDayKey(localKey: string): string {
    if (!localKey) return localKey;
    const [y, m, d] = localKey.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(undefined, {
      year: "numeric", month: "short", day: "numeric",
    });
  }
  function fmtTime(utc: number | null): string {
    if (!utc) return "–";
    return new Date(utc * 1000).toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
  }
  function fmtTimeShort(utc: number | null): string {
    if (!utc) return "–";
    return new Date(utc * 1000).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }
  function fmtDuration(start: number | null, end: number | null): string {
    if (!start || !end) return "–";
    const s = end - start;
    if (s < 60) return `${s}s`;
    if (s < 3600) return `${Math.floor(s/60)}m ${s%60}s`;
    return `${Math.floor(s/3600)}h ${Math.floor((s%3600)/60)}m`;
  }
  function fmtSize(bytes: number): string {
    if (!bytes) return "–";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes/1024).toFixed(1)} KB`;
    return `${(bytes/1048576).toFixed(1)} MB`;
  }
  function fmtSamples(n: number | null): string {
    if (!n) return "–";
    return n >= 1000 ? `${(n/1000).toFixed(1)}k` : String(n);
  }
  function dayPct(utc: number, dayStart: number): number {
    return Math.max(0, Math.min(100, ((utc - dayStart) / 86400) * 100));
  }
  const SESSION_COLORS = ["#3b82f6","#10b981","#f59e0b","#ef4444","#8b5cf6","#ec4899","#06b6d4"];
  function sessionColor(idx: number): string { return SESSION_COLORS[idx % SESSION_COLORS.length]; }

  /** Svelte action: draw a mini sparkline on a canvas element. */
  function drawSparkline(canvas: HTMLCanvasElement, ts: EpochRow[]) {
    renderSparkline(canvas, ts);
    return { update(newTs: EpochRow[]) { renderSparkline(canvas, newTs); } };
  }
  function renderSparkline(canvas: HTMLCanvasElement, ts: EpochRow[]) {
    const dpr = devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    if (ts.length < 3) return;
    const n = ts.length, maxPts = Math.min(n, w * 2), step = Math.max(1, Math.floor(n / maxPts));
    const relaxVals: number[] = [], engageVals: number[] = [];
    for (let i = 0; i < n; i += step) { relaxVals.push(ts[i].relaxation); engageVals.push(ts[i].engagement); }
    function drawLine(vals: number[], color: string) {
      const max = Math.max(...vals, 1);
      ctx.beginPath(); ctx.strokeStyle = color; ctx.lineWidth = 1;
      for (let i = 0; i < vals.length; i++) {
        const x = (i / (vals.length - 1)) * w, y = h - (vals[i] / max) * h;
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      }
      ctx.stroke();
    }
    drawLine(relaxVals, "#10b981"); drawLine(engageVals, "#f59e0b");
  }

  // ── Keyboard navigation ──────────────────────────────────────────────────
  function handleKeydown(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    if (e.key === "ArrowLeft")  { e.preventDefault(); loadDay(currentDayIdx - 1); } // newer
    if (e.key === "ArrowRight") { e.preventDefault(); loadDay(currentDayIdx + 1); } // older
  }

  // ── Mount ────────────────────────────────────────────────────────────────
  onMount(async () => {
    // Wire up titlebar store
    hBar.active = true;
    hCbs.prev          = () => loadDay(currentDayIdx - 1);
    hCbs.next          = () => loadDay(currentDayIdx + 1);
    hCbs.toggleCompare = () => { if (compareMode) exitCompareMode(); else compareMode = true; };
    hCbs.openCompare   = openQuickCompare;
    hCbs.toggleLabels  = () => { showLabels = !showLabels; if (showLabels && allLabels.length === 0) loadLabels(); };
    hCbs.reload        = () => loadDay(currentDayIdx);
    hCbs.setViewMode   = setViewMode;
    hCbs.calendarPrev  = () => calendarNav(-1);
    hCbs.calendarNext  = () => calendarNav(1);

    try {
      allUtcDays = await invoke<string[]>("list_session_days");
    } catch (e) {
      console.error("[history] list_session_days failed:", e);
    }
    daysLoading = false;
    if (localDays.length > 0) await loadDay(0);
    // Load aggregate stats lazily — not needed for initial render
    invoke<HistoryStatsData>("get_history_stats")
      .then(s => { historyStats = s; })
      .catch(() => {});
  });

  onDestroy(() => { hBar.active = false; });

  // Keep titlebar store in sync with local reactive state
  $effect(() => {
    hBar.daysLoading     = daysLoading;
    hBar.dayCount        = localDays.length;
    hBar.currentDayIdx   = currentDayIdx;
    hBar.currentDayLabel = currentLocalKey ? fmtDayKey(currentLocalKey) : "";
    hBar.compareMode     = compareMode;
    hBar.compareCount    = compareSelected.length;
    hBar.showLabels      = showLabels;
    hBar.viewMode        = viewMode;
    hBar.calendarLabel   = calendarLabel;
  });

  useWindowTitle("window.title.history");
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden">
<div class="contents" onkeydown={handleKeydown} tabindex="-1" role="presentation">

  <!-- ── Labels browser panel ─────────────────────────────────────────────── -->
  {#if showLabels}
    <div class="shrink-0 border-b border-border dark:border-white/[0.06]
                bg-muted/30 dark:bg-white/[0.01] px-4 py-3 flex flex-col gap-2">
      <div class="flex items-center gap-2">
        <span class="text-[0.62rem] font-semibold text-foreground/80">{t("history.labels")} ({allLabels.length})</span>
        <input
          bind:value={labelSearchQuery}
          placeholder={t("common.search")}
          class="flex-1 h-6 text-[0.62rem] rounded border border-border dark:border-white/[0.08]
                 bg-background px-2 outline-none focus:ring-1 focus:ring-ring/50" />
        <button onclick={() => showLabels = false}
                class="text-[0.7rem] text-muted-foreground/60 hover:text-foreground">✕</button>
      </div>
      <div class="max-h-32 overflow-y-auto flex flex-col gap-1 scrollbar-thin">
        {#each filteredLabels as label (label.id)}
          <div class="flex items-center gap-2 text-[0.6rem] rounded px-2 py-1
                      hover:bg-muted/60 dark:hover:bg-white/[0.03] group">
            <span class="text-foreground/80 flex-1 truncate">{label.text}</span>
            <span class="text-muted-foreground/40 tabular-nums shrink-0">{fmtTime(label.eeg_start)}</span>
            <button onclick={() => removeLabel(label.id)}
                    class="opacity-0 group-hover:opacity-60 hover:!opacity-100 text-red-500 shrink-0">✕</button>
          </div>
        {/each}
        {#if filteredLabels.length === 0}
          <span class="text-[0.6rem] text-muted-foreground/40 italic px-2">{t("common.noResults")}</span>
        {/if}
      </div>
    </div>
  {/if}

  <!-- ── Main scroll area ──────────────────────────────────────────────────── -->
  <div class="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-3 scrollbar-thin">

    <!-- ── Stats bar (always visible) ──────────────────────────────────── -->
    {#if !daysLoading && localDays.length > 0}
      {#if recordingStreak > 0}
        <div class="rounded-2xl border border-border dark:border-white/[0.06]
                    bg-gradient-to-r from-orange-500/10 via-amber-500/10 to-yellow-500/10
                    dark:from-orange-500/15 dark:via-amber-500/15 dark:to-yellow-500/15
                    px-5 py-4 flex items-center gap-4">
          <div class="flex items-center justify-center w-12 h-12 rounded-xl
                      bg-gradient-to-br from-orange-500 to-amber-400 shadow-lg shadow-orange-500/25
                      text-white text-xl shrink-0">🔥</div>
          <div class="flex flex-col gap-0.5 flex-1">
            <span class="text-[0.75rem] font-bold text-foreground">
              {recordingStreak}-day streak!
            </span>
            <span class="text-[0.62rem] text-muted-foreground/70">
              {recordingStreak >= 7 ? "🏆 Amazing consistency!" :
               recordingStreak >= 3 ? "💪 Keep it going!" :
               "✨ Great start!"}
            </span>
          </div>
          <!-- Stats pills -->
          <div class="flex items-center gap-5">
            <div class="flex flex-col items-center">
              <span class="text-[0.85rem] font-bold tabular-nums">{localDays.length}</span>
              <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">days</span>
            </div>
            {#if historyStats}
              <div class="flex flex-col items-center">
                <span class="text-[0.85rem] font-bold tabular-nums">{totalHours.toFixed(1)}</span>
                <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">hours</span>
              </div>
              <div class="flex flex-col items-center">
                <span class="text-[0.85rem] font-bold tabular-nums">{historyStats.total_sessions}</span>
                <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">sessions</span>
              </div>
              {#if weekTrend && (weekTrend.thisWeek > 0 || weekTrend.lastWeek > 0)}
                <div class="flex flex-col items-center">
                  <div class="flex items-center gap-0.5">
                    <span class="text-[0.85rem] font-bold tabular-nums">{weekTrend.thisWeek.toFixed(1)}</span>
                    {#if weekTrend.lastWeek > 0}
                      <span class="text-[0.55rem] font-semibold
                                   {weekTrend.pctChange > 0 ? 'text-emerald-500' : weekTrend.pctChange < -10 ? 'text-red-400' : 'text-muted-foreground/60'}">
                        {weekTrend.pctChange > 0 ? "↑" : weekTrend.pctChange < 0 ? "↓" : "→"}{Math.abs(weekTrend.pctChange).toFixed(0)}%
                      </span>
                    {/if}
                  </div>
                  <span class="text-[0.45rem] text-muted-foreground/60 uppercase tracking-wider">this week</span>
                </div>
              {/if}
            {/if}
          </div>
        </div>
      {:else}
        <!-- Flat stats row (no streak) -->
        <div class="flex items-center gap-4 mb-1 px-1">
          <div class="flex items-center gap-1">
            <span class="text-[0.62rem] font-bold tabular-nums">{localDays.length}</span>
            <span class="text-[0.52rem] text-muted-foreground">days</span>
          </div>
          {#if historyStats}
            <div class="flex items-center gap-1">
              <span class="text-[0.62rem] font-bold tabular-nums">{totalHours.toFixed(1)}</span>
              <span class="text-[0.52rem] text-muted-foreground">hours total</span>
            </div>
            <div class="flex items-center gap-1">
              <span class="text-[0.62rem] font-bold tabular-nums">{historyStats.total_sessions}</span>
              <span class="text-[0.52rem] text-muted-foreground">sessions</span>
            </div>
          {/if}
        </div>
      {/if}
    {/if}

    <!-- ── Calendar heatmap views (year / month / week) ──────────────── -->
      {#if viewMode !== "day"}
        <div class="flex flex-col gap-2">
          {#if viewMode === "year"}
            <!-- Year heatmap (GitHub-style) -->
            <div class="rounded-xl border border-border dark:border-white/[0.06]
                        bg-white dark:bg-[#14141e] p-4 overflow-x-auto">
              <div class="flex gap-[3px] min-w-max">
                {#each yearWeeks as week, wi}
                  <div class="flex flex-col gap-[3px]">
                    {#each week as cell}
                      {#if cell.dayKey}
                        <button
                          class="w-[11px] h-[11px] rounded-[2px] transition-colors
                                 {cell.count > 0 ? heatColor(cell.count, maxCount) : 'bg-muted/40 dark:bg-white/[0.04]'}
                                 {cell.isToday ? 'ring-1 ring-primary/50' : ''}
                                 {cell.count > 0 ? 'cursor-pointer hover:ring-1 hover:ring-foreground/30' : 'cursor-default'}"
                          title="{cell.dayKey}: {cell.count} {cell.count === 1 ? 'session' : 'sessions'}"
                          onclick={() => cell.count > 0 && navigateToDay(cell.dayKey)}>
                        </button>
                      {:else}
                        <div class="w-[11px] h-[11px]"></div>
                      {/if}
                    {/each}
                  </div>
                {/each}
              </div>
              <!-- Month labels along the top -->
              <div class="flex mt-2 text-[0.45rem] text-muted-foreground/50">
                {#each ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"] as month, i}
                  <span style="width:{100/12}%" class="text-center">{month}</span>
                {/each}
              </div>
              <!-- Legend -->
              <div class="flex items-center gap-1.5 mt-2 justify-end text-[0.45rem] text-muted-foreground/40">
                <span>{t("history.heatmap.less")}</span>
                <div class="w-[11px] h-[11px] rounded-[2px] bg-muted/40 dark:bg-white/[0.04]"></div>
                <div class="w-[11px] h-[11px] rounded-[2px] bg-emerald-200/60 dark:bg-emerald-900/40"></div>
                <div class="w-[11px] h-[11px] rounded-[2px] bg-emerald-300/70 dark:bg-emerald-800/50"></div>
                <div class="w-[11px] h-[11px] rounded-[2px] bg-emerald-400/80 dark:bg-emerald-700/60"></div>
                <div class="w-[11px] h-[11px] rounded-[2px] bg-emerald-500 dark:bg-emerald-600/80"></div>
                <span>{t("history.heatmap.more")}</span>
              </div>
            </div>

          {:else if viewMode === "month"}
            <!-- Month calendar grid -->
            <div class="rounded-xl border border-border dark:border-white/[0.06]
                        bg-white dark:bg-[#14141e] p-3">
              <!-- Weekday headers -->
              <div class="grid grid-cols-7 gap-1 mb-1">
                {#each [0,1,2,3,4,5,6] as dow}
                  <span class="text-center text-[0.48rem] font-semibold text-muted-foreground/50 uppercase tracking-wider">
                    {new Date(2024, 0, dow).toLocaleDateString(undefined, { weekday: "short" }).slice(0, 2)}
                  </span>
                {/each}
              </div>
              <!-- Day cells -->
              <div class="grid grid-cols-7 gap-1">
                {#each calendarCells as cell}
                  <button
                    class="aspect-square rounded-lg flex flex-col items-center justify-center gap-0.5 transition-colors text-[0.62rem]
                           {cell.inRange ? '' : 'opacity-30'}
                           {cell.count > 0 ? heatColor(cell.count, maxCount) + ' cursor-pointer hover:ring-1 hover:ring-foreground/30' : 'bg-muted/20 dark:bg-white/[0.02] cursor-default'}
                           {cell.isToday ? 'ring-1 ring-primary/50' : ''}"
                    title="{cell.dayKey}: {cell.count} {cell.count === 1 ? 'session' : 'sessions'}"
                    onclick={() => cell.count > 0 && navigateToDay(cell.dayKey)}>
                    <span class="font-semibold {cell.inRange ? 'text-foreground' : 'text-muted-foreground/50'}
                                 {cell.count > 0 ? 'text-emerald-900 dark:text-emerald-100' : ''}">
                      {cell.date.getDate()}
                    </span>
                    {#if cell.count > 0}
                      <span class="text-[0.4rem] font-bold text-emerald-700 dark:text-emerald-300">
                        {cell.count}
                      </span>
                    {/if}
                  </button>
                {/each}
              </div>
            </div>

          {:else if viewMode === "week"}
            <!-- Week timeline grid -->
            <div class="rounded-xl border border-border dark:border-white/[0.06]
                        bg-white dark:bg-[#14141e] overflow-hidden">
              <!-- Hour labels header -->
              <div class="relative h-4 border-b border-border/30 dark:border-white/[0.04]
                          bg-muted/20 dark:bg-white/[0.01] select-none">
                {#each [0,3,6,9,12,15,18,21] as hr}
                  <span class="absolute top-0 text-[7px] text-muted-foreground/35 tabular-nums"
                        style="left:{(hr/24)*100}%; transform:translateX({hr === 0 ? '2px' : hr >= 21 ? '-100%' : '-50%'})">
                    {String(hr).padStart(2,"0")}
                  </span>
                {/each}
              </div>
              <!-- Day rows -->
              {#each calendarCells as cell}
                {@const daySessions = weekSessions.get(cell.dayKey) ?? []}
                {@const dayLbls     = labelsForDay(cell.dayKey, daySessions)}
                {@const dayBounds   = localDayBounds(cell.dayKey)}
                {@const hasData     = daySessions.length > 0}
                {@const hasTsData   = hasTsForDay(daySessions)}
                <div class="flex items-stretch border-b border-border/20 dark:border-white/[0.03] last:border-0
                            {cell.isToday ? 'bg-primary/[0.03]' : ''}">
                  <!-- Day label sidebar -->
                  <button
                    class="w-14 shrink-0 flex flex-col items-center justify-center py-1 border-r border-border/20
                           dark:border-white/[0.04] transition-colors
                           {hasData ? 'cursor-pointer hover:bg-accent/40' : 'cursor-default'}"
                    onclick={() => hasData && navigateToDay(cell.dayKey)}>
                    <span class="text-[0.46rem] font-semibold text-muted-foreground/50 uppercase leading-none">
                      {cell.date.toLocaleDateString(undefined, { weekday: "short" })}
                    </span>
                    <span class="text-[0.72rem] font-bold {hasData ? 'text-foreground' : 'text-muted-foreground/30'} leading-tight">
                      {cell.date.getDate()}
                    </span>
                    {#if hasData}
                      <span class="text-[0.4rem] text-muted-foreground/40 tabular-nums">
                        {daySessions.length}
                      </span>
                    {/if}
                  </button>
                  <!-- Epoch dot canvas -->
                  <div class="flex-1 relative min-h-[36px]"
                       style="--dot-grid:{'rgba(128,128,128,0.06)'}; --dot-hour-text:{'rgba(128,128,128,0.0)'}; --dot-label-text:{'rgba(245,158,11,0.75)'};">
                    {#if hasData && hasTsData}
                      {#key cell.dayKey + JSON.stringify(daySessions.map(s => tsCache[s.csv_path] === "loading" ? "l" : "r"))}
                        <canvas class="w-full h-full absolute inset-0"
                                use:drawDayDots={{ sessions: daySessions, dayStart: dayBounds.startSec, labels: dayLbls }}>
                        </canvas>
                      {/key}
                    {:else if hasData}
                      <!-- Session bars fallback (no timeseries yet) -->
                      <div class="absolute inset-0">
                        {#each daySessions as session, sIdx}
                          {#if session.session_start_utc && session.session_end_utc}
                            {@const left  = dayPct(session.session_start_utc, dayBounds.startSec)}
                            {@const width = Math.max(0.3, dayPct(session.session_end_utc, dayBounds.startSec) - left)}
                            <div class="absolute top-1 bottom-1 rounded-[2px] opacity-50"
                                 style="left:{left}%; width:{width}%; background:{sessionColor(sIdx)}">
                            </div>
                          {/if}
                        {/each}
                      </div>
                    {:else}
                      <!-- Empty day -->
                      <div class="absolute inset-0 flex items-center justify-center">
                        <span class="text-[0.45rem] text-muted-foreground/15 select-none">—</span>
                      </div>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}

          <!-- Empty state hint -->
          {#if !daysLoading && localDays.length === 0}
            <div class="flex flex-col items-center gap-2 py-6 text-center">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
                   class="w-8 h-8 text-muted-foreground/15">
                <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
              </svg>
              <span class="text-[0.68rem] font-medium text-muted-foreground/50">{t("history.noSessions")}</span>
              <span class="text-[0.58rem] text-muted-foreground/35 max-w-xs">{t("history.noSessionsHint")}</span>
            </div>
          {/if}

          <!-- Loading overlay -->
          {#if daysLoading}
            <div class="flex items-center justify-center gap-2 py-4 text-muted-foreground/40">
              <Spinner size="w-3 h-3" />
              <span class="text-[0.6rem]">{t("common.loading")}</span>
            </div>
          {/if}

          <!-- Summary stats -->
          {#if !daysLoading && localDays.length > 0}
            <div class="flex items-center gap-4 px-1 text-[0.55rem] text-muted-foreground/50">
              <span>{localDays.length} {t("history.days")}</span>
              {#if historyStats}
                <span>{totalHours.toFixed(1)} {t("history.heatmap.hours")}</span>
                <span>{historyStats.total_sessions} {t("history.sessions")}</span>
              {/if}
              {#if recordingStreak > 0}
                <span>🔥 {recordingStreak}-{t("history.heatmap.dayStreak")}</span>
              {/if}
            </div>
          {/if}
        </div>

      <!-- ── Current day view ──────────────────────────────────────────── -->
      {:else if currentLocalKey}
        <div class="flex flex-col gap-1.5">

          <!-- Date header row -->
          <div class="flex items-center gap-2">
            <span class="text-[0.62rem] font-semibold tracking-widest uppercase text-muted-foreground/70">
              {dateLabel(currentDayKey)}
            </span>
            <span class="text-[0.52rem] text-muted-foreground/40">
              {sessions.length} {sessions.length === 1 ? "session" : "sessions"}
            </span>
            {#if dayLoading}
              <Spinner size="w-2.5 h-2.5" class="text-muted-foreground/40" />
            {/if}
            <Separator class="flex-1 bg-border dark:bg-white/[0.06]" />
          </div>

          <!-- 24-hour timeline bar -->
          {#if sessions.length > 0 || dayLoading}
            <div class="rounded-lg border border-border dark:border-white/[0.06]
                        bg-white dark:bg-[#14141e] overflow-hidden">
              <!-- Hour ticks -->
              <div class="relative h-4 bg-muted/30 dark:bg-white/[0.02] select-none">
                {#each [0,3,6,9,12,15,18,21] as h}
                  <span class="absolute top-0 text-[7px] text-muted-foreground/40 tabular-nums"
                        style="left:{(h/24)*100}%; transform:translateX({h === 0 ? '1px' : h >= 21 ? '-100%' : '-50%'})">
                    {String(h).padStart(2,"0")}:00
                  </span>
                  <span class="absolute bottom-0 w-px h-1 bg-border dark:bg-white/[0.08]"
                        style="left:{(h/24)*100}%"></span>
                {/each}
              </div>
              <!-- Session segments -->
              <div class="relative h-7 bg-muted/10 dark:bg-white/[0.01]">
                {#each [6,12,18] as h}
                  <span class="absolute top-0 bottom-0 w-px bg-border/50 dark:bg-white/[0.04]"
                        style="left:{(h/24)*100}%"></span>
                {/each}
                {#each timelineSessions as { s: session, i: origIdx } (session.csv_path)}
                  {#if session.session_start_utc && session.session_end_utc}
                    {@const leftPct  = dayPct(session.session_start_utc, currentDayStart)}
                    {@const widthPct = Math.max(0.4, dayPct(session.session_end_utc, currentDayStart) - leftPct)}
                    {@const color    = sessionColor(origIdx)}
                    {@const isExpanded = !!expanded[session.csv_path]}
                    {@const dur      = fmtDuration(session.session_start_utc, session.session_end_utc)}
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <button
                      class="absolute top-1 bottom-1 rounded-[3px] cursor-pointer transition-all duration-150
                             overflow-hidden flex items-center justify-center
                             {isExpanded ? 'ring-2 ring-offset-1 ring-offset-background' : ''}
                             {hoveredSession === session.csv_path ? 'brightness-110 scale-y-110' : ''}"
                      style="left:{leftPct}%; width:{widthPct}%; background:{color};
                             opacity:{isExpanded ? 1 : 0.7};
                             z-index:{hoveredSession === session.csv_path || isExpanded ? 20 : 1};
                             {isExpanded ? `ring-color:${color}` : ''}"
                      title="{fmtTimeShort(session.session_start_utc)} → {fmtTimeShort(session.session_end_utc)} · {dur}"
                      onmouseenter={() => hoveredSession = session.csv_path}
                      onmouseleave={() => hoveredSession = null}
                      onclick={() => toggleExpand(session.csv_path)}>
                      {#if widthPct > 4}
                        <span class="text-[7px] font-semibold text-white truncate px-0.5 drop-shadow-sm">{dur}</span>
                      {/if}
                    </button>
                  {:else if session.session_end_utc}
                    <!-- orphaned CSV (no JSON sidecar): tiny dot pinned at end time only -->
                    {@const leftPct  = dayPct(session.session_end_utc, currentDayStart)}
                    {@const color    = sessionColor(origIdx)}
                    {@const isExpanded = !!expanded[session.csv_path]}
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <button
                      class="absolute top-1 bottom-1 rounded-[3px] cursor-pointer transition-all duration-150
                             overflow-hidden flex items-center justify-center
                             {isExpanded ? 'ring-2 ring-offset-1 ring-offset-background' : ''}
                             {hoveredSession === session.csv_path ? 'brightness-110 scale-y-110' : ''}"
                      style="left:{leftPct}%; width:0.4%; background:{color};
                             opacity:{isExpanded ? 1 : 0.55};
                             z-index:{hoveredSession === session.csv_path || isExpanded ? 20 : 1};"
                      title="? → {fmtTimeShort(session.session_end_utc)} · ?"
                      onmouseenter={() => hoveredSession = session.csv_path}
                      onmouseleave={() => hoveredSession = null}
                      onclick={() => toggleExpand(session.csv_path)}>
                    </button>
                  {/if}
                {/each}
              </div>
            </div>
          {/if}

          <!-- Epoch dot timeline -->
          {#if sessions.length > 0}
            {@const dayLbls = sessions.flatMap(s => s.labels)}
            {@const anyTs   = sessions.some(s => { const ts = tsCache[s.csv_path]; return ts && ts !== "loading" && (ts as EpochRow[]).length > 0; })}
            {#if anyTs}
              <div class="rounded-lg border border-border dark:border-white/[0.06]
                          bg-white dark:bg-[#14141e] overflow-hidden"
                   style="--dot-grid:{'rgba(128,128,128,0.06)'}; --dot-hour-text:{'rgba(128,128,128,0.3)'}; --dot-label-text:{'rgba(245,158,11,0.75)'};">
                {#key sessions.map(s => tsCache[s.csv_path] === "loading" ? "l" : "r").join(",")}
                  <canvas class="w-full h-20"
                          use:drawDayDots={{ sessions, dayStart: currentDayStart, labels: dayLbls }}>
                  </canvas>
                {/key}
                <!-- Session color legend -->
                <div class="flex items-center gap-3 px-2.5 py-1 border-t border-border/20 dark:border-white/[0.03]
                            bg-muted/10 dark:bg-white/[0.005]">
                  {#each sessions as session, idx}
                    {#if session.session_start_utc}
                      <div class="flex items-center gap-1">
                        <span class="w-1.5 h-1.5 rounded-full shrink-0" style="background:{sessionColor(idx)}"></span>
                        <span class="text-[0.46rem] text-muted-foreground/50 tabular-nums">
                          {fmtTimeShort(session.session_start_utc)}
                        </span>
                      </div>
                    {/if}
                  {/each}
                  {#if dayLbls.length > 0}
                    <span class="text-[0.46rem] text-amber-500/60 ml-auto">
                      ▲ {dayLbls.length} {dayLbls.length === 1 ? t("history.label") : t("history.labels")}
                    </span>
                  {/if}
                </div>
              </div>
            {/if}
          {/if}

          <!-- Session list (lazy chart rendering via IntersectionObserver) -->
          {#if dayLoading && sessions.length === 0}
            <div class="flex items-center gap-2 py-4 text-muted-foreground/50">
              <Spinner size="w-3.5 h-3.5" />
              <span class="text-[0.65rem]">{t("common.loading")}</span>
            </div>
          {:else if sessions.length === 0}
            <div class="py-6 text-center">
              <span class="text-[0.65rem] text-muted-foreground/40 italic">
                {t("history.noSessions")}
              </span>
            </div>
          {:else}
            {#each sessions as session, idx (session.csv_path)}
              {@const isExpanded = !!expanded[session.csv_path]}
              {@const color = sessionColor(idx)}
              {@const dur = fmtDuration(session.session_start_utc, session.session_end_utc)}
              {@const isHovered = hoveredSession === session.csv_path}

              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                use:inview={() => { renderedRows = new Set([...renderedRows, session.csv_path]); }}
                class="rounded-lg border overflow-hidden transition-all duration-150
                       {isExpanded
                         ? 'border-border dark:border-white/[0.1] bg-white dark:bg-[#14141e]'
                         : isHovered
                           ? 'border-border/60 dark:border-white/[0.06] bg-muted/20 dark:bg-white/[0.015]'
                           : 'border-transparent bg-transparent'}"
                onmouseenter={() => hoveredSession = session.csv_path}
                onmouseleave={() => hoveredSession = null}>

                <!-- Session summary row -->
                <div
                  class="flex items-center gap-2 w-full px-3 py-1.5 text-left cursor-pointer
                         hover:bg-muted/30 dark:hover:bg-white/[0.02] transition-colors rounded-lg"
                  role="button" tabindex="0"
                  onclick={() => compareMode ? toggleCompareSelect(session.csv_path) : toggleExpand(session.csv_path)}
                  onkeydown={(e) => e.key === "Enter" && (compareMode ? toggleCompareSelect(session.csv_path) : toggleExpand(session.csv_path))}>

                  <!-- Compare checkbox -->
                  {#if compareMode}
                    {@const isSelected = compareSelected.includes(session.csv_path)}
                    {@const atLimit = compareSelected.length >= 2 && !isSelected}
                    <div class="w-4 h-4 rounded flex items-center justify-center shrink-0 transition-colors
                                {isSelected ? 'bg-primary' : atLimit ? 'border border-border/50 opacity-40' : 'border border-border dark:border-white/20'}">
                      {#if isSelected}
                        <svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="3"
                             stroke-linecap="round" stroke-linejoin="round" class="w-2.5 h-2.5">
                          <polyline points="20 6 9 17 4 12"/>
                        </svg>
                      {/if}
                    </div>
                  {/if}

                  <span class="w-2 h-2 rounded-full shrink-0" style="background:{color}"></span>

                  <span class="text-[0.68rem] font-semibold text-foreground tabular-nums">
                    {fmtTimeShort(session.session_start_utc)}
                    <span class="text-muted-foreground/40 font-normal">→</span>
                    {fmtTimeShort(session.session_end_utc)}
                  </span>
                  <span class="text-[0.58rem] text-muted-foreground/60 tabular-nums">{dur}</span>

                  <!-- Sparkline — only mounted after the row enters the viewport -->
                  {#if renderedRows.has(session.csv_path) && getTs(session.csv_path)}
                    {@const ts = getTs(session.csv_path)!}
                    <canvas class="h-3 w-16 shrink-0 rounded-sm opacity-60"
                            use:drawSparkline={ts}></canvas>
                  {:else if renderedRows.has(session.csv_path) && tsCache[session.csv_path] === "loading"}
                    <div class="h-3 w-16 rounded-sm bg-muted/30 animate-pulse shrink-0"></div>
                  {/if}

                  {#if session.device_name}
                    <span class="text-[0.56rem] text-muted-foreground/40 truncate min-w-0">
                      {session.device_name}
                    </span>
                  {/if}

                  {#if session.labels.length > 0}
                    <Badge variant="outline"
                      class="text-[0.46rem] font-semibold px-1 py-0 rounded-full shrink-0
                             bg-primary/10 text-primary border-primary/20">
                      {session.labels.length} {session.labels.length === 1 ? t("history.label") : t("history.labels")}
                    </Badge>
                  {/if}

                  <span class="flex-1"></span>
                  <span class="text-[0.5rem] text-muted-foreground/30 tabular-nums shrink-0">
                    {fmtSize(session.file_size_bytes)}
                  </span>

                  {#if !compareMode}
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                         class="w-3 h-3 text-muted-foreground/30 transition-transform duration-200 shrink-0
                                {isExpanded ? 'rotate-180' : ''}">
                      <path d="M6 9l6 6 6-6"/>
                    </svg>
                  {/if}
                </div>

                <!-- ── Expanded details ─────────────────────────────────── -->
                {#if isExpanded}
                  <Separator class="bg-border dark:bg-white/[0.06]" />
                  <div class="px-3.5 py-3 flex flex-col gap-3">
                    <!-- Stats grid -->
                    <div class="grid grid-cols-3 gap-x-4 gap-y-2">
                      <div class="flex flex-col gap-0.5">
                        <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">{t("history.startTime")}</span>
                        <span class="text-[0.65rem] text-foreground tabular-nums">{fmtTime(session.session_start_utc)}</span>
                      </div>
                      <div class="flex flex-col gap-0.5">
                        <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">{t("history.endTime")}</span>
                        <span class="text-[0.65rem] text-foreground tabular-nums">{fmtTime(session.session_end_utc)}</span>
                      </div>
                      <div class="flex flex-col gap-0.5">
                        <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">{t("history.duration")}</span>
                        <span class="text-[0.65rem] text-foreground">{dur}</span>
                      </div>
                      <div class="flex flex-col gap-0.5">
                        <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">{t("history.samples")}</span>
                        <span class="text-[0.65rem] text-foreground tabular-nums">{fmtSamples(session.total_samples)}</span>
                      </div>
                      {#if session.device_name}
                        <div class="flex flex-col gap-0.5">
                          <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">{t("history.device")}</span>
                          <span class="text-[0.65rem] text-foreground">{session.device_name}</span>
                        </div>
                      {/if}
                      {#if session.battery_pct != null}
                        <div class="flex flex-col gap-0.5">
                          <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">{t("history.battery")}</span>
                          <span class="text-[0.65rem] text-foreground">{session.battery_pct.toFixed(0)}%</span>
                        </div>
                      {/if}
                    </div>

                    <!-- Metrics & Charts -->
                    <SessionDetail
                      loading={metricsCache[session.csv_path] === "loading"}
                      metrics={getMetrics(session.csv_path)}
                      timeseries={getTs(session.csv_path)} />

                    <!-- Labels -->
                    {#if session.labels.length > 0}
                      <div class="flex flex-col gap-1.5">
                        <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">
                          {t("history.labels")}
                        </span>
                        {#each session.labels as label}
                          <div class="flex items-start gap-2 rounded-lg border border-border dark:border-white/[0.06]
                                      bg-muted/50 dark:bg-white/[0.02] px-2.5 py-2">
                            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                 stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                                 class="w-3 h-3 shrink-0 text-blue-500 dark:text-blue-400 mt-0.5">
                              <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
                              <line x1="7" y1="7" x2="7.01" y2="7"/>
                            </svg>
                            <div class="flex flex-col gap-0.5 min-w-0">
                              <span class="text-[0.65rem] text-foreground leading-relaxed break-words">{label.text}</span>
                              <span class="text-[0.52rem] text-muted-foreground/50 tabular-nums">
                                {fmtTime(label.eeg_start)} – {fmtTime(label.eeg_end)}
                              </span>
                            </div>
                          </div>
                        {/each}
                      </div>
                    {/if}

                    <!-- Sleep hypnogram -->
                    {#if sleepCache[session.csv_path] === "loading"}
                      <div class="flex items-center gap-2 py-2">
                        <Spinner size="w-3.5 h-3.5" class="text-muted-foreground/50" />
                        <span class="text-[0.6rem] text-muted-foreground/50">{t("sleep.title")}…</span>
                      </div>
                    {:else if sleepCache[session.csv_path] === "short"}
                      <div class="flex items-center gap-1.5 py-1">
                        <span class="text-[0.55rem] text-muted-foreground/40 italic">🌙 {t("sleep.tooShort")}</span>
                      </div>
                    {:else if getSleepData(session.csv_path)}
                      {@const sd = getSleepData(session.csv_path)!}
                      <div class="flex flex-col gap-1.5">
                        <span class="text-[0.48rem] font-semibold tracking-widest uppercase text-muted-foreground/50">
                          {t("sleep.title")}
                        </span>
                        <div class="rounded-lg border border-border dark:border-white/[0.06]
                                    bg-muted/30 dark:bg-white/[0.01] p-2">
                          <Hypnogram epochs={sd.epochs} summary={sd.summary} />
                        </div>
                      </div>
                    {/if}

                    <!-- CSV info -->
                    <div class="flex items-center gap-2 rounded-lg border border-border dark:border-white/[0.06]
                                bg-muted/30 dark:bg-white/[0.01] px-2.5 py-2">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                           stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                           class="w-3 h-3 shrink-0 text-muted-foreground/50">
                        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                        <polyline points="14 2 14 8 20 8"/>
                      </svg>
                      <span class="text-[0.58rem] font-mono text-muted-foreground/60 truncate min-w-0 flex-1">
                        {session.csv_file}
                      </span>
                      <span class="text-[0.52rem] text-muted-foreground/40 tabular-nums shrink-0">
                        {fmtSize(session.file_size_bytes)}
                      </span>
                    </div>

                    <!-- Actions -->
                    <div class="flex items-center gap-2">
                      {#if confirmDelete === session.csv_path}
                        <span class="text-[0.62rem] text-red-600 dark:text-red-400 font-medium">
                          {t("history.confirmDelete")}
                        </span>
                        <Button size="sm" variant="destructive" class="text-[0.62rem] h-6 px-2"
                                onclick={(e: MouseEvent) => { e.stopPropagation(); deleteSession(session.csv_path); }}>
                          {t("history.yesDelete")}
                        </Button>
                        <Button size="sm" variant="ghost" class="text-[0.62rem] h-6 px-2 text-muted-foreground"
                                onclick={(e: MouseEvent) => { e.stopPropagation(); confirmDelete = null; }}>
                          {t("common.cancel")}
                        </Button>
                      {:else}
                        <Button size="sm" variant="ghost" class="text-[0.62rem] h-6 px-2"
                                onclick={(e: MouseEvent) => { e.stopPropagation(); invoke("open_session_window", { csvPath: session.csv_path }); }}>
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                               class="w-3 h-3 mr-1">
                            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>
                            <polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/>
                          </svg>
                          {t("history.popOut")}
                        </Button>
                        <Button size="sm" variant="ghost"
                                class="text-[0.62rem] h-6 px-2 text-red-500 hover:text-red-600 hover:bg-red-500/10"
                                onclick={(e: MouseEvent) => { e.stopPropagation(); confirmDelete = session.csv_path; }}>
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor"
                               stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
                               class="w-3 h-3 mr-1">
                            <polyline points="3 6 5 6 21 6"/>
                            <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/>
                            <path d="M10 11v6"/><path d="M14 11v6"/>
                            <path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
                          </svg>
                          {t("history.delete")}
                        </Button>
                      {/if}
                    </div>
                  </div>
                {/if}
              </div>
            {/each}
          {/if}

        </div><!-- end current-day -->
      {/if}
  </div><!-- end scroll area -->

  <!-- ── Footer ───────────────────────────────────────────────────────────── -->
  <div class="px-4 py-2 border-t border-border dark:border-white/[0.07] shrink-0
              flex items-center justify-between">
    <span class="text-[0.58rem] text-muted-foreground/50">
      {t("history.totalSessions", { n: historyStats?.total_sessions ?? sessions.length })}
    </span>
    <span class="text-[0.58rem] text-muted-foreground/50 tabular-nums">
      {fmtSize(sessions.reduce((a, s) => a + s.file_size_bytes, 0))} {t("history.totalSize")}
    </span>
  </div>
  <DisclaimerFooter />
</div>
</main>
