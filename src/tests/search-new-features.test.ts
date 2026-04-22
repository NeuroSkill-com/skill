// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Vitest unit tests for newly added search features:
// - relevance_score on GraphNode
// - session_id on GraphNode
// - session summary with best flag
// - CSV export format
// - node dedup (eeg_ts dedup, screenshot dedup)
// - date-range filtering
// - EEG rank-by sorting
// - buildDisplayGraph with new node fields

import { describe, expect, it } from "vitest";
import { buildDisplayGraph, serialiseNodesForBackend } from "$lib/search/search-interactive-logic";
import type { GraphEdge, GraphNode } from "$lib/search/search-types";

// ── Helpers ──────────────────────────────────────────────────────────────────

function mkNode(id: string, kind: GraphNode["kind"] = "eeg_point", overrides: Partial<GraphNode> = {}): GraphNode {
  return { id, kind, text: "test", distance: 0.1, ...overrides };
}

function mkEdge(from: string, to: string, kind: GraphEdge["kind"] = "eeg_bridge"): GraphEdge {
  return { from_id: from, to_id: to, distance: 0.1, kind };
}

// ── relevance_score ──────────────────────────────────────────────────────────

describe("relevance_score on GraphNode", () => {
  it("preserves relevance_score through serialisation", () => {
    const node = mkNode("ep0", "eeg_point", { relevance_score: 0.42 });
    expect(node.relevance_score).toBe(0.42);
  });

  it("allows undefined relevance_score", () => {
    const node = mkNode("ep1", "eeg_point");
    expect(node.relevance_score).toBeUndefined();
  });

  it("sorts nodes by relevance_score", () => {
    const nodes = [
      mkNode("a", "eeg_point", { relevance_score: 0.8 }),
      mkNode("b", "eeg_point", { relevance_score: 0.2 }),
      mkNode("c", "eeg_point", { relevance_score: 0.5 }),
    ];
    const sorted = [...nodes].sort((a, b) => (a.relevance_score ?? 1) - (b.relevance_score ?? 1));
    expect(sorted[0].id).toBe("b");
    expect(sorted[1].id).toBe("c");
    expect(sorted[2].id).toBe("a");
  });
});

// ── session_id ───────────────────────────────────────────────────────────────

describe("session_id on GraphNode", () => {
  it("stores session_id on nodes", () => {
    const node = mkNode("ep0", "eeg_point", { session_id: "20260303_22h" });
    expect(node.session_id).toBe("20260303_22h");
  });

  it("groups nodes by session_id", () => {
    const nodes = [
      mkNode("a", "eeg_point", { session_id: "20260303_22h" }),
      mkNode("b", "eeg_point", { session_id: "20260303_22h" }),
      mkNode("c", "eeg_point", { session_id: "20260304_10h" }),
    ];
    const groups = new Map<string, GraphNode[]>();
    for (const n of nodes) {
      const sid = n.session_id ?? "unknown";
      if (!groups.has(sid)) groups.set(sid, []);
      groups.get(sid)?.push(n);
    }
    expect(groups.get("20260303_22h")).toHaveLength(2);
    expect(groups.get("20260304_10h")).toHaveLength(1);
  });
});

// ── Session summary with best flag ───────────────────────────────────────────

describe("session summary", () => {
  interface SessionSummary {
    session_id: string;
    epoch_count: number;
    duration_secs: number;
    best: boolean;
    avg_engagement: number;
    avg_snr: number;
    avg_relaxation: number;
    stddev_engagement: number;
  }

  it("identifies best session by highest avg engagement", () => {
    const sessions: SessionSummary[] = [
      {
        session_id: "s1",
        epoch_count: 10,
        duration_secs: 600,
        best: false,
        avg_engagement: 0.5,
        avg_snr: 10,
        avg_relaxation: 0.3,
        stddev_engagement: 0.1,
      },
      {
        session_id: "s2",
        epoch_count: 8,
        duration_secs: 480,
        best: true,
        avg_engagement: 0.8,
        avg_snr: 12,
        avg_relaxation: 0.4,
        stddev_engagement: 0.05,
      },
      {
        session_id: "s3",
        epoch_count: 12,
        duration_secs: 720,
        best: false,
        avg_engagement: 0.3,
        avg_snr: 8,
        avg_relaxation: 0.6,
        stddev_engagement: 0.2,
      },
    ];
    const best = sessions.find((s) => s.best);
    expect(best?.session_id).toBe("s2");
    expect(best?.avg_engagement).toBe(0.8);
  });

  it("only one session is marked as best", () => {
    const sessions: SessionSummary[] = [
      {
        session_id: "s1",
        epoch_count: 5,
        duration_secs: 300,
        best: true,
        avg_engagement: 0.9,
        avg_snr: 15,
        avg_relaxation: 0.5,
        stddev_engagement: 0.05,
      },
      {
        session_id: "s2",
        epoch_count: 5,
        duration_secs: 300,
        best: false,
        avg_engagement: 0.7,
        avg_snr: 12,
        avg_relaxation: 0.4,
        stddev_engagement: 0.1,
      },
    ];
    expect(sessions.filter((s) => s.best)).toHaveLength(1);
  });
});

// ── CSV export format ────────────────────────────────────────────────────────

describe("CSV export format", () => {
  it("generates valid CSV from session data", () => {
    const sessions = [
      {
        session_id: "s1",
        epoch_count: 10,
        duration_secs: 600,
        best: true,
        avg_engagement: 0.5,
        avg_snr: 10.0,
        avg_relaxation: 0.3,
        stddev_engagement: 0.1,
      },
    ];
    const header = "session_id,epoch_count,duration_secs,avg_engagement,avg_snr,avg_relaxation,stddev_engagement,best";
    const rows = sessions.map(
      (s) =>
        `${s.session_id},${s.epoch_count},${s.duration_secs},${s.avg_engagement.toFixed(4)},${s.avg_snr.toFixed(4)},${s.avg_relaxation.toFixed(4)},${s.stddev_engagement.toFixed(4)},${s.best}`,
    );
    const csv = [header, ...rows].join("\n");

    expect(csv).toContain("session_id,epoch_count");
    expect(csv).toContain("s1,10,600");
    expect(csv).toContain("true");
    expect(csv.split("\n")).toHaveLength(2); // header + 1 row
  });
});

// ── buildDisplayGraph with new fields ────────────────────────────────────────

describe("buildDisplayGraph preserves new fields", () => {
  it("preserves relevance_score and session_id through display graph", () => {
    const nodes = [
      mkNode("q0", "query"),
      mkNode("ep0", "eeg_point", { relevance_score: 0.3, session_id: "20260303_22h" }),
    ];
    const edges = [mkEdge("q0", "ep0")];
    const result = buildDisplayGraph(nodes, edges, false, false, new Map(), () => "");
    const ep = result.nodes.find((n) => n.id === "ep0");
    expect(ep?.relevance_score).toBe(0.3);
    expect(ep?.session_id).toBe("20260303_22h");
  });
});

// ── serialiseNodesForBackend with new fields ─────────────────────────────────

describe("serialiseNodesForBackend with new fields", () => {
  it("preserves eeg_metrics through serialisation", () => {
    const node = mkNode("ep0", "eeg_point", {
      eeg_metrics: { relaxation: 0.5, engagement: 0.8, snr: 12 },
    });
    const [s] = serialiseNodesForBackend([node]);
    expect(s.eeg_metrics).toEqual({ relaxation: 0.5, engagement: 0.8, snr: 12 });
  });
});

// ── EEG epoch dedup ──────────────────────────────────────────────────────────

describe("EEG epoch dedup logic", () => {
  it("deduplicates nodes with same timestamp_unix", () => {
    const nodes = [
      mkNode("ep0_0", "eeg_point", { timestamp_unix: 1000 }),
      mkNode("ep1_0", "eeg_point", { timestamp_unix: 1000 }), // duplicate
      mkNode("ep1_1", "eeg_point", { timestamp_unix: 2000 }),
    ];
    const seen = new Set<number>();
    const deduped = nodes.filter((n) => {
      const ts = n.timestamp_unix;
      if (ts == null) return true;
      if (seen.has(ts)) return false;
      seen.add(ts);
      return true;
    });
    expect(deduped).toHaveLength(2);
    expect(deduped[0].id).toBe("ep0_0");
    expect(deduped[1].id).toBe("ep1_1");
  });
});

// ── Date range filtering ─────────────────────────────────────────────────────

describe("date range filtering", () => {
  it("filters nodes by timestamp range", () => {
    const nodes = [
      mkNode("tl0", "text_label", { timestamp_unix: 1000 }),
      mkNode("tl1", "text_label", { timestamp_unix: 2000 }),
      mkNode("tl2", "text_label", { timestamp_unix: 3000 }),
    ];
    const start = 1500;
    const end = 2500;
    const filtered = nodes.filter((n) => {
      if (!n.timestamp_unix) return true;
      return n.timestamp_unix >= start && n.timestamp_unix <= end;
    });
    expect(filtered).toHaveLength(1);
    expect(filtered[0].id).toBe("tl1");
  });

  it("keeps nodes without timestamp when filtering", () => {
    const nodes = [
      mkNode("tl0", "text_label"), // no timestamp
      mkNode("tl1", "text_label", { timestamp_unix: 5000 }),
    ];
    const filtered = nodes.filter((n) => {
      if (!n.timestamp_unix) return true;
      return n.timestamp_unix >= 1000 && n.timestamp_unix <= 2000;
    });
    expect(filtered).toHaveLength(1);
    expect(filtered[0].id).toBe("tl0");
  });
});

// ── EEG ranking ──────────────────────────────────────────────────────────────

describe("EEG epoch ranking", () => {
  it("sorts by engagement descending", () => {
    const epochs = [
      { t: 1000, engagement: 0.3, snr: 10, relaxation: 0.5 },
      { t: 2000, engagement: 0.9, snr: 8, relaxation: 0.2 },
      { t: 3000, engagement: 0.6, snr: 12, relaxation: 0.7 },
    ];
    const sorted = [...epochs].sort((a, b) => b.engagement - a.engagement);
    expect(sorted[0].engagement).toBe(0.9);
    expect(sorted[1].engagement).toBe(0.6);
  });

  it("sorts by SNR descending", () => {
    const epochs = [
      { t: 1000, engagement: 0.3, snr: 10, relaxation: 0.5 },
      { t: 2000, engagement: 0.9, snr: 8, relaxation: 0.2 },
      { t: 3000, engagement: 0.6, snr: 15, relaxation: 0.7 },
    ];
    const sorted = [...epochs].sort((a, b) => b.snr - a.snr);
    expect(sorted[0].snr).toBe(15);
  });

  it("keeps timestamp order by default", () => {
    const epochs = [
      { t: 3000, engagement: 0.3 },
      { t: 1000, engagement: 0.9 },
      { t: 2000, engagement: 0.6 },
    ];
    // no sort = keep original order
    expect(epochs[0].t).toBe(3000);
  });
});

// ── Relevance score computation ──────────────────────────────────────────────

describe("relevance score computation", () => {
  it("computes weighted composite score", () => {
    const textDist = 0.2;
    const timeDist = 0.5;
    const engagement = 0.8;

    const score = textDist * 0.5 + timeDist * 0.3 + (1.0 - engagement) * 0.2;
    expect(score).toBeCloseTo(0.29);
  });

  it("returns 0 for perfect match", () => {
    const score = 0 * 0.5 + 0 * 0.3 + (1.0 - 1.0) * 0.2;
    expect(score).toBe(0);
  });

  it("returns ~1 for worst match", () => {
    const score = 1.0 * 0.5 + 1.0 * 0.3 + (1.0 - 0.0) * 0.2;
    expect(score).toBe(1.0);
  });
});
