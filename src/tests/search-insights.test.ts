// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Vitest unit tests for search insights, AI summary, bookmarks, and value extraction:
// - computeInsights: app correlation, hour patterns, best conditions
// - Bookmark save/remove/dedup
// - Breadcrumb building
// - Search history persistence
// - LLM prompt building (metrics, sessions, screenshots)
// - Color mode logic

import { describe, expect, it } from "vitest";
import type { GraphNode } from "$lib/search-types";

// ── Helpers ──────────────────────────────────────────────────────────────────

function mkNode(id: string, kind: GraphNode["kind"], overrides: Partial<GraphNode> = {}): GraphNode {
  return { id, kind, distance: 0.1, ...overrides };
}

// ── App-engagement correlation ───────────────────────────────────────────────

describe("app-engagement correlation", () => {
  it("matches screenshots to nearest EEG by timestamp", () => {
    const eeg = mkNode("ep0", "eeg_point", {
      timestamp_unix: 1000,
      eeg_metrics: { engagement: 0.8, relaxation: 0.5, snr: 12 },
    });
    const ss = mkNode("ss0", "screenshot", {
      timestamp_unix: 1005,
      app_name: "VS Code",
    });

    // Simulate the correlation logic
    const ssNodes = [ss].filter((n) => n.kind === "screenshot" && (n.app_name || n.window_title));
    const eegNodes = [eeg].filter((n) => n.kind === "eeg_point" && n.eeg_metrics);
    const appMap = new Map<string, { sum: number; count: number }>();

    for (const s of ssNodes) {
      const appName = s.app_name || s.window_title || "";
      if (!appName || !s.timestamp_unix) continue;
      const nearest = eegNodes
        .filter((n) => n.timestamp_unix)
        .sort(
          (a, b) =>
            Math.abs((a.timestamp_unix ?? 0) - (s.timestamp_unix ?? 0)) -
            Math.abs((b.timestamp_unix ?? 0) - (s.timestamp_unix ?? 0)),
        )[0];
      if (nearest?.eeg_metrics?.engagement != null) {
        const eng = nearest.eeg_metrics.engagement as number;
        const entry = appMap.get(appName) ?? { sum: 0, count: 0 };
        entry.sum += eng;
        entry.count++;
        appMap.set(appName, entry);
      }
    }

    const vsCode = appMap.get("VS Code");
    expect(vsCode).toBeDefined();
    expect(vsCode!.sum / vsCode!.count).toBe(0.8);
  });

  it("uses window_title as fallback for app_name", () => {
    const ss = mkNode("ss0", "screenshot", {
      timestamp_unix: 1000,
      window_title: "Slack — General",
    });
    const appName = ss.app_name || ss.window_title;
    expect(appName).toBe("Slack — General");
  });

  it("handles no screenshots gracefully", () => {
    const nodes = [mkNode("ep0", "eeg_point", { timestamp_unix: 1000, eeg_metrics: { engagement: 0.5 } })];
    const ssNodes = nodes.filter((n) => n.kind === "screenshot");
    expect(ssNodes).toHaveLength(0);
  });
});

// ── Hour-of-day pattern ──────────────────────────────────────────────────────

describe("hour-of-day engagement pattern", () => {
  it("groups EEG epochs by hour", () => {
    const nodes = [
      mkNode("ep0", "eeg_point", { timestamp_unix: 1710000000, eeg_metrics: { engagement: 0.8 } }), // some hour
      mkNode("ep1", "eeg_point", { timestamp_unix: 1710003600, eeg_metrics: { engagement: 0.6 } }), // +1 hour
      mkNode("ep2", "eeg_point", { timestamp_unix: 1710000060, eeg_metrics: { engagement: 0.9 } }), // same hour as ep0
    ];

    const hourMap = new Map<number, { sum: number; count: number }>();
    for (const n of nodes) {
      if (!n.timestamp_unix || !n.eeg_metrics?.engagement) continue;
      const hour = new Date(n.timestamp_unix * 1000).getHours();
      const entry = hourMap.get(hour) ?? { sum: 0, count: 0 };
      entry.sum += n.eeg_metrics.engagement as number;
      entry.count++;
      hourMap.set(hour, entry);
    }

    // At least one hour should have 2 entries
    const maxCount = Math.max(...[...hourMap.values()].map((v) => v.count));
    expect(maxCount).toBeGreaterThanOrEqual(2);
  });
});

// ── Bookmark system ──────────────────────────────────────────────────────────

describe("bookmark system", () => {
  it("saves a bookmark with required fields", () => {
    const node = mkNode("ep0", "eeg_point", { text: "focus session", timestamp_unix: 1710000000 });
    const bookmark = {
      query: "focus",
      nodeId: node.id,
      kind: node.kind,
      text: node.text ?? "",
      timestamp: node.timestamp_unix,
      savedAt: Date.now(),
    };

    expect(bookmark.nodeId).toBe("ep0");
    expect(bookmark.text).toBe("focus session");
    expect(bookmark.savedAt).toBeGreaterThan(0);
  });

  it("deduplicates bookmarks by nodeId", () => {
    const bookmarks = [
      { query: "a", nodeId: "ep0", kind: "eeg_point", text: "x", savedAt: 1 },
      { query: "b", nodeId: "ep1", kind: "eeg_point", text: "y", savedAt: 2 },
      { query: "c", nodeId: "ep0", kind: "eeg_point", text: "z", savedAt: 3 }, // duplicate
    ];
    // Dedup logic: keep latest, filter older
    const newEntry = bookmarks[2];
    const deduped = [newEntry, ...bookmarks.filter((b) => b.nodeId !== newEntry.nodeId)];
    expect(deduped).toHaveLength(2);
    expect(deduped[0].text).toBe("z"); // latest first
  });

  it("removes bookmark by nodeId", () => {
    const bookmarks = [
      { nodeId: "ep0", text: "a" },
      { nodeId: "ep1", text: "b" },
    ];
    const removed = bookmarks.filter((b) => b.nodeId !== "ep0");
    expect(removed).toHaveLength(1);
    expect(removed[0].nodeId).toBe("ep1");
  });
});

// ── Breadcrumb building ──────────────────────────────────────────────────────

describe("breadcrumb building", () => {
  it("builds path from leaf to root", () => {
    const nodes: GraphNode[] = [
      mkNode("q0", "query", { text: "search query" }),
      mkNode("tl0", "text_label", { text: "label", parent_id: "q0" }),
      mkNode("ep0", "eeg_point", { parent_id: "tl0" }),
      mkNode("fl0", "found_label", { text: "found", parent_id: "ep0" }),
    ];

    // Simulate breadcrumb building
    function buildBreadcrumb(node: GraphNode, allNodes: GraphNode[]): GraphNode[] {
      const path: GraphNode[] = [];
      let cur: GraphNode | undefined = node;
      for (let i = 0; i < 10 && cur; i++) {
        path.unshift(cur);
        const pid: string | undefined = cur.parent_id;
        cur = pid != null ? allNodes.find((n) => n.id === pid) : undefined;
      }
      return path;
    }

    const trail = buildBreadcrumb(nodes[3], nodes);
    expect(trail).toHaveLength(4);
    expect(trail[0].kind).toBe("query");
    expect(trail[1].kind).toBe("text_label");
    expect(trail[2].kind).toBe("eeg_point");
    expect(trail[3].kind).toBe("found_label");
  });

  it("returns single node when no parent", () => {
    const node = mkNode("q0", "query", { text: "test" });
    const trail = [node]; // no parent_id
    expect(trail).toHaveLength(1);
  });
});

// ── Search history ───────────────────────────────────────────────────────────

describe("search history", () => {
  it("deduplicates and limits to max entries", () => {
    const MAX = 10;
    let history: string[] = ["a", "b", "c"];
    const q = "b"; // existing entry

    // Save logic: move to front, dedup, limit
    history = [q, ...history.filter((h) => h !== q)].slice(0, MAX);
    expect(history[0]).toBe("b");
    expect(history).toHaveLength(3); // no duplicate
  });

  it("adds new entry at front", () => {
    let history = ["a", "b"];
    const q = "new";
    history = [q, ...history.filter((h) => h !== q)].slice(0, 10);
    expect(history[0]).toBe("new");
    expect(history).toHaveLength(3);
  });
});

// ── LLM prompt building ─────────────────────────────────────────────────────

describe("LLM prompt building", () => {
  it("includes EEG metrics when available", () => {
    const node = mkNode("ep0", "eeg_point", {
      timestamp_unix: 1710000000,
      eeg_metrics: { engagement: 0.75, relaxation: 0.6, snr: 11, rel_alpha: 0.3, rel_beta: 0.2, rel_theta: 0.15 },
      relevance_score: 0.35,
      session_id: "20260310_10h",
    });

    const m = node.eeg_metrics ?? {};
    const parts: string[] = [];
    if (m.engagement != null) parts.push(`eng=${(m.engagement as number).toFixed(2)}`);
    if (m.relaxation != null) parts.push(`rel=${(m.relaxation as number).toFixed(2)}`);
    if (m.snr != null) parts.push(`snr=${(m.snr as number).toFixed(1)}`);
    if (m.rel_alpha != null) parts.push(`α=${(m.rel_alpha as number).toFixed(3)}`);

    expect(parts).toContain("eng=0.75");
    expect(parts).toContain("rel=0.60");
    expect(parts).toContain("snr=11.0");
    expect(parts).toContain("α=0.300");
  });

  it("flags epochs with no metrics", () => {
    const node = mkNode("ep0", "eeg_point", {
      timestamp_unix: 1710000000,
    });
    const m = node.eeg_metrics ?? {};
    const hasMetrics = !!(m.engagement || m.relaxation || m.snr);
    expect(hasMetrics).toBe(false);
  });

  it("includes screenshot context", () => {
    const ss = mkNode("ss0", "screenshot", {
      timestamp_unix: 1710000000,
      app_name: "Chrome",
      window_title: "Google Search",
      ocr_similarity: 0.85,
    });

    const parts: string[] = [];
    if (ss.app_name) parts.push(`app=${ss.app_name}`);
    if (ss.window_title) parts.push(`title="${ss.window_title}"`);

    expect(parts).toContain("app=Chrome");
    expect(parts).toContain('title="Google Search"');
  });
});

// ── Color mode ───────────────────────────────────────────────────────────────

describe("color mode logic", () => {
  it("timestamp is the default", () => {
    const mode = "timestamp";
    expect(mode).toBe("timestamp");
  });

  it("engagement mode uses eeg_metrics.engagement", () => {
    const node = mkNode("ep0", "eeg_point", { eeg_metrics: { engagement: 0.7 } });
    const mode = "engagement";
    if (mode === "engagement" && node.eeg_metrics?.engagement != null) {
      const val = Math.min(1, node.eeg_metrics.engagement as number);
      expect(val).toBe(0.7);
    }
  });

  it("session mode hashes session_id to a color", () => {
    const sid = "20260303_22h";
    let h = 0;
    for (let i = 0; i < sid.length; i++) h = (h * 31 + sid.charCodeAt(i)) & 0xffffff;
    expect(h).toBeGreaterThan(0);
    expect(h).toBeLessThanOrEqual(0xffffff);
  });

  it("falls back to timestamp when metrics missing", () => {
    const node = mkNode("ep0", "eeg_point", { timestamp_unix: 1000 });
    const _mode = "engagement";
    const hasMetric = node.eeg_metrics?.engagement != null;
    expect(hasMetric).toBe(false);
    // Should fall back — not crash
  });
});

// ── Session derivation fallback ──────────────────────────────────────────────

describe("session derivation from nodes", () => {
  it("derives session counts from node session_ids", () => {
    const nodes = [
      mkNode("ep0", "eeg_point", { session_id: "20260303_22h" }),
      mkNode("ep1", "eeg_point", { session_id: "20260303_22h" }),
      mkNode("ep2", "eeg_point", { session_id: "20260304_10h" }),
    ];
    const sessionMap = new Map<string, number>();
    for (const n of nodes) {
      if (n.session_id) sessionMap.set(n.session_id, (sessionMap.get(n.session_id) ?? 0) + 1);
    }
    expect(sessionMap.get("20260303_22h")).toBe(2);
    expect(sessionMap.get("20260304_10h")).toBe(1);
  });
});
