// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  advancedMetrics,
  analyzeUmapClusters,
  bandKeys,
  bv,
  computeInsightDeltas,
  dc,
  diff,
  generateUmapPlaceholder,
  HIGHER_IS_BETTER,
  LOWER_IS_BETTER,
  pct,
  scoreDiff,
  sdc,
} from "$lib/compare/compare-types";
import type { SessionMetrics } from "$lib/dashboard/SessionDetail.svelte";
import type { UmapPoint, UmapResult } from "$lib/types";

// Minimal mock metrics
const mkMetrics = (overrides: Partial<SessionMetrics> = {}): SessionMetrics =>
  ({
    n_epochs: 10,
    rel_delta: 0.2,
    rel_theta: 0.15,
    rel_alpha: 0.3,
    rel_beta: 0.25,
    rel_gamma: 0.1,
    relaxation: 50,
    engagement: 60,
    meditation: 70,
    cognitive_load: 40,
    drowsiness: 20,
    stress_index: 30,
    coherence: 0.5,
    snr: 10,
    mu_suppression: 0.3,
    hr: 72,
    rmssd: 40,
    sdnn: 50,
    pnn50: 30,
    lf_hf_ratio: 1.5,
    stillness: 0.8,
    blink_rate: 15,
    ...overrides,
  }) as SessionMetrics;

describe("bv", () => {
  it("returns 0 for null metrics", () => {
    expect(bv(null, "rel_alpha")).toBe(0);
  });
  it("returns value for present metric", () => {
    expect(bv(mkMetrics({ rel_alpha: 0.42 }), "rel_alpha")).toBeCloseTo(0.42);
  });
});

describe("pct", () => {
  it("formats as percentage", () => {
    expect(pct(0.5)).toBe("50.0");
    expect(pct(1)).toBe("100.0");
  });
});

describe("diff", () => {
  it("shows dash for negligible difference", () => {
    expect(diff(0.5, 0.5)).toBe("—");
  });
  it("shows positive diff with +", () => {
    expect(diff(0.6, 0.4)).toBe("+20.0");
  });
  it("shows negative diff", () => {
    expect(diff(0.4, 0.6)).toBe("-20.0");
  });
});

describe("scoreDiff", () => {
  it("shows dash for small difference", () => {
    expect(scoreDiff(50, 50)).toBe("—");
  });
  it("shows positive score diff", () => {
    expect(scoreDiff(60, 50)).toBe("+10.0");
  });
});

describe("dc (diff color)", () => {
  it("muted for negligible", () => {
    expect(dc(0.5, 0.5)).toContain("muted");
  });
  it("green for positive", () => {
    expect(dc(0.6, 0.4)).toContain("emerald");
  });
  it("red for negative", () => {
    expect(dc(0.4, 0.6)).toContain("red");
  });
});

describe("sdc (score diff color)", () => {
  it("muted for small diff", () => {
    expect(sdc(50, 50)).toContain("muted");
  });
  it("green for positive", () => {
    expect(sdc(60, 50)).toContain("emerald");
  });
});

describe("analyzeUmapClusters", () => {
  const mkUmap = (ptsA: number, ptsB: number, spread: number): UmapResult => {
    const points: UmapPoint[] = [];
    for (let i = 0; i < ptsA; i++)
      points.push({ x: -5 + Math.random() * spread, y: Math.random() * spread, z: 0, session: 0, utc: 0 });
    for (let i = 0; i < ptsB; i++)
      points.push({ x: 5 + Math.random() * spread, y: Math.random() * spread, z: 0, session: 1, utc: 0 });
    return { points, n_a: ptsA, n_b: ptsB, dim: 3 };
  };

  it("returns null for too few points", () => {
    expect(
      analyzeUmapClusters({ points: [{ x: 0, y: 0, z: 0, session: 0, utc: 0 }], n_a: 1, n_b: 0, dim: 3 }),
    ).toBeNull();
  });

  it("computes separation for well-separated clusters", () => {
    const result = analyzeUmapClusters(mkUmap(10, 10, 0.1));
    expect(result).not.toBeNull();
    expect(result?.separationScore).toBeGreaterThan(1);
    expect(result?.interCluster).toBeGreaterThan(0);
  });
});

describe("computeInsightDeltas", () => {
  it("detects improvement in higher-is-better metrics", () => {
    const a = mkMetrics({ relaxation: 50 });
    const b = mkMetrics({ relaxation: 70 });
    const deltas = computeInsightDeltas(a, b);
    const relax = deltas.find((d) => d.key === "relaxation");
    expect(relax?.direction).toBe("improved");
    expect(relax?.delta).toBe(20);
  });

  it("detects improvement in lower-is-better metrics", () => {
    const a = mkMetrics({ stress_index: 80 });
    const b = mkMetrics({ stress_index: 40 });
    const deltas = computeInsightDeltas(a, b);
    const stress = deltas.find((d) => d.key === "stress_index");
    expect(stress?.direction).toBe("improved");
  });

  it("marks stable when change is small", () => {
    const a = mkMetrics({ relaxation: 50 });
    const b = mkMetrics({ relaxation: 50.5 });
    const deltas = computeInsightDeltas(a, b);
    const relax = deltas.find((d) => d.key === "relaxation");
    expect(relax?.direction).toBe("stable");
  });
});

describe("generateUmapPlaceholder", () => {
  it("generates correct point counts", () => {
    const result = generateUmapPlaceholder(5, 10);
    expect(result.points).toHaveLength(15);
    expect(result.n_a).toBe(5);
    expect(result.n_b).toBe(10);
  });
});

describe("constants", () => {
  it("HIGHER_IS_BETTER and LOWER_IS_BETTER don't overlap", () => {
    for (const key of HIGHER_IS_BETTER) {
      expect(LOWER_IS_BETTER.has(key)).toBe(false);
    }
  });

  it("bandKeys has 5 entries", () => {
    expect(bandKeys).toHaveLength(5);
  });

  it("advancedMetrics all have formatters", () => {
    for (const m of advancedMetrics) {
      expect(typeof m.fmt).toBe("function");
      expect(typeof m.fmt(1.234)).toBe("string");
    }
  });
});
