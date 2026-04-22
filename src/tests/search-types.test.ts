// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  buildTextKnnGraph,
  computeSearchAnalysis,
  computeTemporalHeatmap,
  distColor,
  heatColor,
  type LabelNeighbor,
  metricChips,
  type NeighborMetrics,
  type SearchResult,
  simPct,
  simWidth,
  turboColor,
} from "$lib/search/search-types";

describe("distColor", () => {
  it("returns green for near-zero distance", () => {
    expect(distColor(0)).toBe("#22c55e");
  });
  it("returns blue for small distance", () => {
    expect(distColor(0.03)).toBe("#3b82f6");
  });
  it("returns amber for medium distance", () => {
    expect(distColor(0.1)).toBe("#f59e0b");
  });
  it("returns gray for large distance", () => {
    expect(distColor(0.5)).toBe("#94a3b8");
  });
});

describe("simPct", () => {
  it("returns 100% when max is 0", () => {
    expect(simPct(0, 0)).toBe("100%");
  });
  it("returns 0% at max distance", () => {
    expect(simPct(1, 1)).toBe("0.0%");
  });
  it("returns 50% at half distance", () => {
    expect(simPct(0.5, 1)).toBe("50.0%");
  });
});

describe("simWidth", () => {
  it("returns 1 when max is 0", () => {
    expect(simWidth(0, 0)).toBe(1);
  });
  it("returns 0 at max distance", () => {
    expect(simWidth(1, 1)).toBe(0);
  });
  it("clamps to 0", () => {
    expect(simWidth(2, 1)).toBe(0);
  });
});

describe("metricChips", () => {
  it("returns chips for present metrics", () => {
    const m: NeighborMetrics = { focus: 85, relaxation: 72 };
    const chips = metricChips(m);
    expect(chips.length).toBe(2);
    expect(chips[0].l).toBe("Focus");
    expect(chips[0].v).toBe("85");
  });
  it("skips null metrics", () => {
    const m: NeighborMetrics = {};
    expect(metricChips(m)).toHaveLength(0);
  });
  it("skips zero HR", () => {
    const m: NeighborMetrics = { hr: 0 };
    expect(metricChips(m)).toHaveLength(0);
  });
  it("includes non-zero HR", () => {
    const m: NeighborMetrics = { hr: 72 };
    const chips = metricChips(m);
    expect(chips.some((c) => c.l === "HR")).toBe(true);
  });
  it("formats FAA with sign", () => {
    const m: NeighborMetrics = { faa: 0.15 };
    const chips = metricChips(m);
    expect(chips[0].v).toBe("+0.15");
  });
});

describe("computeSearchAnalysis", () => {
  const mkResult = (neighbors: Array<{ distance: number; timestamp_unix: number }>): SearchResult => ({
    start_utc: 0,
    end_utc: 0,
    k: 5,
    ef: 50,
    query_count: 1,
    searched_days: [],
    results: [
      {
        timestamp: 0,
        timestamp_unix: 0,
        neighbors: neighbors.map((n) => ({
          hnsw_id: 0,
          timestamp: 0,
          timestamp_unix: n.timestamp_unix,
          distance: n.distance,
          date: "20260101",
          device_id: null,
          device_name: null,
          labels: [],
        })),
      },
    ],
  });

  it("returns null for empty results", () => {
    const r: SearchResult = { start_utc: 0, end_utc: 0, k: 5, ef: 50, query_count: 0, searched_days: [], results: [] };
    expect(computeSearchAnalysis(r)).toBeNull();
  });

  it("computes stats for valid results", () => {
    const r = mkResult([
      { distance: 0.1, timestamp_unix: 1700000000 },
      { distance: 0.2, timestamp_unix: 1700003600 },
      { distance: 0.3, timestamp_unix: 1700007200 },
    ]);
    const a = computeSearchAnalysis(r)!;
    expect(a).toBeDefined();
    expect(a.totalNeighbors).toBe(3);
    expect(a.distMin).toBeCloseTo(0.1);
    expect(a.distMax).toBeCloseTo(0.3);
    expect(a.distMean).toBeCloseTo(0.2);
    expect(a.hourHist).toHaveLength(24);
  });
});

describe("computeTemporalHeatmap", () => {
  it("returns null for empty results", () => {
    expect(
      computeTemporalHeatmap({ start_utc: 0, end_utc: 0, k: 0, ef: 0, query_count: 0, searched_days: [], results: [] }),
    ).toBeNull();
  });
});

describe("heatColor", () => {
  it("returns transparent for 0", () => {
    expect(heatColor(0, 10)).toBe("transparent");
  });
  it("returns rgba for non-zero", () => {
    expect(heatColor(5, 10)).toMatch(/^rgba\(/);
  });
});

describe("turboColor", () => {
  it("returns hex color", () => {
    expect(turboColor(0)).toMatch(/^#[0-9a-f]{6}$/);
    expect(turboColor(0.5)).toMatch(/^#[0-9a-f]{6}$/);
    expect(turboColor(1)).toMatch(/^#[0-9a-f]{6}$/);
  });
  it("clamps out-of-range values", () => {
    expect(turboColor(-1)).toMatch(/^#[0-9a-f]{6}$/);
    expect(turboColor(2)).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe("buildTextKnnGraph", () => {
  it("returns query point for empty results", () => {
    const graph = buildTextKnnGraph([], "test query");
    expect(graph.points).toHaveLength(1);
    expect(graph.points[0].session).toBe(0);
    expect(graph.n_a).toBe(1);
    expect(graph.n_b).toBe(0);
  });

  it("places results around query", () => {
    const results: LabelNeighbor[] = [
      { label_id: 1, text: "a", context: "", eeg_start: 0, eeg_end: 0, created_at: 0, distance: 0.1 },
      { label_id: 2, text: "b", context: "", eeg_start: 0, eeg_end: 0, created_at: 0, distance: 0.2 },
    ];
    const graph = buildTextKnnGraph(results, "query");
    expect(graph.points).toHaveLength(3); // 1 query + 2 results
    expect(graph.points[0].x).toBe(0); // query at origin
    expect(graph.n_b).toBe(2);
  });
});
