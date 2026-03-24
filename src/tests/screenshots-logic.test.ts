// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import { pushHistory, fmtUs, fmtMs, fmtEta, sparklinePath, areaPath } from "$lib/screenshots-logic";

describe("pushHistory", () => {
  it("appends value", () => {
    expect(pushHistory([1, 2], 3)).toEqual([1, 2, 3]);
  });
  it("trims to maxLen", () => {
    const result = pushHistory([1, 2, 3], 4, 3);
    expect(result).toEqual([2, 3, 4]);
  });
  it("handles empty array", () => {
    expect(pushHistory([], 1)).toEqual([1]);
  });
});

describe("fmtUs", () => {
  it("formats microseconds", () => {
    expect(fmtUs(500)).toBe("500\u00B5s");
  });
  it("formats milliseconds", () => {
    expect(fmtUs(1500)).toBe("1.5ms");
  });
  it("formats seconds", () => {
    expect(fmtUs(2_500_000)).toBe("2.50s");
  });
});

describe("fmtMs", () => {
  it("formats sub-second", () => {
    expect(fmtMs(250)).toBe("250ms");
  });
  it("formats seconds", () => {
    expect(fmtMs(1500)).toBe("1.5s");
  });
});

describe("fmtEta", () => {
  it("returns empty for zero", () => {
    expect(fmtEta(0)).toBe("");
  });
  it("formats seconds", () => {
    expect(fmtEta(45)).toBe("~45s");
  });
  it("formats minutes + seconds", () => {
    expect(fmtEta(150)).toBe("~2m 30s");
  });
  it("formats exact minutes", () => {
    expect(fmtEta(120)).toBe("~2m");
  });
});

describe("sparklinePath", () => {
  it("returns empty for <2 data points", () => {
    expect(sparklinePath([1], 100, 50)).toBe("");
    expect(sparklinePath([], 100, 50)).toBe("");
  });
  it("generates an SVG path for valid data", () => {
    const path = sparklinePath([0, 5, 10], 100, 50);
    expect(path).toContain("M");
    expect(path).toContain("L");
  });
  it("starts with M and contains L for subsequent points", () => {
    const path = sparklinePath([1, 2, 3, 4], 200, 100);
    expect(path.startsWith("M")).toBe(true);
    expect((path.match(/L/g) || []).length).toBe(3);
  });
});

describe("areaPath", () => {
  it("returns empty for insufficient data", () => {
    expect(areaPath([1], 100, 50)).toBe("");
  });
  it("closes the path with Z", () => {
    const path = areaPath([0, 5, 10], 100, 50);
    expect(path.endsWith("Z")).toBe(true);
  });
});
