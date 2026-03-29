// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import { buildColorArray, normalisePoints, randomPositions, rgbToHex, turboApprox } from "$lib/umap-viewer-logic";

describe("turboApprox", () => {
  it("returns values in [0,1] for all inputs", () => {
    for (const t of [0, 0.25, 0.5, 0.75, 1.0]) {
      const [r, g, b] = turboApprox(t);
      expect(r).toBeGreaterThanOrEqual(0);
      expect(r).toBeLessThanOrEqual(1);
      expect(g).toBeGreaterThanOrEqual(0);
      expect(g).toBeLessThanOrEqual(1);
      expect(b).toBeGreaterThanOrEqual(0);
      expect(b).toBeLessThanOrEqual(1);
    }
  });

  it("clamps out-of-range inputs", () => {
    const [r1] = turboApprox(-1);
    const [r2] = turboApprox(2);
    expect(r1).toBeGreaterThanOrEqual(0);
    expect(r2).toBeGreaterThanOrEqual(0);
  });
});

describe("rgbToHex", () => {
  it("converts black", () => {
    expect(rgbToHex(0, 0, 0)).toBe("#000000");
  });
  it("converts white", () => {
    expect(rgbToHex(1, 1, 1)).toBe("#ffffff");
  });
  it("converts red", () => {
    expect(rgbToHex(1, 0, 0)).toBe("#ff0000");
  });
});

describe("normalisePoints", () => {
  it("normalises to [-1, 1] range", () => {
    const pts = [
      { x: 0, y: 0, z: 0, utc: 0, label: undefined, set: "a", session: 0 },
      { x: 10, y: 10, z: 10, utc: 1, label: undefined, set: "a", session: 0 },
    ];
    const arr = normalisePoints(pts);
    expect(arr.length).toBe(6);
    // First point should be at -1, last at +1
    expect(arr[0]).toBeCloseTo(-1);
    expect(arr[3]).toBeCloseTo(1);
  });

  it("handles single point", () => {
    const pts = [{ x: 5, y: 5, z: 5, utc: 0, label: undefined, set: "a", session: 0 }];
    const arr = normalisePoints(pts);
    expect(arr.length).toBe(3);
    // With a single point, range is 0 → scale defaults to 1
    expect(Number.isFinite(arr[0])).toBe(true);
  });

  it("handles 2D points (z=undefined)", () => {
    const pts = [
      { x: 0, y: 0, z: undefined as number | undefined, utc: 0, label: undefined, set: "a", session: 0 },
      { x: 1, y: 1, z: undefined as number | undefined, utc: 1, label: undefined, set: "a", session: 0 },
    ];
    const arr = normalisePoints(pts);
    expect(arr.length).toBe(6);
    expect(Number.isFinite(arr[2])).toBe(true); // z should be finite
  });
});

describe("randomPositions", () => {
  it("returns correct length", () => {
    const arr = randomPositions(100);
    expect(arr.length).toBe(300);
  });

  it("values are in [-1, 1]", () => {
    const arr = randomPositions(50);
    for (let i = 0; i < arr.length; i++) {
      expect(arr[i]).toBeGreaterThanOrEqual(-1);
      expect(arr[i]).toBeLessThanOrEqual(1);
    }
  });

  it("is deterministic with same seed", () => {
    const a = randomPositions(10, 42);
    const b = randomPositions(10, 42);
    expect(Array.from(a)).toEqual(Array.from(b));
  });

  it("differs with different seeds", () => {
    const a = randomPositions(10, 1);
    const b = randomPositions(10, 2);
    expect(Array.from(a)).not.toEqual(Array.from(b));
  });
});

describe("buildColorArray", () => {
  it("returns correct length", () => {
    const arr = buildColorArray([0, 0.5, 1], true);
    expect(arr.length).toBe(9);
  });

  it("applies dimming for light theme", () => {
    const dark = buildColorArray([0.5], true);
    const light = buildColorArray([0.5], false, 0.5);
    // Light theme values should be smaller (dimmed)
    for (let i = 0; i < 3; i++) {
      expect(light[i]).toBeLessThanOrEqual(dark[i]);
    }
  });
});
