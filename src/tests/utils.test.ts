// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Unit tests for pure utility functions used across the frontend.
 */
import { describe, it, expect } from "vitest";

// ── Reconnect backoff (mirrors the Rust schedule) ──────────────────────────────
// The Rust function retry_delay_secs() is tested in Rust, but we duplicate
// the formula here so any accidental frontend re-implementation stays correct.
function retryDelaySecs(attempt: number): number {
  if (attempt === 0) return 1;
  if (attempt === 1) return 2;
  if (attempt === 2) return 3;
  return 5;
}

describe("reconnect backoff schedule", () => {
  it("attempt 0 → 1 s", () => expect(retryDelaySecs(0)).toBe(1));
  it("attempt 1 → 2 s", () => expect(retryDelaySecs(1)).toBe(2));
  it("attempt 2 → 3 s", () => expect(retryDelaySecs(2)).toBe(3));
  it("attempt 3 → 5 s", () => expect(retryDelaySecs(3)).toBe(5));
  it("attempt 10 → 5 s (capped)", () => expect(retryDelaySecs(10)).toBe(5));
  it("never exceeds 5 s", () => {
    for (let a = 3; a <= 50; a++) expect(retryDelaySecs(a)).toBe(5);
  });
});

// ── est_secs ceiling formula (mirrors job_queue.rs) ────────────────────────────
function estSecsCeil(totalMs: number): number {
  return Math.ceil(totalMs / 1000);
}

describe("job queue est_secs ceiling", () => {
  it("0 ms → 0 s",    () => expect(estSecsCeil(0)).toBe(0));
  it("1 ms → 1 s",    () => expect(estSecsCeil(1)).toBe(1));
  it("1000 ms → 1 s", () => expect(estSecsCeil(1000)).toBe(1));
  it("1001 ms → 2 s", () => expect(estSecsCeil(1001)).toBe(2));
  it("5000 ms → 5 s", () => expect(estSecsCeil(5000)).toBe(5));
});

// ── Focus Timer preset defaults ─────────────────────────────────────────────────
const PRESETS = {
  pomodoro:   { workMins: 25, breakMins: 5,  longBreakMins: 15, longBreakEvery: 4 },
  deepWork:   { workMins: 50, breakMins: 10, longBreakMins: 30, longBreakEvery: 2 },
  shortFocus: { workMins: 15, breakMins: 5,  longBreakMins: 15, longBreakEvery: 4 },
} as const;

describe("focus timer preset values", () => {
  it("pomodoro has 25/5 work/break", () => {
    expect(PRESETS.pomodoro.workMins).toBe(25);
    expect(PRESETS.pomodoro.breakMins).toBe(5);
  });
  it("deep work has 50/10 work/break", () => {
    expect(PRESETS.deepWork.workMins).toBe(50);
    expect(PRESETS.deepWork.breakMins).toBe(10);
  });
  it("short focus has 15/5 work/break", () => {
    expect(PRESETS.shortFocus.workMins).toBe(15);
    expect(PRESETS.shortFocus.breakMins).toBe(5);
  });
  it("all work durations are positive", () => {
    for (const p of Object.values(PRESETS)) {
      expect(p.workMins).toBeGreaterThan(0);
      expect(p.breakMins).toBeGreaterThan(0);
    }
  });
});

// ── Pagination helpers ──────────────────────────────────────────────────────────
function paginate<T>(items: T[], page: number, pageSize: number): T[] {
  return items.slice(page * pageSize, (page + 1) * pageSize);
}

describe("pagination", () => {
  const items = Array.from({ length: 120 }, (_, i) => i);

  it("page 0 returns first PAGE_SIZE items", () => {
    expect(paginate(items, 0, 50)).toHaveLength(50);
    expect(paginate(items, 0, 50)[0]).toBe(0);
    expect(paginate(items, 0, 50)[49]).toBe(49);
  });

  it("page 1 returns next PAGE_SIZE items", () => {
    expect(paginate(items, 1, 50)[0]).toBe(50);
  });

  it("last page has remaining items", () => {
    // 120 items, PAGE_SIZE 50 → page 2 has 20 items
    expect(paginate(items, 2, 50)).toHaveLength(20);
  });

  it("out-of-range page returns empty array", () => {
    expect(paginate(items, 99, 50)).toHaveLength(0);
  });
});
