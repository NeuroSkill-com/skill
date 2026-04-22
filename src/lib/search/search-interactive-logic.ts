// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
/**
 * Interactive search mode logic — extracted from search/+page.svelte.
 *
 * Pure functions for graph display, screenshot enrichment, and DOT/SVG
 * node serialisation. No Svelte reactivity — can be unit-tested independently.
 */

import type { GraphEdge, GraphNode } from "$lib/search/search-types";
import { dedupeFoundLabels } from "$lib/search/search-types";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface SsEntry {
  filename: string;
  appName: string;
  windowTitle: string;
  unixTs: number;
  similarity: number;
}

export interface SsResult {
  unix_ts: number;
  filename: string;
  app_name: string;
  window_title: string;
  similarity: number;
}

// ── Display graph derivation ──────────────────────────────────────────────────

/**
 * Build the display graph by optionally deduplicating found-labels and
 * injecting screenshot nodes from the screenshot map.
 */
export function buildDisplayGraph(
  nodes: GraphNode[],
  edges: GraphEdge[],
  dedupeLabels: boolean,
  showScreenshots: boolean,
  screenshotMap: Map<string, SsEntry>,
  imgSrc: (filename: string) => string,
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  let { nodes: n, edges: e } = dedupeLabels
    ? dedupeFoundLabels(nodes, edges)
    : { nodes: [...nodes], edges: [...edges] };

  if (showScreenshots && screenshotMap.size > 0) {
    const extraNodes: GraphNode[] = [];
    const extraEdges: GraphEdge[] = [];
    const nodeIds = new Set(n.map((nd) => nd.id));
    let idx = 0;
    for (const [key, ss] of screenshotMap) {
      const sepIdx = key.indexOf("\0");
      const parentId = sepIdx > 0 ? key.slice(0, sepIdx) : key;
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
}

// ── Screenshot result collection ──────────────────────────────────────────────

/**
 * Accumulate screenshot results into a deduped Map keyed by "nodeId\0filename".
 * Returns a new Map (immutable update pattern for Svelte reactivity).
 */
export function collectScreenshotResults(results: Array<{ nodeId: string; hits: SsResult[] }>): Map<string, SsEntry> {
  const map = new Map<string, SsEntry>();
  const usedFilenames = new Set<string>();

  for (const { nodeId, hits } of results) {
    for (const r of hits) {
      if (usedFilenames.has(r.filename)) continue;
      usedFilenames.add(r.filename);
      map.set(`${nodeId}\0${r.filename}`, {
        filename: r.filename,
        appName: r.app_name,
        windowTitle: r.window_title,
        unixTs: r.unix_ts,
        similarity: r.similarity ?? 0,
      });
    }
  }

  return map;
}

// ── DOT/SVG node serialisation ────────────────────────────────────────────────

/**
 * Serialise display-graph nodes for the backend `regenerate_interactive_*` commands.
 */
export function serialiseNodesForBackend(nodes: GraphNode[]) {
  return nodes.map((n) => ({
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
  }));
}

/**
 * Serialise display-graph edges for the backend.
 */
export function serialiseEdgesForBackend(edges: GraphEdge[]) {
  return edges.map((e) => ({
    from_id: e.from_id,
    to_id: e.to_id,
    distance: e.distance,
    kind: e.kind,
  }));
}

/**
 * Find the closest screenshot to a target timestamp from a list.
 */
export function closestScreenshot(screenshots: SsResult[], targetTimestamp: number): SsResult | null {
  if (screenshots.length === 0) return null;
  return [...screenshots].sort(
    (a, b) => Math.abs(a.unix_ts - targetTimestamp) - Math.abs(b.unix_ts - targetTimestamp),
  )[0];
}
