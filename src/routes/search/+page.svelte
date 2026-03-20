<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
  import { onMount }    from "svelte";
  import { invoke }     from "@tauri-apps/api/core";
  import { Button }     from "$lib/components/ui/button";
  import { Badge }      from "$lib/components/ui/badge";
  import { t }          from "$lib/i18n/index.svelte";
  import { getAppName } from "$lib/stores/app-name.svelte";
  import { useWindowTitle } from "$lib/stores/window-title.svelte";
  import DisclaimerFooter   from "$lib/DisclaimerFooter.svelte";
  import { Spinner }        from "$lib/components/ui/spinner";
  import UmapViewer3D       from "$lib/UmapViewer3D.svelte";
  import InteractiveGraph3D from "$lib/InteractiveGraph3D.svelte";
  import {
    SEARCH_PAGE_SIZE, JOB_POLL_INTERVAL_MS, UMAP_POLL_INTERVAL_MS,
    UMAP_COLOR_A, UMAP_COLOR_B,
  } from "$lib/constants";
  import {
    pad, fmtTime, fmtDate, fmtDateTimeSecs as fmtDateTime,
    fmtDuration as fmtDurationSecs, fmtUtcDay, fmtSecs,
    fmtDateTimeLocalInput, parseDateTimeLocalInput,
    dateToCompactKey, fromUnix, fmtDateTimeLocale,
  } from "$lib/format";
  import type { UmapPoint, UmapResult } from "$lib/types";
  import {
    type NeighborMetrics, type LabelEntry, type NeighborEntry, type QueryEntry,
    type SearchResult, type LabelNeighbor, type SearchAnalysis,
    type GraphNode, type GraphEdge, type JobTicket, type JobPollResult,
    type ImgResult,
    distColor, simPct, simWidth, metricChips,
    computeSearchAnalysis, computeTemporalHeatmap, heatColor,
    turboColor, DAY_NAMES, PRESETS,
    buildTextKnnGraph, computeIxTimeHeatmap, ixHeatColor, dedupeFoundLabels,
  } from "$lib/search-types";
  import { generateUmapPlaceholder } from "$lib/compare-types";

  // ── Formatting helpers ───────────────────────────────────────────────────
  function toInputValue(d: Date) {
    return fmtDateTimeLocalInput(Math.floor(d.getTime() / 1000));
  }
  function fromInputValue(s: string) { return parseDateTimeLocalInput(s); }
  function fmtDuration(s: number, e: number) { return fmtDurationSecs(e - s); }

  // Analysis chip rows (i18n labels)
  function analysisChips(sa: SearchAnalysis): Array<[string, string, string]> {
    return [
      [t("search.analysisNeighbors"), sa.totalNeighbors.toString(),                               ""],
      [t("search.analysisDistMin"),   sa.distMin.toFixed(4),                                      "text-emerald-500"],
      [t("search.analysisMeanSd"),    `${sa.distMean.toFixed(4)} ± ${sa.distStddev.toFixed(4)}`,  ""],
      [t("search.analysisDistMax"),   sa.distMax.toFixed(4),                                      "text-red-400"],
      [t("search.analysisPeakHour"),  `${String(sa.peakHour).padStart(2, "0")}:00`,               ""],
    ];
  }

  // ── Mode ─────────────────────────────────────────────────────────────────
  type SearchMode = "eeg" | "text" | "interactive" | "images";
  let mode = $state<SearchMode>("interactive");
  const SEARCH_MODE_EVENT = "skill:search-mode";
  const SEARCH_SET_MODE_EVENT = "skill:search-set-mode";

  function normalizeSearchMode(value: unknown): SearchMode {
    return value === "eeg" || value === "text" || value === "interactive" || value === "images"
      ? value
      : "interactive";
  }

  function emitSearchMode(value: SearchMode) {
    window.dispatchEvent(new CustomEvent(SEARCH_MODE_EVENT, { detail: { mode: value } }));
  }

  function switchMode(m: SearchMode) { mode = m; error = ""; page = 0; }

  onMount(() => {
    const onTitlebarSetMode = (event: Event) => {
      const next = normalizeSearchMode((event as CustomEvent<{ mode?: unknown }>).detail?.mode);
      switchMode(next);
    };

    window.addEventListener(SEARCH_SET_MODE_EVENT, onTitlebarSetMode as EventListener);

    const initialMode = normalizeSearchMode(new URLSearchParams(window.location.search).get("mode"));
    switchMode(initialMode);
    emitSearchMode(initialMode);

    // Load screenshot server port for image URLs
    invoke<[string, number]>("get_screenshots_dir")
      .then(([, port]) => { imgPort = port; })
      .catch(e => console.warn("[search] get_screenshots_dir failed:", e));

    return () => {
      window.removeEventListener(SEARCH_SET_MODE_EVENT, onTitlebarSetMode as EventListener);
    };
  });

  $effect(() => {
    const currentMode = mode;
    const params = new URLSearchParams(window.location.search);
    if (params.get("mode") !== currentMode) {
      params.set("mode", currentMode);
      const next = `${window.location.pathname}?${params.toString()}${window.location.hash}`;
      history.replaceState(history.state, "", next);
    }
    emitSearchMode(currentMode);
  });

  // ── Shared ───────────────────────────────────────────────────────────────
  let kVal  = $state(10);
  let error = $state("");
  let page  = $state(0);

  // ── EEG mode ─────────────────────────────────────────────────────────────
  const now  = new Date();
  const ago5 = new Date(now.getTime() - 5 * 60_000);
  let startInput      = $state(toInputValue(ago5));
  let endInput        = $state(toInputValue(now));
  let efVal           = $state(50);
  let searching       = $state(false);
  let searchCancelled = $state(false);
  let searchStatus    = $state("");
  let searchElapsed   = $state(0);
  let streamTotal     = $state(0);
  let streamDone      = $state(0);
  let streamDays      = $state<string[]>([]);
  let labelFilter     = $state("");
  let labelsOnly      = $state(false);
  let result          = $state<SearchResult | null>(null);
  let showAnalysis    = $state(true);

  function applyPreset(mins: number) {
    const e = new Date(); const s = new Date(e.getTime() - mins * 60_000);
    startInput = toInputValue(s); endInput = toInputValue(e);
  }

  const filtered = $derived.by(() => {
    if (!result) return [];
    let entries = result.results;
    const lf = labelFilter.trim().toLowerCase();
    if (lf || labelsOnly) {
      entries = entries.map(q => {
        const matched = q.neighbors.filter(nb => {
          if (nb.labels.length === 0 && labelsOnly) return false;
          if (!lf) return true;
          return nb.labels.some(l => l.text.toLowerCase().includes(lf));
        });
        return matched.length > 0 ? { ...q, neighbors: matched } : null;
      }).filter((q): q is QueryEntry => q !== null);
    }
    return entries;
  });
  const totalPages = $derived(Math.ceil(filtered.length / SEARCH_PAGE_SIZE) || 0);
  const pageSlice  = $derived(filtered.slice(page * SEARCH_PAGE_SIZE, (page + 1) * SEARCH_PAGE_SIZE));

  const searchAnalysis = $derived.by(() => result ? computeSearchAnalysis(result) : null);
  const temporalHeatmap = $derived.by(() => result ? computeTemporalHeatmap(result) : null);
  const heatmapMax = $derived(temporalHeatmap ? Math.max(...temporalHeatmap.flat(), 1) : 1);

  // UMAP
  let umapResult      = $state<UmapResult | null>(null);
  let umapPlaceholder = $state<UmapResult | null>(null);
  let umapLoading     = $state(false);
  let umapEta         = $state("");
  let umapElapsed     = $state(0);
  let umapTimer: ReturnType<typeof setInterval> | null = null;
  let showUmap        = $state(false);
  let umapColorByDate = $state(false);

  function fireUmap() {
    if (!result || result.results.length === 0) return;
    const allNb = result.results.flatMap(q => q.neighbors);
    if (!allNb.length) return;
    umapResult = null; umapLoading = true; umapEta = "";
    const nbMin = Math.min(...allNb.map(n => n.timestamp_unix));
    const nbMax = Math.max(...allNb.map(n => n.timestamp_unix));
    umapPlaceholder = generateUmapPlaceholder(Math.min(result.query_count, 200), Math.min(allNb.length, 200));
    const t0 = performance.now(); umapElapsed = 0;
    umapTimer = setInterval(() => { umapElapsed = Math.floor((performance.now() - t0) / 1000); }, 250);
    invoke<JobTicket>("enqueue_umap_compare", {
      aStartUtc: result.start_utc, aEndUtc: result.end_utc,
      bStartUtc: nbMin, bEndUtc: nbMax,
    }).then(ticket => {
      umapEta = ticket.queue_position > 0 ? t("search.queued", { n: ticket.queue_position+1 }) : t("search.computing3d");
      pollUmap(ticket.job_id);
    }).catch(() => finishUmap());
  }
  /**
   * Build a utc-seconds → label-text map from every labeled neighbor in the
   * current EEG search result.  This is the authoritative source: the search
   * command hydrates labels straight from the database, so no timestamp
   * boundary condition can cause a miss here.
   */
  function buildNeighborLabelMap(): Map<number, string> {
    const map = new Map<number, string>();
    if (!result) return map;
    for (const q of result.results) {
      for (const nb of q.neighbors) {
        if (nb.labels.length > 0 && !map.has(nb.timestamp_unix)) {
          map.set(nb.timestamp_unix, nb.labels[0].text);
        }
      }
    }
    return map;
  }

  /**
   * Merge the ground-truth label map into a raw UMAP result.
   * Points the backend already labeled are left untouched;
   * unlabeled points that match a neighbor timestamp get the label injected.
   */
  function enrichUmapLabels(raw: UmapResult, labelMap: Map<number, string>): UmapResult {
    if (labelMap.size === 0) return raw;
    const points = raw.points.map(pt =>
      (!pt.label && labelMap.has(pt.utc)) ? { ...pt, label: labelMap.get(pt.utc)! } : pt
    );
    return { ...raw, points };
  }

  async function pollUmap(jobId: number) {
    while (true) {
      await new Promise(r => setTimeout(r, UMAP_POLL_INTERVAL_MS));
      try {
        const r = await invoke<JobPollResult>("poll_job", { jobId });
        if (r.status === "complete") {
          const res = r.result as UmapResult | undefined;
          let raw: UmapResult | null = res?.points?.length ? res : null;
          if (raw) raw = enrichUmapLabels(raw, buildNeighborLabelMap());
          umapResult = raw;
          finishUmap(); return;
        }
        if (r.status === "error" || r.status === "not_found") { finishUmap(); return; }
        umapEta = r.queue_position! > 0 ? t("search.queued", { n: r.queue_position!+1 }) : t("search.computing3d");
      } catch { finishUmap(); return; }
    }
  }
  function finishUmap() {
    umapLoading = false; umapEta = "";
    if (umapTimer) { clearInterval(umapTimer); umapTimer = null; }
  }

  async function searchEeg() {
    const startUtc = fromInputValue(startInput);
    const endUtc   = fromInputValue(endInput);
    if (isNaN(startUtc) || isNaN(endUtc) || endUtc <= startUtc) { error = t("search.endAfterStart"); return; }
    searching = true; searchCancelled = false; error = ""; result = null; page = 0;
    streamTotal = 0; streamDone = 0; streamDays = [];
    searchStatus = t("search.searching"); searchElapsed = 0;
    const t0 = performance.now();
    const timer = setInterval(() => { searchElapsed = Math.floor((performance.now()-t0)/1000); }, 500);
    let acc: SearchResult = { start_utc: startUtc, end_utc: endUtc, k: kVal, ef: efVal, query_count: 0, searched_days: [], results: [] };
    try {
      const { Channel } = await import("@tauri-apps/api/core");
      const ch = new Channel<{ kind: string; query_count?: number; searched_days?: string[]; entry?: QueryEntry; done_count?: number; total?: number; error?: string; }>();
      ch.onmessage = (msg) => {
        if (searchCancelled) return;
        if (msg.kind === "started")       { streamTotal = msg.query_count ?? 0; streamDays = msg.searched_days ?? []; acc.query_count = streamTotal; acc.searched_days = streamDays; searchStatus = t("search.searchingIndices"); }
        else if (msg.kind === "result" && msg.entry) { streamDone = msg.done_count ?? streamDone+1; acc.results = [...acc.results, msg.entry]; result = { ...acc }; }
        else if (msg.kind === "done")     { streamDone = msg.total ?? streamDone; }
        else if (msg.kind === "error")    { error = msg.error ?? "Unknown error"; }
      };
      await invoke("stream_search_embeddings", { startUtc, endUtc, k: kVal || undefined, ef: efVal || undefined, onProgress: ch });
      result = { ...acc };
      // Mark "run a similarity search" onboarding step as done.
      try {
        const ob = JSON.parse(localStorage.getItem("onboardDone") ?? "{}");
        if (!ob.searchRun) { ob.searchRun = true; localStorage.setItem("onboardDone", JSON.stringify(ob)); }
      } catch (e) { console.warn("[search] onboarding localStorage update failed:", e); }
    } catch (e) { error = String(e); }
    finally { clearInterval(timer); searching = false; searchStatus = ""; }
    if (result && result.results.length > 0) { showUmap = true; fireUmap(); }
  }

  // ── Interactive mode ──────────────────────────────────────────────────────
  let ixQuery        = $state("");
  let ixKText        = $state(5);
  let ixKEeg         = $state(5);
  let ixKLabels      = $state(3);
  let ixReachMinutes = $state(10);  // temporal window around each EEG point (1–60 min)
  let ixSearching = $state(false);
  let ixSearched  = $state(false);
  let ixStatus    = $state("");
  let ixNodes     = $state<GraphNode[]>([]);
  let ixEdges     = $state<GraphEdge[]>([]);
  let ixDot       = $state("");
  let ixSvg       = $state("");      // SVG with PCA scatter layout
  let ixSvgCol    = $state("");      // SVG with classic column layout
  let showIxGraph    = $state(true);
  let showIxCard     = $state(true);  // single collapsible card: query + pipeline + button
  let ixDedupeLabels = $state(true);  // deduplicate found_labels by text
  let ixUsePca       = $state(true);  // cluster found_labels by embedding similarity

  // ── Derived display graph (applies found-label deduplication) ────────────
  const ixDisplayGraph = $derived.by(() => {
    if (!ixDedupeLabels) return { nodes: ixNodes, edges: ixEdges };
    return dedupeFoundLabels(ixNodes, ixEdges);
  });

  async function searchInteractive() {
    if (!ixQuery.trim()) return;
    // Compress the controls so the results panel gets maximum space
    showIxCard = false;
    ixSearching = true; ixSearched = false; error = "";
    ixNodes = []; ixEdges = []; ixDot = ""; ixSvg = ""; ixSvgCol = ""; dotSavedPath = ""; svgSavedPath = ""; svgError = "";
    ixStatus = t("search.interactiveStep1");
    try {
      const res = await invoke<{ nodes: GraphNode[]; edges: GraphEdge[]; dot: string; svg: string; svg_col: string }>(
        "interactive_search", {
          query:         ixQuery.trim(),
          kText:         ixKText,
          kEeg:          ixKEeg,
          kLabels:       ixKLabels,
          reachMinutes:  ixReachMinutes,
          usePca:        ixUsePca,
          svgLabels: {
            layerQuery:        t("svg.layerQuery"),
            layerTextMatches:  t("svg.layerTextMatches"),
            layerEegNeighbors: t("svg.layerEegNeighbors"),
            layerFoundLabels:  t("svg.layerFoundLabels"),
            legendQuery:       t("svg.legendQuery"),
            legendText:        t("svg.legendText"),
            legendEeg:         t("svg.legendEeg"),
            legendFound:       t("svg.legendFound"),
            generatedBy:       t("svg.generatedBy", { app: getAppName() }),
          },
        }
      );
      ixNodes    = res.nodes;
      ixEdges    = res.edges;
      ixDot      = res.dot;
      ixSvg      = res.svg;
      ixSvgCol   = res.svg_col;
      ixSearched = true;
    } catch (e) { error = String(e); }
    finally { ixSearching = false; ixStatus = ""; }
  }

  function onIxKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); searchInteractive(); }
  }

  let dotSaving    = $state(false);
  let dotSavedPath = $state("");
  let svgSaving    = $state(false);
  let svgSavedPath = $state("");
  let svgError     = $state("");

  async function downloadDot() {
    if (!ixDot || dotSaving) return;
    dotSaving = true; dotSavedPath = "";
    try {
      dotSavedPath = await invoke<string>("save_dot_file", { dot: ixDot, query: ixQuery.trim() });
    } catch (e) { error = String(e); }
    finally { dotSaving = false; }
  }

  async function downloadSvg() {
    const svgData = ixUsePca ? ixSvg : ixSvgCol;
    if (!svgData || svgSaving) return;
    svgSaving = true; svgSavedPath = ""; svgError = "";
    try {
      svgSavedPath = await invoke<string>("save_svg_file", { svg: svgData, query: ixQuery.trim() });
    } catch (e) { svgError = String(e); }
    finally { svgSaving = false; }
  }

  // ── Interactive time heatmap (days × hours) ─────────────────────────────
  const ixTimeHeatmap = $derived.by(() => computeIxTimeHeatmap(ixNodes));

  async function openSession(nb: NeighborEntry) {
    try {
      const ref = await invoke<{ csv_path: string } | null>("find_session_for_timestamp", { timestampUnix: nb.timestamp_unix, date: nb.date });
      await invoke(ref ? "open_session_window" : "open_history_window", ref ? { csvPath: ref.csv_path } : {});
    } catch { /* swallow */ }
  }

  // ── Text mode ─────────────────────────────────────────────────────────────
  let textQuery     = $state("");
  let textSearching = $state(false);
  let textResults   = $state<LabelNeighbor[]>([]);
  let textSearched  = $state(false);
  let textFilter    = $state("");
  let textSort      = $state<"sim" | "date">("sim");

  // ── Text mode 3D kNN graph ─────────────────────────────────────────────
  let showTextGraph = $state(true);
  let textGraphData = $state<UmapResult | null>(null);

  /**
   * Build a client-side UmapResult for the text kNN results:
   * - Query anchor at the origin (session 0)
   * - Each result on a sphere shell whose radius = normalised distance (session 1)
   * - Angular positions spread via the golden-angle Fibonacci spiral
   */

  // Rebuild whenever the filtered text results change
  $effect(() => {
    const r = textFiltered;
    if (!textSearched || r.length === 0) { textGraphData = null; return; }
    textGraphData = buildTextKnnGraph(r, textQuery.trim());
  });

  async function searchText() {
    if (!textQuery.trim()) return;
    textSearching = true; error = ""; textSearched = false; page = 0; textFilter = "";
    try {
      textResults = await invoke<LabelNeighbor[]>("search_labels_by_text", { query: textQuery.trim(), k: kVal });
      textSearched = true;
    } catch (e) { error = String(e); }
    finally { textSearching = false; }
  }

  function onTextKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); searchText(); }
  }

  const textFiltered = $derived.by(() => {
    let r = textResults;
    const tf = textFilter.trim().toLowerCase();
    if (tf) r = r.filter(x => x.text.toLowerCase().includes(tf) || x.context.toLowerCase().includes(tf));
    if (textSort === "date") return [...r].sort((a, b) => b.eeg_start - a.eeg_start);
    return r;  // similarity order is already from backend
  });
  const textMaxDist    = $derived(textFiltered.length > 0 ? Math.max(0.0001, ...textFiltered.map(r => r.distance)) : 1);
  const textPageSlice  = $derived(textFiltered.slice(page * SEARCH_PAGE_SIZE, (page + 1) * SEARCH_PAGE_SIZE));
  const textTotalPages = $derived(Math.ceil(textFiltered.length / SEARCH_PAGE_SIZE) || 0);

  async function openSessionForLabel(nb: LabelNeighbor) {
    try {
      const dateStr = dateToCompactKey(fromUnix(nb.eeg_start));
      const ref = await invoke<{ csv_path: string } | null>("find_session_for_timestamp", { timestampUnix: nb.eeg_start, date: dateStr });
      await invoke(ref ? "open_session_window" : "open_history_window", ref ? { csvPath: ref.csv_path } : {});
    } catch { /* swallow */ }
  }

  // ── Images mode (screenshot OCR search) ──────────────────────────────────
  let imgQuery       = $state("");
  let imgResults     = $state<ImgResult[]>([]);
  let imgSearching   = $state(false);
  let imgSearched    = $state(false);
  let imgSearchMode  = $state<"substring" | "semantic">("substring");
  let imgPort        = $state(8375);

  function imgSrc(filename: string): string {
    return filename ? `http://127.0.0.1:${imgPort}/screenshots/${filename}` : "";
  }

  async function searchImages() {
    if (!imgQuery.trim()) return;
    imgSearching = true;
    imgSearched = false;
    try {
      imgResults = await invoke<ImgResult[]>("search_screenshots_by_text", {
        query: imgQuery.trim(), k: 20, mode: imgSearchMode,
      });
      imgSearched = true;
    } catch {
      imgResults = [];
      imgSearched = true;
    } finally {
      imgSearching = false;
    }
  }

  useWindowTitle("window.title.search");
</script>

<main class="h-full min-h-0 bg-background text-foreground flex flex-col overflow-hidden">

  <!-- ── Status strip ─────────────────────────────────────────────────── -->
  <div class="relative flex items-center justify-end px-4 py-1.5 shrink-0
              border-b border-border dark:border-white/[0.07]">
    <div class="min-w-0 flex items-center justify-end">
      {#if mode === "eeg" && result}
        <span class="text-[0.6rem] text-muted-foreground/55 select-none tabular-nums truncate">
          {t("search.resultSummary", { queries: result.query_count, k: result.k, days: result.searched_days.length })}
        </span>
      {:else if mode === "text" && textSearched}
        <span class="text-[0.6rem] text-muted-foreground/55 select-none tabular-nums">
          {textFiltered.length} / {textResults.length} {t("search.textResultsCount")}
        </span>
      {:else if mode === "interactive" && ixSearched}
        <span class="text-[0.6rem] text-muted-foreground/55 select-none tabular-nums">
          {t("search.interactiveNodeCount", { n: ixNodes.length, e: ixEdges.length })}
        </span>
      {:else if mode === "images" && imgSearched}
        <span class="text-[0.6rem] text-muted-foreground/55 select-none tabular-nums">
          {imgResults.length} {t("search.imageResultsCount")}
        </span>
      {/if}
    </div>

    <!-- Streaming progress line -->
    {#if mode === "eeg" && searching && streamTotal > 0}
      <div class="absolute bottom-0 left-0 right-0 h-[2px] bg-muted/20 overflow-hidden">
        <div class="h-full bg-blue-500 transition-[width] duration-300"
             style="width:{Math.min(100,(streamDone/streamTotal)*100).toFixed(1)}%"></div>
      </div>
    {/if}
  </div>

  <!-- ── Controls ─────────────────────────────────────────────────────── -->
  <div class="px-4 py-3 flex flex-col gap-2.5 shrink-0
              border-b border-border dark:border-white/[0.06]
              bg-muted/20 dark:bg-white/[0.01]">

    {#if mode === "eeg"}
      <!-- Row 1: presets + date range -->
      <div class="flex items-center gap-1.5 flex-wrap">
        <span class="text-[0.58rem] font-semibold tracking-widest uppercase text-muted-foreground/60 shrink-0">
          {t("search.last")}
        </span>
        {#each PRESETS as [label, mins]}
          <button onclick={() => applyPreset(mins)}
                  class="px-2 py-0.5 rounded text-[0.62rem] font-semibold border
                         border-border dark:border-white/[0.1]
                         bg-background hover:bg-accent transition-colors
                         text-muted-foreground hover:text-foreground select-none">
            {label}
          </button>
        {/each}
        <div class="flex items-center gap-1 ml-auto">
          <input type="datetime-local" step="1" bind:value={startInput}
                 class="rounded border border-border dark:border-white/[0.1]
                        bg-background px-2 py-1 text-[0.7rem]
                        focus:outline-none focus:ring-1 focus:ring-ring" />
          <span class="text-muted-foreground/40 text-[0.65rem] select-none">→</span>
          <input type="datetime-local" step="1" bind:value={endInput}
                 class="rounded border border-border dark:border-white/[0.1]
                        bg-background px-2 py-1 text-[0.7rem]
                        focus:outline-none focus:ring-1 focus:ring-ring" />
        </div>
      </div>

      <!-- Row 2: k · ef · search button -->
      <div class="flex items-center gap-2.5">
        <div class="flex items-center gap-1.5">
          <span class="text-[0.6rem] text-muted-foreground/60 font-mono select-none">k</span>
          <input type="number" min="1" max="100" bind:value={kVal}
                 class="w-14 rounded border border-border dark:border-white/[0.1]
                        bg-background px-1.5 py-1 text-[0.72rem] text-center
                        focus:outline-none focus:ring-1 focus:ring-ring" />
        </div>
        <div class="flex items-center gap-1.5">
          <span class="text-[0.6rem] text-muted-foreground/60 font-mono select-none">ef</span>
          <input type="number" min="10" max="500" bind:value={efVal}
                 class="w-16 rounded border border-border dark:border-white/[0.1]
                        bg-background px-1.5 py-1 text-[0.72rem] text-center
                        focus:outline-none focus:ring-1 focus:ring-ring" />
        </div>
        {#if error}
          <span class="text-[0.62rem] text-destructive flex-1 truncate" title={error}>{error}</span>
        {:else}
          <span class="flex-1"></span>
        {/if}
        {#if searching}
          <Button onclick={() => { searchCancelled = true; searching = false; searchStatus = ""; }}
                  variant="outline" size="sm"
                  class="gap-1 px-3 h-7 text-[0.68rem] text-destructive border-destructive/30 hover:bg-destructive/10">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" class="w-3 h-3">
              <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
            </svg>
            {t("common.cancel")}
          </Button>
        {/if}
        <Button onclick={searchEeg} disabled={searching} size="sm" class="gap-1.5 h-7 px-4 text-[0.72rem]">
          {#if searching}
            <Spinner size="w-3 h-3" /> {t("search.searching")}
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
              <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
            </svg>
            {t("common.search")}
          {/if}
        </Button>
      </div>

    {:else if mode === "interactive"}
      <!-- ── Single unified card: query + pipeline + button ──────────────── -->
      <div class="rounded-lg border border-border dark:border-white/[0.07] overflow-hidden">

        <!-- Card header (always visible) -->
        <button onclick={() => showIxCard = !showIxCard}
                class="w-full flex items-center gap-2 px-3 py-1.5
                       bg-muted/20 hover:bg-muted/30 transition-colors text-left select-none">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
               stroke-linecap="round" stroke-linejoin="round"
               class="w-3 h-3 text-muted-foreground/50 transition-transform duration-150 shrink-0
                      {showIxCard ? 'rotate-0' : '-rotate-90'}">
            <polyline points="6 9 12 15 18 9"/>
          </svg>
          <span class="text-[0.58rem] font-semibold text-muted-foreground/70 uppercase tracking-wider shrink-0">
            {t("search.interactiveQueryLabel")}
          </span>
          <!-- Collapsed summary: query preview + key params -->
          {#if !showIxCard}
            {#if ixQuery.trim()}
              <span class="text-[0.55rem] text-muted-foreground/55 italic truncate min-w-0">
                "{ixQuery.trim()}"
              </span>
            {/if}
            <span class="ml-auto text-[0.48rem] text-muted-foreground/35 font-mono shrink-0">
              k{ixKText}·{ixKEeg}·{ixKLabels} ±{ixReachMinutes}m
            </span>
          {/if}
        </button>

        {#if showIxCard}
          <!-- Query textarea -->
          <div class="px-3 pt-2 pb-0 border-t border-border dark:border-white/[0.06]">
            <textarea bind:value={ixQuery}
                      onkeydown={onIxKeydown}
                      placeholder={t("search.interactiveQueryPlaceholder")}
                      rows="2"
                      class="w-full rounded-md border border-border dark:border-white/[0.1]
                             bg-background px-3 py-2 text-[0.8rem] leading-relaxed
                             placeholder:text-muted-foreground/30 resize-none
                         focus:outline-none focus:ring-1 focus:ring-ring/50
                             transition-shadow">
            </textarea>
          </div>

          <!-- Pipeline step rows -->
          {#each [
            { n: 2, color: "#3b82f6", title: "Text similarity",
              hint: "Similar labels from history.",
              min: 1, max: 20, val: ixKText,    set: (v: number) => { ixKText = v; } },
            { n: 3, color: "#f59e0b", title: "EEG bridge depth",
              hint: "EEG neighbors per label.",
              min: 1, max: 20, val: ixKEeg,     set: (v: number) => { ixKEeg = v; } },
            { n: 4, color: "#10b981", title: "Label reach (count)",
              hint: "Labels per EEG neighbor.",
              min: 1, max: 10, val: ixKLabels,  set: (v: number) => { ixKLabels = v; } },
            { n: 5, color: "#06b6d4", title: "Time window (min)",
              hint: "±minutes around each EEG point.",
              min: 1, max: 60, val: ixReachMinutes, set: (v: number) => { ixReachMinutes = v; } },
          ] as step}
            <div class="flex items-center gap-2 px-3 py-1.5
                        border-t border-border dark:border-white/[0.05]
                        hover:bg-muted/10 transition-colors">
              <span class="w-3.5 h-3.5 rounded-full flex items-center justify-center
                           text-[0.42rem] font-bold text-white shrink-0 select-none"
                    style="background:{step.color}">{step.n}</span>
              <span class="text-[0.6rem] font-medium text-foreground/70 shrink-0">{step.title}</span>
              <span class="text-[0.52rem] text-muted-foreground/40 flex-1 min-w-0 truncate">{step.hint}</span>
              <div class="flex items-center gap-0.5 shrink-0">
                <button onclick={() => step.set(Math.max(step.min, step.val - 1))}
                        class="w-5 h-5 rounded flex items-center justify-center text-[0.7rem] font-bold
                               text-muted-foreground/50 hover:text-foreground hover:bg-muted/40
                               transition-colors select-none">−</button>
                <input type="number" min={step.min} max={step.max} value={step.val}
                       oninput={(e) => step.set(Number((e.target as HTMLInputElement).value))}
                       class="w-9 rounded border border-border dark:border-white/[0.1]
                              bg-background px-0.5 py-0.5 text-[0.68rem] text-center font-mono
                              focus:outline-none focus:ring-1 focus:ring-ring" />
                <button onclick={() => step.set(Math.min(step.max, step.val + 1))}
                        class="w-5 h-5 rounded flex items-center justify-center text-[0.7rem] font-bold
                               text-muted-foreground/50 hover:text-foreground hover:bg-muted/40
                               transition-colors select-none">+</button>
              </div>
            </div>
          {/each}

          <!-- Search button row -->
          <div class="flex items-center gap-2 px-3 py-2
                      border-t border-border dark:border-white/[0.06]">
            {#if error}
              <span class="text-[0.6rem] text-destructive flex-1 truncate" title={error}>{error}</span>
            {:else}
              <span class="flex-1 text-[0.48rem] text-muted-foreground/25 select-none">
                {t("search.interactiveCmdEnter")}
              </span>
            {/if}
            <Button onclick={searchInteractive} disabled={ixSearching || !ixQuery.trim()} size="sm"
                    class="gap-1.5 h-7 px-4 text-[0.72rem] bg-emerald-600 hover:bg-emerald-700 text-white shrink-0">
              {#if ixSearching}
                <Spinner size="w-3 h-3" />
                {ixStatus || t("search.interactiveSearching")}
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
                  <circle cx="12" cy="5" r="2"/><circle cx="5" cy="19" r="2"/><circle cx="19" cy="19" r="2"/>
                  <line x1="12" y1="7" x2="5"  y2="17"/>
                  <line x1="12" y1="7" x2="19" y2="17"/>
                  <line x1="5"  y1="19" x2="19" y2="19"/>
                </svg>
                {t("search.modeInteractive")}
              {/if}
            </Button>
          </div>
        {/if}
      </div>

    {:else if mode === "text"}
      <!-- Text mode: query box -->
      <div class="flex flex-col gap-1">
        <label for="search-text-query" class="text-[0.58rem] text-muted-foreground/60 uppercase tracking-widest font-semibold select-none">
          {t("search.textQueryLabel")}
        </label>
        <textarea id="search-text-query" bind:value={textQuery}
                  onkeydown={onTextKeydown}
                  placeholder={t("search.textQueryPlaceholder")}
                  rows="2"
                  class="w-full rounded-md border border-border dark:border-white/[0.1]
                         bg-background px-3 py-2 text-[0.8rem] leading-relaxed
                         placeholder:text-muted-foreground/30 resize-none
                         focus:outline-none focus:ring-1 focus:ring-violet-500/50
                         transition-shadow">
        </textarea>
      </div>

      <!-- k + button -->
      <div class="flex items-center gap-2.5">
        <div class="flex items-center gap-1.5">
          <span class="text-[0.6rem] text-muted-foreground/60 font-mono select-none">k</span>
          <input type="number" min="1" max="100" bind:value={kVal}
                 class="w-14 rounded border border-border dark:border-white/[0.1]
                        bg-background px-1.5 py-1 text-[0.72rem] text-center
                        focus:outline-none focus:ring-1 focus:ring-ring" />
        </div>
        {#if error}
          <span class="text-[0.62rem] text-destructive flex-1 truncate" title={error}>{error}</span>
        {:else}
          <span class="flex-1 text-[0.55rem] text-muted-foreground/30 select-none text-right">{t("search.textCmdEnter")}</span>
        {/if}
        <Button onclick={searchText} disabled={textSearching || !textQuery.trim()} size="sm"
                class="gap-1.5 h-7 px-4 text-[0.72rem] bg-violet-600 hover:bg-violet-700 text-white">
          {#if textSearching}
            <Spinner size="w-3 h-3" />
            {t("search.searching")}
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
              <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
            </svg>
            {t("common.search")}
          {/if}
        </Button>
      </div>

    {:else if mode === "images"}
      <!-- Images mode: OCR text search -->
      <div class="flex flex-col gap-1.5">
        <div class="flex items-center justify-between">
          <label for="search-img-query" class="text-[0.58rem] text-muted-foreground/60 uppercase tracking-widest font-semibold select-none">
            {t("search.imageQueryLabel")}
          </label>
          <div class="flex rounded-lg border border-border dark:border-white/[0.08] overflow-hidden">
            <button onclick={() => { imgSearchMode = "substring"; }}
                    class="px-2 py-0.5 text-[0.52rem] font-medium transition-colors
                           {imgSearchMode === 'substring' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}">
              {t("search.imageMatchText")}
            </button>
            <button onclick={() => { imgSearchMode = "semantic"; }}
                    class="px-2 py-0.5 text-[0.52rem] font-medium transition-colors
                           {imgSearchMode === 'semantic' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground'}">
              {t("search.imageMatchSemantic")}
            </button>
          </div>
        </div>
        <div class="flex gap-2">
          <input id="search-img-query" type="text" bind:value={imgQuery}
                 onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') searchImages(); }}
                 placeholder={t("search.imagePlaceholder")}
                 class="flex-1 rounded-md border border-border dark:border-white/[0.1]
                        bg-background px-3 py-2 text-[0.8rem]
                        placeholder:text-muted-foreground/30
                        focus:outline-none focus:ring-1 focus:ring-primary/50" />
          <Button onclick={searchImages} disabled={imgSearching || !imgQuery.trim()} size="sm"
                  class="gap-1.5 h-9 px-4 text-[0.72rem] shrink-0">
            {#if imgSearching}
              <Spinner size="w-3 h-3" />
              {t("search.searching")}
            {:else}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-3.5 h-3.5">
                <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/>
              </svg>
              {t("search.modeImages")}
            {/if}
          </Button>
        </div>
      </div>
    {/if}
  </div>

  <!-- ── Filter bar (mode-specific) ───────────────────────────────────── -->
  {#if mode === "eeg" && result && result.results.length > 0}
    <div class="flex items-center gap-2 px-4 py-1.5 shrink-0
                border-b border-border dark:border-white/[0.05] bg-muted/10">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           class="w-3 h-3 shrink-0 text-muted-foreground/40 pointer-events-none">
        <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
        <line x1="7" y1="7" x2="7.01" y2="7"/>
      </svg>
      <input type="text" placeholder={t("search.filterByLabel")} bind:value={labelFilter}
             oninput={() => { page = 0; }}
             class="flex-1 bg-transparent text-[0.7rem] focus:outline-none
                    placeholder:text-muted-foreground/30" />
      <label class="flex items-center gap-1.5 cursor-pointer select-none shrink-0">
        <input type="checkbox" bind:checked={labelsOnly} onchange={() => page = 0} class="rounded border-border h-3 w-3" />
        <span class="text-[0.6rem] text-muted-foreground/60">{t("search.labeledOnly")}</span>
      </label>
      {#if labelFilter || labelsOnly}
        <span class="text-[0.55rem] text-muted-foreground/40 tabular-nums shrink-0">
          {filtered.length} / {result.results.length}
        </span>
      {/if}
    </div>
  {:else if mode === "text" && textSearched && textResults.length > 0}
    <div class="flex items-center gap-2 px-4 py-1.5 shrink-0
                border-b border-border dark:border-white/[0.05] bg-muted/10">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           class="w-3 h-3 shrink-0 text-muted-foreground/40 pointer-events-none">
        <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
      </svg>
      <input type="text" placeholder={t("search.textFilterLabel")} bind:value={textFilter}
             oninput={() => { page = 0; }}
             class="flex-1 bg-transparent text-[0.7rem] focus:outline-none
                    placeholder:text-muted-foreground/30" />
      <!-- Sort toggle -->
      <div class="flex items-center gap-0.5 shrink-0">
        <span class="text-[0.5rem] text-muted-foreground/40 mr-1 select-none uppercase tracking-widest">Sort</span>
        <button onclick={() => { textSort = "sim"; page = 0; }}
                class="px-2 py-0.5 rounded text-[0.58rem] font-semibold transition-colors
                       {textSort === 'sim'
                          ? 'bg-violet-500/15 text-violet-600 dark:text-violet-400'
                          : 'text-muted-foreground/50 hover:text-muted-foreground'}">
          {t("search.textSortSim")}
        </button>
        <button onclick={() => { textSort = "date"; page = 0; }}
                class="px-2 py-0.5 rounded text-[0.58rem] font-semibold transition-colors
                       {textSort === 'date'
                          ? 'bg-violet-500/15 text-violet-600 dark:text-violet-400'
                          : 'text-muted-foreground/50 hover:text-muted-foreground'}">
          {t("search.textSortDate")}
        </button>
      </div>
    </div>
  {/if}

  <!-- ── Results ───────────────────────────────────────────────────────── -->
  <div class="flex-1 min-h-0 overflow-y-auto">

    <!-- ══════════════════ EEG MODE ════════════════════════════════════ -->
    {#if mode === "eeg"}

      {#if !result && !searching}
        <!-- Empty state -->
        <div class="flex flex-col items-center justify-center h-full gap-4 text-center px-10">
          <div class="w-16 h-16 rounded-2xl bg-blue-500/8 flex items-center justify-center">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
                 class="w-8 h-8 text-blue-500/40">
              <circle cx="11" cy="11" r="8"/><path d="m21 21-4.35-4.35"/>
            </svg>
          </div>
          <p class="text-[0.8rem] text-muted-foreground/50 max-w-[280px] leading-relaxed">
            {t("search.emptyState")}
          </p>
        </div>

      {:else if searching && !result}
        <!-- Loading state -->
        <div class="flex flex-col items-center justify-center h-full gap-3 px-8">
          <Spinner size="w-5 h-5" class="text-blue-500" />
          <span class="text-[0.78rem] text-muted-foreground">{searchStatus || t("search.searchingIndices")}</span>
          {#if streamTotal > 0}
            <div class="w-full max-w-xs flex flex-col gap-1.5">
              <div class="h-1.5 rounded-full bg-muted/30 overflow-hidden">
                <div class="h-full rounded-full bg-blue-500 transition-[width] duration-300"
                     style="width:{Math.min(100,(streamDone/streamTotal)*100).toFixed(1)}%"></div>
              </div>
              <div class="flex justify-between text-[0.58rem] text-muted-foreground/50 tabular-nums">
                <span>{streamDone} / {streamTotal}</span>
                {#if streamDays.length > 0}
                  <span class="truncate max-w-[180px]">{streamDays.map(fmtUtcDay).join(", ")}</span>
                {/if}
              </div>
            </div>
          {:else if searchElapsed > 0}
            <span class="text-[0.6rem] text-muted-foreground/40 tabular-nums">{t("search.elapsed", { n: searchElapsed })}</span>
          {/if}
        </div>

      {:else if result}
        {#if result.results.length === 0}
          <div class="flex flex-col items-center justify-center h-full gap-2 text-center px-8">
            <p class="text-[0.78rem] text-muted-foreground/60">
              {t("search.noEmbeddings", { start: fmtDateTime(result.start_utc), end: fmtDateTime(result.end_utc) })}
            </p>
            <p class="text-[0.65rem] text-muted-foreground/40">
              {t("search.daysSearched", { days: result.searched_days.map(fmtUtcDay).join(", ") || "—" })}
            </p>
          </div>

        {:else}
          {@const maxDist = Math.max(0.0001, ...filtered.flatMap(q => q.neighbors.map(n => n.distance)))}

          <!-- Result meta chips -->
          <div class="flex items-center gap-1.5 px-4 py-2 border-b border-border dark:border-white/[0.05] flex-wrap">
            <Badge variant="outline" class="text-[0.58rem] py-0 px-1.5 font-mono">
              {filtered.length}{filtered.length !== result.query_count ? `/${result.query_count}` : ""} {t("search.query").toLowerCase()}
            </Badge>
            <Badge variant="outline" class="text-[0.58rem] py-0 px-1.5 font-mono">k={result.k}</Badge>
            <Badge variant="outline" class="text-[0.58rem] py-0 px-1.5 font-mono">ef={result.ef}</Badge>
            {#each result.searched_days as day}
              <Badge variant="outline" class="text-[0.58rem] py-0 px-1.5 font-mono">{fmtUtcDay(day)}</Badge>
            {/each}
            <span class="ml-auto text-[0.58rem] text-muted-foreground/45 tabular-nums select-none">
              {fmtDateTime(result.start_utc)} → {fmtDateTime(result.end_utc)}
            </span>
          </div>

          <!-- ── Collapsible analysis ─────────────────────────────────── -->
          {#if searchAnalysis}
            {@const sa = searchAnalysis}
            {@const maxH = Math.max(...sa.hourHist, 1)}
            <div class="border-b border-border dark:border-white/[0.05]">
              <button onclick={() => showAnalysis = !showAnalysis}
                      class="w-full flex items-center gap-2 px-4 py-2 text-left
                             hover:bg-muted/20 transition-colors select-none">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                     class="w-3 h-3 text-muted-foreground/50 transition-transform duration-200 shrink-0
                            {showAnalysis ? 'rotate-0' : '-rotate-90'}">
                  <polyline points="6 9 12 15 18 9"/>
                </svg>
                <span class="text-[0.62rem] font-semibold text-muted-foreground/70 uppercase tracking-wider">
                  {t("search.analysisPanel")}
                </span>
                <!-- Mini stats always visible -->
                {#each analysisChips(sa) as [chipLabel, chipVal, chipCls]}
                  <span class="text-[0.55rem] text-muted-foreground/40 tabular-nums">
                    <span class="text-muted-foreground/25">{chipLabel}</span>
                    <span class="ml-0.5 {chipCls || 'text-foreground/60'}">{chipVal}</span>
                  </span>
                {/each}
              </button>

              {#if showAnalysis}
                <div class="px-4 pb-3">
                  <div class="rounded-xl border border-border dark:border-white/[0.06]
                              bg-white dark:bg-[#14141e] overflow-hidden">

                    <!-- Hour histogram -->
                    <div class="px-3.5 py-2.5">
                      <span class="text-[0.45rem] font-semibold text-muted-foreground/55 uppercase tracking-wider">
                        {t("search.analysisHourOfDay")}
                      </span>
                      <div class="flex items-end gap-[2px] h-[36px] mt-1.5">
                        {#each sa.hourHist as count, h}
                          <div class="flex-1 rounded-sm transition-all"
                               style="height:{Math.max(count/maxH*32, count>0?2:0)}px;
                                      background:{h===sa.peakHour?'#3b82f6':'currentColor'};
                                      opacity:{h===sa.peakHour?0.9:0.2}"
                               title="{String(h).padStart(2,'0')}:00 — {count}"></div>
                        {/each}
                      </div>
                      <!-- Hour labels (every 6h) -->
                      <div class="flex justify-between mt-0.5">
                        {#each [0,6,12,18] as h}
                          <span class="text-[0.35rem] text-muted-foreground/35 tabular-nums">{String(h).padStart(2,'0')}h</span>
                        {/each}
                      </div>
                    </div>

                    <!-- Day × hour heatmap -->
                    {#if temporalHeatmap}
                      <div class="px-3.5 py-2.5 border-t border-border/40 dark:border-white/[0.04]">
                        <span class="text-[0.45rem] font-semibold text-muted-foreground/55 uppercase tracking-wider">
                          {t("search.analysisHourByDay")}
                        </span>
                        <div class="mt-1.5 flex flex-col gap-[2px]">
                          {#each temporalHeatmap as row, di}
                            <div class="flex items-center gap-[2px]">
                              <span class="text-[0.34rem] text-muted-foreground/35 w-[18px] shrink-0 text-right pr-0.5 select-none">
                                {DAY_NAMES[di]}
                              </span>
                              {#each row as count, h}
                                <div class="flex-1 h-[9px] rounded-[1px]"
                                     style="background:{heatColor(count,heatmapMax)};
                                            {count===0?'border:0.5px solid var(--color-border);opacity:0.25':''}"
                                     title="{DAY_NAMES[di]} {String(h).padStart(2,'0')}:00 — {count}"></div>
                              {/each}
                            </div>
                          {/each}
                        </div>
                      </div>
                    {/if}

                    <!-- Top days -->
                    {#if sa.topDays.length > 0}
                      <div class="flex items-center gap-2 px-3.5 pb-2.5 flex-wrap border-t border-border/40 dark:border-white/[0.04] pt-2">
                        <span class="text-[0.42rem] text-muted-foreground/45 uppercase tracking-wider shrink-0">
                          {t("search.analysisTopDays")}
                        </span>
                        {#each sa.topDays as td}
                          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded
                                       bg-primary/10 text-primary text-[0.52rem] font-medium">
                            {td.day} <span class="opacity-55">({td.count})</span>
                          </span>
                        {/each}
                      </div>
                    {/if}
                  </div>
                </div>
              {/if}
            </div>
          {/if}

          <!-- ── UMAP 3D map (collapsible, always-visible header when data present) ── -->
          {#if (umapPlaceholder || umapResult)}
            <div class="border-b border-border dark:border-white/[0.05]">
              <div class="rounded-xl border border-border bg-card mx-4 my-3 overflow-hidden">

                <!-- Card header — always visible -->
                <div class="flex items-center gap-2 px-4 py-2 border-b border-border dark:border-white/[0.06]">
                  <!-- Graph icon -->
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       stroke-linecap="round" stroke-linejoin="round"
                       class="w-3.5 h-3.5 shrink-0 text-blue-500/70 pointer-events-none">
                    <circle cx="12" cy="12" r="2"/>
                    <circle cx="4"  cy="6"  r="2"/>
                    <circle cx="20" cy="6"  r="2"/>
                    <circle cx="4"  cy="18" r="2"/>
                    <circle cx="20" cy="18" r="2"/>
                    <line x1="6"  y1="7"  x2="10" y2="11"/>
                    <line x1="18" y1="7"  x2="14" y2="11"/>
                    <line x1="6"  y1="17" x2="10" y2="13"/>
                    <line x1="18" y1="17" x2="14" y2="13"/>
                  </svg>
                  <span class="text-[0.62rem] font-semibold select-none">Brain Nebula™</span>
                  <span class="text-[0.45rem] text-muted-foreground/40 font-normal select-none">{t("search.umap")}</span>

                  <!-- Status / point count -->
                  {#if umapLoading}
                    <Spinner size="w-3 h-3" class="text-muted-foreground/40" />
                    <span class="text-[0.45rem] text-muted-foreground italic">
                      {umapEta || t("search.computing3d")}{#if umapElapsed > 0} · {fmtSecs(umapElapsed)}{/if}
                    </span>
                  {:else if umapResult}
                    <span class="text-[0.45rem] text-muted-foreground/55 tabular-nums">
                      {umapResult.n_a} + {umapResult.n_b} pts · dim={umapResult.dim}
                    </span>
                  {/if}

                  <!-- By-date toggle -->
                  <!-- svelte-ignore a11y_consider_explicit_label -->
                  <button class="text-[0.5rem] px-1.5 py-0.5 rounded border transition-colors
                                 {umapColorByDate
                                    ? 'text-primary border-primary/30 bg-primary/10'
                                    : 'text-muted-foreground/40 hover:text-muted-foreground border-transparent hover:border-border'}"
                          onclick={() => umapColorByDate = !umapColorByDate}>
                    {t("search.byDate")}
                  </button>

                  <!-- Collapse / expand toggle -->
                  <!-- svelte-ignore a11y_consider_explicit_label -->
                  <button class="ml-auto text-[0.5rem] text-muted-foreground/40 hover:text-muted-foreground
                                 transition-colors px-1.5 py-0.5 rounded border border-transparent hover:border-border"
                          onclick={() => showUmap = !showUmap}>
                    {showUmap ? `▲ ${t("search.umapHide")}` : `▼ ${t("search.umapShow")}`}
                  </button>
                </div>

                <!-- Collapsible body -->
                {#if showUmap}
                  <div style="width:100%; height:480px">
                    <UmapViewer3D data={umapResult ?? umapPlaceholder ?? { points:[], n_a:0, n_b:0, dim:0 }}
                                  computing={umapLoading} colorByDate={umapColorByDate}
                                  autoConnectLabels={true} />
                  </div>

                  <!-- Legend bar -->
                  <div class="flex items-center gap-4 text-[0.42rem] text-muted-foreground/55
                              px-4 py-2 border-t border-border dark:border-white/[0.06] flex-wrap">
                    {#if !umapColorByDate}
                      <div class="flex items-center gap-1">
                        <span class="inline-block w-2.5 h-2.5 rounded-full shrink-0"
                              style="background:#{UMAP_COLOR_A.toString(16).padStart(6,'0')}"></span>
                        {t("search.umapQuery")} ({(umapResult ?? umapPlaceholder)?.n_a ?? 0})
                      </div>
                      <div class="flex items-center gap-1">
                        <span class="inline-block w-2.5 h-2.5 rounded-full shrink-0"
                              style="background:#{UMAP_COLOR_B.toString(16).padStart(6,'0')}"></span>
                        {t("search.umapNeighbor")} ({(umapResult ?? umapPlaceholder)?.n_b ?? 0})
                      </div>
                    {:else}
                      <span>{t("search.umapDateMode")}</span>
                    {/if}
                    <span class="ml-auto italic opacity-70">{t("search.umapDesc")}</span>
                  </div>
                {/if}

              </div>
            </div>
          {/if}

          <!-- ── Query entry list ────────────────────────────────────── -->
          <div class="divide-y divide-border dark:divide-white/[0.04]">
            {#each pageSlice as qentry, qi}
              {@const gi = page * SEARCH_PAGE_SIZE + qi}
              <div class="px-4 py-3">
                <!-- Query header -->
                <div class="flex items-center gap-2 mb-2">
                  <div class="flex items-center gap-1.5 shrink-0">
                    <span class="text-[0.5rem] font-bold tracking-widest uppercase
                                 text-muted-foreground/40 select-none">{t("search.query")}</span>
                    <span class="font-mono text-[0.72rem] font-bold text-foreground">
                      {fmtTime(qentry.timestamp_unix)}
                    </span>
                    <span class="text-[0.6rem] text-muted-foreground/40">{fmtDate(qentry.timestamp_unix)}</span>
                  </div>
                  <span class="ml-auto text-[0.55rem] text-muted-foreground/30 tabular-nums select-none">
                    #{gi+1}/{filtered.length}
                  </span>
                </div>

                <!-- Neighbors -->
                <div class="flex flex-col gap-1">
                  {#each qentry.neighbors as nb, ni}
                    {@const isSelf = nb.distance < 0.001}
                    {@const sw = simWidth(nb.distance, maxDist)}
                    {@const color = distColor(nb.distance)}
                    <div class="flex gap-0 rounded-md overflow-hidden bg-muted/20 dark:bg-white/[0.02]
                                border border-border/60 dark:border-white/[0.05]
                                hover:border-border dark:hover:border-white/[0.1] transition-colors">
                      <!-- Left rank stripe -->
                      <div class="w-[3px] shrink-0 rounded-l-md" style="background:{color}"></div>

                      <div class="flex-1 px-2.5 py-2">
                        <!-- Sim bar row -->
                        <div class="flex items-center gap-2 mb-1.5">
                          <span class="shrink-0 text-[0.55rem] text-muted-foreground/35 font-mono tabular-nums w-4">
                            #{ni+1}
                          </span>
                          <div class="flex-1 h-[5px] rounded-full bg-muted/30 dark:bg-white/[0.05] overflow-hidden">
                            <div class="h-full rounded-full" style="width:{(sw*100).toFixed(1)}%;background:{color}"></div>
                          </div>
                          <span class="shrink-0 text-[0.64rem] font-bold tabular-nums" style="color:{color}">
                            {isSelf ? "100%" : simPct(nb.distance, maxDist)}
                          </span>
                          <span class="shrink-0 font-mono text-[0.56rem] text-muted-foreground/35 tabular-nums">
                            {isSelf ? t("search.analysisSelf") : nb.distance.toFixed(4)}
                          </span>
                        </div>

                        <!-- Time + device + action -->
                        <div class="flex items-center gap-1.5 flex-wrap">
                          <span class="font-mono text-[0.68rem] font-semibold">{fmtTime(nb.timestamp_unix)}</span>
                          <span class="text-[0.58rem] text-muted-foreground/40 font-mono">{fmtDate(nb.timestamp_unix)}</span>
                          {#if nb.device_name}
                            <span class="text-[0.58rem] text-muted-foreground/55 truncate max-w-[120px]">{nb.device_name}</span>
                          {/if}
                          <button class="ml-auto text-[0.55rem] text-primary/60 hover:text-primary font-medium
                                         transition-colors px-1.5 py-0.5 rounded hover:bg-primary/8"
                                  onclick={(e) => { e.stopPropagation(); openSession(nb); }}>
                            {t("search.viewSession")} ↗
                          </button>
                        </div>

                        <!-- Metrics -->
                        {#if nb.metrics}
                          <div class="flex flex-wrap gap-x-2.5 gap-y-0.5 mt-1">
                            {#each metricChips(nb.metrics) as chip}
                              <span class="inline-flex items-center gap-0.5 text-[0.52rem] tabular-nums">
                                <span class="text-muted-foreground/40">{chip.l}</span>
                                <span class="font-bold" style="color:{chip.c}">{chip.v}</span>
                              </span>
                            {/each}
                          </div>
                        {/if}

                        <!-- Labels -->
                        {#if nb.labels.length > 0}
                          <div class="flex flex-wrap gap-1 mt-1.5">
                            {#each nb.labels as lbl}
                              <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded
                                           bg-emerald-500/10 border border-emerald-500/20
                                           text-emerald-700 dark:text-emerald-400 text-[0.58rem] leading-tight"
                                    title="{lbl.text} ({fmtDateTime(lbl.eeg_start)} → {fmtDateTime(lbl.eeg_end)})">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                                     class="w-2.5 h-2.5 shrink-0 pointer-events-none">
                                  <path d="M20.59 13.41l-7.17 7.17a2 2 0 01-2.83 0L2 12V2h10l8.59 8.59a2 2 0 010 2.82z"/>
                                  <line x1="7" y1="7" x2="7.01" y2="7"/>
                                </svg>
                                <span class="truncate max-w-[260px]">{lbl.text}</span>
                                <span class="shrink-0 opacity-50 font-mono text-[0.5rem]">
                                  {fmtTime(lbl.eeg_start)}–{fmtTime(lbl.eeg_end)}
                                </span>
                              </span>
                            {/each}
                          </div>
                        {/if}
                      </div>
                    </div>
                  {/each}
                </div>
              </div>
            {/each}
          </div>

          <!-- Pagination -->
          {#if totalPages > 1}
            <div class="sticky bottom-0 flex items-center justify-between px-4 py-2
                        border-t border-border dark:border-white/[0.06]
                        bg-background/95 backdrop-blur-sm">
              <Button variant="outline" size="sm" class="h-7 px-3 text-[0.68rem]"
                      disabled={page===0} onclick={() => page--}>{t("search.prev")}</Button>
              <span class="text-[0.62rem] text-muted-foreground/55 tabular-nums">
                {t("search.pageOf", { page: page+1, total: totalPages })}
                <span class="text-muted-foreground/30 mx-1">·</span>
                {t("search.queriesRange", { start: page*SEARCH_PAGE_SIZE+1, end: Math.min((page+1)*SEARCH_PAGE_SIZE, filtered.length) })}
              </span>
              <Button variant="outline" size="sm" class="h-7 px-3 text-[0.68rem]"
                      disabled={page>=totalPages-1} onclick={() => page++}>{t("search.next")}</Button>
            </div>
          {/if}
        {/if}
      {/if}

    <!-- ══════════════════ INTERACTIVE MODE ════════════════════════════ -->
    {:else if mode === "interactive"}

      {#if !ixSearched && !ixSearching}
        <!-- Empty state -->
        <div class="flex flex-col items-center justify-center h-full gap-4 text-center px-10">
          <div class="w-16 h-16 rounded-2xl bg-emerald-500/8 flex items-center justify-center">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
                 class="w-8 h-8 text-emerald-500/40">
              <circle cx="12" cy="5"  r="2"/>
              <circle cx="5"  cy="19" r="2"/>
              <circle cx="19" cy="19" r="2"/>
              <line x1="12" y1="7"  x2="5"  y2="17"/>
              <line x1="12" y1="7"  x2="19" y2="17"/>
              <line x1="5"  y1="19" x2="19" y2="19"/>
            </svg>
          </div>
          <div class="flex flex-col gap-1.5">
            <p class="text-[0.82rem] font-semibold text-foreground/70">{t("search.modeInteractive")}</p>
            <p class="text-[0.72rem] text-muted-foreground/50 max-w-[320px] leading-relaxed">
              {t("search.interactiveEmptyState")}
            </p>
          </div>
          <!-- Pipeline steps hint -->
          <div class="flex flex-col gap-1.5 text-left max-w-[280px] mt-1">
            {#each [
              { n: 1, col: "#8b5cf6", label: "Query → text embedding" },
              { n: 2, col: "#3b82f6", label: "Find semantically similar labels" },
              { n: 3, col: "#f59e0b", label: "Bridge to EEG embedding space" },
              { n: 4, col: "#f59e0b", label: "Find neighboring EEG patterns" },
              { n: 5, col: "#10b981", label: "Discover nearby label context" },
            ] as step}
              <div class="flex items-center gap-2">
                <span class="w-4 h-4 rounded-full flex items-center justify-center text-[0.45rem]
                             font-bold text-white shrink-0"
                      style="background:{step.col}">
                  {step.n}
                </span>
                <span class="text-[0.65rem] text-muted-foreground/60">{step.label}</span>
              </div>
            {/each}
          </div>
        </div>

      {:else if ixSearching}
        <!-- Loading state with pipeline steps animation -->
        <div class="flex flex-col items-center justify-center h-full gap-4 px-8">
          <Spinner size="w-5 h-5" class="text-emerald-500" />
          <span class="text-[0.78rem] text-muted-foreground">
            {ixStatus || t("search.interactiveSearching")}
          </span>
          <!-- Animated pipeline -->
          <div class="flex flex-col gap-2 w-full max-w-xs mt-2">
            {#each [
              { n: 1, label: "Text embedding", col: "#8b5cf6" },
              { n: 2, label: "Label similarity", col: "#3b82f6" },
              { n: 3, label: "EEG bridge", col: "#f59e0b" },
              { n: 4, label: "Temporal labels", col: "#10b981" },
            ] as step}
              <div class="flex items-center gap-2.5">
                <span class="w-4 h-4 rounded-full flex items-center justify-center text-[0.45rem]
                             font-bold text-white shrink-0 animate-pulse"
                      style="background:{step.col}">
                  {step.n}
                </span>
                <div class="flex-1 h-1 rounded-full bg-muted/30 overflow-hidden">
                  <div class="h-full rounded-full animate-pulse"
                       style="background:{step.col}; width: 60%; opacity: 0.6;"></div>
                </div>
                <span class="text-[0.55rem] text-muted-foreground/50 w-20 text-right shrink-0">
                  {step.label}
                </span>
              </div>
            {/each}
          </div>
        </div>

      {:else if ixNodes.length === 0}
        <div class="flex flex-col items-center justify-center h-full gap-2 text-center px-8">
          <p class="text-[0.78rem] text-muted-foreground/60">{t("search.interactiveNoResults")}</p>
        </div>

      {:else}
        <!-- ── Single merged header row ─────────────────────────────────── -->
        {@const dispNodes = ixDisplayGraph.nodes}
        {@const dispEdges = ixDisplayGraph.edges}
        {@const tlCount   = ixNodes.filter(n => n.kind === "text_label").length}
        {@const epCount   = ixNodes.filter(n => n.kind === "eeg_point").length}
        {@const flRaw     = ixNodes.filter(n => n.kind === "found_label").length}
        {@const flDisp    = dispNodes.filter(n => n.kind === "found_label").length}

        <div class="flex items-center gap-2 px-4 py-1.5 border-b border-border dark:border-white/[0.05] shrink-0">
          <!-- Coloured node-kind dots + counts -->
          <span class="flex items-center gap-0.5 text-[0.52rem] text-violet-500/80 tabular-nums shrink-0">
            <span class="w-1.5 h-1.5 rounded-full bg-violet-500 shrink-0"></span>1
          </span>
          <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
          <span class="flex items-center gap-0.5 text-[0.52rem] text-blue-500/80 tabular-nums shrink-0">
            <span class="w-1.5 h-1.5 rounded-full bg-blue-500 shrink-0"></span>{tlCount}
          </span>
          <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
          <span class="flex items-center gap-0.5 text-[0.52rem] text-amber-500/80 tabular-nums shrink-0">
            <span class="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0"></span>{epCount}
          </span>
          <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
          <span class="flex items-center gap-0.5 text-[0.52rem] text-emerald-500/80 tabular-nums shrink-0">
            <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 shrink-0"></span>{flDisp}{#if ixDedupeLabels && flRaw > flDisp}<span class="opacity-40">/{flRaw}</span>{/if}
          </span>
          <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
          <span class="text-[0.48rem] text-muted-foreground/40 tabular-nums shrink-0">
            {dispEdges.length} edges
          </span>

          <!-- Deduplicate toggle -->
          <label class="flex items-center gap-1 cursor-pointer select-none shrink-0 ml-1">
            <input type="checkbox" bind:checked={ixDedupeLabels}
                   class="rounded border-border h-2.5 w-2.5 accent-violet-500" />
            <span class="text-[0.48rem] text-muted-foreground/50">{t("search.interactiveDedupe")}</span>
          </label>

          <!-- PCA cluster toggle -->
          <label class="flex items-center gap-1 cursor-pointer select-none shrink-0">
            <input type="checkbox" bind:checked={ixUsePca}
                   class="rounded border-border h-2.5 w-2.5 accent-violet-500" />
            <span class="text-[0.48rem] text-muted-foreground/50">PCA cluster</span>
          </label>

          <!-- Export buttons (DOT + SVG) — shown once a search has been run -->
          {#if ixDot}
            <!-- .dot -->
            <button onclick={downloadDot} disabled={dotSaving}
                    title={dotSavedPath ? t("search.exportSavedTitle", { path: dotSavedPath }) : t("search.exportDotTitle")}
                    class="flex items-center gap-1 px-1.5 py-0.5 rounded border transition-colors select-none shrink-0 text-[0.48rem]
                           {dotSavedPath ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                                         : 'border-border dark:border-white/[0.1] bg-background hover:bg-muted/40 text-muted-foreground/55 hover:text-foreground'}">
              {#if dotSaving}
                <svg class="w-2.5 h-2.5 animate-spin shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>
              {:else if dotSavedPath}
                <svg class="w-2.5 h-2.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
              {:else}
                <svg class="w-2.5 h-2.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
              {/if}
              {dotSavedPath ? t("search.exportSaved") : dotSaving ? t("search.exportSavingDot") : ".dot"}
            </button>

            <!-- .svg — pure Rust renderer, no external binary -->
            <button onclick={downloadSvg} disabled={svgSaving || !ixSvg}
                    title={svgSavedPath ? t("search.exportSavedTitle", { path: svgSavedPath }) : svgError ? svgError : t("search.exportSvgTitle")}
                    class="flex items-center gap-1 px-1.5 py-0.5 rounded border transition-colors select-none shrink-0 text-[0.48rem]
                           {svgSavedPath
                             ? 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
                             : svgError
                               ? 'border-red-500/30 bg-red-500/10 text-red-500 dark:text-red-400'
                               : 'border-border dark:border-white/[0.1] bg-background hover:bg-muted/40 text-muted-foreground/55 hover:text-foreground'}">
              {#if svgSaving}
                <svg class="w-2.5 h-2.5 animate-spin shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>
              {:else if svgSavedPath}
                <svg class="w-2.5 h-2.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
              {:else if svgError}
                <svg class="w-2.5 h-2.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
              {:else}
                <svg class="w-2.5 h-2.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M8 12h8M12 8v8"/></svg>
              {/if}
              {svgSavedPath ? t("search.exportSaved") : svgSaving ? t("search.exportRenderingSvg") : svgError ? t("search.exportError") : ".svg"}
            </button>

            <!-- Inline error hint (truncated, full text on hover via button title) -->
            {#if svgError}
              <span class="text-[0.44rem] text-red-500/70 truncate max-w-[140px]"
                    title={svgError}>{svgError}</span>
            {/if}
          {/if}

          <!-- Show/hide graph toggle -->
          <button class="ml-auto text-[0.48rem] text-muted-foreground/40 hover:text-muted-foreground
                         transition-colors px-1.5 py-0.5 rounded border border-transparent
                         hover:border-border select-none shrink-0"
                  onclick={() => showIxGraph = !showIxGraph}>
            {showIxGraph ? `▲ ${t("search.interactiveGraphHide")}` : `▼ ${t("search.interactiveGraphShow")}`}
          </button>
        </div>

        <!-- 3D Graph panel -->
        <div class="border-b border-border dark:border-white/[0.05] shrink-0">
          <div class="rounded-xl border border-border bg-card mx-4 my-3 overflow-hidden">
            <!-- (header merged into row above) -->

            {#if showIxGraph}
              <div style="width:100%; height:500px">
                <InteractiveGraph3D nodes={dispNodes} edges={dispEdges} usePca={ixUsePca} />
              </div>
            {/if}
          </div>
        </div>

        <!-- ── Time heatmap: days × hours ─────────────────────────────── -->
        {#if ixTimeHeatmap}
          {@const hm = ixTimeHeatmap}
          <div class="mx-4 mb-3 rounded-xl border border-border bg-card overflow-hidden shrink-0">
            <!-- Header -->
            <div class="flex items-center gap-2 px-3.5 py-2 border-b border-border dark:border-white/[0.06]">
              <!-- Clock icon -->
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                   class="w-3.5 h-3.5 shrink-0 text-amber-500/70 pointer-events-none">
                <circle cx="12" cy="12" r="10"/>
                <polyline points="12 6 12 12 16 14"/>
              </svg>
              <span class="text-[0.62rem] font-semibold select-none">EEG Time Distribution</span>
              <span class="text-[0.45rem] text-muted-foreground/45 tabular-nums">
                {hm.days.length} day{hm.days.length !== 1 ? "s" : ""} ·
                hours 0–23
              </span>
              <!-- Turbo gradient pill -->
              <div class="ml-auto flex items-center gap-1.5">
                <span class="text-[0.4rem] text-muted-foreground/40 select-none">early</span>
                <div class="w-14 h-1.5 rounded-full"
                     style="background:linear-gradient(to right,{turboColor(0)},{turboColor(0.25)},{turboColor(0.5)},{turboColor(0.75)},{turboColor(1)})">
                </div>
                <span class="text-[0.4rem] text-muted-foreground/40 select-none">late</span>
              </div>
            </div>

            <!-- Grid body -->
            <div class="px-3.5 py-2.5 overflow-x-auto">
              <!-- Layout: hour-label column (20px) + one column per day (20px each) -->
              <div style="display:grid;
                          grid-template-columns: 20px repeat({hm.days.length}, 20px);
                          grid-template-rows: 16px repeat(24, 7px);
                          column-gap:2px;
                          row-gap:1px;
                          min-width:max-content">

                <!-- Top-left corner spacer -->
                <div></div>
                <!-- Day header row -->
                {#each hm.days as _day, di}
                  <div class="text-center overflow-hidden select-none leading-none
                               flex items-end justify-center pb-0.5"
                       style="font-size:0.3rem; color:rgba(150,150,150,0.7)">
                    {hm.dayLabels[di]}
                  </div>
                {/each}

                <!-- 24 hour rows -->
                {#each Array.from({length: 24}, (_, h) => h) as h}
                  <!-- Hour label (every 3 hours) -->
                  <div class="flex items-center justify-end pr-0.5 select-none tabular-nums"
                       style="font-size:0.3rem; color:rgba(150,150,150,{h % 3 === 0 ? '0.6' : '0.2'})">
                    {h % 3 === 0 ? String(h).padStart(2,'0') : ''}
                  </div>
                  <!-- Day cells for this hour -->
                  {#each hm.days as _day, di}
                    {@const count = hm.grid[di][h]}
                    {@const cellColor = ixHeatColor(di, h, count, hm)}
                    <div class="rounded-[1.5px]"
                         style="background:{cellColor};
                                {count === 0 ? 'outline:0.5px solid rgba(127,127,127,0.08)' : ''}"
                         title="{hm.dayLabels[di]} {String(h).padStart(2,'0')}:00 — {count} node{count !== 1 ? 's' : ''}">
                    </div>
                  {/each}
                {/each}
              </div>
            </div>
          </div>
        {/if}

        <!-- Node list — scrollable detail view -->
        <div class="flex-1 min-h-0 overflow-y-auto">
          <!-- Text labels section -->
          {#if dispNodes.filter(n => n.kind === "text_label").length > 0}
            <div class="px-4 pt-3 pb-1">
              <span class="text-[0.52rem] font-semibold uppercase tracking-widest text-blue-500/70 select-none">
                Text Matches
              </span>
            </div>
            <div class="divide-y divide-border/50 dark:divide-white/[0.03]">
              {#each dispNodes.filter(n => n.kind === "text_label") as node}
                {@const sw = 1 - node.distance / Math.max(...dispNodes.filter(n => n.kind === "text_label").map(n => n.distance), 0.001)}
                <div class="flex gap-0 hover:bg-muted/10 transition-colors">
                  <div class="w-[3px] shrink-0" style="background:#3b82f6; opacity:0.7"></div>
                  <div class="flex-1 px-3.5 py-2.5">
                    <div class="flex items-center gap-2 mb-1">
                      <div class="flex-1 h-[4px] rounded-full bg-muted/25 overflow-hidden">
                        <div class="h-full rounded-full bg-blue-500"
                             style="width:{(sw * 100).toFixed(1)}%"></div>
                      </div>
                      <span class="shrink-0 text-[0.58rem] font-bold text-blue-500 tabular-nums">
                        {((1 - node.distance) * 100).toFixed(1)}%
                      </span>
                      <span class="shrink-0 font-mono text-[0.5rem] text-muted-foreground/35">
                        {node.distance.toFixed(4)}
                      </span>
                    </div>
                    <p class="text-[0.8rem] font-medium leading-snug text-foreground">{node.text}</p>
                    {#if node.timestamp_unix}
                      <span class="text-[0.58rem] text-muted-foreground/50 font-mono">
                        {fmtDate(node.timestamp_unix)} · {fmtTime(node.timestamp_unix)}
                      </span>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}

          <!-- Found labels section -->
          {#if dispNodes.filter(n => n.kind === "found_label").length > 0}
            {@const flNodes = dispNodes.filter(n => n.kind === "found_label")}
            {@const flHasPca = flNodes.some(n => (n as any).proj_x !== undefined)}
            <div class="px-4 pt-3 pb-1 mt-1 flex items-center gap-2 flex-wrap">
              <span class="text-[0.52rem] font-semibold uppercase tracking-widest text-emerald-500/70 select-none">
                Discovered via EEG
              </span>
              {#if flHasPca}
                <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded
                             bg-emerald-500/8 border border-emerald-500/20
                             text-emerald-600 dark:text-emerald-400 text-[0.46rem] select-none">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       class="w-2 h-2 shrink-0">
                    <circle cx="12" cy="12" r="2"/><circle cx="5" cy="6" r="2"/>
                    <circle cx="19" cy="6" r="2"/><circle cx="5" cy="18" r="2"/>
                    <circle cx="19" cy="18" r="2"/>
                    <line x1="7" y1="7" x2="10" y2="11"/>
                    <line x1="17" y1="7" x2="14" y2="11"/>
                  </svg>
                  clustered by embedding similarity
                </span>
              {/if}
              {#if ixDedupeLabels && flRaw > flDisp}
                <span class="text-[0.48rem] text-muted-foreground/40 italic select-none">
                  {flDisp} unique · {flRaw - flDisp} merged
                </span>
              {/if}
            </div>
            <!-- Sort found_labels by PCA proximity (proj_x asc) when available,
                 so the list reflects the same left-to-right ordering as the 3D sphere. -->
            {@const sortedFl = flHasPca
              ? [...flNodes].sort((a, b) => {
                  const ax = (a as any).proj_x ?? 0;
                  const bx = (b as any).proj_x ?? 0;
                  if (Math.abs(ax - bx) > 0.05) return ax - bx;
                  return ((a as any).proj_y ?? 0) - ((b as any).proj_y ?? 0);
                })
              : flNodes}
            <div class="divide-y divide-border/50 dark:divide-white/[0.03]">
              {#each sortedFl as node}
                {@const projX = (node as any).proj_x as number | undefined}
                {@const projY = (node as any).proj_y as number | undefined}
                <div class="flex gap-0 hover:bg-muted/10 transition-colors">
                  <!-- Left stripe: opacity encodes PCA cluster density (proj_x maps to hue shift) -->
                  <div class="w-[3px] shrink-0" style="background:#10b981; opacity:0.7"></div>
                  <div class="flex-1 px-3.5 py-2.5">
                    <p class="text-[0.8rem] font-medium leading-snug text-foreground">{node.text}</p>
                    <div class="flex items-center gap-2 flex-wrap mt-0.5">
                      {#if node.timestamp_unix}
                        <span class="text-[0.58rem] text-muted-foreground/50 font-mono">
                          {fmtDate(node.timestamp_unix)} · {fmtTime(node.timestamp_unix)}
                        </span>
                      {/if}
                      {#if node.distance > 0}
                        <span class="text-[0.52rem] text-muted-foreground/35">
                          ±{(node.distance * 60).toFixed(0)}min from EEG
                        </span>
                      {/if}
                      {#if projX !== undefined && projY !== undefined}
                        <!-- Tiny PCA coordinate badge — helps user cross-reference with 3D graph -->
                        <span class="text-[0.44rem] font-mono text-emerald-500/40 select-none"
                              title="PCA embedding coordinates (x={projX.toFixed(2)}, y={projY.toFixed(2)})">
                          pca ({projX.toFixed(2)}, {projY.toFixed(2)})
                        </span>
                      {/if}
                    </div>
                  </div>
                </div>
              {/each}
            </div>
          {/if}

          <!-- EEG points summary -->
          {#if dispNodes.filter(n => n.kind === "eeg_point").length > 0}
            <div class="px-4 pt-3 pb-2 mt-1">
              <span class="text-[0.52rem] font-semibold uppercase tracking-widest text-amber-500/70 select-none">
                EEG Neighbor Timestamps
              </span>
              <div class="flex flex-wrap gap-1.5 mt-1.5">
                {#each dispNodes.filter(n => n.kind === "eeg_point") as node}
                  <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded
                               bg-amber-500/8 border border-amber-500/20
                               text-amber-600 dark:text-amber-400 text-[0.52rem] font-mono">
                    {#if node.timestamp_unix}
                      {fmtDate(node.timestamp_unix)} {fmtTime(node.timestamp_unix)}
                    {:else}
                      —
                    {/if}
                    <span class="opacity-50">{node.distance.toFixed(3)}</span>
                  </span>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {/if}

    <!-- ══════════════════ TEXT MODE ════════════════════════════════════ -->
    {:else if mode === "text"}

      {#if !textSearched && !textSearching}
        <!-- Empty state -->
        <div class="flex flex-col items-center justify-center h-full gap-4 text-center px-10">
          <div class="w-16 h-16 rounded-2xl bg-violet-500/8 flex items-center justify-center">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
                 class="w-8 h-8 text-violet-500/40">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
            </svg>
          </div>
          <div class="flex flex-col gap-1.5">
            <p class="text-[0.82rem] font-semibold text-foreground/70">{t("search.modeText")}</p>
            <p class="text-[0.72rem] text-muted-foreground/50 max-w-[300px] leading-relaxed">
              {t("search.textEmptyState")}
            </p>
          </div>
        </div>

      {:else if textSearching}
        <div class="flex flex-col items-center justify-center h-full gap-3">
          <Spinner size="w-5 h-5" class="text-violet-500" />
          <span class="text-[0.78rem] text-muted-foreground">{t("search.searching")}</span>
        </div>

      {:else if textFiltered.length === 0}
        <div class="flex flex-col items-center justify-center h-full gap-2 text-center px-8">
          <p class="text-[0.8rem] text-muted-foreground/60">{t("search.textNoResults")}</p>
          <p class="text-[0.65rem] text-muted-foreground/40 max-w-[280px] leading-relaxed">
            {t("search.textNoResultsHint")}
          </p>
        </div>

      {:else}
        <!-- Result summary bar -->
        <div class="flex items-center gap-1.5 px-4 py-2 border-b border-border dark:border-white/[0.05] flex-wrap">
          <Badge variant="outline"
                 class="text-[0.58rem] py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/25">
            {textFiltered.length} {t("search.textResultsCount")}
          </Badge>
          <Badge variant="outline" class="text-[0.58rem] py-0 px-1.5 font-mono">k={kVal}</Badge>
          {#if textResults[0]?.embedding_model}
            <Badge variant="outline" class="text-[0.58rem] py-0 px-1.5 font-mono max-w-[180px] truncate">
              {t("search.textViaModel", { model: textResults[0].embedding_model })}
            </Badge>
          {/if}
          <span class="text-[0.52rem] text-muted-foreground/30 ml-auto select-none italic">
            {textSort === "sim" ? t("search.textSortSim") : t("search.textSortDate")}
          </span>
        </div>

        <!-- ── kNN Similarity 3D Graph ─────────────────────────── -->
        {#if textGraphData}
          <div class="border-b border-border dark:border-white/[0.05]">
            <div class="rounded-xl border border-border bg-card mx-4 my-3 overflow-hidden">
              <!-- Panel header -->
              <div class="flex items-center gap-2 px-4 py-2 border-b border-border dark:border-white/[0.06]">
                <!-- Title + graph icon -->
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                     stroke-linecap="round" stroke-linejoin="round"
                     class="w-3.5 h-3.5 shrink-0 text-violet-500/70 pointer-events-none">
                  <circle cx="12" cy="12" r="2"/>
                  <circle cx="4"  cy="6"  r="2"/>
                  <circle cx="20" cy="6"  r="2"/>
                  <circle cx="4"  cy="18" r="2"/>
                  <circle cx="20" cy="18" r="2"/>
                  <line x1="6"  y1="7"  x2="10" y2="11"/>
                  <line x1="18" y1="7"  x2="14" y2="11"/>
                  <line x1="6"  y1="17" x2="10" y2="13"/>
                  <line x1="18" y1="17" x2="14" y2="13"/>
                </svg>
                <span class="text-[0.62rem] font-semibold select-none">{t("search.textKnnGraph")}</span>
                <span class="text-[0.45rem] text-muted-foreground/45 tabular-nums">
                  1 + {textGraphData.n_b} pts
                </span>
                <!-- Toggle visibility -->
                <!-- svelte-ignore a11y_consider_explicit_label -->
                <button class="ml-auto text-[0.5rem] text-muted-foreground/40 hover:text-muted-foreground
                               transition-colors px-1.5 py-0.5 rounded border border-transparent hover:border-border"
                        onclick={() => showTextGraph = !showTextGraph}>
                  {showTextGraph ? `▲ ${t("search.textKnnGraphHide")}` : `▼ ${t("search.textKnnGraphShow")}`}
                </button>
              </div>

              {#if showTextGraph}
                <!-- 3D scene -->
                <div style="width:100%; height:300px">
                  <UmapViewer3D data={textGraphData} computing={false} colorByDate={false}
                                autoConnectLabels={true} />
                </div>

                <!-- Legend bar -->
                <div class="flex items-center gap-4 text-[0.42rem] text-muted-foreground/55
                            px-4 py-2 border-t border-border dark:border-white/[0.06] flex-wrap">
                  <!-- Query legend dot -->
                  <div class="flex items-center gap-1">
                    <span class="inline-block w-2.5 h-2.5 rounded-full shrink-0"
                          style="background:#{UMAP_COLOR_A.toString(16).padStart(6,'0')}"></span>
                    <span>{t("search.textKnnGraphQuery")}</span>
                  </div>
                  <!-- Results legend dot -->
                  <div class="flex items-center gap-1">
                    <span class="inline-block w-2.5 h-2.5 rounded-full shrink-0"
                          style="background:#{UMAP_COLOR_B.toString(16).padStart(6,'0')}"></span>
                    <span>{t("search.textKnnGraphResults")} ({textGraphData.n_b})</span>
                  </div>
                  <!-- Distance gradient hint -->
                  <div class="flex items-center gap-1 ml-2">
                    <span class="text-muted-foreground/30">←</span>
                    <span class="text-[0.4rem] text-muted-foreground/35 select-none">{t("search.textKnnGraphNear")}</span>
                    <div class="w-14 h-1 rounded-full mx-0.5"
                         style="background: linear-gradient(to right,
                           #{UMAP_COLOR_A.toString(16).padStart(6,'0')}80,
                           #{UMAP_COLOR_B.toString(16).padStart(6,'0')}80)">
                    </div>
                    <span class="text-[0.4rem] text-muted-foreground/35 select-none">{t("search.textKnnGraphFar")}</span>
                    <span class="text-muted-foreground/30">→</span>
                  </div>
                  <span class="ml-auto italic opacity-60">{t("search.textKnnGraphDesc")}</span>
                </div>
              {/if}
            </div>
          </div>
        {/if}

        <!-- Result cards -->
        <div class="divide-y divide-border dark:divide-white/[0.04]">
          {#each textPageSlice as nb, ni}
            {@const gi = page * SEARCH_PAGE_SIZE + ni}
            {@const sw = simWidth(nb.distance, textMaxDist)}
            {@const color = distColor(nb.distance)}

            <div class="flex gap-0 hover:bg-muted/15 transition-colors">
              <!-- Left accent strip -->
              <div class="w-[4px] shrink-0" style="background:{color};opacity:0.7"></div>

              <div class="flex-1 px-4 py-3.5">
                <!-- Top row: rank + similarity bar + score + distance -->
                <div class="flex items-center gap-2 mb-2.5">
                  <span class="shrink-0 text-[0.58rem] font-bold text-muted-foreground/40
                               tabular-nums select-none w-6">#{gi+1}</span>
                  <div class="flex-1 h-1.5 rounded-full bg-muted/25 dark:bg-white/[0.05] overflow-hidden">
                    <div class="h-full rounded-full transition-[width] duration-500"
                         style="width:{(sw*100).toFixed(1)}%;background:{color}"></div>
                  </div>
                  <span class="shrink-0 text-[0.72rem] font-bold tabular-nums" style="color:{color}">
                    {simPct(nb.distance, textMaxDist)}
                  </span>
                  <span class="shrink-0 font-mono text-[0.58rem] text-muted-foreground/35 tabular-nums">
                    {nb.distance.toFixed(4)}
                  </span>
                </div>

                <!-- Label text (main content) -->
                <p class="text-[0.86rem] font-semibold text-foreground leading-snug mb-2">{nb.text}</p>

                <!-- Context (collapsible) -->
                {#if nb.context && nb.context.trim()}
                  {@const preview = nb.context.trim().slice(0, 140)}
                  {@const hasMore = nb.context.trim().length > 140}
                  <details class="group/ctx mb-2.5">
                    <summary class="list-none cursor-pointer text-[0.7rem] text-muted-foreground/50
                                    italic leading-snug hover:text-muted-foreground/70 transition-colors select-none">
                      {preview}{hasMore ? "…" : ""}
                      {#if hasMore}
                        <span class="not-italic font-semibold text-violet-500/60 ml-1
                                     group-open/ctx:hidden">{t("labels.showMore")}</span>
                      {/if}
                    </summary>
                    {#if hasMore}
                      <p class="mt-1.5 text-[0.7rem] text-muted-foreground/50 italic
                                leading-relaxed whitespace-pre-wrap">{nb.context.trim()}</p>
                    {/if}
                  </details>
                {/if}

                <!-- EEG window time -->
                <div class="flex items-center gap-1.5 text-[0.62rem] text-muted-foreground/45
                            font-mono mb-2">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
                       class="w-3 h-3 shrink-0 text-muted-foreground/30 pointer-events-none">
                    <rect x="3" y="4" width="18" height="18" rx="2" ry="2"/>
                    <line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/>
                    <line x1="3" y1="10" x2="21" y2="10"/>
                  </svg>
                  <span>
                    {fmtDate(nb.eeg_start)}
                    · {fmtTime(nb.eeg_start)} – {fmtTime(nb.eeg_end)}
                    <span class="text-muted-foreground/30 ml-1">({fmtDuration(nb.eeg_start, nb.eeg_end)})</span>
                  </span>
                </div>

                <!-- Bottom row: EEG metrics + open session -->
                <div class="flex items-end gap-2">
                  {#if nb.eeg_metrics}
                    <div class="flex flex-wrap gap-x-2.5 gap-y-0.5 flex-1">
                      {#each metricChips(nb.eeg_metrics) as chip}
                        <span class="inline-flex items-center gap-0.5 text-[0.52rem] tabular-nums">
                          <span class="text-muted-foreground/40">{chip.l}</span>
                          <span class="font-bold" style="color:{chip.c}">{chip.v}</span>
                        </span>
                      {/each}
                    </div>
                  {:else}
                    <span class="flex-1"></span>
                  {/if}
                  <button onclick={() => openSessionForLabel(nb)}
                          class="shrink-0 text-[0.62rem] font-medium text-primary/60 hover:text-primary
                                 transition-colors px-2 py-1 rounded border border-transparent
                                 hover:border-primary/20 hover:bg-primary/5">
                    {t("search.openSession")} ↗
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>

        <!-- Pagination -->
        {#if textTotalPages > 1}
          <div class="sticky bottom-0 flex items-center justify-between px-4 py-2
                      border-t border-border dark:border-white/[0.06]
                      bg-background/95 backdrop-blur-sm">
            <Button variant="outline" size="sm" class="h-7 px-3 text-[0.68rem]"
                    disabled={page===0} onclick={() => page--}>{t("search.prev")}</Button>
            <span class="text-[0.62rem] text-muted-foreground/55 tabular-nums">
              {t("search.pageOf", { page: page+1, total: textTotalPages })}
            </span>
            <Button variant="outline" size="sm" class="h-7 px-3 text-[0.68rem]"
                    disabled={page>=textTotalPages-1} onclick={() => page++}>{t("search.next")}</Button>
          </div>
        {/if}
      {/if}

    <!-- ══════════════════ IMAGES MODE ═══════════════════════════════════ -->
    {:else if mode === "images"}
      {#if !imgSearched && !imgSearching}
        <div class="flex flex-col items-center justify-center h-full gap-4 text-center px-10">
          <div class="w-16 h-16 rounded-2xl bg-primary/8 flex items-center justify-center">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
                 class="w-8 h-8 text-primary/40">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
              <circle cx="8.5" cy="8.5" r="1.5"/><path d="m21 15-5-5L5 21"/>
            </svg>
          </div>
          <p class="text-[0.8rem] text-muted-foreground/50 max-w-[300px] leading-relaxed">
            {t("search.imageEmptyState")}
          </p>
        </div>
      {:else if imgSearching}
        <div class="flex items-center justify-center h-full gap-2">
          <Spinner size="w-4 h-4" />
          <span class="text-[0.72rem] text-muted-foreground">{t("search.searching")}</span>
        </div>
      {:else if imgResults.length === 0}
        <div class="flex flex-col items-center justify-center h-full gap-3 text-center px-10">
          <span class="text-3xl opacity-30">🔍</span>
          <p class="text-[0.78rem] text-muted-foreground/50">{t("search.imageNoResults")}</p>
        </div>
      {:else}
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 p-4">
          {#each imgResults as r}
            <div class="rounded-xl border border-border dark:border-white/[0.06]
                        bg-white dark:bg-[#14141e] overflow-hidden shadow-sm
                        hover:shadow-md transition-shadow">
              <!-- Thumbnail -->
              {#if r.filename}
                <img src={imgSrc(r.filename)} alt="Screenshot"
                     class="w-full h-auto max-h-48 object-cover bg-black/5 dark:bg-white/[0.02]"
                     loading="lazy" />
              {/if}
              <!-- Metadata -->
              <div class="px-3 py-2 flex flex-col gap-1">
                <div class="flex items-center gap-2">
                  <span class="text-[0.66rem] font-semibold text-foreground truncate">
                    {r.app_name || '—'}
                  </span>
                  {#if r.similarity > 0}
                    <span class="rounded-full px-1.5 py-0 text-[0.48rem] font-semibold
                                 bg-primary/15 text-primary border border-primary/25 shrink-0">
                      {(r.similarity * 100).toFixed(0)}%
                    </span>
                  {/if}
                  <span class="ml-auto text-[0.5rem] text-muted-foreground/40 tabular-nums shrink-0">
                    {fmtDateTimeLocale(r.unix_ts)}
                  </span>
                </div>
                {#if r.window_title}
                  <span class="text-[0.58rem] text-muted-foreground truncate">{r.window_title}</span>
                {/if}
                {#if r.ocr_text}
                  <p class="text-[0.54rem] text-foreground/60 leading-relaxed
                            whitespace-pre-wrap break-words max-h-24 overflow-y-auto
                            rounded bg-muted/40 dark:bg-white/[0.03] px-2 py-1.5 mt-0.5
                            font-mono">
                    {r.ocr_text.length > 400 ? r.ocr_text.slice(0, 400) + '…' : r.ocr_text}
                  </p>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}

  </div>

  <DisclaimerFooter />
</main>

<style>
  input[type="datetime-local"]::-webkit-calendar-picker-indicator { opacity: 0.4; cursor: pointer; }
</style>
