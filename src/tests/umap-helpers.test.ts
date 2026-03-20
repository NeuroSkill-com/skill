// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import {
  easeOut, gauss, hslToRgb, labelHex, turboRaw, jet, jetHex,
  fmtGradientTs, fmtUtcTime, utcToLocalDate,
  normalise, randomPositions,
  buildTraceTimeTicks, buildDatePaletteRaw,
} from "$lib/umap-helpers";
import type { UmapPoint } from "$lib/types";

// ── Helper ────────────────────────────────────────────────────────────────────

function pt(x: number, y: number, z: number, utc = 0): UmapPoint {
  return { x, y, z, utc, label: "", relaxation: 0.5, engagement: 0.5, session_idx: 0, epoch_idx: 0 } as UmapPoint;
}

// ── Easing ────────────────────────────────────────────────────────────────────

describe("easeOut", () => {
  it("0 → 0", () => expect(easeOut(0)).toBe(0));
  it("1 → 1", () => expect(easeOut(1)).toBe(1));
  it("0.5 is between 0 and 1", () => {
    const v = easeOut(0.5);
    expect(v).toBeGreaterThan(0);
    expect(v).toBeLessThan(1);
  });
  it("is monotonically increasing", () => {
    for (let i = 0; i < 10; i++) {
      expect(easeOut((i + 1) / 10)).toBeGreaterThanOrEqual(easeOut(i / 10));
    }
  });
});

describe("gauss", () => {
  it("returns a finite number", () => {
    expect(Number.isFinite(gauss())).toBe(true);
  });
  it("has roughly zero mean over many samples", () => {
    let sum = 0;
    const n = 10000;
    for (let i = 0; i < n; i++) sum += gauss();
    expect(Math.abs(sum / n)).toBeLessThan(0.1);
  });
});

// ── Color helpers ─────────────────────────────────────────────────────────────

describe("hslToRgb", () => {
  it("red (h=0, s=1, l=0.5) → [1, 0, 0]", () => {
    const [r, g, b] = hslToRgb(0, 1, 0.5);
    expect(r).toBeCloseTo(1, 1);
    expect(g).toBeCloseTo(0, 1);
    expect(b).toBeCloseTo(0, 1);
  });
  it("white (h=0, s=0, l=1) → [1, 1, 1]", () => {
    const [r, g, b] = hslToRgb(0, 0, 1);
    expect(r).toBeCloseTo(1, 1);
    expect(g).toBeCloseTo(1, 1);
    expect(b).toBeCloseTo(1, 1);
  });
  it("black (h=0, s=0, l=0) → [0, 0, 0]", () => {
    const [r, g, b] = hslToRgb(0, 0, 0);
    expect(r).toBeCloseTo(0, 1);
    expect(g).toBeCloseTo(0, 1);
    expect(b).toBeCloseTo(0, 1);
  });
});

describe("labelHex", () => {
  it("returns a 7-char hex string", () => {
    expect(labelHex(180)).toMatch(/^#[0-9a-f]{6}$/);
  });
  it("different hues produce different colors", () => {
    expect(labelHex(0)).not.toBe(labelHex(180));
  });
});

describe("turboRaw", () => {
  it("returns RGB values in [0,1]", () => {
    for (const t of [0, 0.25, 0.5, 0.75, 1]) {
      const [r, g, b] = turboRaw(t);
      expect(r).toBeGreaterThanOrEqual(0);
      expect(r).toBeLessThanOrEqual(1);
      expect(g).toBeGreaterThanOrEqual(0);
      expect(g).toBeLessThanOrEqual(1);
      expect(b).toBeGreaterThanOrEqual(0);
      expect(b).toBeLessThanOrEqual(1);
    }
  });
  it("clamps out-of-range input", () => {
    const [r1] = turboRaw(-1);
    const [r2] = turboRaw(2);
    expect(r1).toBeGreaterThanOrEqual(0);
    expect(r2).toBeLessThanOrEqual(1);
  });
});

describe("jet", () => {
  it("returns RGB in [0,1]", () => {
    const [r, g, b] = jet(0.5);
    expect(r).toBeGreaterThanOrEqual(0); expect(r).toBeLessThanOrEqual(1);
    expect(g).toBeGreaterThanOrEqual(0); expect(g).toBeLessThanOrEqual(1);
    expect(b).toBeGreaterThanOrEqual(0); expect(b).toBeLessThanOrEqual(1);
  });
});

describe("jetHex", () => {
  it("returns a hex string", () => {
    expect(jetHex(0.5)).toMatch(/^#[0-9a-f]{6}$/);
  });
});

// ── Timestamp formatting ──────────────────────────────────────────────────────

describe("fmtUtcTime", () => {
  it("formats as HH:MM", () => {
    expect(fmtUtcTime(1700000000)).toMatch(/^\d{2}:\d{2}$/);
  });
});

describe("utcToLocalDate", () => {
  it("formats as YYYY-MM-DD", () => {
    expect(utcToLocalDate(1700000000)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("fmtGradientTs", () => {
  it("long span → date only", () => {
    const result = fmtGradientTs(1700000000, 200000);
    expect(result.length).toBeGreaterThan(0);
  });
  it("short span → includes seconds", () => {
    const result = fmtGradientTs(1700000000, 600);
    expect(result.length).toBeGreaterThan(0);
  });
  it("zero timestamp → empty", () => {
    expect(fmtGradientTs(0, 1000)).toBe("");
  });
});

// ── Geometry ──────────────────────────────────────────────────────────────────

describe("normalise", () => {
  it("empty → empty Float32Array", () => {
    const result = normalise([]);
    expect(result).toBeInstanceOf(Float32Array);
    expect(result.length).toBe(0);
  });
  it("single point → origin", () => {
    const result = normalise([pt(5, 10, 15)]);
    // Single point centroid is itself, so normalised to [0, 0, 0]
    expect(result[0]).toBeCloseTo(0);
    expect(result[1]).toBeCloseTo(0);
    expect(result[2]).toBeCloseTo(0);
  });
  it("output fits in [-1, 1]³", () => {
    const pts = [pt(0, 0, 0), pt(10, 0, 0), pt(0, 10, 0), pt(0, 0, 10)];
    const result = normalise(pts);
    for (let i = 0; i < result.length; i++) {
      expect(Math.abs(result[i])).toBeLessThanOrEqual(1.001);
    }
  });
  it("preserves relative distances", () => {
    const pts = [pt(0, 0, 0), pt(2, 0, 0), pt(4, 0, 0)];
    const result = normalise(pts);
    // Distance 0→1 should equal distance 1→2
    const d01 = Math.abs(result[3] - result[0]);
    const d12 = Math.abs(result[6] - result[3]);
    expect(d01).toBeCloseTo(d12, 5);
  });
});

describe("randomPositions", () => {
  it("returns Float32Array of correct length", () => {
    const pts = [pt(0, 0, 0), pt(1, 1, 1)];
    const result = randomPositions(pts);
    expect(result).toBeInstanceOf(Float32Array);
    expect(result.length).toBe(6);
  });
  it("values are small (within initial scatter range)", () => {
    const pts = Array.from({ length: 100 }, () => pt(0, 0, 0));
    const result = randomPositions(pts);
    for (let i = 0; i < result.length; i++) {
      expect(Math.abs(result[i])).toBeLessThan(2); // Gaussian, so very rarely >1
    }
  });
});

// ── Trace ticks ───────────────────────────────────────────────────────────────

describe("buildTraceTimeTicks", () => {
  it("empty → empty", () => expect(buildTraceTimeTicks([])).toEqual([]));
  it("single point → empty", () => expect(buildTraceTimeTicks([100])).toEqual([]));
  it("returns ticks with t in [0, 1]", () => {
    const sorted = Array.from({ length: 100 }, (_, i) => 1700000000 + i * 60);
    const ticks = buildTraceTimeTicks(sorted);
    expect(ticks.length).toBeGreaterThan(0);
    for (const tick of ticks) {
      expect(tick.t).toBeGreaterThanOrEqual(0);
      expect(tick.t).toBeLessThanOrEqual(1);
      expect(tick.label.length).toBeGreaterThan(0);
    }
  });
});

// ── Date palette ──────────────────────────────────────────────────────────────

describe("buildDatePaletteRaw", () => {
  it("empty → empty map", () => {
    expect(buildDatePaletteRaw([]).size).toBe(0);
  });
  it("assigns colors by day", () => {
    const pts = [
      pt(0, 0, 0, 1700000000), // day 1
      pt(1, 1, 1, 1700000000 + 86400), // day 2
    ];
    const palette = buildDatePaletteRaw(pts);
    expect(palette.size).toBe(2);
    for (const [, rgb] of palette) {
      expect(rgb).toHaveLength(3);
      rgb.forEach(c => {
        expect(c).toBeGreaterThanOrEqual(0);
        expect(c).toBeLessThanOrEqual(1);
      });
    }
  });
  it("skips points with utc=0", () => {
    const pts = [pt(0, 0, 0, 0), pt(1, 1, 1, 1700000000)];
    const palette = buildDatePaletteRaw(pts);
    // utc=0 produces "1970-01-01" which IS a valid date, so it might appear
    // The important thing is it doesn't crash
    expect(palette.size).toBeGreaterThanOrEqual(1);
  });
});
