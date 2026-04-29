// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import { computeStats, LAPSE_THRESHOLD_MS, lapseCount, mean, median, slowest10Mean } from "../lib/settings/pvt-stats";

describe("PVT statistics — Dinges & Powell convention", () => {
  it("mean returns 0 for empty input", () => {
    expect(mean([])).toBe(0);
  });

  it("mean of a single value equals that value", () => {
    expect(mean([300])).toBe(300);
  });

  it("mean averages correctly", () => {
    expect(mean([200, 300, 400])).toBe(300);
    expect(mean([100, 200])).toBe(150);
  });

  it("median picks the upper-middle of an even-length array", () => {
    expect(median([1, 2, 3, 4])).toBe(3);
    expect(median([100, 200, 300, 400])).toBe(300);
  });

  it("median is the middle of an odd-length array, regardless of input order", () => {
    expect(median([5, 3, 1, 4, 2])).toBe(3);
  });

  it("median returns 0 for empty input", () => {
    expect(median([])).toBe(0);
  });

  it("lapseCount uses 500 ms threshold", () => {
    expect(LAPSE_THRESHOLD_MS).toBe(500);
    expect(lapseCount([200, 300, 500, 501, 999])).toBe(2);
    expect(lapseCount([200, 300])).toBe(0);
  });

  it("lapseCount accepts an explicit threshold override", () => {
    expect(lapseCount([200, 300, 400, 500], 350)).toBe(2);
  });

  it("slowest10Mean averages the worst 10% of trials", () => {
    // 20 trials → cut at 18, slowest 10% = trials 18 + 19 (the two largest).
    const trials = [100, 110, 120, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250, 260, 270, 800, 900];
    expect(slowest10Mean(trials)).toBe(850);
  });

  it("slowest10Mean falls back to the slowest single trial when n < 10", () => {
    // 5 trials → floor(5 * 0.9) = 4 → slice(4) = [last]; mean = that trial.
    expect(slowest10Mean([100, 200, 300, 400, 1500])).toBe(1500);
  });

  it("slowest10Mean returns 0 for empty input", () => {
    expect(slowest10Mean([])).toBe(0);
  });

  it("computeStats produces the canonical record posted to the daemon", () => {
    const rts = [200, 250, 300, 350, 400, 450, 600, 700, 800, 900];
    const s = computeStats(rts);
    expect(s.response_count).toBe(10);
    expect(s.mean_rt_ms).toBe(495);
    expect(s.median_rt_ms).toBe(450);
    expect(s.lapse_count).toBe(4); // 600, 700, 800, 900 are > 500 ms
    // floor(10 * 0.9) = 9 → slice(9) = [900]; mean = 900
    expect(s.slowest10_rt_ms).toBe(900);
  });

  it("computeStats survives an empty session (user cancelled before any responses)", () => {
    const s = computeStats([]);
    expect(s).toEqual({
      mean_rt_ms: 0,
      median_rt_ms: 0,
      slowest10_rt_ms: 0,
      lapse_count: 0,
      response_count: 0,
    });
  });
});
