// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Search page business logic — extracted from the monolithic +page.svelte.
 *
 * Pure functions and helpers that don't depend on Svelte reactivity.
 * Keep UI state management in the Svelte component; keep data transformations here.
 */

import type { UmapResult, UmapPoint } from "$lib/types";
import type { SearchResult, QueryEntry, SearchAnalysis } from "$lib/search-types";
import { computeSearchAnalysis, computeTemporalHeatmap } from "$lib/search-types";
import {
  fmtDateTimeLocalInput, parseDateTimeLocalInput,
  fmtDuration as fmtDurationSecs,
} from "$lib/format";

// ── Time helpers ──────────────────────────────────────────────────────────────

/** Format a Date as a datetime-local input value. */
export function toInputValue(d: Date): string {
  return fmtDateTimeLocalInput(Math.floor(d.getTime() / 1000));
}

/** Parse a datetime-local input value to a Unix timestamp. */
export function fromInputValue(s: string): number {
  return parseDateTimeLocalInput(s);
}

/** Format a start/end pair as a human-readable duration. */
export function fmtDuration(s: number, e: number): string {
  return fmtDurationSecs(e - s);
}

// ── UMAP label enrichment ─────────────────────────────────────────────────────

/**
 * Build a utc-seconds → label-text map from every labeled neighbor in the
 * current EEG search result.
 */
export function buildNeighborLabelMap(result: SearchResult | null): Map<number, string> {
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
export function enrichUmapLabels(raw: UmapResult, labelMap: Map<number, string>): UmapResult {
  if (labelMap.size === 0) return raw;
  const points = raw.points.map((pt: UmapPoint) =>
    (!pt.label && labelMap.has(pt.utc)) ? { ...pt, label: labelMap.get(pt.utc)! } : pt
  );
  return { ...raw, points };
}

// ── Search mode ───────────────────────────────────────────────────────────────

export type SearchMode = "eeg" | "text" | "interactive" | "images";

export const SEARCH_MODE_EVENT = "skill:search-mode";
export const SEARCH_SET_MODE_EVENT = "skill:search-set-mode";

export function normalizeSearchMode(value: unknown): SearchMode {
  return value === "eeg" || value === "text" || value === "interactive" || value === "images"
    ? value
    : "interactive";
}

export function emitSearchMode(value: SearchMode): void {
  window.dispatchEvent(new CustomEvent(SEARCH_MODE_EVENT, { detail: { mode: value } }));
}

// ── Analysis chips ────────────────────────────────────────────────────────────

export function analysisChips(
  sa: SearchAnalysis,
  tFn: (key: string) => string,
): Array<[string, string, string]> {
  return [
    [tFn("search.analysisNeighbors"), sa.totalNeighbors.toString(), ""],
    [tFn("search.analysisDistMin"), sa.distMin.toFixed(4), "text-emerald-500"],
    [tFn("search.analysisMeanSd"), `${sa.distMean.toFixed(4)} \u00b1 ${sa.distStddev.toFixed(4)}`, ""],
    [tFn("search.analysisDistMax"), sa.distMax.toFixed(4), "text-red-400"],
    [tFn("search.analysisPeakHour"), `${String(sa.peakHour).padStart(2, "0")}:00`, ""],
  ];
}

// ── Preset helpers ────────────────────────────────────────────────────────────

/** Compute start/end input values for a preset duration. */
export function computePreset(mins: number): { start: string; end: string } {
  const e = new Date();
  const s = new Date(e.getTime() - mins * 60_000);
  return { start: toInputValue(s), end: toInputValue(e) };
}
