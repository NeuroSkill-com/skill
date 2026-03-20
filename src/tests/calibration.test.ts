// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * End-to-end tests for calibration timing and TTS synchronisation logic.
 *
 * These replicate the core countdown and calibration-loop algorithms from
 * `src/routes/calibration/+page.svelte` in a pure-JS environment so we can
 * assert timing accuracy and correct TTS call ordering without a running
 * Tauri backend.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ── Types (mirror the Svelte component) ──────────────────────────────────────

interface CalibrationAction { label: string; duration_secs: number }

interface CalibrationProfile {
  id: string;
  name: string;
  actions: CalibrationAction[];
  break_duration_secs: number;
  loop_count: number;
}

type PhaseKind = "idle" | "action" | "break" | "done";
interface Phase { kind: PhaseKind; actionIndex: number; loop: number }

// ── Helpers (copied verbatim from the component) ─────────────────────────────

function sleep(ms: number) {
  return new Promise<void>((r) => setTimeout(r, ms));
}

/**
 * Wall-clock-based countdown — identical to the implementation in the
 * calibration page after the timing-drift fix.
 */
async function runCountdown(
  secs: number,
  state: { countdown: number; totalSecs: number; running: boolean },
): Promise<boolean> {
  state.totalSecs = secs;
  state.countdown = secs;
  const endTime = Date.now() + secs * 1000;
  while (state.countdown > 0) {
    const remaining = endTime - Date.now();
    if (remaining <= 0) {
      state.countdown = 0;
      break;
    }
    const nextTick = remaining % 1000 || 1000;
    await sleep(nextTick);
    if (!state.running) return false;
    state.countdown = Math.max(0, Math.round((endTime - Date.now()) / 1000));
  }
  return true;
}

// ── Old (buggy) countdown for comparison ─────────────────────────────────────

async function runCountdownOld(
  secs: number,
  state: { countdown: number; totalSecs: number; running: boolean },
): Promise<boolean> {
  state.totalSecs = secs;
  state.countdown = secs;
  while (state.countdown > 0) {
    await sleep(1000);
    if (!state.running) return false;
    state.countdown--;
  }
  return true;
}

// ── Fake timers setup ────────────────────────────────────────────────────────

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  vi.useRealTimers();
});

// ─────────────────────────────────────────────────────────────────────────────
// 1. Wall-clock countdown accuracy
// ─────────────────────────────────────────────────────────────────────────────

describe("wall-clock runCountdown", () => {
  it("completes a 5-second countdown in ~5 000 ms", async () => {
    const state = { countdown: 0, totalSecs: 0, running: true };
    const start = Date.now();
    const p = runCountdown(5, state);
    await vi.advanceTimersByTimeAsync(5_500);
    const ok = await p;
    const elapsed = Date.now() - start;

    expect(ok).toBe(true);
    expect(state.countdown).toBe(0);
    // Wall-clock approach should finish within ±200 ms of target
    expect(elapsed).toBeGreaterThanOrEqual(4_800);
    expect(elapsed).toBeLessThanOrEqual(5_500);
  });

  it("countdown value decreases monotonically", async () => {
    const state = { countdown: 0, totalSecs: 0, running: true };
    const values: number[] = [];

    const p = runCountdown(3, state);
    // Sample countdown every 400 ms over 3.5 s
    for (let i = 0; i < 9; i++) {
      values.push(state.countdown);
      await vi.advanceTimersByTimeAsync(400);
    }
    await vi.advanceTimersByTimeAsync(1_000);
    await p;
    values.push(state.countdown);

    // Must be non-increasing
    for (let i = 1; i < values.length; i++) {
      expect(values[i]).toBeLessThanOrEqual(values[i - 1]!);
    }
    // Last value is 0
    expect(values[values.length - 1]).toBe(0);
  });

  it("can be cancelled mid-countdown", async () => {
    const state = { countdown: 0, totalSecs: 0, running: true };
    const p = runCountdown(10, state);

    await vi.advanceTimersByTimeAsync(3_000);
    expect(state.countdown).toBeGreaterThan(0);

    state.running = false;
    await vi.advanceTimersByTimeAsync(2_000);
    const ok = await p;

    expect(ok).toBe(false);
    expect(state.countdown).toBeGreaterThan(0); // stopped early
  });

  it("0-second countdown completes immediately", async () => {
    const state = { countdown: 0, totalSecs: 0, running: true };
    const p = runCountdown(0, state);
    await vi.advanceTimersByTimeAsync(100);
    const ok = await p;
    expect(ok).toBe(true);
    expect(state.countdown).toBe(0);
  });

  it("1-second countdown shows 1 then 0", async () => {
    const state = { countdown: 0, totalSecs: 0, running: true };
    const p = runCountdown(1, state);
    expect(state.countdown).toBe(1);
    await vi.advanceTimersByTimeAsync(1_200);
    await p;
    expect(state.countdown).toBe(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 2. TTS call ordering in the calibration loop
// ─────────────────────────────────────────────────────────────────────────────

describe("calibration loop TTS ordering", () => {
  /**
   * Simulate the full calibration loop with a mock TTS that records every
   * call and its type (wait / fire-and-forget).
   */
  async function simulateCalibration(profile: CalibrationProfile) {
    const calls: { text: string; awaited: boolean; phase: PhaseKind; timestamp: number }[] = [];
    const phases: Phase[] = [];

    let running = true;
    const state = { countdown: 0, totalSecs: 0, running: true };
    let phase: Phase = { kind: "idle", actionIndex: 0, loop: 1 };

    // TTS mock — records call and simulates a short delay for speech
    async function ttsSpeakWait(text: string): Promise<void> {
      calls.push({ text, awaited: true, phase: phase.kind, timestamp: Date.now() });
      await sleep(50); // simulate short synthesis time
    }
    function ttsSpeak(text: string): void {
      calls.push({ text, awaited: false, phase: phase.kind, timestamp: Date.now() });
    }

    // ── Start ──
    await ttsSpeakWait(
      `Calibration starting. ${profile.actions.length} actions, ${profile.loop_count} loops.`,
    );

    for (let loop = 1; loop <= profile.loop_count; loop++) {
      if (!state.running) break;

      for (let ai = 0; ai < profile.actions.length; ai++) {
        if (!state.running) break;
        const action = profile.actions[ai]!;

        // ACTION phase
        phase = { kind: "action", actionIndex: ai, loop };
        phases.push({ ...phase });
        await ttsSpeakWait(action.label);
        if (!state.running) break;

        const p = runCountdown(action.duration_secs, state);
        await vi.advanceTimersByTimeAsync(action.duration_secs * 1000 + 200);
        if (!(await p)) break;

        // BREAK phase
        const isLast = loop === profile.loop_count && ai === profile.actions.length - 1;
        if (!isLast && state.running) {
          const nextAction = profile.actions[(ai + 1) % profile.actions.length]!;
          phase = { kind: "break", actionIndex: ai, loop };
          phases.push({ ...phase });

          // Fixed: both awaited
          await ttsSpeakWait("Break.");
          if (!state.running) break;
          await sleep(50);
          await ttsSpeakWait(`Next: ${nextAction.label}.`);
          if (!state.running) break;

          const bp = runCountdown(profile.break_duration_secs, state);
          await vi.advanceTimersByTimeAsync(profile.break_duration_secs * 1000 + 200);
          if (!(await bp)) break;
        }
      }
    }

    if (state.running) {
      phase = { kind: "done", actionIndex: 0, loop: profile.loop_count };
      phases.push({ ...phase });
      ttsSpeak(`Calibration complete. ${profile.loop_count} loops recorded.`);
    }

    return { calls, phases };
  }

  const PROFILE: CalibrationProfile = {
    id: "test",
    name: "Test Profile",
    actions: [
      { label: "Eyes Open", duration_secs: 2 },
      { label: "Eyes Closed", duration_secs: 2 },
    ],
    break_duration_secs: 1,
    loop_count: 2,
  };

  it("all TTS calls during action/break phases are awaited (no fire-and-forget)", async () => {
    const { calls } = await simulateCalibration(PROFILE);

    // Every call during action or break phase must be awaited
    const phaseCalls = calls.filter(
      (c) => c.phase === "action" || c.phase === "break",
    );
    for (const c of phaseCalls) {
      expect(c.awaited).toBe(true);
    }
  });

  it("TTS announces action label before each action countdown", async () => {
    const { calls } = await simulateCalibration(PROFILE);

    const actionCalls = calls.filter((c) => c.phase === "action" && c.awaited);
    const labels = actionCalls.map((c) => c.text);

    // For 2 actions × 2 loops = 4 action announcements
    expect(labels).toEqual([
      "Eyes Open",
      "Eyes Closed",
      "Eyes Open",
      "Eyes Closed",
    ]);
  });

  it("break phase announces 'Break.' then 'Next: <action>.' in order", async () => {
    const { calls } = await simulateCalibration(PROFILE);

    const breakCalls = calls.filter((c) => c.phase === "break");

    // Between action pairs and loops we expect break announcements.
    // 2 actions × 2 loops − 1 (no break after last action) = 3 break phases
    // Each break phase has 2 TTS calls: "Break." and "Next: X."
    expect(breakCalls.length).toBe(6); // 3 breaks × 2 calls each

    // Verify ordering within each pair
    for (let i = 0; i < breakCalls.length; i += 2) {
      expect(breakCalls[i]!.text).toBe("Break.");
      expect(breakCalls[i + 1]!.text).toMatch(/^Next: /);
      // "Break." comes before "Next:" in time
      expect(breakCalls[i]!.timestamp).toBeLessThanOrEqual(breakCalls[i + 1]!.timestamp);
    }
  });

  it("'Next:' TTS is awaited — never fire-and-forget", async () => {
    const { calls } = await simulateCalibration(PROFILE);

    const nextCalls = calls.filter((c) => c.text.startsWith("Next: "));
    expect(nextCalls.length).toBeGreaterThan(0);
    for (const c of nextCalls) {
      expect(c.awaited).toBe(true);
    }
  });

  it("phase transitions follow correct order", async () => {
    const { phases } = await simulateCalibration(PROFILE);

    const kinds = phases.map((p) => p.kind);
    // Expected: action, break, action, break, action, break, action, done
    // (2 actions × 2 loops with breaks between all except the last)
    expect(kinds[0]).toBe("action");
    expect(kinds[kinds.length - 1]).toBe("done");

    // Every break is followed by an action (or done)
    for (let i = 0; i < kinds.length - 1; i++) {
      if (kinds[i] === "break") {
        expect(kinds[i + 1]).toMatch(/^(action|done)$/);
      }
    }
  });

  it("calibration starts with an awaited starting announcement", async () => {
    const { calls } = await simulateCalibration(PROFILE);
    expect(calls[0]!.text).toContain("Calibration starting");
    expect(calls[0]!.awaited).toBe(true);
  });

  it("calibration ends with completion announcement", async () => {
    const { calls } = await simulateCalibration(PROFILE);
    const last = calls[calls.length - 1]!;
    expect(last.text).toContain("Calibration complete");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 3. Single-action profile (no breaks needed)
// ─────────────────────────────────────────────────────────────────────────────

describe("single-action profile", () => {
  it("runs without break phases", async () => {
    const calls: { text: string; awaited: boolean }[] = [];
    const state = { countdown: 0, totalSecs: 0, running: true };

    const profile: CalibrationProfile = {
      id: "single",
      name: "Single",
      actions: [{ label: "Meditate", duration_secs: 2 }],
      break_duration_secs: 1,
      loop_count: 3,
    };

    async function ttsSpeakWait(text: string) {
      calls.push({ text, awaited: true });
      await sleep(10);
    }
    function ttsSpeak(text: string) {
      calls.push({ text, awaited: false });
    }

    // Start
    await ttsSpeakWait(`Calibration starting. 1 actions, 3 loops.`);

    for (let loop = 1; loop <= profile.loop_count; loop++) {
      if (!state.running) break;
      const action = profile.actions[0]!;
      await ttsSpeakWait(action.label);

      const p = runCountdown(action.duration_secs, state);
      await vi.advanceTimersByTimeAsync(action.duration_secs * 1000 + 200);
      await p;

      // Break between loops (not after last)
      const isLast = loop === profile.loop_count;
      if (!isLast && state.running) {
        await ttsSpeakWait("Break.");
        await sleep(10);
        await ttsSpeakWait(`Next: ${action.label}.`);
        const bp = runCountdown(profile.break_duration_secs, state);
        await vi.advanceTimersByTimeAsync(profile.break_duration_secs * 1000 + 200);
        await bp;
      }
    }
    ttsSpeak("Calibration complete. 3 loops recorded.");

    // 3 action announcements + start + 2 break pairs + completion = 1 + 3 + 4 + 1 = 9
    const actionAnn = calls.filter((c) => c.text === "Meditate");
    expect(actionAnn).toHaveLength(3);

    const breakAnn = calls.filter((c) => c.text === "Break.");
    expect(breakAnn).toHaveLength(2);

    // No fire-and-forget except the final completion
    const fireAndForget = calls.filter((c) => !c.awaited);
    expect(fireAndForget).toHaveLength(1);
    expect(fireAndForget[0]!.text).toContain("Calibration complete");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// 4. Countdown progress percentage
// ─────────────────────────────────────────────────────────────────────────────

describe("progress percentage", () => {
  function progressPct(totalSecs: number, countdown: number): number {
    return totalSecs > 0 ? ((totalSecs - countdown) / totalSecs) * 100 : 0;
  }

  it("starts at 0%", () => expect(progressPct(10, 10)).toBe(0));
  it("ends at 100%", () => expect(progressPct(10, 0)).toBe(100));
  it("is 50% at midpoint", () => expect(progressPct(10, 5)).toBe(50));
  it("returns 0 for zero duration", () => expect(progressPct(0, 0)).toBe(0));
});

// ─────────────────────────────────────────────────────────────────────────────
// 5. Drift comparison: old vs new countdown
// ─────────────────────────────────────────────────────────────────────────────

describe("drift comparison (old vs new)", () => {
  it("new wall-clock countdown reaches 0 with correct duration", async () => {
    const state = { countdown: 0, totalSecs: 0, running: true };
    const start = Date.now();

    const p = runCountdown(3, state);
    await vi.advanceTimersByTimeAsync(3_500);
    await p;

    const elapsed = Date.now() - start;
    expect(state.countdown).toBe(0);
    // Should be within 500ms of 3000ms
    expect(elapsed).toBeGreaterThanOrEqual(2_800);
    expect(elapsed).toBeLessThanOrEqual(3_500);
  });

  it("old sequential-sleep countdown also completes (baseline)", async () => {
    const state = { countdown: 0, totalSecs: 0, running: true };
    const start = Date.now();

    const p = runCountdownOld(3, state);
    await vi.advanceTimersByTimeAsync(3_500);
    await p;

    const elapsed = Date.now() - start;
    expect(state.countdown).toBe(0);
    // With fake timers the old approach also works; the drift issue manifests
    // with real setTimeout jitter which fake timers don't reproduce. This test
    // serves as a baseline to confirm both implementations complete.
    expect(elapsed).toBeGreaterThanOrEqual(2_800);
  });
});
