// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import { tsToUnix, relativeAge } from "$lib/hooks-logic";

describe("tsToUnix", () => {
  it("passes through seconds-range timestamps", () => {
    expect(tsToUnix(1700000000)).toBe(1700000000);
  });
  it("converts millisecond timestamps", () => {
    expect(tsToUnix(1700000000000)).toBe(1700000000);
  });
  it("converts microsecond timestamps", () => {
    expect(tsToUnix(1700000000000000)).toBe(1700000000);
  });
  it("returns 0 for NaN", () => {
    expect(tsToUnix(NaN)).toBe(0);
  });
  it("returns 0 for zero", () => {
    expect(tsToUnix(0)).toBe(0);
  });
});

describe("relativeAge", () => {
  const now = 1700000100;

  it("shows seconds for <1m", () => {
    expect(relativeAge(now - 30, now, "ago")).toBe("30s ago");
  });
  it("shows minutes for <1h", () => {
    expect(relativeAge(now - 300, now, "ago")).toBe("5m ago");
  });
  it("shows hours for <1d", () => {
    expect(relativeAge(now - 7200, now, "ago")).toBe("2h ago");
  });
  it("shows days for >1d", () => {
    expect(relativeAge(now - 172800, now, "ago")).toBe("2d ago");
  });
  it("returns empty for future timestamps", () => {
    expect(relativeAge(now + 100, now)).toBe("");
  });
  it("returns empty for invalid input", () => {
    expect(relativeAge(NaN, now)).toBe("");
  });
  it("handles microsecond timestamps", () => {
    const tsMicro = (now - 60) * 1_000_000;
    expect(relativeAge(tsMicro, now, "ago")).toBe("1m ago");
  });
});
