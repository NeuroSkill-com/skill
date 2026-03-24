// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import {
  sigmoid100,
  computeRawScores,
  emaSmooth,
  fmtUptime,
  fmtEeg,
  redact,
  goalProgress,
  isMuseDevice,
  hasBattery,
} from "$lib/dashboard-logic";

describe("sigmoid100", () => {
  it("returns ~50 at x=1 (midpoint)", () => {
    const v = sigmoid100(1);
    expect(v).toBeCloseTo(50, 0);
  });
  it("approaches 0 for large negative x", () => {
    expect(sigmoid100(-10)).toBeLessThan(1);
  });
  it("approaches 100 for large positive x", () => {
    expect(sigmoid100(10)).toBeGreaterThan(99);
  });
  it("is monotonically increasing", () => {
    const a = sigmoid100(0.5);
    const b = sigmoid100(1.0);
    const c = sigmoid100(2.0);
    expect(a).toBeLessThan(b);
    expect(b).toBeLessThan(c);
  });
});

describe("computeRawScores", () => {
  it("returns null for empty channels", () => {
    expect(computeRawScores([])).toBeNull();
  });
  it("returns scores for valid channels", () => {
    const result = computeRawScores([{ rel_alpha: 0.3, rel_beta: 0.4, rel_theta: 0.2 }]);
    expect(result).not.toBeNull();
    expect(result!.focus).toBeGreaterThan(0);
    expect(result!.relax).toBeGreaterThan(0);
    expect(result!.engagement).toBeGreaterThan(0);
  });
  it("handles zero-power channels gracefully", () => {
    const result = computeRawScores([{ rel_alpha: 0, rel_beta: 0, rel_theta: 0 }]);
    expect(result).not.toBeNull();
  });
  it("averages across multiple channels", () => {
    const one = computeRawScores([{ rel_alpha: 0.5, rel_beta: 0.5, rel_theta: 0.1 }]);
    const two = computeRawScores([
      { rel_alpha: 0.5, rel_beta: 0.5, rel_theta: 0.1 },
      { rel_alpha: 0.5, rel_beta: 0.5, rel_theta: 0.1 },
    ]);
    expect(one!.focus).toBeCloseTo(two!.focus, 5);
  });
});

describe("emaSmooth", () => {
  it("returns raw when tau=1 (instant tracking)", () => {
    expect(emaSmooth(50, 100, 1)).toBe(100);
  });
  it("returns prev when tau=0 (no tracking)", () => {
    expect(emaSmooth(50, 100, 0)).toBe(50);
  });
  it("interpolates for intermediate tau", () => {
    const v = emaSmooth(0, 100, 0.5);
    expect(v).toBe(50);
  });
});

describe("fmtUptime", () => {
  it("formats 0 seconds", () => {
    expect(fmtUptime(0)).toBe("00:00:00");
  });
  it("formats hours, minutes, seconds", () => {
    expect(fmtUptime(3661)).toBe("01:01:01");
  });
  it("formats large values", () => {
    expect(fmtUptime(86399)).toBe("23:59:59");
  });
});

describe("fmtEeg", () => {
  it("formats positive values", () => {
    expect(fmtEeg(1.23)).toBe("+1.2 \u00B5V");
  });
  it("formats negative values", () => {
    expect(fmtEeg(-0.5)).toBe("-0.5 \u00B5V");
  });
  it("returns dash for null", () => {
    expect(fmtEeg(null)).toBe("\u2014");
  });
  it("returns dash for undefined", () => {
    expect(fmtEeg(undefined)).toBe("\u2014");
  });
});

describe("redact", () => {
  it("redacts all segments except last", () => {
    expect(redact("AA-BB-CC")).toBe("**-**-CC");
  });
  it("handles single segment", () => {
    expect(redact("ABCDEF")).toBe("ABCDEF");
  });
});

describe("goalProgress", () => {
  it("returns 0 for zero goal", () => {
    expect(goalProgress(3600, 0)).toBe(0);
  });
  it("returns 100 when goal reached", () => {
    expect(goalProgress(3600, 60)).toBe(100);
  });
  it("clamps at 100", () => {
    expect(goalProgress(7200, 60)).toBe(100);
  });
  it("returns 50 at halfway", () => {
    expect(goalProgress(1800, 60)).toBe(50);
  });
});

describe("device classification", () => {
  it("isMuse for muse and unknown", () => {
    expect(isMuseDevice("muse")).toBe(true);
    expect(isMuseDevice("unknown")).toBe(true);
    expect(isMuseDevice("ganglion")).toBe(false);
  });
  it("hasBattery for supported devices", () => {
    expect(hasBattery("muse")).toBe(true);
    expect(hasBattery("mw75")).toBe(true);
    expect(hasBattery("emotiv")).toBe(true);
    expect(hasBattery("idun")).toBe(true);
    expect(hasBattery("ganglion")).toBe(false);
    expect(hasBattery("hermes")).toBe(false);
  });
});
