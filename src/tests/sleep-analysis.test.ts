// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import { analyzeSleep } from "$lib/settings/sleep-analysis";
import type { SleepStages } from "$lib/types";

function mkStages(stages: number[], epochSecs = 30): SleepStages {
  const counts = [0, 0, 0, 0, 0]; // wake, n1, n2, n3, rem
  for (const s of stages) counts[s]++;
  return {
    epochs: stages.map((stage, i) => ({
      utc: 1700000000 + i * epochSecs,
      stage,
      rel_delta: 0,
      rel_theta: 0,
      rel_alpha: 0,
      rel_beta: 0,
    })),
    summary: {
      total_epochs: stages.length,
      wake_epochs: counts[0],
      n1_epochs: counts[1],
      n2_epochs: counts[2],
      n3_epochs: counts[3],
      rem_epochs: counts[4],
      epoch_secs: epochSecs,
    },
  };
}

describe("analyzeSleep", () => {
  it("computes efficiency", () => {
    // 2 wake, 8 sleep → 80% efficiency
    const stages = mkStages([0, 0, 1, 2, 2, 3, 3, 2, 1, 4]);
    const result = analyzeSleep(stages);
    expect(result.efficiency).toBeCloseTo(80);
  });

  it("computes onset latency", () => {
    // 3 wake epochs × 30s = 1.5 min onset
    const stages = mkStages([0, 0, 0, 1, 2, 2, 3]);
    const result = analyzeSleep(stages);
    expect(result.onsetLatencyMin).toBeCloseTo(1.5);
  });

  it("computes REM latency from onset", () => {
    // onset at index 2, REM at index 5 → (5-2)*30/60 = 1.5 min
    const stages = mkStages([0, 0, 1, 2, 3, 4, 4]);
    const result = analyzeSleep(stages);
    expect(result.remLatencyMin).toBeCloseTo(1.5);
  });

  it("returns -1 for REM latency when no REM", () => {
    const stages = mkStages([0, 1, 2, 2, 3, 3, 2]);
    const result = analyzeSleep(stages);
    expect(result.remLatencyMin).toBe(-1);
  });

  it("counts awakenings", () => {
    // sleep→wake transitions: at index 3 and index 6
    const stages = mkStages([0, 1, 2, 0, 1, 2, 0]);
    const result = analyzeSleep(stages);
    expect(result.awakenings).toBe(2);
  });

  it("computes stage minutes", () => {
    const stages = mkStages([0, 0, 1, 2, 2, 3, 4], 60); // 60s epochs
    const result = analyzeSleep(stages);
    expect(result.stageMinutes.wake).toBeCloseTo(2);
    expect(result.stageMinutes.n1).toBeCloseTo(1);
    expect(result.stageMinutes.n2).toBeCloseTo(2);
    expect(result.stageMinutes.n3).toBeCloseTo(1);
    expect(result.stageMinutes.rem).toBeCloseTo(1);
    expect(result.totalMin).toBeCloseTo(7);
  });

  it("handles all-wake recording", () => {
    const stages = mkStages([0, 0, 0, 0]);
    const result = analyzeSleep(stages);
    expect(result.efficiency).toBe(0);
    expect(result.onsetLatencyMin).toBeCloseTo(result.totalMin);
    expect(result.awakenings).toBe(0);
  });
});
