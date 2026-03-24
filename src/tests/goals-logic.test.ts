// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import { barColor, fmtMins } from "$lib/goals-logic";

describe("barColor", () => {
  it("returns green when goal met", () => {
    expect(barColor(60, 60)).toBe("#22c55e");
    expect(barColor(90, 60)).toBe("#22c55e");
  });
  it("returns blue for halfway+", () => {
    expect(barColor(35, 60)).toBe("#3b82f6");
  });
  it("returns transparent for zero", () => {
    expect(barColor(0, 60)).toBe("transparent");
  });
  it("returns indigo for some progress", () => {
    expect(barColor(10, 60)).toBe("#6366f1");
  });
});

describe("fmtMins", () => {
  it("returns dash for zero", () => {
    expect(fmtMins(0)).toBe("\u2014");
  });
  it("formats minutes only", () => {
    expect(fmtMins(45)).toBe("45m");
  });
  it("formats hours and minutes", () => {
    expect(fmtMins(83)).toBe("1h 23m");
  });
  it("formats exact hours", () => {
    expect(fmtMins(120)).toBe("2h");
  });
});
