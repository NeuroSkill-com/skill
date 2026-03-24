// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import { add3, scale3, normalize3, length3, fibSphere, turbo, turboCss, turboHex } from "$lib/graph3d-logic";

describe("add3", () => {
  it("adds vectors", () => {
    expect(add3([1, 2, 3], [4, 5, 6])).toEqual([5, 7, 9]);
  });
  it("handles zeros", () => {
    expect(add3([0, 0, 0], [1, 2, 3])).toEqual([1, 2, 3]);
  });
});

describe("scale3", () => {
  it("scales vector", () => {
    expect(scale3([1, 2, 3], 2)).toEqual([2, 4, 6]);
  });
  it("scales by zero", () => {
    expect(scale3([1, 2, 3], 0)).toEqual([0, 0, 0]);
  });
});

describe("normalize3", () => {
  it("normalizes to unit length", () => {
    const [x, y, z] = normalize3([3, 4, 0]);
    expect(Math.sqrt(x ** 2 + y ** 2 + z ** 2)).toBeCloseTo(1);
  });
  it("handles zero vector", () => {
    const [x, y, z] = normalize3([0, 0, 0]);
    expect(x).toBe(0);
    expect(y).toBe(0);
    expect(z).toBe(0);
  });
});

describe("length3", () => {
  it("computes length", () => {
    expect(length3([3, 4, 0])).toBeCloseTo(5);
  });
  it("returns 0 for zero vector", () => {
    expect(length3([0, 0, 0])).toBe(0);
  });
});

describe("fibSphere", () => {
  it("places points on unit sphere", () => {
    for (let i = 0; i < 10; i++) {
      const [x, y, z] = fibSphere(i, 10);
      expect(Math.sqrt(x ** 2 + y ** 2 + z ** 2)).toBeCloseTo(1, 5);
    }
  });
  it("first point is near north pole", () => {
    const [, y] = fibSphere(0, 100);
    expect(y).toBeCloseTo(1, 1);
  });
  it("last point is near south pole", () => {
    const [, y] = fibSphere(99, 100);
    expect(y).toBeCloseTo(-1, 1);
  });
  it("handles n=1", () => {
    const [x, y, z] = fibSphere(0, 1);
    expect(Number.isFinite(x)).toBe(true);
    expect(Number.isFinite(y)).toBe(true);
    expect(Number.isFinite(z)).toBe(true);
  });
});

describe("turbo", () => {
  it("returns values in [0,1]", () => {
    for (const t of [0, 0.25, 0.5, 0.75, 1]) {
      const [r, g, b] = turbo(t);
      expect(r).toBeGreaterThanOrEqual(0);
      expect(r).toBeLessThanOrEqual(1);
      expect(g).toBeGreaterThanOrEqual(0);
      expect(g).toBeLessThanOrEqual(1);
      expect(b).toBeGreaterThanOrEqual(0);
      expect(b).toBeLessThanOrEqual(1);
    }
  });
});

describe("turboCss", () => {
  it("returns a hex string", () => {
    const s = turboCss(0.5);
    expect(s).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe("turboHex", () => {
  it("returns a packed integer", () => {
    const v = turboHex(0);
    expect(typeof v).toBe("number");
    expect(v).toBeGreaterThanOrEqual(0);
    expect(v).toBeLessThanOrEqual(0xffffff);
  });
});
