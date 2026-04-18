<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onMount, untrack } from "svelte";
import { afterNavigate, replaceState } from "$app/navigation";
import { parseAssistantOutput } from "$lib/chat-utils";
import { generateUmapPlaceholder } from "$lib/compare-types";
import { Badge } from "$lib/components/ui/badge";
import { Button } from "$lib/components/ui/button";
import { Spinner } from "$lib/components/ui/spinner";
import {
  JOB_POLL_INTERVAL_MS,
  SEARCH_PAGE_SIZE,
  UMAP_COLOR_A,
  UMAP_COLOR_B,
  UMAP_POLL_INTERVAL_MS,
} from "$lib/constants";
import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { onDaemonEvent } from "$lib/daemon/ws";
import {
  dateToCompactKey,
  fmtDate,
  fmtDateTimeSecs as fmtDateTime,
  fmtDateTimeLocale,
  fmtDateTimeLocalInput,
  fmtDuration as fmtDurationSecs,
  fmtSecs,
  fmtTime,
  fmtUtcDay,
  fromUnix,
  pad,
  parseDateTimeLocalInput,
} from "$lib/format";
import InteractiveGraph3D from "$lib/InteractiveGraph3D.svelte";
import { t } from "$lib/i18n/index.svelte";
import MarkdownRenderer from "$lib/MarkdownRenderer.svelte";
import {
  buildTextKnnGraph,
  computeIxTimeHeatmap,
  computeSearchAnalysis,
  computeTemporalHeatmap,
  DAY_NAMES,
  dedupeFoundLabels,
  distColor,
  type GraphEdge,
  type GraphNode,
  heatColor,
  type ImgResult,
  ixHeatColor,
  type JobPollResult,
  type JobTicket,
  type LabelEntry,
  type LabelNeighbor,
  metricChips,
  type NeighborEntry,
  type NeighborMetrics,
  PRESETS,
  type QueryEntry,
  type SearchAnalysis,
  type SearchResult,
  simPct,
  simWidth,
  turboColor,
} from "$lib/search-types";
import { getAppName } from "$lib/stores/app-name.svelte";
import { useWindowTitle } from "$lib/stores/window-title.svelte";
import type { UmapPoint, UmapResult } from "$lib/types";
import UmapViewer3D from "$lib/UmapViewer3D.svelte";

// ── Formatting helpers ───────────────────────────────────────────────────
function toInputValue(d: Date) {
  return fmtDateTimeLocalInput(Math.floor(d.getTime() / 1000));
}
function fromInputValue(s: string) {
  return parseDateTimeLocalInput(s);
}
function fmtDuration(s: number, e: number) {
  return fmtDurationSecs(e - s);
}

// Analysis chip rows (i18n labels)
function analysisChips(sa: SearchAnalysis): Array<[string, string, string]> {
  return [
    [t("search.analysisNeighbors"), sa.totalNeighbors.toString(), ""],
    [t("search.analysisDistMin"), sa.distMin.toFixed(4), "text-emerald-500"],
    [t("search.analysisMeanSd"), `${sa.distMean.toFixed(4)} ± ${sa.distStddev.toFixed(4)}`, ""],
    [t("search.analysisDistMax"), sa.distMax.toFixed(4), "text-red-400"],
    [t("search.analysisPeakHour"), `${String(sa.peakHour).padStart(2, "0")}:00`, ""],
  ];
}

// ── Corpus-wide stats (streamed: fast fields arrive first, slow fields follow)
interface CorpusStats {
  // Tier 1+2 (fast, <50ms)
  eeg_days: number;
  eeg_first_day: string;
  eeg_last_day: string;
  label_total: number;
  label_text_index: number;
  label_eeg_index: number;
  label_embed_model: string;
  screenshot_total: number;
  screenshot_embedded: number;
  // Tier 3 (slow, streamed later)
  eeg_total_sessions?: number;
  eeg_total_secs?: number;
  label_stale?: number;
  eeg_total_epochs?: number;
  eeg_embedded_epochs?: number;
  eeg_missing_epochs?: number;
}
let corpusStats = $state<CorpusStats | null>(null);

// ── Mode ─────────────────────────────────────────────────────────────────
type SearchMode = "eeg" | "text" | "interactive" | "images";
let mode = $state<SearchMode>("interactive");
const SEARCH_MODE_EVENT = "skill:search-mode";
const SEARCH_SET_MODE_EVENT = "skill:search-set-mode";

function normalizeSearchMode(value: unknown): SearchMode {
  return value === "eeg" || value === "text" || value === "interactive" || value === "images" ? value : "interactive";
}

function emitSearchMode(value: SearchMode) {
  window.dispatchEvent(new CustomEvent(SEARCH_MODE_EVENT, { detail: { mode: value } }));
}

function switchMode(m: SearchMode) {
  mode = m;
  error = "";
  page = 0;
}

onMount(() => {
  const onTitlebarSetMode = (event: Event) => {
    const next = normalizeSearchMode((event as CustomEvent<{ mode?: unknown }>).detail?.mode);
    switchMode(next);
  };

  window.addEventListener(SEARCH_SET_MODE_EVENT, onTitlebarSetMode as EventListener);

  const initialMode = normalizeSearchMode(new URLSearchParams(window.location.search).get("mode"));
  switchMode(initialMode);
  emitSearchMode(initialMode);

  // Load screenshot server port + auth token for image URLs
  import("$lib/daemon/http")
    .then(({ getDaemonBaseUrl }) =>
      getDaemonBaseUrl().then(({ port, token }) => {
        imgPort = port;
        imgToken = token;
      }),
    )
    .catch(() => {});

  // Stream corpus stats: fast tier arrives instantly, slow tier follows
  import("$lib/daemon/http")
    .then(({ getDaemonBaseUrl }) =>
      getDaemonBaseUrl().then(({ port, token }) => {
        const es = new EventSource(
          `http://127.0.0.1:${port}/v1/search/stats/stream?token=${encodeURIComponent(token)}`,
        );
        es.addEventListener("fast", (e) => {
          try {
            corpusStats = JSON.parse(e.data);
          } catch {}
        });
        es.addEventListener("slow", (e) => {
          try {
            const slow = JSON.parse(e.data);
            if (corpusStats) {
              corpusStats = { ...corpusStats, ...slow };
            }
          } catch {}
          es.close();
        });
        es.onerror = () => {
          es.close();
          // Fallback to non-streaming endpoint
          daemonInvoke<CorpusStats>("search_corpus_stats", {})
            .then((s) => {
              corpusStats = s;
            })
            .catch(() => {});
        };
      }),
    )
    .catch(() => {});

  // Fire-and-forget: backfill metrics_json for old embeddings from CSV data.
  daemonInvoke("backfill_eeg_metrics", {}).catch(() => {});

  // Load available devices for the filter dropdown.
  daemonInvoke<{ devices: string[] }>("list_search_devices")
    .then((r) => {
      deviceList = r.devices;
    })
    .catch(() => {});

  // Listen for reembed progress → auto-refresh corpus stats when done.
  const unlistenReembed = onDaemonEvent("reembed-progress", (ev) => {
    const s = (ev.payload as { status?: string }).status ?? "";
    if (s === "done" || s === "idle_done" || s === "complete") {
      // Refresh corpus stats after reembed completes.
      daemonInvoke<CorpusStats>("search_corpus_stats", {})
        .then((fresh) => {
          corpusStats = fresh;
        })
        .catch(() => {});
    }
  });

  return () => {
    window.removeEventListener(SEARCH_SET_MODE_EVENT, onTitlebarSetMode as EventListener);
    unlistenReembed();
  };
});

let routerReady = $state(false);
afterNavigate(() => { routerReady = true; });

$effect(() => {
  const currentMode = mode;
  const ready = routerReady;
  untrack(() => {
    if (ready) {
      const params = new URLSearchParams(window.location.search);
      if (params.get("mode") !== currentMode) {
        params.set("mode", currentMode);
        const next = `${window.location.pathname}?${params.toString()}${window.location.hash}`;
        replaceState(next, {});
      }
    }
    emitSearchMode(currentMode);
  });
});

// ── Shared ───────────────────────────────────────────────────────────────
let kVal = $state(10);
let error = $state("");
let page = $state(0);
let rebuildingLabelIndex = $state(false);

// ── EEG mode ─────────────────────────────────────────────────────────────
const now = new Date();
const ago5 = new Date(now.getTime() - 5 * 60_000);
let startInput = $state(toInputValue(ago5));
let endInput = $state(toInputValue(now));
let efVal = $state(50);
let searching = $state(false);
let searchCancelled = $state(false);
let searchStatus = $state("");
let searchElapsed = $state(0);
let streamTotal = $state(0);
let streamDone = $state(0);
let streamDays = $state<string[]>([]);
let labelFilter = $state("");
let labelsOnly = $state(false);
let deviceFilter = $state("all");
let deviceList = $state<string[]>([]);
let result = $state<SearchResult | null>(null);
let showAnalysis = $state(true);

function applyPreset(mins: number) {
  const e = new Date();
  const s = new Date(e.getTime() - mins * 60_000);
  startInput = toInputValue(s);
  endInput = toInputValue(e);
}

const filtered = $derived.by(() => {
  if (!result?.results) return [];
  // During streaming, skip filtering to avoid O(n²) on every incoming result.
  if (searching) return result.results;
  let entries = result.results;
  const lf = labelFilter.trim().toLowerCase();
  if (lf || labelsOnly) {
    entries = entries
      .map((q) => {
        const matched = (q.neighbors ?? []).filter((nb) => {
          if ((nb.labels?.length ?? 0) === 0 && labelsOnly) return false;
          if (!lf) return true;
          return (nb.labels ?? []).some((l) => (l.text ?? "").toLowerCase().includes(lf));
        });
        return matched.length > 0 ? { ...q, neighbors: matched } : null;
      })
      .filter((q): q is QueryEntry => q !== null);
  }
  return entries;
});
const totalPages = $derived(Math.ceil(filtered.length / SEARCH_PAGE_SIZE) || 0);
const pageSlice = $derived(filtered.slice(page * SEARCH_PAGE_SIZE, (page + 1) * SEARCH_PAGE_SIZE));

// Only recompute analysis when not actively streaming (avoids 1750 re-computations).
const searchAnalysis = $derived.by(() => (result && !searching ? computeSearchAnalysis(result) : null));
const temporalHeatmap = $derived.by(() => (result && !searching ? computeTemporalHeatmap(result) : null));
const heatmapMax = $derived(temporalHeatmap ? Math.max(...temporalHeatmap.flat(), 1) : 1);

// UMAP
let umapResult = $state<UmapResult | null>(null);
let umapPlaceholder = $state<UmapResult | null>(null);
let umapLoading = $state(false);
let umapEta = $state("");
let umapElapsed = $state(0);
let umapTimer: ReturnType<typeof setInterval> | null = null;
let showUmap = $state(false);
let umapColorByDate = $state(false);

function fireUmap() {
  if (!result || result.results.length === 0) return;
  const allNb = result.results.flatMap((q) => q.neighbors);
  if (!allNb.length) return;
  umapResult = null;
  umapLoading = true;
  umapEta = "";
  const nbMin = Math.min(...allNb.map((n) => n.timestamp_unix));
  const nbMax = Math.max(...allNb.map((n) => n.timestamp_unix));
  umapPlaceholder = generateUmapPlaceholder(Math.min(result.query_count, 200), Math.min(allNb.length, 200));
  const t0 = performance.now();
  umapElapsed = 0;
  umapTimer = setInterval(() => {
    umapElapsed = Math.floor((performance.now() - t0) / 1000);
  }, 250);
  daemonInvoke<JobTicket>("enqueue_umap_compare", {
    aStartUtc: result.start_utc,
    aEndUtc: result.end_utc,
    bStartUtc: nbMin,
    bEndUtc: nbMax,
  })
    .then((ticket) => {
      umapEta =
        ticket.queue_position > 0 ? t("search.queued", { n: ticket.queue_position + 1 }) : t("search.computing3d");
      pollUmap(ticket.job_id);
    })
    .catch(() => finishUmap());
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
  for (const q of result.results ?? []) {
    for (const nb of q.neighbors ?? []) {
      if ((nb.labels?.length ?? 0) > 0 && !map.has(nb.timestamp_unix)) {
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
  const points = raw.points.map((pt) =>
    // biome-ignore lint/style/noNonNullAssertion: labelMap.has() guard above ensures get() returns a value
    !pt.label && labelMap.has(pt.utc) ? { ...pt, label: labelMap.get(pt.utc)! } : pt,
  );
  return { ...raw, points };
}

async function pollUmap(jobId: number) {
  while (true) {
    await new Promise((r) => setTimeout(r, UMAP_POLL_INTERVAL_MS));
    try {
      const r = await daemonInvoke<JobPollResult>("poll_job", { jobId });
      if (r.status === "complete") {
        const res = r.result as UmapResult | undefined;
        let raw: UmapResult | null = res?.points?.length ? res : null;
        if (raw) raw = enrichUmapLabels(raw, buildNeighborLabelMap());
        umapResult = raw;
        finishUmap();
        return;
      }
      if (r.status === "error" || r.status === "not_found") {
        finishUmap();
        return;
      }
      // biome-ignore lint/style/noNonNullAssertion: queue_position present when status === pending
      umapEta = r.queue_position! > 0 ? t("search.queued", { n: r.queue_position! + 1 }) : t("search.computing3d");
    } catch {
      finishUmap();
      return;
    }
  }
}
function finishUmap() {
  umapLoading = false;
  umapEta = "";
  if (umapTimer) {
    clearInterval(umapTimer);
    umapTimer = null;
  }
}

async function searchEeg() {
  const startUtc = fromInputValue(startInput);
  const endUtc = fromInputValue(endInput);
  if (Number.isNaN(startUtc) || Number.isNaN(endUtc) || endUtc <= startUtc) {
    error = t("search.endAfterStart");
    return;
  }
  searching = true;
  searchCancelled = false;
  error = "";
  result = null;
  page = 0;
  streamTotal = 0;
  streamDone = 0;
  streamDays = [];
  searchStatus = t("search.searching");
  searchElapsed = 0;
  const t0 = performance.now();
  const timer = setInterval(() => {
    searchElapsed = Math.floor((performance.now() - t0) / 1000);
  }, 500);
  let acc: SearchResult = {
    start_utc: startUtc,
    end_utc: endUtc,
    k: kVal,
    ef: efVal,
    query_count: 0,
    searched_days: [],
    results: [],
  };
  try {
    const ch = {
      onmessage: null as
        | ((msg: {
            kind: string;
            query_count?: number;
            searched_days?: string[];
            entry?: QueryEntry;
            done_count?: number;
            total?: number;
            error?: string;
          }) => void)
        | null,
    };
    ch.onmessage = (msg) => {
      if (searchCancelled) return;
      if (msg.kind === "started") {
        streamTotal = msg.query_count ?? 0;
        streamDays = msg.searched_days ?? [];
        acc.query_count = streamTotal;
        acc.searched_days = streamDays;
        searchStatus = t("search.searchingIndices");
      } else if (msg.kind === "result" && msg.entry) {
        streamDone = msg.done_count ?? streamDone + 1;
        // Ensure all neighbor fields are non-null for safe template rendering.
        const neighbors = (msg.entry.neighbors ?? []).map(
          (nb: NeighborEntry) =>
            ({
              ...nb,
              distance: nb.distance ?? 0,
              timestamp: nb.timestamp ?? 0,
              timestamp_unix: nb.timestamp_unix ?? 0,
              labels: nb.labels ?? [],
              date: nb.date ?? "",
              hnsw_id: nb.hnsw_id ?? 0,
              device_id: nb.device_id ?? null,
              device_name: nb.device_name ?? null,
            }) satisfies NeighborEntry,
        );
        const entry: QueryEntry = { ...msg.entry, neighbors };
        acc.results = [...acc.results, entry];
        result = { ...acc };
      } else if (msg.kind === "done") {
        streamDone = msg.total ?? streamDone;
      } else if (msg.kind === "error") {
        error = msg.error ?? "Unknown error";
      }
    };
    await daemonInvoke("stream_search_embeddings", {
      startUtc,
      endUtc,
      k: kVal || undefined,
      ef: efVal || undefined,
      deviceName: deviceFilter !== "all" ? deviceFilter : undefined,
      onProgress: ch,
    });
    result = { ...acc };
    // Mark "run a similarity search" onboarding step as done.
    try {
      const ob = JSON.parse(localStorage.getItem("onboardDone") ?? "{}");
      if (!ob.searchRun) {
        ob.searchRun = true;
        localStorage.setItem("onboardDone", JSON.stringify(ob));
      }
    } catch (e) {}
  } catch (e) {
    error = String(e);
  } finally {
    clearInterval(timer);
    searching = false;
    searchStatus = "";
  }
  if (result && result.results.length > 0) {
    showUmap = true;
    fireUmap();
  }
}

// ── Interactive mode ──────────────────────────────────────────────────────
let ixQuery = $state("");
// Pipeline params loaded from persisted settings (see SETTINGS_KEY below)
const _ps = (() => {
  try {
    return JSON.parse(localStorage.getItem("skill_search_settings") ?? "{}");
  } catch {
    return {};
  }
})();
let ixKText = $state((_ps.kText as number) ?? 5);
let ixKEeg = $state((_ps.kEeg as number) ?? 5);
let ixKLabels = $state((_ps.kLabels as number) ?? 3);
let ixReachMinutes = $state((_ps.reachMinutes as number) ?? 10);
let ixSearching = $state(false);
let ixSearched = $state(false);
let ixStatus = $state("");
let ixNodes = $state<GraphNode[]>([]);
let ixEdges = $state<GraphEdge[]>([]);
let ixDot = $state("");
let ixSvg = $state(""); // SVG with PCA scatter layout
let ixSvgCol = $state(""); // SVG with classic column layout
let showIxGraph = $state(true);
let showIxCard = $state(true); // single collapsible card: query + pipeline + button
// ── Persist pipeline settings ─────────────────────────────────────────────
const SETTINGS_KEY = "skill_search_settings";
function loadSettings(): Record<string, unknown> {
  try {
    return JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}");
  } catch {
    return {};
  }
}
function saveSetting(key: string, value: unknown) {
  const s = loadSettings();
  s[key] = value;
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(s));
}
const _s = loadSettings();

let ixDedupeLabels = $state((_s.dedupeLabels as boolean) ?? true);
let ixUsePca = $state((_s.usePca as boolean) ?? true);
let ixSnrPositiveOnly = $state((_s.snrPositiveOnly as boolean) ?? false);
let ixFilterStartUtc = $state<number | undefined>(_s.filterStartUtc as number | undefined);
let ixFilterEndUtc = $state<number | undefined>(_s.filterEndUtc as number | undefined);
let ixEegRankBy = $state((_s.eegRankBy as string) ?? "timestamp");
let ixShowAdvanced = $state((_s.showAdvanced as boolean) ?? false);

// Auto-persist settings on change
$effect(() => {
  saveSetting("dedupeLabels", ixDedupeLabels);
  saveSetting("usePca", ixUsePca);
  saveSetting("snrPositiveOnly", ixSnrPositiveOnly);
  saveSetting("filterStartUtc", ixFilterStartUtc);
  saveSetting("filterEndUtc", ixFilterEndUtc);
  saveSetting("eegRankBy", ixEegRankBy);
  saveSetting("showAdvanced", ixShowAdvanced);
  saveSetting("kText", ixKText);
  saveSetting("kEeg", ixKEeg);
  saveSetting("kLabels", ixKLabels);
  saveSetting("reachMinutes", ixReachMinutes);
});
let ixPerf = $state<{
  embed_ms?: number;
  graph_ms?: number;
  total_ms?: number;
  node_count?: number;
  edge_count?: number;
  cpu_usage_pct?: number;
  mem_used_mb?: number;
  mem_total_mb?: number;
} | null>(null);
let ixSessions = $state<
  Array<{
    session_id: string;
    epoch_count: number;
    duration_secs: number;
    best: boolean;
    avg_engagement: number;
    avg_snr: number;
    avg_relaxation: number;
    stddev_engagement: number;
  }>
>([]);
let ixSelectedNode = $state<import("$lib/search-types").GraphNode | null>(null); // selected node for detail panel
let ixSortByRelevance = $state(false); // sort displayed nodes by relevance_score
let ixHiddenKinds = $state<import("$lib/search-types").GraphNode["kind"][]>([]); // node kind filter for 3D graph
let ixColorMode = $state<"timestamp" | "engagement" | "snr" | "session">("timestamp");
let ixCompareNode = $state<import("$lib/search-types").GraphNode | null>(null); // second node for compare mode
let ixDetailTimeseries = $state<
  Array<{ t: number; ra: number; rb: number; rt: number; engagement: number; relaxation: number; snr: number }>
>([]);

// ── Insights & value extraction ──────────────────────────────────────────
let ixLlmSummary = $state("");
let ixLlmPrompt = $state("");
let ixLlmLoading = $state(false);
let ixLlmMaxTokens = $state(2048);
let ixLlmSessionId = $state(0);
let ixLlmPhase = $state<"" | "text" | "vision-loading" | "vision-analyzing" | "done">("");
let ixLlmScreenshots = $state<Array<{ url: string; label: string }>>([]);
let ixShowInsights = $state(false);

/** Save the completed AI summary to a chat session for "Continue in Chat". */
function saveSummaryToChat(summary: string) {
  const p = parseAssistantOutput(summary);
  daemonInvoke<{ id: number }>("new_chat_session")
    .then(async (res) => {
      const sid = res?.id ?? 0;
      if (sid > 0) {
        ixLlmSessionId = sid;
        await daemonInvoke("rename_chat_session", { id: sid, title: `Search: ${ixQuery}` }).catch(() => {});
        await daemonInvoke("save_chat_message", {
          sessionId: sid,
          role: "user",
          content: ixLlmPrompt,
          thinking: null,
        }).catch(() => {});
        await daemonInvoke("save_chat_message", {
          sessionId: sid,
          role: "assistant",
          content: [p.leadIn, p.content].filter((s) => s.trim()).join("\n\n"),
          thinking: p.thinking || null,
        }).catch(() => {});
      }
    })
    .catch(() => {});
}

// Bookmarks (persisted in localStorage)
const BOOKMARKS_KEY = "skill_search_bookmarks";
let ixBookmarks = $state<
  Array<{ query: string; nodeId: string; kind: string; text: string; timestamp?: number; savedAt: number }>
>([]);
try {
  ixBookmarks = JSON.parse(localStorage.getItem(BOOKMARKS_KEY) ?? "[]");
} catch {
  /* ignore */
}
function saveBookmark(node: GraphNode) {
  const entry = {
    query: ixQuery,
    nodeId: node.id,
    kind: node.kind,
    text: node.text ?? "",
    timestamp: node.timestamp_unix,
    savedAt: Date.now(),
  };
  ixBookmarks = [entry, ...ixBookmarks.filter((b) => b.nodeId !== node.id)].slice(0, 50);
  localStorage.setItem(BOOKMARKS_KEY, JSON.stringify(ixBookmarks));
}
function removeBookmark(nodeId: string) {
  ixBookmarks = ixBookmarks.filter((b) => b.nodeId !== nodeId);
  localStorage.setItem(BOOKMARKS_KEY, JSON.stringify(ixBookmarks));
}

// Computed insights from search results
interface AppEngagement {
  app: string;
  avgEngagement: number;
  count: number;
}
interface HourEngagement {
  hour: number;
  avgEngagement: number;
  count: number;
}

function computeInsights(nodes: GraphNode[]): {
  appCorrelation: AppEngagement[];
  hourPattern: HourEngagement[];
  bestConditions: string[];
  topMetric: { key: string; value: number } | null;
} {
  const eegNodes = nodes.filter((n) => n.kind === "eeg_point" && n.eeg_metrics);
  const ssNodes = nodes.filter((n) => n.kind === "screenshot" && (n.app_name || n.window_title));

  // App-engagement correlation: match screenshots to nearby EEG by timestamp
  const appMap = new Map<string, { sum: number; count: number }>();
  for (const ss of ssNodes) {
    const appName = ss.app_name || ss.window_title;
    if (!appName || !ss.timestamp_unix) continue;
    // Find nearest EEG epoch
    const nearest = eegNodes
      .filter((n) => n.timestamp_unix)
      .sort(
        (a, b) =>
          Math.abs((a.timestamp_unix ?? 0) - (ss.timestamp_unix ?? 0)) -
          Math.abs((b.timestamp_unix ?? 0) - (ss.timestamp_unix ?? 0)),
      )[0];
    if (nearest?.eeg_metrics?.engagement != null) {
      const eng = nearest.eeg_metrics.engagement as number;
      const entry = appMap.get(appName) ?? { sum: 0, count: 0 };
      entry.sum += eng;
      entry.count++;
      appMap.set(appName, entry);
    }
  }
  const appCorrelation: AppEngagement[] = [...appMap.entries()]
    .map(([app, { sum, count }]) => ({ app, avgEngagement: sum / count, count }))
    .sort((a, b) => b.avgEngagement - a.avgEngagement);

  // Hour-of-day engagement pattern
  const hourMap = new Map<number, { sum: number; count: number }>();
  for (const n of eegNodes) {
    if (!n.timestamp_unix || !n.eeg_metrics?.engagement) continue;
    const hour = new Date(n.timestamp_unix * 1000).getHours();
    const entry = hourMap.get(hour) ?? { sum: 0, count: 0 };
    entry.sum += n.eeg_metrics.engagement as number;
    entry.count++;
    hourMap.set(hour, entry);
  }
  const hourPattern: HourEngagement[] = [...hourMap.entries()]
    .map(([hour, { sum, count }]) => ({ hour, avgEngagement: sum / count, count }))
    .sort((a, b) => a.hour - b.hour);

  // Best conditions
  const bestConditions: string[] = [];
  const bestHour = hourPattern.sort((a, b) => b.avgEngagement - a.avgEngagement)[0];
  if (bestHour) bestConditions.push(`Peak engagement at ${bestHour.hour}:00 (${bestHour.avgEngagement.toFixed(2)})`);
  const bestApp = appCorrelation[0];
  if (bestApp) bestConditions.push(`Highest engagement in ${bestApp.app} (${bestApp.avgEngagement.toFixed(2)})`);
  // Re-sort hourPattern by hour for display
  hourPattern.sort((a, b) => a.hour - b.hour);

  // Top metric across all EEG nodes
  let topMetric: { key: string; value: number } | null = null;
  if (eegNodes.length > 0) {
    let maxEng = 0;
    for (const n of eegNodes) {
      const eng = (n.eeg_metrics?.engagement as number) ?? 0;
      if (eng > maxEng) {
        maxEng = eng;
        topMetric = { key: "peak engagement", value: eng };
      }
    }
  }

  return { appCorrelation, hourPattern, bestConditions, topMetric };
}

// ── Search history (persisted in localStorage) ──────────────────────────
const HISTORY_KEY = "skill_search_history";
const MAX_HISTORY = 10;
let ixSearchHistory = $state<string[]>([]);
try {
  ixSearchHistory = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? "[]");
} catch {
  /* ignore */
}
function saveToHistory(q: string) {
  if (!q.trim()) return;
  ixSearchHistory = [q.trim(), ...ixSearchHistory.filter((h) => h !== q.trim())].slice(0, MAX_HISTORY);
  localStorage.setItem(HISTORY_KEY, JSON.stringify(ixSearchHistory));
}

let ixShowScreenshots = $state(false); // show screenshot thumbnails on EEG nodes
/**
 * Screenshot results.  Keys are `"parentNodeId_filename"`, values carry
 * the screenshot data.  The parentNodeId portion tells us which graph
 * node the screenshot should be connected to (query, text_label, or
 * eeg_point).
 */
let ixScreenshotMap = $state<
  Map<string, { filename: string; appName: string; windowTitle: string; unixTs: number; similarity: number }>
>(new Map());

// ── Derived display graph (applies found-label deduplication) ────────────
const ixDisplayGraph = $derived.by(() => {
  let { nodes: n, edges: e } = ixDedupeLabels
    ? dedupeFoundLabels(ixNodes, ixEdges)
    : { nodes: ixNodes, edges: ixEdges };

  // Inject screenshot nodes + edges when the toggle is on and we have data
  if (ixShowScreenshots && ixScreenshotMap.size > 0) {
    const extraNodes: typeof n = [];
    const extraEdges: typeof e = [];
    // Collect valid parent IDs from current graph
    const nodeIds = new Set(n.map((nd) => nd.id));
    let idx = 0;
    for (const [key, ss] of ixScreenshotMap) {
      // key = "parentNodeId\0filename"
      const sepIdx = key.indexOf("\0");
      const parentId = sepIdx > 0 ? key.slice(0, sepIdx) : key;
      // Parent must exist in the current (possibly deduped) graph
      if (!nodeIds.has(parentId)) continue;
      const ssId = `ss_${idx++}_${ss.filename}`;
      extraNodes.push({
        id: ssId,
        kind: "screenshot",
        text: ss.appName || ss.windowTitle || "Screenshot",
        timestamp_unix: ss.unixTs,
        distance: ss.similarity,
        parent_id: parentId,
        screenshot_url: imgSrc(ss.filename),
        filename: ss.filename,
        app_name: ss.appName,
        window_title: ss.windowTitle,
        ocr_similarity: ss.similarity,
      });
      extraEdges.push({
        from_id: parentId,
        to_id: ssId,
        distance: ss.similarity,
        kind: "screenshot_link",
      });
    }
    n = [...n, ...extraNodes];
    e = [...e, ...extraEdges];
  }

  return { nodes: n, edges: e };
});

// Fetch screenshots reactively when the toggle flips or new results arrive
$effect(() => {
  const _show = ixShowScreenshots;
  const _len = ixNodes.length;
  if (ixSearched && _show && _len > 0) {
    fetchIxScreenshots();
  } else if (!_show) {
    ixScreenshotMap = new Map();
  }
});

async function searchInteractive() {
  if (!ixQuery.trim()) return;
  saveToHistory(ixQuery.trim());
  // Compress the controls so the results panel gets maximum space
  showIxCard = false;
  ixSelectedNode = null;
  ixSearching = true;
  ixSearched = false;
  error = "";
  ixNodes = [];
  ixEdges = [];
  ixDot = "";
  ixSvg = "";
  ixSvgCol = "";
  dotSavedPath = "";
  svgSavedPath = "";
  svgError = "";
  ixStatus = t("search.interactiveStep1");
  try {
    const res = await daemonInvoke<{
      nodes: GraphNode[];
      edges: GraphEdge[];
      dot: string;
      svg: string;
      svg_col: string;
      reembed_needed?: { stale: number; total: number; current_model: string };
      perf?: {
        embed_ms: number;
        graph_ms: number;
        total_ms: number;
        node_count: number;
        edge_count: number;
        cpu_usage_pct: number;
        mem_used_mb: number;
        mem_total_mb: number;
      };
      sessions?: Array<{
        session_id: string;
        epoch_count: number;
        duration_secs: number;
        best: boolean;
        avg_engagement: number;
        avg_snr: number;
        avg_relaxation: number;
        stddev_engagement: number;
      }>;
    }>("interactive_search", {
      query: ixQuery.trim(),
      kText: ixKText,
      kEeg: ixKEeg,
      kLabels: ixKLabels,
      kScreenshots: 5,
      reachMinutes: ixReachMinutes,
      snrPositiveOnly: ixSnrPositiveOnly,
      deviceName: deviceFilter !== "all" ? deviceFilter : undefined,
      filterStartUtc: ixFilterStartUtc,
      filterEndUtc: ixFilterEndUtc,
      eegRankBy: ixEegRankBy !== "timestamp" ? ixEegRankBy : undefined,
      usePca: ixUsePca,
      svgLabels: {
        layerQuery: t("svg.layerQuery"),
        layerTextMatches: t("svg.layerTextMatches"),
        layerEegNeighbors: t("svg.layerEegNeighbors"),
        layerFoundLabels: t("svg.layerFoundLabels"),
        legendQuery: t("svg.legendQuery"),
        legendText: t("svg.legendText"),
        legendEeg: t("svg.legendEeg"),
        legendFound: t("svg.legendFound"),
        generatedBy: t("svg.generatedBy", { app: getAppName() }),
      },
    });
    ixNodes = res.nodes ?? [];
    ixEdges = res.edges ?? [];
    ixDot = res.dot;
    ixSvg = res.svg;
    ixSvgCol = res.svg_col;
    ixPerf = res.perf ?? null;
    ixSessions = res.sessions ?? [];
    ixSearched = true;

    // Auto re-embed stale labels and retry the search transparently
    if (res.reembed_needed && res.reembed_needed.stale > 0) {
      ixStatus = t("search.reembedAuto", {
        stale: String(res.reembed_needed.stale),
        total: String(res.reembed_needed.total),
      });
      ixSearching = true;
      ixSearched = false;
      try {
        const reembedResult = await daemonInvoke<{ ok: boolean }>("reembed_labels");
        if (reembedResult.ok) {
          ixStatus = t("search.interactiveStep1");
          const retry = await daemonInvoke<typeof res>("interactive_search", {
            query: ixQuery.trim(),
            kText: ixKText,
            kEeg: ixKEeg,
            kLabels: ixKLabels,
            reachMinutes: ixReachMinutes,
            usePca: ixUsePca,
            svgLabels: {
              layerQuery: t("svg.layerQuery"),
              layerTextMatches: t("svg.layerTextMatches"),
              layerEegNeighbors: t("svg.layerEegNeighbors"),
              layerFoundLabels: t("svg.layerFoundLabels"),
              legendQuery: t("svg.legendQuery"),
              legendText: t("svg.legendText"),
              legendEeg: t("svg.legendEeg"),
              legendFound: t("svg.legendFound"),
              generatedBy: t("svg.generatedBy", { app: getAppName() }),
            },
          });
          ixNodes = retry.nodes ?? [];
          ixEdges = retry.edges ?? [];
          ixDot = retry.dot;
          ixSvg = retry.svg;
          ixSvgCol = retry.svg_col;
        }
      } catch (_) {
        // Re-embed failed silently — show whatever we had
      }
      ixSearched = true;
      ixSearching = false;
      ixStatus = "";
    }
  } catch (e) {
    error = String(e);
  } finally {
    ixSearching = false;
    ixStatus = "";
  }
}

type SsEntry = { filename: string; appName: string; windowTitle: string; unixTs: number; similarity: number };

/**
 * Multi-strategy screenshot fetch:
 *  1. Semantic text search with the query → attach to "query" node
 *  2. Semantic text search per text_label → attach to that label node
 *  3. Timestamp proximity (±30 min) per eeg_point → attach to that EEG node
 *
 * Deduplicates by filename so the same screenshot doesn't appear twice.
 */
async function fetchIxScreenshots() {
  if (!ixShowScreenshots) {
    ixScreenshotMap = new Map();
    return;
  }
  if (ixNodes.length === 0) {
    ixScreenshotMap = new Map();
    return;
  }

  type SsResult = { unix_ts: number; filename: string; app_name: string; window_title: string; similarity: number };
  const results = new Map<string, SsEntry>();
  const usedFilenames = new Set<string>();

  function addResult(nodeId: string, r: SsResult) {
    if (usedFilenames.has(r.filename)) return;
    usedFilenames.add(r.filename);
    results.set(`${nodeId}\0${r.filename}`, {
      filename: r.filename,
      appName: r.app_name,
      windowTitle: r.window_title,
      unixTs: r.unix_ts,
      similarity: r.similarity ?? 0,
    });
  }

  const promises: Promise<void>[] = [];

  // Strategy 1: semantic search with the interactive query → attach to "query"
  promises.push(
    (async () => {
      try {
        const hits = await daemonInvoke<SsResult[]>("search_screenshots_by_text", {
          query: ixQuery.trim(),
          k: 5,
          mode: "semantic",
        });
        for (const h of hits) addResult("query", h);
      } catch {
        /* no semantic index or model */
      }
    })(),
  );

  // Strategy 2: semantic search per text_label text → attach to that label
  const textLabels = ixNodes.filter((n) => n.kind === "text_label" && n.text);
  for (const tl of textLabels.slice(0, 5)) {
    promises.push(
      (async () => {
        try {
          const hits = await daemonInvoke<SsResult[]>("search_screenshots_by_text", {
            // biome-ignore lint/style/noNonNullAssertion: text always set on label nodes
            query: tl.text!,
            k: 2,
            mode: "semantic",
          });
          for (const h of hits) addResult(tl.id, h);
        } catch {
          /* skip */
        }
      })(),
    );
  }

  // Strategy 3: timestamp proximity (±30 min) per eeg_point
  const eegNodes = ixNodes.filter((n) => n.kind === "eeg_point" && n.timestamp_unix != null);
  for (const node of eegNodes) {
    promises.push(
      (async () => {
        try {
          const around = await daemonInvoke<SsResult[]>("get_screenshots_around", {
            // biome-ignore lint/style/noNonNullAssertion: timestamp_unix always set on eeg nodes
            timestamp: Math.floor(node.timestamp_unix!),
            windowSecs: 1800,
          });
          if (around.length > 0) {
            // Pick the closest screenshot to the node timestamp
            const sorted = [...around].sort(
              // biome-ignore lint/style/noNonNullAssertion: timestamp_unix always set on eeg nodes
              (a, b) => Math.abs(a.unix_ts - node.timestamp_unix!) - Math.abs(b.unix_ts - node.timestamp_unix!),
            );
            addResult(node.id, { ...sorted[0], similarity: 0 });
          }
        } catch {
          /* skip */
        }
      })(),
    );
  }

  await Promise.all(promises);
  ixScreenshotMap = results;
}

function buildBreadcrumb(node: GraphNode, allNodes: GraphNode[]): GraphNode[] {
  const path: GraphNode[] = [];
  let cur: GraphNode | undefined = node;
  for (let i = 0; i < 10 && cur; i++) {
    // max 10 depth
    path.unshift(cur);
    const pid: string | undefined = cur.parent_id;
    cur = pid != null ? allNodes.find((n) => n.id === pid) : undefined;
  }
  return path;
}

function onIxKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    searchInteractive();
  }
  // Escape clears graph selection
  if (e.key === "Escape" && ixSelectedNode) {
    ixSelectedNode = null;
  }
  // Up arrow in empty textarea cycles through search history
  if (e.key === "ArrowUp" && !ixQuery.trim() && ixSearchHistory.length > 0) {
    e.preventDefault();
    ixQuery = ixSearchHistory[0];
  }
}

let dotSaving = $state(false);
let dotSavedPath = $state("");
let svgSaving = $state(false);
let svgSavedPath = $state("");
let svgError = $state("");

async function downloadDot() {
  if (!ixDot || dotSaving) return;
  dotSaving = true;
  dotSavedPath = "";
  try {
    let dotData = ixDot;
    const disp = ixDisplayGraph;
    const hasScreenshots = disp.nodes.some((n) => n.kind === "screenshot");
    if (hasScreenshots) {
      dotData = await daemonInvoke<string>("regenerate_interactive_dot", {
        nodes: disp.nodes.map((n) => ({
          id: n.id,
          kind: n.kind,
          text: n.text ?? null,
          timestamp_unix: n.timestamp_unix != null ? Math.floor(n.timestamp_unix) : null,
          distance: n.distance,
          eeg_metrics: n.eeg_metrics ?? null,
          parent_id: n.parent_id ?? null,
          proj_x: n.proj_x ?? null,
          proj_y: n.proj_y ?? null,
          filename: n.filename ?? null,
          app_name: n.app_name ?? null,
          window_title: n.window_title ?? null,
          ocr_text: null,
          ocr_similarity: n.ocr_similarity ?? null,
        })),
        edges: disp.edges.map((e) => ({
          from_id: e.from_id,
          to_id: e.to_id,
          distance: e.distance,
          kind: e.kind,
        })),
      });
    }
    dotSavedPath = await daemonInvoke<string>("save_dot_file", { dot: dotData, query: ixQuery.trim() });
  } catch (e) {
    error = String(e);
  } finally {
    dotSaving = false;
  }
}

async function downloadSvg() {
  svgSaving = true;
  svgSavedPath = "";
  svgError = "";
  try {
    let svgData: string;
    const disp = ixDisplayGraph;
    const hasScreenshots = disp.nodes.some((n) => n.kind === "screenshot");

    if (hasScreenshots) {
      // Re-generate SVG on the backend with screenshot nodes included
      svgData = await daemonInvoke<string>("regenerate_interactive_svg", {
        nodes: disp.nodes.map((n) => ({
          id: n.id,
          kind: n.kind,
          text: n.text ?? null,
          timestamp_unix: n.timestamp_unix != null ? Math.floor(n.timestamp_unix) : null,
          distance: n.distance,
          eeg_metrics: n.eeg_metrics ?? null,
          parent_id: n.parent_id ?? null,
          proj_x: n.proj_x ?? null,
          proj_y: n.proj_y ?? null,
          filename: n.filename ?? null,
          app_name: n.app_name ?? null,
          window_title: n.window_title ?? null,
          ocr_text: null,
          ocr_similarity: n.ocr_similarity ?? null,
        })),
        edges: disp.edges.map((e) => ({
          from_id: e.from_id,
          to_id: e.to_id,
          distance: e.distance,
          kind: e.kind,
        })),
        svgLabels: {
          layerQuery: t("svg.layerQuery"),
          layerTextMatches: t("svg.layerTextMatches"),
          layerEegNeighbors: t("svg.layerEegNeighbors"),
          layerFoundLabels: t("svg.layerFoundLabels"),
          legendQuery: t("svg.legendQuery"),
          legendText: t("svg.legendText"),
          legendEeg: t("svg.legendEeg"),
          legendFound: t("svg.legendFound"),
          generatedBy: t("svg.generatedBy", { app: getAppName() }),
        },
        usePca: ixUsePca,
      });
    } else {
      svgData = ixUsePca ? ixSvg : ixSvgCol;
    }

    if (!svgData) {
      svgError = "No SVG data";
      return;
    }
    svgSavedPath = await daemonInvoke<string>("save_svg_file", { svg: svgData, query: ixQuery.trim() });
  } catch (e) {
    svgError = String(e);
  } finally {
    svgSaving = false;
  }
}

// ── Interactive time heatmap (days × hours) ─────────────────────────────
const ixTimeHeatmap = $derived.by(() => computeIxTimeHeatmap(ixNodes));

async function openSession(nb: NeighborEntry) {
  try {
    const ref = await daemonInvoke<{ csv_path: string } | null>("find_session_for_timestamp", {
      timestampUnix: nb.timestamp_unix,
      date: nb.date,
    });
    await invoke(ref ? "open_session_window" : "open_history_window", ref ? { csvPath: ref.csv_path } : {});
  } catch {
    /* swallow */
  }
}

// ── Text mode ─────────────────────────────────────────────────────────────
let textQuery = $state("");
let textSearching = $state(false);
let textResults = $state<LabelNeighbor[]>([]);
let textSearched = $state(false);
let textFilter = $state("");
let textSort = $state<"sim" | "date">("sim");

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
  if (!textSearched || r.length === 0) {
    textGraphData = null;
    return;
  }
  textGraphData = buildTextKnnGraph(r, textQuery.trim());
});

async function searchText() {
  if (!textQuery.trim()) return;
  textSearching = true;
  error = "";
  textSearched = false;
  page = 0;
  textFilter = "";
  try {
    const res = await daemonInvoke<{ results: LabelNeighbor[] } | LabelNeighbor[]>("search_labels_by_text", {
      query: textQuery.trim(),
      k: kVal,
    });
    textResults = Array.isArray(res) ? res : (res?.results ?? []);
    textSearched = true;
  } catch (e) {
    error = String(e);
  } finally {
    textSearching = false;
  }
}

function onTextKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
    e.preventDefault();
    searchText();
  }
}

const textFiltered = $derived.by(() => {
  let r = textResults;
  const tf = textFilter.trim().toLowerCase();
  if (tf)
    r = r.filter((x) => (x.text ?? "").toLowerCase().includes(tf) || (x.context ?? "").toLowerCase().includes(tf));
  if (textSort === "date") return [...r].sort((a, b) => b.eeg_start - a.eeg_start);
  return r; // similarity order is already from backend
});
const textMaxDist = $derived(textFiltered.length > 0 ? Math.max(0.0001, ...textFiltered.map((r) => r.distance)) : 1);
const textPageSlice = $derived(textFiltered.slice(page * SEARCH_PAGE_SIZE, (page + 1) * SEARCH_PAGE_SIZE));
const textTotalPages = $derived(Math.ceil(textFiltered.length / SEARCH_PAGE_SIZE) || 0);

async function openSessionForLabel(nb: LabelNeighbor) {
  try {
    const dateStr = dateToCompactKey(fromUnix(nb.eeg_start));
    const ref = await daemonInvoke<{ csv_path: string } | null>("find_session_for_timestamp", {
      timestampUnix: nb.eeg_start,
      date: dateStr,
    });
    await invoke(ref ? "open_session_window" : "open_history_window", ref ? { csvPath: ref.csv_path } : {});
  } catch {
    /* swallow */
  }
}

// ── Images mode (screenshot OCR search) ──────────────────────────────────
let imgQuery = $state("");
let imgResults = $state<ImgResult[]>([]);
let imgSearching = $state(false);
let imgSearched = $state(false);
let imgSearchMode = $state<"substring" | "semantic">("substring");
let imgPort = $state(8375);
let imgToken = $state("");

function imgSrc(filename: string): string {
  if (!filename) return "";
  const tokenParam = imgToken ? `?token=${encodeURIComponent(imgToken)}` : "";
  return `http://127.0.0.1:${imgPort}/screenshots/${filename}${tokenParam}`;
}

async function searchImages() {
  if (!imgQuery.trim()) return;
  imgSearching = true;
  imgSearched = false;
  try {
    imgResults = await daemonInvoke<ImgResult[]>("search_screenshots_by_text", {
      query: imgQuery.trim(),
      k: 20,
      mode: imgSearchMode,
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
        {#each PRESETS as [label, mins] (mins)}
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
                 aria-label="Search start time"
                 class="rounded border border-border dark:border-white/[0.1]
                        bg-background px-2 py-1 text-[0.7rem]
                        focus:outline-none focus:ring-1 focus:ring-ring" />
          <span class="text-muted-foreground/40 text-[0.65rem] select-none">→</span>
          <input type="datetime-local" step="1" bind:value={endInput}
                 aria-label="Search end time"
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
                 aria-label="k nearest neighbors"
                 class="w-14 rounded border border-border dark:border-white/[0.1]
                        bg-background px-1.5 py-1 text-[0.72rem] text-center
                        focus:outline-none focus:ring-1 focus:ring-ring" />
        </div>
        <div class="flex items-center gap-1.5">
          <span class="text-[0.6rem] text-muted-foreground/60 font-mono select-none">ef</span>
          <input type="number" min="10" max="500" bind:value={efVal}
                 aria-label="ef search parameter"
                 class="w-16 rounded border border-border dark:border-white/[0.1]
                        bg-background px-1.5 py-1 text-[0.72rem] text-center
                        focus:outline-none focus:ring-1 focus:ring-ring" />
        </div>
        {#if deviceList.length > 0}
          <select bind:value={deviceFilter}
                  aria-label="Filter by device"
                  class="rounded border border-border dark:border-white/[0.1]
                         bg-background px-1.5 py-1 text-[0.68rem]
                         focus:outline-none focus:ring-1 focus:ring-ring max-w-[10rem] truncate">
            <option value="all">{t("search.allDevices")}</option>
            {#each deviceList as dev}
              <option value={dev}>{dev}</option>
            {/each}
          </select>
        {/if}
        {#if error}
          <span class="text-[0.62rem] text-destructive flex-1 truncate" title={error}>{error}</span>
        {:else}
          <span class="flex-1"></span>
        {/if}
        {#if searching}
          <Button onclick={() => {
            searchCancelled = true; searching = false; searchStatus = "";
            import("$lib/daemon/invoke-proxy").then(m => m.cancelSearch());
          }}
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
                aria-expanded={showIxCard}
                aria-label={t("search.interactiveQueryLabel")}
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
                      aria-label="Interactive search query"
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
                        aria-label="Decrease {step.title}"
                        class="w-5 h-5 rounded flex items-center justify-center text-[0.7rem] font-bold
                               text-muted-foreground/50 hover:text-foreground hover:bg-muted/40
                               transition-colors select-none">−</button>
                <input type="number" min={step.min} max={step.max} value={step.val}
                       aria-label="{step.title} value"
                       oninput={(e) => step.set(Number((e.target as HTMLInputElement).value))}
                       class="w-9 rounded border border-border dark:border-white/[0.1]
                              bg-background px-0.5 py-0.5 text-[0.68rem] text-center font-mono
                              focus:outline-none focus:ring-1 focus:ring-ring" />
                <button onclick={() => step.set(Math.min(step.max, step.val + 1))}
                        aria-label="Increase {step.title}"
                        class="w-5 h-5 rounded flex items-center justify-center text-[0.7rem] font-bold
                               text-muted-foreground/50 hover:text-foreground hover:bg-muted/40
                               transition-colors select-none">+</button>
              </div>
            </div>
          {/each}

          <!-- Advanced filters toggle -->
          <button onclick={() => ixShowAdvanced = !ixShowAdvanced}
                  class="flex items-center gap-1.5 w-full px-3 py-1
                         border-t border-border dark:border-white/[0.05]
                         text-[0.5rem] text-muted-foreground/40 hover:text-muted-foreground/70
                         transition-colors select-none">
            <span>{ixShowAdvanced ? "▾" : "▸"}</span>
            <span class="uppercase tracking-widest font-semibold">{t("search.advancedFilters")}</span>
            {#if ixSnrPositiveOnly || deviceFilter !== "all" || ixFilterStartUtc || ixEegRankBy !== "timestamp"}
              <span class="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0" title="Filters active"></span>
            {/if}
          </button>

          {#if ixShowAdvanced}
          <!-- SNR filter toggle -->
          <div class="flex items-center gap-2 px-3 py-1.5
                      border-t border-border dark:border-white/[0.05]
                      hover:bg-muted/10 transition-colors">
            <span class="w-3.5 h-3.5 rounded-full flex items-center justify-center
                         text-[0.42rem] font-bold text-white shrink-0 select-none"
                  style="background:#ef4444">6</span>
            <label for="snr-toggle" class="text-[0.6rem] font-medium text-foreground/70 shrink-0 cursor-pointer">{t("search.snrFilterLabel")}</label>
            <span class="text-[0.52rem] text-muted-foreground/40 flex-1 min-w-0 truncate">{t("search.snrFilterHint")}</span>
            <input id="snr-toggle" type="checkbox" bind:checked={ixSnrPositiveOnly}
                   class="w-3.5 h-3.5 rounded border-border accent-emerald-600 cursor-pointer shrink-0" />
          </div>

          <!-- Device filter -->
          {#if deviceList.length > 0}
            <div class="flex items-center gap-2 px-3 py-1.5
                        border-t border-border dark:border-white/[0.05]
                        hover:bg-muted/10 transition-colors">
              <span class="w-3.5 h-3.5 rounded-full flex items-center justify-center
                           text-[0.42rem] font-bold text-white shrink-0 select-none"
                    style="background:#8b5cf6">7</span>
              <span class="text-[0.6rem] font-medium text-foreground/70 shrink-0">{t("search.deviceFilterLabel")}</span>
              <span class="text-[0.52rem] text-muted-foreground/40 flex-1 min-w-0 truncate">{t("search.deviceFilterHint")}</span>
              <select bind:value={deviceFilter}
                      aria-label={t("search.deviceFilterLabel")}
                      class="rounded border border-border dark:border-white/[0.1]
                             bg-background px-1 py-0.5 text-[0.6rem]
                             focus:outline-none focus:ring-1 focus:ring-ring max-w-[8rem] truncate shrink-0">
                <option value="all">{t("search.allDevices")}</option>
                {#each deviceList as dev}
                  <option value={dev}>{dev}</option>
                {/each}
              </select>
            </div>
          {/if}

          <!-- Date range filter -->
          <div class="flex items-center gap-2 px-3 py-1.5
                      border-t border-border dark:border-white/[0.05]
                      hover:bg-muted/10 transition-colors">
            <span class="w-3.5 h-3.5 rounded-full flex items-center justify-center
                         text-[0.42rem] font-bold text-white shrink-0 select-none"
                  style="background:#f97316">8</span>
            <span class="text-[0.6rem] font-medium text-foreground/70 shrink-0">{t("search.dateRangeLabel")}</span>
            <div class="flex items-center gap-1 flex-1 min-w-0">
              <input type="datetime-local"
                     value={ixFilterStartUtc ? new Date(ixFilterStartUtc * 1000).toISOString().slice(0, 16) : ""}
                     oninput={(e) => { const v = (e.target as HTMLInputElement).value; ixFilterStartUtc = v ? Math.floor(new Date(v).getTime() / 1000) : undefined; }}
                     class="rounded border border-border dark:border-white/[0.1] bg-background px-1 py-0.5 text-[0.55rem]
                            focus:outline-none focus:ring-1 focus:ring-ring flex-1 min-w-0" />
              <span class="text-[0.5rem] text-muted-foreground/40">→</span>
              <input type="datetime-local"
                     value={ixFilterEndUtc ? new Date(ixFilterEndUtc * 1000).toISOString().slice(0, 16) : ""}
                     oninput={(e) => { const v = (e.target as HTMLInputElement).value; ixFilterEndUtc = v ? Math.floor(new Date(v).getTime() / 1000) : undefined; }}
                     class="rounded border border-border dark:border-white/[0.1] bg-background px-1 py-0.5 text-[0.55rem]
                            focus:outline-none focus:ring-1 focus:ring-ring flex-1 min-w-0" />
            </div>
            <!-- Quick date presets -->
            <div class="flex items-center gap-1 shrink-0">
              {#each [
                { label: "24h", mins: 1440 },
                { label: "7d",  mins: 10080 },
                { label: "30d", mins: 43200 },
              ] as p}
                <button onclick={() => { const now = Math.floor(Date.now() / 1000); ixFilterStartUtc = now - p.mins * 60; ixFilterEndUtc = now; }}
                        class="px-1.5 py-0.5 rounded text-[0.48rem] border border-border dark:border-white/[0.1]
                               bg-background hover:bg-muted/40 text-muted-foreground/50 hover:text-foreground
                               transition-colors select-none">{p.label}</button>
              {/each}
              {#if ixFilterStartUtc || ixFilterEndUtc}
                <button onclick={() => { ixFilterStartUtc = undefined; ixFilterEndUtc = undefined; }}
                        class="px-1 py-0.5 rounded text-[0.48rem] text-muted-foreground/40 hover:text-foreground
                               transition-colors select-none">✕</button>
              {/if}
            </div>
          </div>

          <!-- EEG rank-by selector -->
          <div class="flex items-center gap-2 px-3 py-1.5
                      border-t border-border dark:border-white/[0.05]
                      hover:bg-muted/10 transition-colors">
            <span class="w-3.5 h-3.5 rounded-full flex items-center justify-center
                         text-[0.42rem] font-bold text-white shrink-0 select-none"
                  style="background:#ec4899">9</span>
            <span class="text-[0.6rem] font-medium text-foreground/70 shrink-0">{t("search.rankByLabel")}</span>
            <span class="text-[0.52rem] text-muted-foreground/40 flex-1 min-w-0 truncate">{t("search.rankByHint")}</span>
            <select bind:value={ixEegRankBy}
                    aria-label={t("search.rankByLabel")}
                    class="rounded border border-border dark:border-white/[0.1]
                           bg-background px-1 py-0.5 text-[0.6rem]
                           focus:outline-none focus:ring-1 focus:ring-ring shrink-0">
              <option value="timestamp">{t("search.rankTimestamp")}</option>
              <option value="engagement">{t("search.rankEngagement")}</option>
              <option value="snr">{t("search.rankSnr")}</option>
              <option value="relaxation">{t("search.rankRelaxation")}</option>
            </select>
          </div>

          <!-- Perf stats (shown after search) -->
          {#if ixPerf}
            <div class="flex items-center gap-3 px-3 py-1 border-t border-border dark:border-white/[0.05]
                        text-[0.5rem] text-muted-foreground/50 font-mono select-none">
              <span>{t("search.perfEmbed")} {ixPerf.embed_ms}ms</span>
              <span>{t("search.perfGraph")} {ixPerf.graph_ms}ms</span>
              <span class="font-semibold">{t("search.perfTotal")} {ixPerf.total_ms}ms</span>
              <span>{ixPerf.node_count} {t("search.perfNodes")}</span>
              <span>{ixPerf.edge_count} {t("search.perfEdges")}</span>
              <span>{t("search.perfCpu")} {ixPerf.cpu_usage_pct?.toFixed(0)}%</span>
              <span>{t("search.perfMem")} {ixPerf.mem_used_mb}/{ixPerf.mem_total_mb}MB</span>
            </div>
          {/if}

          {/if}<!-- end ixShowAdvanced -->

          <!-- Sessions summary (shown after search) -->
          {#if ixSessions.length > 0}
            <div class="px-3 py-1.5 border-t border-border dark:border-white/[0.05]">
              <div class="flex items-center justify-between mb-1">
                <span class="text-[0.55rem] font-semibold text-foreground/60 uppercase tracking-widest select-none">{t("search.sessionsTitle")}</span>
                <button onclick={() => {
                  const csv = ["session_id,epoch_count,duration_secs,avg_engagement,avg_snr,avg_relaxation,stddev_engagement,best"]
                    .concat(ixSessions.map(s => `${s.session_id},${s.epoch_count},${s.duration_secs},${s.avg_engagement?.toFixed(4)},${s.avg_snr?.toFixed(4)},${s.avg_relaxation?.toFixed(4)},${s.stddev_engagement?.toFixed(4)},${s.best}`))
                    .join("\n");
                  const blob = new Blob([csv], { type: "text/csv" });
                  const a = document.createElement("a");
                  a.href = URL.createObjectURL(blob);
                  a.download = `sessions_${Date.now()}.csv`;
                  a.click();
                  URL.revokeObjectURL(a.href);
                }}
                        class="text-[0.5rem] text-muted-foreground/50 hover:text-foreground transition-colors cursor-pointer select-none">
                  {t("search.exportCsv")}
                </button>
              </div>
              <!-- Cross-session engagement trend chart -->
              {#if ixSessions.length > 1}
                {@const maxEng = Math.max(...ixSessions.map(s => s.avg_engagement ?? 0), 0.01)}
                {@const pts = ixSessions.map((s, i) => `${(i / (ixSessions.length - 1)) * 120},${40 - (s.avg_engagement / maxEng) * 36}`).join(" ")}
                <svg viewBox="0 0 120 44" class="w-full h-8 mb-1" preserveAspectRatio="none">
                  <polyline points={pts} fill="none" stroke="#10b981" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round" opacity="0.6" />
                  {#each ixSessions as s, i}
                    {@const x = (i / (ixSessions.length - 1)) * 120}
                    {@const y = 40 - (s.avg_engagement / maxEng) * 36}
                    <circle cx={x} cy={y} r={s.best ? 3 : 2} fill={s.best ? "#10b981" : "#f59e0b"} opacity="0.8" />
                  {/each}
                </svg>
              {/if}

              {#each ixSessions as s}
                {@const engPct = Math.round(Math.min(1, s.avg_engagement ?? 0) * 100)}
                {@const snrPct = Math.round(Math.min(1, (s.avg_snr ?? 0) / 20) * 100)}
                <div class="flex items-center gap-2 py-0.5 text-[0.5rem] font-mono
                            {s.best ? 'text-emerald-500 font-semibold' : 'text-muted-foreground/50'}">
                  {#if s.best}<span title={t("search.bestSession")}>★</span>{/if}
                  <span class="w-20 truncate">{s.session_id}</span>
                  <span>{s.epoch_count}ep</span>
                  <span>{Math.round(s.duration_secs / 60)}m</span>
                  <!-- Mini engagement bar -->
                  <div class="w-12 h-2 rounded-full bg-muted/20 overflow-hidden shrink-0" title="eng:{s.avg_engagement?.toFixed(2)} ±{s.stddev_engagement?.toFixed(2)}">
                    <div class="h-full rounded-full transition-all duration-700 ease-out {s.best ? 'bg-emerald-500' : 'bg-amber-500/60'}" style="width:{engPct}%"></div>
                  </div>
                  <span class="w-8 text-right">{s.avg_engagement?.toFixed(2)}</span>
                  <!-- Mini SNR bar -->
                  <div class="w-8 h-2 rounded-full bg-muted/20 overflow-hidden shrink-0" title="snr:{s.avg_snr?.toFixed(1)}">
                    <div class="h-full rounded-full transition-all duration-700 ease-out bg-blue-500/50" style="width:{snrPct}%"></div>
                  </div>
                  <span class="w-8 text-right">{s.avg_snr?.toFixed(1)}</span>
                </div>
              {/each}
            </div>
          {/if}

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
                 aria-label="k nearest neighbors"
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
             aria-label="Filter by label"
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
             aria-label="Filter text results"
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
          {#if corpusStats}
            <div class="flex flex-col gap-1 text-[0.6rem] text-muted-foreground/45 tabular-nums max-w-[320px]">
              {#if corpusStats.eeg_days > 0}
                <div class="flex items-center justify-center gap-3">
                  <span><span class="font-semibold text-foreground/50">{corpusStats.eeg_days}</span> day{corpusStats.eeg_days !== 1 ? "s" : ""} of EEG</span>
                  {#if corpusStats.eeg_total_sessions != null}
                    <span><span class="font-semibold text-foreground/50">{corpusStats.eeg_total_sessions}</span> session{corpusStats.eeg_total_sessions !== 1 ? "s" : ""}</span>
                  {/if}
                  {#if corpusStats.eeg_total_secs != null}
                    <span><span class="font-semibold text-foreground/50">{fmtSecs(corpusStats.eeg_total_secs)}</span></span>
                  {/if}
                </div>
                <span class="text-[0.5rem] text-muted-foreground/30">{corpusStats.eeg_first_day} – {corpusStats.eeg_last_day}</span>
                {#if corpusStats.eeg_total_epochs != null && corpusStats.eeg_total_epochs > 0}
                  {@const covPct = corpusStats.eeg_total_epochs > 0 ? Math.round(((corpusStats.eeg_embedded_epochs ?? 0) / corpusStats.eeg_total_epochs) * 100) : 0}
                  <div class="flex items-center justify-center gap-2 mt-0.5">
                    <span class="text-[0.5rem] {covPct >= 95 ? 'text-emerald-500/70' : covPct >= 50 ? 'text-amber-500/70' : 'text-red-500/70'}">
                      {t("search.eegCoverage")}:
                    </span>
                    <div class="h-1 w-12 rounded-full bg-muted/30 overflow-hidden">
                      <div class="h-full rounded-full {covPct >= 95 ? 'bg-emerald-500/60' : covPct >= 50 ? 'bg-amber-500/60' : 'bg-red-500/60'}"
                           style="width:{covPct}%"></div>
                    </div>
                    <span class="text-[0.5rem] {covPct >= 95 ? 'text-emerald-500/70' : covPct >= 50 ? 'text-amber-500/70' : 'text-red-500/70'}">
                      {t("search.eegCoverageLabel", {
                        embedded: (corpusStats.eeg_embedded_epochs ?? 0).toLocaleString(),
                        total: corpusStats.eeg_total_epochs.toLocaleString(),
                        pct: String(covPct),
                      })}
                    </span>
                  </div>
                {/if}
              {:else}
                <span class="text-muted-foreground/30">No EEG recordings yet</span>
              {/if}
              <div class="flex items-center justify-center gap-3 mt-0.5">
                <span><span class="font-semibold text-foreground/50">{corpusStats.label_total}</span> labels</span>
                {#if corpusStats.screenshot_total > 0}
                  <span><span class="font-semibold text-foreground/50">{corpusStats.screenshot_total}</span> screenshots</span>
                {/if}
                <span class="text-muted-foreground/25">{corpusStats.label_embed_model}</span>
              </div>
            </div>
          {/if}
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
          <!-- Corpus context line -->
          {#if corpusStats && corpusStats.eeg_days > 0}
            <div class="flex items-center gap-2.5 px-4 py-1 border-b border-border dark:border-white/[0.04]
                        text-[0.5rem] text-muted-foreground/40 tabular-nums select-none">
              <span>Searching {result.searched_days.length} of <span class="text-foreground/50">{corpusStats.eeg_days}</span> days</span>
              {#if corpusStats.eeg_total_sessions != null}
                <span class="text-muted-foreground/15">·</span>
                <span><span class="text-foreground/50">{corpusStats.eeg_total_sessions}</span> sessions total</span>
              {/if}
              {#if corpusStats.eeg_total_secs != null}
                <span class="text-muted-foreground/15">·</span>
                <span><span class="text-foreground/50">{fmtSecs(corpusStats.eeg_total_secs)}</span> recorded</span>
              {/if}
              <span class="text-muted-foreground/15">·</span>
              <span><span class="text-foreground/50">{corpusStats.label_total}</span> labels</span>
              <span class="text-muted-foreground/15">·</span>
              <span class="text-muted-foreground/25">{corpusStats.label_embed_model}</span>
            </div>
          {/if}

          <!-- ── Collapsible analysis ─────────────────────────────────── -->
          {#if searchAnalysis}
            {@const sa = searchAnalysis}
            {@const maxH = Math.max(...sa.hourHist, 1)}
            <div class="border-b border-border dark:border-white/[0.05]">
              <button onclick={() => showAnalysis = !showAnalysis}
                      aria-expanded={showAnalysis}
                      aria-label={t("search.analysisLabel")}
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
                            {isSelf ? t("search.analysisSelf") : (nb.distance ?? 0).toFixed(4)}
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
                        {#if nb.labels?.length > 0}
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

          <!-- Recent searches -->
          {#if ixSearchHistory.length > 0}
            <div class="flex flex-col gap-1 mt-3 max-w-[300px]">
              <span class="text-[0.5rem] text-muted-foreground/40 uppercase tracking-widest font-semibold select-none">{t("search.recentSearches")}</span>
              <div class="flex flex-wrap gap-1">
                {#each ixSearchHistory.slice(0, 6) as q}
                  <button onclick={() => { ixQuery = q; searchInteractive(); }}
                          class="px-2 py-0.5 rounded-full border border-border dark:border-white/[0.08]
                                 bg-background hover:bg-muted/30 text-[0.55rem] text-muted-foreground/60
                                 hover:text-foreground transition-colors truncate max-w-[140px]">{q}</button>
                {/each}
              </div>
            </div>
          {/if}

          <!-- Saved bookmarks -->
          {#if ixBookmarks.length > 0}
            <div class="flex flex-col gap-1 mt-3 max-w-[300px]">
              <span class="text-[0.5rem] text-muted-foreground/40 uppercase tracking-widest font-semibold select-none">{t("search.savedFindings")}</span>
              <div class="flex flex-col gap-0.5">
                {#each ixBookmarks.slice(0, 5) as bm}
                  <div class="flex items-center gap-1.5 text-[0.55rem]">
                    <span class="text-amber-500">★</span>
                    <button onclick={() => { ixQuery = bm.query; searchInteractive(); }}
                            class="text-muted-foreground/60 hover:text-foreground truncate max-w-[200px] transition-colors text-left">
                      {bm.text || bm.kind}
                    </button>
                    <button onclick={() => removeBookmark(bm.nodeId)}
                            class="text-muted-foreground/20 hover:text-red-400 transition-colors ml-auto shrink-0">✕</button>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>

      {:else if ixSearching}
        <!-- Loading skeleton with graph shape + pipeline progress -->
        <div class="flex flex-col items-center justify-center h-full gap-4 px-8">
          <!-- Graph skeleton -->
          <div class="w-full max-w-sm h-48 rounded-xl border border-border/30 bg-muted/5 relative overflow-hidden">
            <!-- Animated skeleton circles representing graph nodes -->
            <div class="absolute inset-0 flex items-center justify-center">
              <svg viewBox="0 0 200 120" class="w-full h-full opacity-30">
                <circle cx="100" cy="20" r="8" fill="#8b5cf6" class="animate-pulse" />
                <circle cx="55" cy="55" r="6" fill="#3b82f6" class="animate-pulse" style="animation-delay:0.2s" />
                <circle cx="145" cy="55" r="6" fill="#3b82f6" class="animate-pulse" style="animation-delay:0.3s" />
                <circle cx="35" cy="90" r="4" fill="#f59e0b" class="animate-pulse" style="animation-delay:0.5s" />
                <circle cx="75" cy="95" r="4" fill="#f59e0b" class="animate-pulse" style="animation-delay:0.6s" />
                <circle cx="125" cy="90" r="4" fill="#f59e0b" class="animate-pulse" style="animation-delay:0.7s" />
                <circle cx="165" cy="95" r="4" fill="#f59e0b" class="animate-pulse" style="animation-delay:0.8s" />
                <line x1="100" y1="28" x2="55" y2="49" stroke="#8b5cf6" stroke-width="1" opacity="0.3" />
                <line x1="100" y1="28" x2="145" y2="49" stroke="#8b5cf6" stroke-width="1" opacity="0.3" />
                <line x1="55" y1="61" x2="35" y2="86" stroke="#f59e0b" stroke-width="1" opacity="0.3" />
                <line x1="55" y1="61" x2="75" y2="91" stroke="#f59e0b" stroke-width="1" opacity="0.3" />
                <line x1="145" y1="61" x2="125" y2="86" stroke="#f59e0b" stroke-width="1" opacity="0.3" />
                <line x1="145" y1="61" x2="165" y2="91" stroke="#f59e0b" stroke-width="1" opacity="0.3" />
              </svg>
            </div>
            <!-- Shimmer overlay -->
            <div class="absolute inset-0 bg-gradient-to-r from-transparent via-white/5 to-transparent animate-pulse"></div>
          </div>
          <span class="text-[0.78rem] text-muted-foreground">
            {ixStatus || t("search.interactiveSearching")}
          </span>
          <!-- Pipeline progress -->
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
        <div class="flex flex-col items-center justify-center h-full gap-3 text-center px-8">
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
        {@const ssCount   = dispNodes.filter(n => n.kind === "screenshot").length}

        <div class="flex items-center gap-2 px-4 py-1.5 border-b border-border dark:border-white/[0.05] shrink-0">
          <!-- Coloured node-kind dots + counts + labels -->
          <span class="flex items-center gap-0.5 text-[0.52rem] text-violet-500/80 tabular-nums shrink-0" title={t("search.nodeQueryTip")}>
            <span class="w-1.5 h-1.5 rounded-full bg-violet-500 shrink-0"></span>1 <span class="text-[0.42rem] text-muted-foreground/40 font-normal">{t("search.nodeQuery")}</span>
          </span>
          <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
          <span class="flex items-center gap-0.5 text-[0.52rem] text-blue-500/80 tabular-nums shrink-0" title={t("search.nodeTextTip")}>
            <span class="w-1.5 h-1.5 rounded-full bg-blue-500 shrink-0"></span>{tlCount} <span class="text-[0.42rem] text-muted-foreground/40 font-normal">{t("search.nodeText")}</span>
          </span>
          <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
          <span class="flex items-center gap-0.5 text-[0.52rem] text-amber-500/80 tabular-nums shrink-0" title={t("search.nodeEegTip")}>
            <span class="w-1.5 h-1.5 rounded-full bg-amber-500 shrink-0"></span>{epCount} <span class="text-[0.42rem] text-muted-foreground/40 font-normal">{t("search.nodeEeg")}</span>
          </span>
          <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
          <span class="flex items-center gap-0.5 text-[0.52rem] text-emerald-500/80 tabular-nums shrink-0" title={t("search.nodeFoundTip")}>
            <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 shrink-0"></span>{flDisp}{#if ixDedupeLabels && flRaw > flDisp}<span class="opacity-40">/{flRaw}</span>{/if} <span class="text-[0.42rem] text-muted-foreground/40 font-normal">{t("search.nodeFound")}</span>
          </span>
          {#if ssCount > 0}
            <span class="text-muted-foreground/20 select-none text-[0.5rem]">·</span>
            <span class="flex items-center gap-0.5 text-[0.52rem] text-cyan-500/80 tabular-nums shrink-0" title={t("search.nodeScreenshotsTip")}>
              <span class="w-1.5 h-1.5 rounded-full bg-cyan-500 shrink-0"></span>{ssCount} <span class="text-[0.42rem] text-muted-foreground/40 font-normal">{t("search.nodeScreenshots")}</span>
            </span>
          {/if}
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

          <!-- Screenshots toggle -->
          <label class="flex items-center gap-1 cursor-pointer select-none shrink-0">
            <input type="checkbox" bind:checked={ixShowScreenshots}
                   class="rounded border-border h-2.5 w-2.5 accent-amber-500" />
            <span class="text-[0.48rem] text-muted-foreground/50">{t("search.interactiveScreenshots")}</span>
          </label>

          <!-- Sort by relevance toggle -->
          <label class="flex items-center gap-1 cursor-pointer select-none shrink-0">
            <input type="checkbox" bind:checked={ixSortByRelevance}
                   class="rounded border-border h-2.5 w-2.5 accent-rose-500" />
            <span class="text-[0.48rem] text-muted-foreground/50">{t("search.sortRelevance")}</span>
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

          <!-- Color mode selector -->
          <select bind:value={ixColorMode}
                  aria-label="Graph color mode"
                  class="rounded border border-border dark:border-white/[0.1]
                         bg-background px-1.5 py-0.5 text-[0.5rem]
                         focus:outline-none focus:ring-1 focus:ring-ring shrink-0">
            <option value="timestamp">{t("search.colorTimestamp")}</option>
            <option value="engagement">{t("search.colorEngagement")}</option>
            <option value="snr">{t("search.colorSnr")}</option>
            <option value="session">{t("search.colorSession")}</option>
          </select>

          <!-- Node kind filter toggles -->
          {#each [
            { kind: "eeg_point" as const, label: "EEG", color: "amber" },
            { kind: "found_label" as const, label: "Labels", color: "emerald" },
            { kind: "screenshot" as const, label: "Screens", color: "cyan" },
          ] as f}
            <label class="flex items-center gap-0.5 cursor-pointer select-none shrink-0">
              <input type="checkbox"
                     checked={!ixHiddenKinds.includes(f.kind)}
                     onchange={() => { ixHiddenKinds = ixHiddenKinds.includes(f.kind) ? ixHiddenKinds.filter(k => k !== f.kind) : [...ixHiddenKinds, f.kind]; }}
                     class="rounded border-border h-2.5 w-2.5 accent-{f.color}-500" />
              <span class="text-[0.44rem] text-muted-foreground/45">{f.label}</span>
            </label>
          {/each}

          <!-- Show/hide graph toggle -->
          <button class="ml-auto text-[0.48rem] text-muted-foreground/40 hover:text-muted-foreground
                         transition-colors px-1.5 py-0.5 rounded border border-transparent
                         hover:border-border select-none shrink-0"
                  onclick={() => showIxGraph = !showIxGraph}>
            {showIxGraph ? `▲ ${t("search.interactiveGraphHide")}` : `▼ ${t("search.interactiveGraphShow")}`}
          </button>
        </div>

        <!-- Corpus metadata banner -->
        {#if corpusStats}
          {@const eegTotal = corpusStats.eeg_total_epochs ?? 0}
          {@const eegDone = corpusStats.eeg_embedded_epochs ?? 0}
          {@const eegPct = eegTotal > 0 ? Math.round(eegDone / eegTotal * 100) : 100}
          {@const scrTotal = corpusStats.screenshot_total}
          {@const scrDone = corpusStats.screenshot_embedded}
          {@const scrPct = scrTotal > 0 ? Math.round(scrDone / scrTotal * 100) : 100}
          {@const lblTotal = corpusStats.label_total}
          {@const lblEeg = corpusStats.label_eeg_index}
          {@const lblText = corpusStats.label_text_index}
          {@const lblStale = corpusStats.label_stale ?? 0}
          {@const anyIssue = eegPct < 100 || scrPct < 100 || lblEeg === 0 && lblTotal > 0 && eegDone > 0 || lblStale > 0}
          <div class="flex items-center gap-3 px-4 py-1.5 border-b border-border dark:border-white/[0.04]
                      bg-muted/10 text-[0.5rem] text-muted-foreground/55 tabular-nums select-none flex-wrap">
            {#if corpusStats.eeg_days > 0}
              <span title="EEG recording days">
                <span class="font-semibold text-foreground/50">{corpusStats.eeg_days}</span> day{corpusStats.eeg_days !== 1 ? "s" : ""}
                {#if corpusStats.eeg_first_day && corpusStats.eeg_last_day}
                  <span class="text-muted-foreground/35">({corpusStats.eeg_first_day}–{corpusStats.eeg_last_day})</span>
                {/if}
              </span>
              {#if corpusStats.eeg_total_sessions != null}
                <span class="text-muted-foreground/20">·</span>
                <span title="Recording sessions">
                  <span class="font-semibold text-foreground/50">{corpusStats.eeg_total_sessions}</span> session{corpusStats.eeg_total_sessions !== 1 ? "s" : ""}
                </span>
              {/if}
              {#if corpusStats.eeg_total_secs != null}
                <span class="text-muted-foreground/20">·</span>
                <span title="Total recording time">
                  <span class="font-semibold text-foreground/50">{fmtSecs(corpusStats.eeg_total_secs)}</span> recorded
                </span>
              {/if}
              <span class="text-muted-foreground/20">·</span>
            {/if}
            <span title="Labels (text / EEG index)">
              <span class="font-semibold text-foreground/50">{lblTotal}</span> labels
              <span class="text-muted-foreground/30">({lblText} text / {lblEeg} eeg)</span>
            </span>
            {#if scrTotal > 0}
              <span class="text-muted-foreground/20">·</span>
              <span title="Screenshots (embedded / total)">
                <span class="font-semibold text-foreground/50">{scrDone}</span>/<span class="font-semibold text-foreground/50">{scrTotal}</span> screenshots
              </span>
            {/if}
            {#if eegTotal > 0}
              <span class="text-muted-foreground/20">·</span>
              <span title="EEG embedding coverage" class="flex items-center gap-1">
                <span class="{eegPct >= 95 ? 'text-emerald-500/70' : eegPct >= 50 ? 'text-amber-500/70' : 'text-red-500/70'}">
                  {eegDone.toLocaleString()}/{eegTotal.toLocaleString()} epochs ({eegPct}%)
                </span>
              </span>
            {/if}
            <span class="text-muted-foreground/20">·</span>
            <span title="Embedding model" class="text-muted-foreground/30">{corpusStats.label_embed_model}</span>
          </div>

          <!-- Embedding health banner — shown when any modality needs attention -->
          {#if anyIssue}
            <div class="flex flex-col gap-1.5 px-4 py-2 border-b border-amber-500/15
                        bg-amber-500/[0.03] text-[0.5rem]">
              <!-- EXG epochs -->
              {#if eegTotal > 0 && eegPct < 100}
                <div class="flex items-center gap-2">
                  <span class="w-10 shrink-0 text-[0.48rem] font-semibold text-amber-600/80 uppercase">EXG</span>
                  <div class="flex-1 h-1.5 rounded-full bg-muted/30 overflow-hidden">
                    <div class="h-full rounded-full transition-all duration-500
                                {eegPct >= 80 ? 'bg-emerald-500' : eegPct >= 40 ? 'bg-amber-500' : 'bg-red-400'}"
                         style="width:{eegPct}%"></div>
                  </div>
                  <span class="w-20 shrink-0 text-right tabular-nums text-muted-foreground/60">
                    {eegDone.toLocaleString()}/{eegTotal.toLocaleString()} ({eegPct}%)
                  </span>
                </div>
              {/if}
              <!-- Screenshots -->
              {#if scrTotal > 0 && scrPct < 100}
                <div class="flex items-center gap-2">
                  <span class="w-10 shrink-0 text-[0.48rem] font-semibold text-cyan-600/80 uppercase">IMG</span>
                  <div class="flex-1 h-1.5 rounded-full bg-muted/30 overflow-hidden">
                    <div class="h-full rounded-full bg-cyan-500 transition-all duration-500"
                         style="width:{scrPct}%"></div>
                  </div>
                  <span class="w-20 shrink-0 text-right tabular-nums text-muted-foreground/60">
                    {scrDone.toLocaleString()}/{scrTotal.toLocaleString()} ({scrPct}%)
                  </span>
                </div>
              {/if}
              <!-- Label EEG index -->
              {#if lblTotal > 0 && lblEeg === 0 && eegDone > 0}
                <div class="flex items-center gap-2">
                  <span class="w-10 shrink-0 text-[0.48rem] font-semibold text-violet-600/80 uppercase">LBL</span>
                  <div class="flex-1 h-1.5 rounded-full bg-muted/30 overflow-hidden">
                    <div class="h-full rounded-full bg-red-400" style="width:0%"></div>
                  </div>
                  <span class="w-20 shrink-0 text-right tabular-nums text-muted-foreground/60">
                    {lblEeg}/{lblTotal} (0%)
                  </span>
                  <button
                    disabled={rebuildingLabelIndex}
                    onclick={async () => {
                      rebuildingLabelIndex = true;
                      try {
                        const r = await daemonInvoke<{ ok: boolean; eeg_nodes?: number }>("rebuild_label_index");
                        if (r.ok && r.eeg_nodes != null) {
                          // Force full refresh so @const vars recompute.
                          const fresh = await daemonInvoke<CorpusStats>("search_corpus_stats", {});
                          corpusStats = fresh;
                        }
                      } catch {}
                      rebuildingLabelIndex = false;
                    }}
                    class="shrink-0 text-[0.48rem] text-amber-600 hover:text-amber-500
                           underline underline-offset-2 cursor-pointer font-semibold
                           disabled:opacity-50 disabled:cursor-wait"
                  >{rebuildingLabelIndex ? "Rebuilding…" : t("search.rebuildLabelIndex")}</button>
                </div>
              {:else if lblStale > 0}
                <div class="flex items-center gap-2">
                  <span class="w-10 shrink-0 text-[0.48rem] font-semibold text-violet-600/80 uppercase">LBL</span>
                  <span class="text-amber-500/70">{lblStale} stale label{lblStale !== 1 ? "s" : ""} need re-embedding</span>
                </div>
              {/if}
            </div>
          {/if}
        {/if}

        <!-- 3D Graph panel -->
        <div class="border-b border-border dark:border-white/[0.05] shrink-0">
          <div class="rounded-xl border border-border bg-card mx-4 my-3 overflow-hidden">
            <!-- (header merged into row above) -->

            {#if showIxGraph}
              <div style="width:100%; height:500px">
                <InteractiveGraph3D nodes={dispNodes} edges={dispEdges} usePca={ixUsePca}
                                    hiddenKinds={ixHiddenKinds}
                                    colorMode={ixColorMode}
                                    onselect={(n) => { ixSelectedNode = n; ixCompareNode = null; ixDetailTimeseries = []; }} />
              </div>
            {/if}

          </div>
        </div>

        <!-- Node detail panel — separate card below the graph -->
        {#if ixSelectedNode}
          {@const sn = ixSelectedNode}
          {@const kindColor = sn.kind === 'query' ? '#8b5cf6' : sn.kind === 'text_label' ? '#3b82f6' : sn.kind === 'eeg_point' ? '#f59e0b' : sn.kind === 'found_label' ? '#10b981' : '#06b6d4'}
          <div class="mx-4 mb-3 rounded-xl border border-border bg-card overflow-hidden shrink-0"
               style="border-left: 3px solid {kindColor}">

            <div class="px-6 py-5">
              <!-- Header row -->
              <div class="flex items-center gap-3 mb-4">
                <span class="w-4 h-4 rounded-full shrink-0" style="background:{kindColor}"></span>
                <span class="text-base font-semibold text-foreground/90 capitalize">{sn.kind.replace("_", " ")}</span>
                {#if sn.session_id}
                  <span class="px-2.5 py-1 rounded-full bg-muted/30 text-xs text-muted-foreground/60 font-mono">{sn.session_id}</span>
                {/if}
                {#if sn.relevance_score != null}
                  <span class="px-2.5 py-1 rounded-full bg-muted/30 text-xs text-muted-foreground/60 font-mono">relevance {sn.relevance_score.toFixed(3)}</span>
                {/if}
                <div class="ml-auto flex items-center gap-2">
                  {#if sn.text && sn.kind !== "query"}
                    <button onclick={() => { ixQuery = sn.text ?? ""; ixSelectedNode = null; searchInteractive(); }}
                            class="text-xs px-3 py-1.5 rounded-md border border-emerald-500/30
                                   bg-emerald-500/10 text-emerald-600 dark:text-emerald-400
                                   hover:bg-emerald-500/20 transition-colors select-none">{t("search.moreLikeThis")}</button>
                  {/if}
                  <!-- Bookmark button -->
                  <button onclick={() => saveBookmark(sn)}
                          class="text-xs px-3 py-1.5 rounded-md border
                                 {ixBookmarks.some(b => b.nodeId === sn.id) ? 'border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400' : 'border-border/30 bg-muted/10 text-muted-foreground/50 hover:text-foreground'}
                                 transition-colors select-none"
                          title={ixBookmarks.some(b => b.nodeId === sn.id) ? t("search.bookmarked") : t("search.bookmark")}>
                    {ixBookmarks.some(b => b.nodeId === sn.id) ? "★" : "☆"} {t("search.bookmark")}
                  </button>
                  <button onclick={() => ixSelectedNode = null}
                          aria-label="Close"
                          class="text-muted-foreground/40 hover:text-foreground transition-colors p-1.5 rounded-md hover:bg-muted/30">
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4"><path d="M18 6 6 18M6 6l12 12"/></svg>
                  </button>
                </div>
              </div>

              <!-- Breadcrumb trail -->
              {#each [buildBreadcrumb(sn, ixNodes)] as trail}
                {#if trail.length > 1}
                  <div class="flex items-center gap-2 mb-4 text-sm text-muted-foreground/60 overflow-x-auto pb-1">
                    {#each trail as tn, ti}
                      {#if ti > 0}<span class="text-muted-foreground/25 text-xs">→</span>{/if}
                      <button onclick={() => { ixSelectedNode = tn; }}
                              class="px-2.5 py-1 rounded-md text-xs {tn.id === sn.id ? 'bg-foreground/10 font-semibold text-foreground/80' : 'hover:bg-muted/30 text-muted-foreground/60'} transition-colors truncate max-w-[160px]"
                              title={tn.text ?? tn.kind}>
                        {tn.text?.slice(0, 30) ?? tn.kind.replace("_", " ")}
                      </button>
                    {/each}
                  </div>
                {/if}
              {/each}

              <!-- Text content -->
              {#if sn.text}
                <p class="text-sm text-foreground/80 mb-3 leading-relaxed">{sn.text}</p>
              {/if}

              <!-- Timestamp -->
              {#if sn.timestamp_unix}
                <p class="text-sm text-muted-foreground/50 font-mono mb-3">{new Date(sn.timestamp_unix * 1000).toLocaleString()}</p>
              {/if}

              <!-- EEG metrics grid -->
              {#if sn.eeg_metrics}
                {@const metrics = Object.entries(sn.eeg_metrics).filter(([, v]) => v != null && v !== 0)}
                {#if metrics.length > 0}
                  <div class="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 gap-2.5 mt-3">
                    {#each metrics as [key, val]}
                      <div class="rounded-lg bg-muted/15 border border-border/30 px-3 py-2 text-center">
                        <div class="text-[0.65rem] text-muted-foreground/50 uppercase tracking-wider mb-1">{key}</div>
                        <div class="text-sm font-mono font-semibold text-foreground/75">{typeof val === "number" ? val.toFixed(3) : val}</div>
                      </div>
                    {/each}
                  </div>
                {/if}
              {/if}

              <!-- Screenshot preview (for screenshot nodes) -->
              {#if sn.kind === "screenshot" && sn.filename}
                <div class="mt-3 rounded-lg border border-border/30 overflow-hidden">
                  <img src={imgSrc(sn.filename)} alt={sn.window_title ?? "Screenshot"}
                       class="w-full max-h-48 object-contain bg-black/5" />
                </div>
              {/if}

              <!-- EEG sparkline (for eeg_point nodes — fetch on demand) -->
              {#if sn.kind === "eeg_point" && sn.timestamp_unix}
                <div class="mt-3">
                  {#if ixDetailTimeseries.length === 0}
                    <button onclick={async () => {
                      const startUtc = (sn.timestamp_unix ?? 0) - 60;
                      const endUtc = (sn.timestamp_unix ?? 0) + 60;
                      try {
                        const ts = await daemonInvoke<Array<{ t: number; ra: number; rb: number; rt: number; engagement: number; relaxation: number; snr: number }>>(
                          "get_session_timeseries", { startUtc, endUtc }
                        );
                        ixDetailTimeseries = ts ?? [];
                      } catch { ixDetailTimeseries = []; }
                    }}
                    class="text-xs px-3 py-1.5 rounded-md border border-blue-500/30
                           bg-blue-500/10 text-blue-600 dark:text-blue-400
                           hover:bg-blue-500/20 transition-colors select-none">
                      {t("search.loadEegSparkline")}
                    </button>
                  {:else}
                    <!-- Mini band-power chart -->
                    {@const pts = ixDetailTimeseries}
                    {@const maxVal = Math.max(0.01, ...pts.map(p => Math.max(p.ra, p.rb, p.rt)))}
                    <div class="rounded-lg border border-border/30 bg-muted/5 p-3">
                      <div class="flex items-center gap-3 mb-2 text-[0.6rem] text-muted-foreground/50">
                        <span class="flex items-center gap-1"><span class="w-2 h-0.5 rounded bg-blue-500 inline-block"></span>α</span>
                        <span class="flex items-center gap-1"><span class="w-2 h-0.5 rounded bg-amber-500 inline-block"></span>β</span>
                        <span class="flex items-center gap-1"><span class="w-2 h-0.5 rounded bg-emerald-500 inline-block"></span>θ</span>
                        <span class="ml-auto text-[0.5rem]">{pts.length} epochs · ±60s</span>
                      </div>
                      <svg viewBox="0 0 300 60" class="w-full h-16" preserveAspectRatio="none">
                        {#each [
                          { data: pts.map(p => p.ra), color: "#3b82f6" },
                          { data: pts.map(p => p.rb), color: "#f59e0b" },
                          { data: pts.map(p => p.rt), color: "#10b981" },
                        ] as line}
                          <polyline
                            points={line.data.map((v, i) => `${(i / Math.max(1, line.data.length - 1)) * 300},${58 - (v / maxVal) * 54}`).join(" ")}
                            fill="none" stroke={line.color} stroke-width="1.5" stroke-linejoin="round" opacity="0.7" />
                        {/each}
                        <!-- Marker for the selected timestamp -->
                        {#if sn.timestamp_unix && pts.length > 0}
                          {@const tMin = pts[0].t}
                          {@const tMax = pts[pts.length - 1].t}
                          {@const tRange = tMax - tMin || 1}
                          {@const mx = ((sn.timestamp_unix - tMin) / tRange) * 300}
                          <line x1={mx} y1="0" x2={mx} y2="60" stroke="#ef4444" stroke-width="1" stroke-dasharray="3,2" opacity="0.6" />
                        {/if}
                      </svg>
                    </div>
                  {/if}
                </div>
              {/if}

              <!-- Compare button -->
              {#if sn.kind === "eeg_point" && !ixCompareNode}
                <button onclick={() => { ixCompareNode = sn; }}
                        class="mt-3 text-xs px-3 py-1.5 rounded-md border border-violet-500/30
                               bg-violet-500/10 text-violet-600 dark:text-violet-400
                               hover:bg-violet-500/20 transition-colors select-none">
                  {t("search.compareSelect")}
                </button>
              {/if}
            </div>

            <!-- Compare panel (shown when two nodes are selected) -->
            {#if ixCompareNode && ixSelectedNode && ixCompareNode.id !== ixSelectedNode.id && ixSelectedNode.kind === "eeg_point"}
              {@const a = ixCompareNode}
              {@const b = ixSelectedNode}
              <div class="px-6 py-4 border-t border-violet-500/20 bg-violet-500/5">
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-sm font-semibold text-violet-600 dark:text-violet-400">{t("search.compareTitle")}</span>
                  <button onclick={() => ixCompareNode = null}
                          class="ml-auto text-muted-foreground/40 hover:text-foreground transition-colors text-xs">✕ clear</button>
                </div>
                <div class="grid grid-cols-3 gap-2 text-xs font-mono">
                  <div class="text-right text-muted-foreground/50">Metric</div>
                  <div class="text-center font-semibold">Node A</div>
                  <div class="text-center font-semibold">Node B</div>
                  {#each ["engagement", "relaxation", "snr", "rel_alpha", "rel_beta", "rel_theta"] as key}
                    {@const va = (a.eeg_metrics as Record<string, number | null> | null)?.[key]}
                    {@const vb = (b.eeg_metrics as Record<string, number | null> | null)?.[key]}
                    {#if va != null || vb != null}
                      <div class="text-right text-muted-foreground/60">{key}</div>
                      <div class="text-center">{va != null ? (va as number).toFixed(3) : "—"}</div>
                      <div class="text-center {va != null && vb != null && (vb as number) > (va as number) ? 'text-emerald-500' : va != null && vb != null && (vb as number) < (va as number) ? 'text-red-400' : ''}">
                        {vb != null ? (vb as number).toFixed(3) : "—"}
                      </div>
                    {/if}
                  {/each}
                  {#if a.timestamp_unix && b.timestamp_unix}
                    <div class="text-right text-muted-foreground/60">time gap</div>
                    <div class="col-span-2 text-center text-muted-foreground/50">
                      {Math.abs(a.timestamp_unix - b.timestamp_unix)}s ({(Math.abs(a.timestamp_unix - b.timestamp_unix) / 60).toFixed(1)}m)
                    </div>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {/if}

        <!-- Timeline scrubber — all timestamped nodes on a horizontal axis -->
        {#if ixNodes.length > 0}
          {@const tsNodes = ixNodes.filter(n => n.timestamp_unix != null && n.timestamp_unix > 0)}
          {#if tsNodes.length > 1}
            {@const tMin = Math.min(...tsNodes.map(n => n.timestamp_unix!))}
            {@const tMax = Math.max(...tsNodes.map(n => n.timestamp_unix!))}
            {@const tRange = tMax - tMin || 1}
            <div class="mx-4 mb-3 rounded-xl border border-border bg-card overflow-hidden shrink-0">
              <div class="flex items-center gap-2 px-4 py-2 border-b border-border dark:border-white/[0.06]">
                <span class="text-[0.62rem] font-semibold select-none">{t("search.timelineScrubber")}</span>
                <span class="text-[0.45rem] text-muted-foreground/45 font-mono">
                  {new Date(tMin * 1000).toLocaleDateString()} → {new Date(tMax * 1000).toLocaleDateString()}
                </span>
              </div>
              <div class="px-4 py-3">
                <svg viewBox="0 0 600 40" class="w-full h-10" preserveAspectRatio="xMidYMid meet">
                  <!-- Track -->
                  <rect x="0" y="18" width="600" height="4" rx="2" fill="currentColor" opacity="0.08" />
                  <!-- Nodes as dots -->
                  {#each tsNodes as n}
                    {@const x = ((n.timestamp_unix! - tMin) / tRange) * 600}
                    {@const col = n.kind === "text_label" ? "#3b82f6" : n.kind === "eeg_point" ? "#f59e0b" : n.kind === "found_label" ? "#10b981" : "#06b6d4"}
                    {@const isSelected = ixSelectedNode?.id === n.id}
                    <circle cx={x} cy="20" r={isSelected ? 5 : 3}
                            fill={col} opacity={isSelected ? 1 : 0.5}
                            class="cursor-pointer"
                            onclick={() => { ixSelectedNode = n; ixDetailTimeseries = []; }}
                            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { ixSelectedNode = n; ixDetailTimeseries = []; } }}
                            role="button" tabindex="-1" aria-label={n.text ?? n.kind}>
                      <title>{n.kind}: {n.text?.slice(0, 40) ?? ""} ({new Date(n.timestamp_unix! * 1000).toLocaleTimeString()})</title>
                    </circle>
                  {/each}
                </svg>
              </div>
            </div>
          {/if}
        {/if}

        <!-- ── Insights & Value Panel ──────────────────────────────────── -->
        {#if ixNodes.length > 0}
          <div class="mx-4 mb-3 rounded-xl border border-border bg-card overflow-hidden shrink-0">
            <button onclick={() => ixShowInsights = !ixShowInsights}
                    class="w-full flex items-center gap-2 px-4 py-2.5 text-left hover:bg-muted/10 transition-colors">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="w-4 h-4 text-amber-500/70 shrink-0">
                <path d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"/>
              </svg>
              <span class="text-sm font-semibold">{t("search.insightsTitle")}</span>
              <span class="text-[0.5rem] text-muted-foreground/40">{ixShowInsights ? "▾" : "▸"}</span>
            </button>

            {#if ixShowInsights}
              {@const insights = computeInsights([...ixNodes, ...(ixDisplayGraph?.nodes ?? [])])}
              <div class="px-5 py-4 border-t border-border dark:border-white/[0.06]">

                <!-- Best conditions -->
                {#if insights.bestConditions.length > 0}
                  <div class="mb-4">
                    <h4 class="text-xs font-semibold text-foreground/70 uppercase tracking-wider mb-2">{t("search.optimalConditions")}</h4>
                    {#each insights.bestConditions as cond}
                      <p class="text-sm text-foreground/60 flex items-center gap-2 mb-1">
                        <span class="text-amber-500">★</span> {cond}
                      </p>
                    {/each}
                  </div>
                {/if}

                <!-- App-engagement correlation -->
                {#if insights.appCorrelation.length > 0}
                  <div class="mb-4">
                    <h4 class="text-xs font-semibold text-foreground/70 uppercase tracking-wider mb-2">{t("search.appCorrelation")}</h4>
                    <div class="space-y-1.5">
                      {#each insights.appCorrelation.slice(0, 6) as app}
                        {@const pct = Math.round(Math.min(1, app.avgEngagement) * 100)}
                        <div class="flex items-center gap-2 text-xs">
                          <span class="w-24 truncate text-foreground/60 font-mono">{app.app}</span>
                          <div class="flex-1 h-2 rounded-full bg-muted/20 overflow-hidden">
                            <div class="h-full rounded-full bg-amber-500/60 transition-all duration-500" style="width:{pct}%"></div>
                          </div>
                          <span class="w-10 text-right font-mono text-muted-foreground/50">{app.avgEngagement.toFixed(2)}</span>
                          <span class="w-6 text-right text-muted-foreground/30">×{app.count}</span>
                        </div>
                      {/each}
                    </div>
                  </div>
                {/if}

                <!-- Hour-of-day engagement pattern -->
                {#if insights.hourPattern.length > 0}
                  <div class="mb-4">
                    <h4 class="text-xs font-semibold text-foreground/70 uppercase tracking-wider mb-2">{t("search.hourPattern")}</h4>
                    {#each [Math.max(...insights.hourPattern.map(h => h.avgEngagement), 0.01)] as maxH}
                    <svg viewBox="0 0 288 50" class="w-full h-12" preserveAspectRatio="none">
                      {#each insights.hourPattern as hp}
                        {@const x = hp.hour * 12}
                        {@const h = (hp.avgEngagement / maxH) * 44}
                        <rect x={x} y={50 - h} width="10" height={h} rx="2"
                              fill="#f59e0b" opacity="0.5">
                          <title>{hp.hour}:00 — eng:{hp.avgEngagement.toFixed(2)} (×{hp.count})</title>
                        </rect>
                      {/each}
                    </svg>
                    {/each}
                    <div class="flex justify-between text-[0.45rem] text-muted-foreground/30 font-mono mt-0.5">
                      <span>0h</span><span>6h</span><span>12h</span><span>18h</span><span>23h</span>
                    </div>
                  </div>
                {/if}

                <!-- Empty state when no EEG insights available -->
                {#if insights.bestConditions.length === 0 && insights.appCorrelation.length === 0 && insights.hourPattern.length === 0}
                  {@const allNodes = [...ixNodes, ...(ixDisplayGraph?.nodes ?? [])]}
                  {@const eegCount = allNodes.filter(n => n.kind === "eeg_point").length}
                  {@const hasMetrics = allNodes.some(n => n.kind === "eeg_point" && n.eeg_metrics?.engagement != null)}
                  <p class="text-xs text-muted-foreground/50 italic mb-3">
                    {#if eegCount === 0}
                      {t("search.insightsNoEeg")}
                    {:else if !hasMetrics}
                      {t("search.insightsMetricsPending")}
                    {:else}
                      {t("search.insightsNoData")}
                    {/if}
                  </p>
                {/if}

                <!-- LLM Summary -->
                <div class="border-t border-border/30 pt-3 mt-2">
                  <div class="flex items-center gap-2 mb-2">
                    <h4 class="text-xs font-semibold text-foreground/70 uppercase tracking-wider">{t("search.llmSummary")}</h4>
                    {#if !ixLlmSummary && !ixLlmLoading}
                      <button onclick={async () => {
                        // Build prompt
                        const labelNodes = ixNodes.filter(n => (n.kind === "text_label" || n.kind === "found_label") && n.text);
                        const labels = labelNodes.map(n => {
                          const ts = n.timestamp_unix ? new Date(n.timestamp_unix * 1000).toLocaleString() : "";
                          return `"${n.text}"${ts ? ` (${ts})` : ""}`;
                        }).slice(0, 10);
                        const eegNodes = ixNodes.filter(n => n.kind === "eeg_point" && n.timestamp_unix);
                        const timeRange = eegNodes.length > 0
                          ? `${new Date(Math.min(...eegNodes.map(n => n.timestamp_unix!)) * 1000).toLocaleString()} → ${new Date(Math.max(...eegNodes.map(n => n.timestamp_unix!)) * 1000).toLocaleString()}`
                          : "unknown";
                        const sessions = ixSessions.slice(0, 5).map(s => `${s.session_id}: eng=${s.avg_engagement.toFixed(2)}, snr=${s.avg_snr.toFixed(1)}, ${Math.round(s.duration_secs/60)}min`);
                        const allNodes = [...ixNodes, ...(ixDisplayGraph?.nodes ?? [])];
                        const apps = computeInsights(allNodes).appCorrelation.slice(0, 5).map(a => `${a.app}: eng=${a.avgEngagement.toFixed(2)}`);
                        // Include EEG metrics for each epoch
                        const eegDetails = eegNodes.slice(0, 10).map(n => {
                          const ts = new Date(n.timestamp_unix! * 1000).toLocaleString();
                          const m = n.eeg_metrics ?? {};
                          const parts = [`t=${ts}`, `dist=${n.distance.toFixed(3)}`];
                          if (m.engagement != null) parts.push(`eng=${(m.engagement as number).toFixed(2)}`);
                          if (m.relaxation != null) parts.push(`rel=${(m.relaxation as number).toFixed(2)}`);
                          if (m.snr != null) parts.push(`snr=${(m.snr as number).toFixed(1)}`);
                          if (m.rel_alpha != null) parts.push(`α=${(m.rel_alpha as number).toFixed(3)}`);
                          if (m.rel_beta != null) parts.push(`β=${(m.rel_beta as number).toFixed(3)}`);
                          if (m.rel_theta != null) parts.push(`θ=${(m.rel_theta as number).toFixed(3)}`);
                          if (m.faa != null) parts.push(`faa=${(m.faa as number).toFixed(3)}`);
                          if (m.mood != null && (m.mood as number) > 0) parts.push(`mood=${(m.mood as number).toFixed(1)}`);
                          if (m.meditation != null && (m.meditation as number) > 0) parts.push(`meditation=${(m.meditation as number).toFixed(1)}`);
                          if (m.cognitive_load != null && (m.cognitive_load as number) > 0) parts.push(`cognitive_load=${(m.cognitive_load as number).toFixed(1)}`);
                          if (m.drowsiness != null && (m.drowsiness as number) > 0) parts.push(`drowsiness=${(m.drowsiness as number).toFixed(1)}`);
                          if (m.tar != null) parts.push(`tar=${(m.tar as number).toFixed(3)}`);
                          if (m.hr != null && (m.hr as number) > 0) parts.push(`hr=${(m.hr as number).toFixed(0)}`);
                          if (n.relevance_score != null) parts.push(`relevance=${n.relevance_score.toFixed(3)}`);
                          if (n.session_id) parts.push(`session=${n.session_id}`);
                          // Flag epochs with no metrics
                          if (!m.engagement && !m.relaxation && !m.snr) parts.push("(no EEG metrics stored)");
                          return parts.join(", ");
                        }).join("\n  ");
                        // If no epochs have metrics, try to fetch them on demand
                        // When graph nodes lack metrics, fetch from session CSV (more reliable than embeddings table)
                        let fetchedMetrics = "";
                        if (eegNodes.length > 0 && eegNodes.every(n => !n.eeg_metrics?.engagement)) {
                          const startUtc = Math.min(...eegNodes.map(n => n.timestamp_unix!));
                          const endUtc = Math.max(...eegNodes.map(n => n.timestamp_unix!));

                          // Try get_session_metrics first (reads from metrics CSV, more likely to have data)
                          try {
                            const sm = await daemonInvoke<Record<string, number>>("get_session_metrics", { startUtc, endUtc });
                            if (sm && (sm.engagement > 0 || sm.relaxation > 0 || sm.snr > 0)) {
                              const parts = [`${sm.n_epochs ?? 0} epochs from CSV`];
                              if (sm.engagement > 0) parts.push(`avg_engagement=${sm.engagement.toFixed(3)}`);
                              if (sm.relaxation > 0) parts.push(`avg_relaxation=${sm.relaxation.toFixed(3)}`);
                              if (sm.snr > 0) parts.push(`avg_snr=${sm.snr.toFixed(1)}`);
                              if (sm.rel_alpha > 0) parts.push(`avg_α=${sm.rel_alpha.toFixed(3)}`);
                              if (sm.rel_beta > 0) parts.push(`avg_β=${sm.rel_beta.toFixed(3)}`);
                              if (sm.rel_theta > 0) parts.push(`avg_θ=${sm.rel_theta.toFixed(3)}`);
                              if (sm.rel_delta > 0) parts.push(`avg_δ=${sm.rel_delta.toFixed(3)}`);
                              if (sm.rel_gamma > 0) parts.push(`avg_γ=${sm.rel_gamma.toFixed(3)}`);
                              if (sm.faa) parts.push(`faa=${sm.faa.toFixed(3)}`);
                              if (sm.hr > 0) parts.push(`avg_hr=${sm.hr.toFixed(0)}`);
                              if (sm.mood > 0) parts.push(`mood=${sm.mood.toFixed(2)}`);
                              if (sm.meditation > 0) parts.push(`meditation=${sm.meditation.toFixed(2)}`);
                              if (sm.cognitive_load > 0) parts.push(`cognitive_load=${sm.cognitive_load.toFixed(2)}`);
                              if (sm.drowsiness > 0) parts.push(`drowsiness=${sm.drowsiness.toFixed(2)}`);
                              if (sm.stress_index > 0) parts.push(`stress=${sm.stress_index.toFixed(2)}`);
                              fetchedMetrics = `\nAggregated EEG metrics from session recordings:\n  ${parts.join(", ")}`;
                            }
                          } catch { /* ignore */ }

                          // Fallback: try timeseries (embeddings table)
                          if (!fetchedMetrics) {
                            try {
                              const ts = await daemonInvoke<Array<{ t: number; engagement: number; relaxation: number; snr: number; ra: number; rb: number; rt: number; hr: number }>>(
                                "get_session_timeseries", { startUtc: startUtc - 5, endUtc: endUtc + 5 }
                              );
                              if (ts && ts.length > 0) {
                                const hasMetrics = ts.filter(r => r.engagement > 0 || r.relaxation > 0 || r.snr > 0);
                                if (hasMetrics.length > 0) {
                                  const avg = (arr: number[]) => arr.reduce((s, v) => s + v, 0) / arr.length;
                                  fetchedMetrics = `\nFetched EEG averages (${hasMetrics.length} valid epochs):\n  avg_engagement=${avg(hasMetrics.map(r => r.engagement)).toFixed(3)}, avg_relaxation=${avg(hasMetrics.map(r => r.relaxation)).toFixed(3)}, avg_snr=${avg(hasMetrics.map(r => r.snr)).toFixed(1)}, avg_α=${avg(hasMetrics.map(r => r.ra)).toFixed(3)}, avg_β=${avg(hasMetrics.map(r => r.rb)).toFixed(3)}, avg_θ=${avg(hasMetrics.map(r => r.rt)).toFixed(3)}`;
                                }
                              }
                            } catch { /* ignore */ }
                          }

                          if (!fetchedMetrics) {
                            fetchedMetrics = "\nNote: No EEG metrics available for this time range. Metrics are computed during live sessions — these epochs may have been recorded before the analysis pipeline was active.";
                          }
                        }
                        // Include label distances (similarity to query)
                        const labelDetails = labelNodes.slice(0, 10).map(n => {
                          const ts = n.timestamp_unix ? new Date(n.timestamp_unix * 1000).toLocaleString() : "no time";
                          return `"${n.text}" (${ts}, dist=${n.distance.toFixed(3)})`;
                        }).join("; ");
                        // Session metrics — from backend or derived from nodes
                        let sessionDetails = ixSessions.slice(0, 5).map(s =>
                          `${s.session_id}: eng=${s.avg_engagement.toFixed(2)}±${s.stddev_engagement.toFixed(2)}, snr=${s.avg_snr.toFixed(1)}, relax=${s.avg_relaxation.toFixed(2)}, ${Math.round(s.duration_secs/60)}min, ${s.epoch_count} epochs${s.best ? " [BEST]" : ""}`
                        ).join("\n  ");
                        // If no session data from backend, derive from node session_ids
                        if (!sessionDetails && eegNodes.length > 0) {
                          const sessionMap = new Map<string, number>();
                          for (const n of eegNodes) {
                            if (n.session_id) sessionMap.set(n.session_id, (sessionMap.get(n.session_id) ?? 0) + 1);
                          }
                          sessionDetails = [...sessionMap.entries()].map(([sid, count]) => `${sid}: ${count} epochs`).join("; ");
                        }
                        // Include screenshot context (OCR text + app)
                        const ssNodes = allNodes.filter(n => n.kind === "screenshot");
                        const screenshotContext = ssNodes.slice(0, 5).map(n => {
                          const ts = n.timestamp_unix ? new Date(n.timestamp_unix * 1000).toLocaleString() : "";
                          const parts = [];
                          if (n.app_name) parts.push(`app=${n.app_name}`);
                          if (n.window_title) parts.push(`title="${n.window_title}"`);
                          if (ts) parts.push(`time=${ts}`);
                          if (n.ocr_similarity != null) parts.push(`ocr_sim=${n.ocr_similarity.toFixed(2)}`);
                          return parts.join(", ");
                        }).join("\n  ");

                        const prompt = `Analyze this EEG search for "${ixQuery}".

Time range: ${timeRange}. ${eegNodes.length} EEG epochs found.

Labels (sorted by similarity to query "${ixQuery}"):
  ${labelDetails || "none"}

EEG epochs (with brain metrics):
  ${eegDetails || "none"}${fetchedMetrics}

Sessions:
  ${sessionDetails || "none"}

App engagement: ${apps.length > 0 ? apps.join("; ") : "no app data"}.

Screenshots captured nearby:
  ${screenshotContext || "none"}

Give 2-3 concise, specific insights about:
1. Brain state patterns (engagement, relaxation, band power ratios) during "${ixQuery}" activities
2. Optimal conditions (time of day, session characteristics) for peak performance
3. Actionable recommendations based on the EEG data
Reference specific metrics and timestamps.`;
                        ixLlmPrompt = prompt;
                        ixLlmSummary = ""; // show prompt immediately while streaming
                        ixLlmSessionId = 0; // new summary = new session
                        ixLlmLoading = true;
                        ixLlmPhase = "text";
                        ixLlmScreenshots = [];

                        // Suggest optimal max_tokens based on prompt size, but user controls final value
                        const promptTokens = Math.ceil(prompt.length / 4);
                        const suggested = Math.min(8192, Math.max(2048, promptTokens * 2));
                        if (ixLlmMaxTokens <= 2048 && suggested > ixLlmMaxTokens) ixLlmMaxTokens = suggested;
                        const maxTokens = ixLlmMaxTokens;

                        // ── Phase 1: Stream text summary ──────────────
                        let acc = "";
                        const textChannel = {
                          onmessage: (chunk: { type: string; content?: string; message?: string }) => {
                            if (chunk.type === "delta" && chunk.content) {
                              acc += chunk.content;
                              ixLlmSummary = acc;
                            } else if (chunk.type === "error") {
                              ixLlmSummary = acc || "__NO_LLM__";
                              ixLlmLoading = false;
                            }
                            // "done" handled after await below
                          }
                        };
                        try {
                          await daemonInvoke("chat_completions_ipc", {
                            messages: [{ role: "user", content: prompt }],
                            params: { temperature: 0.3, max_tokens: maxTokens },
                            channel: textChannel,
                          });
                        } catch {
                          if (!acc) ixLlmSummary = "__NO_LLM__";
                          ixLlmLoading = false;
                        }
                        if (!acc) { ixLlmLoading = false; ixLlmPhase = ""; return; }

                        // ── Phase 2: Screenshot vision follow-up ─────
                        const screenshotFiles = ssNodes.slice(0, 3).filter(n => n.filename);
                        if (screenshotFiles.length > 0) {
                          // Show screenshot thumbnails immediately
                          ixLlmScreenshots = screenshotFiles.map(sn => ({
                            url: imgSrc(sn.filename!),
                            label: [sn.app_name, sn.window_title, sn.timestamp_unix ? new Date(sn.timestamp_unix * 1000).toLocaleString() : ""].filter(Boolean).join(" — "),
                          }));

                          try {
                            // Ensure vision is available — load mmproj if needed
                            type LlmStatus = { supports_vision?: boolean; status?: string };
                            let st = await daemonInvoke<LlmStatus>("get_llm_server_status");

                            if (!st.supports_vision) {
                              type CatEntry = { filename: string; is_mmproj: boolean; state: string };
                              type Catalog = { entries: CatEntry[] };
                              const cat = await daemonInvoke<Catalog>("get_llm_catalog");
                              const mmproj = cat?.entries?.find(e => e.is_mmproj && e.state === "downloaded");
                              if (mmproj) {
                                ixLlmPhase = "vision-loading";
                                await daemonInvoke("switch_llm_mmproj", { filename: mmproj.filename });
                                // Wait for server to restart with vision
                                for (let i = 0; i < 60; i++) {
                                  await new Promise(r => setTimeout(r, 1000));
                                  st = await daemonInvoke<LlmStatus>("get_llm_server_status");
                                  if (st.supports_vision) break;
                                  if (st.status === "stopped") break;
                                }
                              }
                            }

                            if (!st.supports_vision) {
                              // No vision available — skip screenshot analysis
                              ixLlmScreenshots = [];
                            } else {
                              // Fetch screenshot images as base64
                              ixLlmPhase = "vision-analyzing";
                              const imgParts: Array<{type: string; text?: string; image_url?: {url: string}}> = [];
                              imgParts.push({ type: "text", text: `Now look at these ${screenshotFiles.length} screenshot(s) captured during the "${ixQuery}" sessions. Update and enhance your analysis by incorporating what you see on screen. Explain how the visible activity relates to the EEG brain state patterns (engagement, relaxation, focus) you identified. Rewrite your full response with the screenshot context integrated — don't repeat raw metrics, but add new insights from the visual context.` });

                              let loadedCount = 0;
                              for (const sn of screenshotFiles) {
                                try {
                                  const url = imgSrc(sn.filename!);
                                  // Convert to PNG via canvas (LLM decoder doesn't support WebP)
                                  const dataUrl: string = await new Promise((resolve, reject) => {
                                    const img = new Image();
                                    img.crossOrigin = "anonymous";
                                    img.onload = () => {
                                      const canvas = document.createElement("canvas");
                                      canvas.width = img.naturalWidth;
                                      canvas.height = img.naturalHeight;
                                      const ctx2d = canvas.getContext("2d");
                                      if (!ctx2d) { reject(new Error("no canvas")); return; }
                                      ctx2d.drawImage(img, 0, 0);
                                      resolve(canvas.toDataURL("image/png"));
                                    };
                                    img.onerror = () => reject(new Error("img load failed"));
                                    img.src = url;
                                  });
                                  imgParts.push({ type: "image_url", image_url: { url: dataUrl } });
                                  const ts = sn.timestamp_unix ? new Date(sn.timestamp_unix * 1000).toLocaleString() : "";
                                  const label = [sn.app_name, sn.window_title, ts].filter(Boolean).join(" — ");
                                  if (label) imgParts.push({ type: "text", text: `(${label})` });
                                  loadedCount++;
                                } catch { /* skip this screenshot */ }
                              }

                              if (loadedCount > 0) {
                                const textSummary = acc; // preserve original
                                let visionError = "";
                                let visionAcc = "";

                                const visionChannel = {
                                  onmessage: (chunk2: { type: string; content?: string; message?: string }) => {
                                    if (chunk2.type === "delta" && chunk2.content) {
                                      visionAcc += chunk2.content;
                                      // Replace the display with the enhanced version
                                      ixLlmSummary = visionAcc;
                                    } else if (chunk2.type === "error") {
                                      visionError = chunk2.message ?? "Vision analysis failed";
                                    }
                                  }
                                };
                                try {
                                  await daemonInvoke("chat_completions_ipc", {
                                    messages: [
                                      { role: "user", content: prompt },
                                      { role: "assistant", content: textSummary },
                                      { role: "user", content: imgParts },
                                    ],
                                    params: { temperature: 0.3, max_tokens: maxTokens },
                                    channel: visionChannel,
                                  });
                                } catch (e) {
                                  visionError = String(e);
                                }
                                if (visionAcc) {
                                  // Use the enhanced version
                                  acc = visionAcc;
                                } else if (visionError) {
                                  console.warn("Vision analysis error:", visionError);
                                }
                                // If vision failed, keep the original text summary (acc unchanged)
                              }
                            }
                          } catch (e) {
                            // Vision failed — log but keep text summary
                            console.warn("Screenshot vision analysis failed:", e);
                          }
                        }

                        ixLlmPhase = "done";
                        ixLlmLoading = false;
                        saveSummaryToChat(acc);
                      }}
                      class="text-xs px-2.5 py-1 rounded-md border border-violet-500/30
                             bg-violet-500/10 text-violet-600 dark:text-violet-400
                             hover:bg-violet-500/20 transition-colors select-none">
                        {t("search.generateSummary")}
                      </button>
                      <!-- Max tokens slider -->
                      <div class="flex items-center gap-1.5 ml-auto">
                        <span class="text-[0.5rem] text-muted-foreground/40 select-none">{t("search.maxTokens")}</span>
                        <input type="range" min="512" max="8192" step="256" bind:value={ixLlmMaxTokens}
                               class="w-16 h-1 accent-violet-500 cursor-pointer" />
                        <span class="text-[0.5rem] text-muted-foreground/50 font-mono w-8 text-right">{ixLlmMaxTokens}</span>
                      </div>
                    {/if}
                  </div>
                  <!-- Show prompt immediately when generating -->
                  {#if ixLlmPrompt && (ixLlmLoading || ixLlmSummary)}
                    <details class="mb-2 rounded-lg border border-border/20 bg-muted/5 overflow-hidden" open={!ixLlmSummary}>
                      <summary class="flex items-center gap-2 px-3 py-1.5 cursor-pointer text-xs text-muted-foreground/50 hover:text-muted-foreground/80 select-none">
                        <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3 shrink-0"><path d="M6 3l5 5-5 5V3z"/></svg>
                        Prompt
                      </summary>
                      <div class="px-3 pb-2 pt-1 text-xs text-muted-foreground/50 leading-relaxed border-t border-border/10 whitespace-pre-line font-mono">
                        {ixLlmPrompt}
                      </div>
                    </details>
                  {/if}
                  {#if ixLlmLoading && !ixLlmSummary}
                    <div class="flex items-center gap-2 text-sm text-muted-foreground/50">
                      <div class="w-3 h-3 rounded-full border-2 border-violet-500/30 border-t-violet-500 animate-spin"></div>
                      Analyzing EEG data...
                    </div>
                  {:else if ixLlmSummary === "__NO_LLM__"}
                    <div class="flex flex-col gap-2">
                      <p class="text-sm text-muted-foreground/60">{t("search.llmNotLoaded")}</p>
                      <button onclick={async () => {
                        ixLlmSummary = "";
                        ixLlmLoading = true;
                        try {
                          await daemonInvoke("start_llm_server", {});
                          // Wait a moment for the server to start
                          await new Promise(r => setTimeout(r, 3000));
                          // Retry the summary
                          const { daemonPost } = await import("$lib/daemon/http");
                          const res2 = await daemonPost<{ choices?: Array<{ message?: { content?: string } }>; content?: string }>("/v1/llm/chat-completions", {
                            messages: [{ role: "user", content: ixLlmPrompt }],
                            temperature: 0.3,
                            max_tokens: 1024,
                          }, 120_000);
                          ixLlmSummary = res2?.choices?.[0]?.message?.content ?? res2?.content ?? "__NO_LLM__";
                        } catch (err) {
                          ixLlmSummary = `Could not generate summary: ${err}`;
                        }
                        ixLlmLoading = false;
                      }}
                      class="text-xs px-3 py-1.5 rounded-md border border-amber-500/30
                             bg-amber-500/10 text-amber-600 dark:text-amber-400
                             hover:bg-amber-500/20 transition-colors select-none w-fit">
                        {t("search.startLlmRetry")}
                      </button>
                    </div>
                  {:else if ixLlmSummary && ixLlmSummary !== "__NO_LLM__"}
                    {@const parsed = parseAssistantOutput(ixLlmSummary)}
                    <!-- Thinking block (collapsible) -->
                    {#if parsed.thinking}
                      {#each [false] as _, _i}
                        {@const thinkId = `llm-think-${_i}`}
                        <details class="mb-2 rounded-lg border border-violet-500/15 bg-violet-500/5 overflow-hidden">
                          <summary class="flex items-center gap-2 px-3 py-1.5 cursor-pointer text-xs text-violet-500/70 hover:text-violet-500 select-none">
                            <svg viewBox="0 0 16 16" fill="currentColor" class="w-3 h-3 shrink-0"><path d="M6 3l5 5-5 5V3z"/></svg>
                            Thought
                            <span class="ml-auto text-[0.6rem] text-muted-foreground/40">{parsed.thinking.trim().split(/\s+/).length} words</span>
                          </summary>
                          <div class="px-3 pb-2 pt-1 text-xs text-muted-foreground/60 leading-relaxed border-t border-violet-500/10 whitespace-pre-line">
                            {parsed.thinking}
                          </div>
                        </details>
                      {/each}
                    {/if}
                    <!-- Main content (rendered as markdown) -->
                    {#if parsed.content}
                      <div class="text-sm text-foreground/70 leading-relaxed prose prose-sm dark:prose-invert max-w-none
                                  prose-p:my-1.5 prose-ul:my-1 prose-li:my-0.5
                                  prose-headings:text-foreground/80
                                  prose-h1:text-sm prose-h1:font-bold prose-h1:mt-3 prose-h1:mb-1.5
                                  prose-h2:text-sm prose-h2:font-bold prose-h2:mt-3 prose-h2:mb-1.5
                                  prose-h3:text-xs prose-h3:font-semibold prose-h3:mt-2 prose-h3:mb-1
                                  prose-h4:text-xs prose-h4:font-semibold prose-h4:mt-2 prose-h4:mb-1
                                  prose-table:text-xs prose-th:px-2 prose-th:py-1 prose-td:px-2 prose-td:py-1
                                  prose-blockquote:border-violet-500/30 prose-blockquote:bg-violet-500/5 prose-blockquote:rounded-md prose-blockquote:px-3 prose-blockquote:py-2 prose-blockquote:not-italic
                                  prose-strong:text-foreground/80
                                  prose-code:text-xs prose-code:bg-muted/30 prose-code:px-1 prose-code:rounded">
                        <MarkdownRenderer content={parsed.content} />
                      </div>
                    {/if}
                    <!-- Screenshot thumbnails -->
                    {#if ixLlmScreenshots.length > 0}
                      <div class="mt-3 pt-3 border-t border-border/20">
                        <div class="flex items-center gap-2 mb-2.5">
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"
                               class="w-3.5 h-3.5 shrink-0 {ixLlmPhase === 'vision-analyzing' ? 'text-cyan-500' : 'text-muted-foreground/50'}">
                            <rect x="2" y="3" width="20" height="14" rx="2"/><circle cx="8" cy="10" r="2"/>
                            <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>
                          </svg>
                          <span class="text-[0.65rem] font-semibold text-foreground/60 uppercase tracking-wider">
                            {ixLlmScreenshots.length} Screenshot{ixLlmScreenshots.length !== 1 ? "s" : ""} Found
                          </span>
                          {#if ixLlmPhase === "vision-loading"}
                            <div class="flex items-center gap-1.5 ml-1">
                              <div class="w-2 h-2 rounded-full border border-amber-500/40 border-t-amber-500 animate-spin"></div>
                              <span class="text-[0.6rem] text-amber-500/70">Loading vision model...</span>
                            </div>
                          {:else if ixLlmPhase === "vision-analyzing"}
                            <div class="flex items-center gap-1.5 ml-1">
                              <div class="w-2 h-2 rounded-full border border-cyan-500/40 border-t-cyan-500 animate-spin"></div>
                              <span class="text-[0.6rem] text-cyan-500/70">Analyzing...</span>
                            </div>
                          {:else if ixLlmPhase === "done" || !ixLlmLoading}
                            <span class="text-[0.5rem] px-1.5 py-0.5 rounded-full bg-emerald-500/10 text-emerald-500/70 font-medium">done</span>
                          {/if}
                        </div>
                        <div class="flex gap-2 overflow-x-auto pb-1">
                          {#each ixLlmScreenshots as ss, idx}
                            {@const analyzing = ixLlmPhase === "vision-analyzing"}
                            {@const loading = ixLlmPhase === "vision-loading"}
                            <div class="shrink-0 flex flex-col items-start gap-0.5">
                              <div class="relative rounded-md border overflow-hidden
                                          {analyzing ? 'border-cyan-500/40 ring-1 ring-cyan-500/30' : 'border-border/30'}
                                          {loading ? 'opacity-50' : ''}">
                                <img src={ss.url} alt={ss.label}
                                     class="h-20 w-auto max-w-[14rem] object-cover bg-muted/20"
                                     onerror={(e) => {
                                       const t = e.currentTarget as HTMLImageElement;
                                       t.style.height = "3rem";
                                       t.style.width = "5rem";
                                       t.alt = "Failed to load";
                                     }}
                                     loading="lazy" />
                                {#if analyzing}
                                  <div class="absolute inset-0 bg-cyan-500/5 animate-pulse pointer-events-none"></div>
                                {/if}
                              </div>
                              {#if ss.label}
                                <span class="text-[0.5rem] text-muted-foreground/40 truncate max-w-[14rem] leading-tight">
                                  {ss.label}
                                </span>
                              {/if}
                            </div>
                          {/each}
                        </div>
                      </div>
                    {/if}

                    {#if ixLlmLoading && ixLlmPhase === "text"}
                      <div class="flex items-center gap-2 mt-2 text-xs text-muted-foreground/40">
                        <div class="w-2.5 h-2.5 rounded-full border-2 border-violet-500/30 border-t-violet-500 animate-spin"></div>
                        Generating...
                      </div>
                    {:else if ixLlmLoading && ixLlmPhase === "vision-analyzing"}
                      <div class="flex items-center gap-2 mt-2 text-xs text-cyan-500/50">
                        <div class="w-2.5 h-2.5 rounded-full border-2 border-cyan-500/30 border-t-cyan-500 animate-spin"></div>
                        Analyzing screenshots...
                      </div>
                    {:else if !ixLlmLoading}
                      <!-- Continue in Chat button -->
                      <div class="flex items-center gap-2 mt-3 pt-2 border-t border-border/20">
                        <button onclick={async () => {
                          try {
                            // Reuse the auto-saved session, or create one if it wasn't saved yet
                            let sid = ixLlmSessionId;
                            if (!sid) {
                              const res = await daemonInvoke<{id: number}>("new_chat_session");
                              sid = res?.id ?? 0;
                              if (sid > 0) {
                                ixLlmSessionId = sid;
                                await daemonInvoke("rename_chat_session", { id: sid, title: `Search: ${ixQuery}` });
                                await daemonInvoke("save_chat_message", { sessionId: sid, role: "user", content: ixLlmPrompt, thinking: null });
                                const parsed = parseAssistantOutput(ixLlmSummary);
                                await daemonInvoke("save_chat_message", {
                                  sessionId: sid,
                                  role: "assistant",
                                  content: [parsed.leadIn, parsed.content].filter(s => s.trim()).join("\n\n"),
                                  thinking: parsed.thinking || null,
                                });
                              }
                            }
                            const { invoke } = await import("@tauri-apps/api/core");
                            await invoke("open_chat_window", { sessionId: sid > 0 ? sid : null });
                          } catch { /* ignore nav errors */ }
                        }}
                        class="text-xs px-3 py-1.5 rounded-md border border-blue-500/30
                               bg-blue-500/10 text-blue-600 dark:text-blue-400
                               hover:bg-blue-500/20 transition-colors select-none">
                          {t("search.continueInChat")}
                        </button>
                      </div>
                    {/if}
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        {/if}

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
