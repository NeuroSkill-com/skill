// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  buildNeighborLabelMap,
  computePreset,
  enrichUmapLabels,
  fmtDuration,
  fromInputValue,
  normalizeSearchMode,
  toInputValue,
} from "$lib/search-logic";
import type { SearchResult } from "$lib/search-types";
import type { UmapPoint, UmapResult } from "$lib/types";

describe("normalizeSearchMode", () => {
  it("passes through valid modes", () => {
    expect(normalizeSearchMode("eeg")).toBe("eeg");
    expect(normalizeSearchMode("text")).toBe("text");
    expect(normalizeSearchMode("interactive")).toBe("interactive");
    expect(normalizeSearchMode("images")).toBe("images");
  });

  it("defaults to interactive for invalid values", () => {
    expect(normalizeSearchMode("bad")).toBe("interactive");
    expect(normalizeSearchMode(null)).toBe("interactive");
    expect(normalizeSearchMode(undefined)).toBe("interactive");
    expect(normalizeSearchMode(42)).toBe("interactive");
  });
});

describe("toInputValue / fromInputValue roundtrip", () => {
  it("round-trips a date", () => {
    const d = new Date(2026, 2, 24, 14, 30, 0); // March 24, 2026 14:30
    const input = toInputValue(d);
    const unix = fromInputValue(input);
    // Should be within 60 seconds (input has minute precision)
    const diff = Math.abs(unix - Math.floor(d.getTime() / 1000));
    expect(diff).toBeLessThan(60);
  });
});

describe("fmtDuration", () => {
  it("formats a duration between two timestamps", () => {
    const result = fmtDuration(1000, 1300); // 300 seconds = 5 minutes
    expect(result).toBeTruthy();
    expect(typeof result).toBe("string");
  });
});

describe("buildNeighborLabelMap", () => {
  it("returns empty map for null result", () => {
    const map = buildNeighborLabelMap(null);
    expect(map.size).toBe(0);
  });

  it("builds map from labeled neighbors", () => {
    const result = {
      start_utc: 1000,
      end_utc: 2000,
      k: 10,
      ef: 50,
      query_count: 1,
      searched_days: ["2026-03-24"],
      results: [
        {
          epoch_utc: 1500,
          neighbors: [
            { timestamp_unix: 1200, distance: 0.1, labels: [{ text: "hello", id: 1 }] },
            { timestamp_unix: 1300, distance: 0.2, labels: [] },
          ],
        },
      ],
    } as unknown as SearchResult;

    const map = buildNeighborLabelMap(result);
    expect(map.size).toBe(1);
    expect(map.get(1200)).toBe("hello");
  });
});

describe("enrichUmapLabels", () => {
  function makeUmapResult(points: Array<Omit<UmapPoint, "session"> & { session?: number }>): UmapResult {
    return { points: points.map((p) => ({ session: 0, ...p, label: p.label ?? undefined })), n_a: 0, n_b: 0, dim: 3 };
  }

  it("injects labels from map", () => {
    const raw = makeUmapResult([
      { x: 0, y: 0, z: 0, utc: 1200 },
      { x: 1, y: 1, z: 1, utc: 1300, label: "existing" },
    ]);
    const labelMap = new Map([[1200, "injected"]]);
    const enriched = enrichUmapLabels(raw, labelMap);
    expect(enriched.points[0].label).toBe("injected");
    expect(enriched.points[1].label).toBe("existing"); // not overwritten
  });

  it("returns raw when label map is empty", () => {
    const raw = makeUmapResult([{ x: 0, y: 0, z: 0, utc: 1200 }]);
    const result = enrichUmapLabels(raw, new Map());
    expect(result).toBe(raw);
  });
});

describe("computePreset", () => {
  it("returns start before end", () => {
    const { start, end } = computePreset(5);
    const startTs = fromInputValue(start);
    const endTs = fromInputValue(end);
    expect(endTs).toBeGreaterThan(startTs);
  });
});
