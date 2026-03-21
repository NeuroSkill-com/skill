// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Types and pure helper functions for the search page.
 *
 * Extracted from `routes/search/+page.svelte`.
 */

// ── Types ────────────────────────────────────────────────────────────────────

export interface NeighborMetrics {
  focus?: number; relaxation?: number; engagement?: number;
  faa?: number; tar?: number; mood?: number;
  meditation?: number; cognitive_load?: number; drowsiness?: number;
  hr?: number; snr?: number;
  rel_alpha?: number; rel_beta?: number; rel_theta?: number;
}

export interface LabelEntry {
  id: number; eeg_start: number; eeg_end: number;
  label_start: number; label_end: number; text: string;
}

export interface NeighborEntry {
  hnsw_id: number; timestamp: number; timestamp_unix: number;
  distance: number; date: string;
  device_id: string | null; device_name: string | null;
  labels: LabelEntry[];
  metrics?: NeighborMetrics;
}

export interface QueryEntry {
  timestamp: number; timestamp_unix: number; neighbors: NeighborEntry[];
}

export interface SearchResult {
  start_utc: number; end_utc: number; k: number; ef: number;
  query_count: number; searched_days: string[];
  results: QueryEntry[];
}

export interface LabelNeighbor {
  label_id: number; text: string; context: string;
  eeg_start: number; eeg_end: number; created_at: number;
  embedding_model?: string; distance: number;
  eeg_metrics?: NeighborMetrics;
}

export interface SearchAnalysis {
  totalNeighbors: number;
  distMin: number; distMax: number; distMean: number; distStddev: number;
  hourHist: number[]; topDays: { day: string; count: number }[]; peakHour: number;
}

export interface GraphNode {
  id: string; kind: "query" | "text_label" | "eeg_point" | "found_label" | "screenshot";
  text?: string; timestamp_unix?: number; distance: number;
  eeg_metrics?: Record<string, number | null> | null; parent_id?: string;
  proj_x?: number; proj_y?: number;
  /** Screenshot image URL — only present on kind === "screenshot" nodes. */
  screenshot_url?: string;
  /** Screenshot filename — for backend SVG re-generation. */
  filename?: string;
  /** App name at capture — for backend SVG re-generation. */
  app_name?: string;
  /** Window title at capture — for backend SVG re-generation. */
  window_title?: string;
  /** OCR similarity score. */
  ocr_similarity?: number;
}

export interface GraphEdge {
  from_id: string; to_id: string; distance: number;
  kind: "text_sim" | "eeg_bridge" | "eeg_sim" | "label_prox" | "screenshot_link";
}

export interface JobTicket {
  job_id: number; estimated_ready_utc: number;
  queue_position: number; estimated_secs: number;
}

export interface JobPollResult {
  status: string; job_id: number; result?: unknown;
  elapsed_ms?: number; error?: string;
  queue_position?: number; estimated_secs?: number;
  progress?: Record<string, unknown>;
}

export interface ImgResult {
  timestamp: number; unix_ts: number; filename: string;
  app_name: string; window_title: string; ocr_text: string; similarity: number;
}

// ── Pure helpers ─────────────────────────────────────────────────────────────

/** Distance → color indicator. */
export function distColor(d: number): string {
  if (d < 0.001) return "#22c55e";
  if (d < 0.05)  return "#3b82f6";
  if (d < 0.15)  return "#f59e0b";
  return "#94a3b8";
}

/** Similarity percentage string. */
export function simPct(d: number, max: number): string {
  if (max <= 0) return "100%";
  return ((1 - d / max) * 100).toFixed(1) + "%";
}

/** Similarity width (0–1). */
export function simWidth(d: number, max: number): number {
  if (max <= 0) return 1;
  return Math.max(0, 1 - d / max);
}

/** EEG metric chips from a NeighborMetrics record. */
export function metricChips(met: NeighborMetrics): { l: string; v: string; c: string }[] {
  return [
    met.focus           != null ? { l: "Focus",  v: met.focus.toFixed(0),           c: "#3b82f6" } : null,
    met.relaxation      != null ? { l: "Relax",  v: met.relaxation.toFixed(0),      c: "#10b981" } : null,
    met.engagement      != null ? { l: "Engage", v: met.engagement.toFixed(0),      c: "#f59e0b" } : null,
    met.meditation      != null ? { l: "Med",    v: met.meditation.toFixed(0),      c: "#8b5cf6" } : null,
    met.mood            != null ? { l: "Mood",   v: met.mood.toFixed(0),            c: "#f59e0b" } : null,
    met.cognitive_load  != null ? { l: "Cog",    v: met.cognitive_load.toFixed(0),  c: "#0ea5e9" } : null,
    met.drowsiness      != null ? { l: "Drow",   v: met.drowsiness.toFixed(0),      c: "#ef4444" } : null,
    met.hr != null && met.hr > 0 ? { l: "HR",   v: met.hr.toFixed(0),              c: "#ef4444" } : null,
    met.faa != null ? { l: "FAA", v: (met.faa >= 0 ? "+" : "") + met.faa.toFixed(2), c: "#8b5cf6" } : null,
    met.snr != null ? { l: "SNR", v: met.snr.toFixed(1), c: "#0ea5e9" } : null,
  ].filter((x): x is { l: string; v: string; c: string } => x !== null);
}

/** Compute search analysis from results. */
export function computeSearchAnalysis(result: SearchResult): SearchAnalysis | null {
  if (result.results.length === 0) return null;
  const allNb = result.results.flatMap(q => q.neighbors);
  if (allNb.length === 0) return null;
  const dists = allNb.map(n => n.distance);
  const distMin  = Math.min(...dists), distMax = Math.max(...dists);
  const distMean = dists.reduce((a, b) => a + b, 0) / dists.length;
  const distStddev = Math.sqrt(dists.reduce((s, d) => s + (d - distMean)**2, 0) / dists.length);
  const hourHist   = new Array(24).fill(0);
  for (const n of allNb) hourHist[new Date(n.timestamp_unix * 1000).getHours()]++;
  const peakHour   = hourHist.indexOf(Math.max(...hourHist));
  const dayCounts  = new Map<string, number>();
  for (const n of allNb) {
    const d = new Date(n.timestamp_unix * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
    dayCounts.set(d, (dayCounts.get(d) || 0) + 1);
  }
  const topDays = [...dayCounts.entries()].sort((a, b) => b[1]-a[1]).slice(0,5).map(([day,count]) => ({day,count}));
  return { totalNeighbors: allNb.length, distMin, distMax, distMean, distStddev, hourHist, topDays, peakHour };
}

/** Compute temporal heatmap (7 days × 24 hours) from search results. */
export function computeTemporalHeatmap(result: SearchResult): number[][] | null {
  if (result.results.length === 0) return null;
  const grid = Array.from({ length: 7 }, () => new Array(24).fill(0));
  for (const q of result.results)
    for (const n of q.neighbors)
      grid[new Date(n.timestamp_unix*1000).getDay()][new Date(n.timestamp_unix*1000).getHours()]++;
  return grid.flat().some(v => v > 0) ? grid : null;
}

/** Heat color for temporal heatmap cells. */
export function heatColor(count: number, max: number): string {
  if (count === 0) return "transparent";
  const t = count / max;
  return `rgba(${Math.round(59+(1-t)*180)},${Math.round(130+(1-t)*100)},246,${0.15+t*0.75})`;
}

/** Turbo/jet colormap. */
export function turboColor(t: number): string {
  const c = Math.max(0, Math.min(1, t));
  const r = Math.max(0, Math.min(1, 0.13572138 + c * (4.61539260 + c * (-42.66032258 + c * (132.13108234 + c * (-152.54893924 + c *  59.28637943))))));
  const g = Math.max(0, Math.min(1, 0.09140261 + c * (2.19418839 + c * (  4.84296658 + c * (-14.18503333 + c * (  4.27729857 + c *   2.82956604))))));
  const b = Math.max(0, Math.min(1, 0.10667330 + c * (12.64194608 + c * (-60.58204836 + c * (110.36276771 + c * (-89.90310912 + c *  27.34824973))))));
  const h = (v: number) => Math.round(v * 255).toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

/** Locale-appropriate short day names (Sun=index 0). */
export const DAY_NAMES = Array.from({ length: 7 }, (_, i) =>
  new Date(Date.UTC(2023, 0, i + 1)).toLocaleDateString(undefined, { weekday: "short" })
);

/** Time presets for EEG search. */
export const PRESETS: [string, number][] = [
  ["5m", 5], ["30m", 30], ["1h", 60], ["6h", 360], ["24h", 1440],
];

import type { UmapPoint, UmapResult } from "$lib/types";
import { dateToCompactKey, fromUnix } from "$lib/format";

/**
 * Build a client-side UmapResult for text kNN results:
 * query at origin (session 0), results on a Fibonacci sphere (session 1).
 */
export function buildTextKnnGraph(results: LabelNeighbor[], query: string): UmapResult {
  const GRAPH_SCALE = 12;
  const pts: UmapPoint[] = [];
  pts.push({ x: 0, y: 0, z: 0, session: 0, utc: 0, label: query.slice(0, 80) });

  if (results.length === 0) return { points: pts, n_a: 1, n_b: 0, dim: 3 };

  const sorted = [...results].sort((a, b) => a.distance - b.distance);
  const minD   = sorted[0].distance;
  const maxD   = sorted[sorted.length - 1].distance;
  const range  = maxD - minD || 1;
  const golden = Math.PI * (3 - Math.sqrt(5));
  const n = sorted.length;

  for (let i = 0; i < n; i++) {
    const nb = sorted[i];
    const t      = n === 1 ? 0.5 : (nb.distance - minD) / range;
    const radius = (0.18 + t * 0.70) * GRAPH_SCALE;
    const yN     = 1 - (i / Math.max(n - 1, 1)) * 2;
    const rN     = Math.sqrt(Math.max(0, 1 - yN * yN));
    const theta  = golden * i;
    pts.push({
      x: Math.cos(theta) * rN * radius,
      y: yN * radius,
      z: Math.sin(theta) * rN * radius,
      session: 1, utc: nb.eeg_start,
      label: nb.text.slice(0, 80), dist: nb.distance,
    });
  }
  return { points: pts, n_a: 1, n_b: n, dim: 3 };
}

/**
 * Compute interactive-mode time heatmap from graph nodes.
 */
export function computeIxTimeHeatmap(nodes: GraphNode[]) {
  const eeg = nodes.filter(n => n.kind === "eeg_point" && n.timestamp_unix != null);
  if (eeg.length === 0) return null;

  const daySet = new Set<string>();
  for (const n of eeg) daySet.add(dateToCompactKey(fromUnix(n.timestamp_unix!)));
  const days = [...daySet].sort();

  const grid: number[][] = days.map(() => new Array(24).fill(0));
  for (const n of eeg) {
    const d   = fromUnix(n.timestamp_unix!);
    const key = dateToCompactKey(d);
    const di  = days.indexOf(key);
    if (di >= 0) grid[di][d.getHours()]++;
  }

  const allTs = eeg.map(n => n.timestamp_unix!);
  const tMin  = Math.min(...allTs);
  const tMax  = Math.max(...allTs);

  const dayLabels = days.map(d =>
    new Date(Date.UTC(+d.slice(0,4), +d.slice(4,6)-1, +d.slice(6,8), 12))
      .toLocaleDateString(undefined, { month: "short", day: "numeric" })
  );

  const maxCount = Math.max(...grid.flat(), 1);
  return { days, dayLabels, grid, maxCount, tMin, tMax };
}

/** Interactive heatmap cell color. */
export function ixHeatColor(
  dayIdx: number, hour: number, count: number,
  heatmap: { days: string[]; tMin: number; tMax: number; maxCount: number },
): string {
  if (count === 0) return "transparent";
  const cellUnix = new Date(
    +heatmap.days[dayIdx].slice(0,4),
    +heatmap.days[dayIdx].slice(4,6)-1,
    +heatmap.days[dayIdx].slice(6,8),
    hour, 30
  ).getTime() / 1000;
  const tRange = heatmap.tMax - heatmap.tMin || 1;
  const t = Math.max(0, Math.min(1, (cellUnix - heatmap.tMin) / tRange));
  const alpha = 0.15 + (count / heatmap.maxCount) * 0.75;
  const hex = turboColor(t);
  const rv = parseInt(hex.slice(1,3), 16);
  const gv = parseInt(hex.slice(3,5), 16);
  const bv = parseInt(hex.slice(5,7), 16);
  return `rgba(${rv},${gv},${bv},${alpha.toFixed(2)})`;
}

/**
 * Deduplicate found_label nodes by text and remap edges.
 */
export function dedupeFoundLabels(
  nodes: GraphNode[], edges: GraphEdge[],
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const canonMap = new Map<string, string>();
  const keepNodes: GraphNode[] = [];

  for (const n of nodes) {
    if (n.kind !== "found_label") { keepNodes.push(n); continue; }
    const key = (n.text ?? "").trim().toLowerCase();
    if (!canonMap.has(key)) { canonMap.set(key, n.id); keepNodes.push(n); }
  }

  const idToText = new Map<string, string>();
  for (const n of nodes) {
    if (n.kind === "found_label") idToText.set(n.id, (n.text ?? "").trim().toLowerCase());
  }

  const seenEdge = new Set<string>();
  const keepEdges: GraphEdge[] = [];
  for (const e of edges) {
    const textKey = idToText.get(e.to_id);
    const canonId = textKey !== undefined ? (canonMap.get(textKey) ?? e.to_id) : e.to_id;
    const sig = `${e.from_id}→${canonId}`;
    if (seenEdge.has(sig)) continue;
    seenEdge.add(sig);
    keepEdges.push(canonId === e.to_id ? e : { ...e, to_id: canonId });
  }

  return { nodes: keepNodes, edges: keepEdges };
}
