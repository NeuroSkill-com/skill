// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  autoSelectRange,
  localMidnight,
  pointerToUtc,
  sessionLabel,
  sessionsInRange,
  snapToMinute,
  sortedSessionDays,
  timeStrToUtc,
  utcToTimeStr,
} from "$lib/compare/compare-logic";
import type { EmbeddingSession } from "$lib/compare/compare-types";

const sess = (start: number, end: number, n = 10): EmbeddingSession => ({
  start_utc: start,
  end_utc: end,
  n_epochs: n,
  day: "2026-03-24",
});

describe("localMidnight", () => {
  it("returns midnight UTC for local timezone", () => {
    const m = localMidnight("2026-03-24");
    const d = new Date(m * 1000);
    expect(d.getHours()).toBe(0);
    expect(d.getMinutes()).toBe(0);
    expect(d.getDate()).toBe(24);
  });
});

describe("utcToTimeStr", () => {
  it("formats a timestamp as HH:MM", () => {
    const t = utcToTimeStr(localMidnight("2026-03-24") + 3600 * 14 + 30 * 60);
    expect(t).toBe("14:30");
  });
});

describe("timeStrToUtc", () => {
  it("round-trips with utcToTimeStr", () => {
    const original = localMidnight("2026-03-24") + 3600 * 9 + 45 * 60;
    const timeStr = utcToTimeStr(original);
    const backToUtc = timeStrToUtc("2026-03-24", timeStr);
    expect(Math.abs(backToUtc - original)).toBeLessThan(60);
  });
});

describe("sessionsInRange", () => {
  const sessions = [sess(1000, 2000), sess(3000, 4000), sess(5000, 6000)];

  it("returns sessions overlapping the range", () => {
    const result = sessionsInRange(sessions, 1500, 3500);
    expect(result.length).toBe(2); // sessions 0 and 1 overlap
  });

  it("returns empty for non-overlapping range", () => {
    const result = sessionsInRange(sessions, 2001, 2999);
    expect(result.length).toBe(0);
  });

  it("returns all for wide range", () => {
    const result = sessionsInRange(sessions, 0, 99999);
    expect(result.length).toBe(3);
  });
});

describe("sessionLabel", () => {
  it("includes epoch count", () => {
    const label = sessionLabel(sess(1000, 2000, 42));
    expect(label).toContain("42 ep");
  });
});

describe("sortedSessionDays", () => {
  it("returns unique days newest first", () => {
    const now = Math.floor(Date.now() / 1000);
    const yesterday = now - 86400;
    const sessions = [sess(yesterday, yesterday + 3600), sess(now, now + 3600)];
    const days = sortedSessionDays(sessions);
    expect(days.length).toBeGreaterThanOrEqual(2);
    // Newest first
    expect(days[0] > days[days.length - 1]).toBe(true);
  });
});

describe("autoSelectRange", () => {
  it("selects range covering sessions in the 48h window", () => {
    const anchor = 10000;
    const sessions = [sess(10500, 11000), sess(12000, 13000)];
    const { start, end } = autoSelectRange(sessions, anchor);
    expect(start).toBe(10500);
    expect(end).toBe(13000);
  });

  it("falls back to anchor day when no sessions", () => {
    const anchor = 10000;
    const { start, end } = autoSelectRange([], anchor);
    expect(start).toBe(anchor);
    expect(end).toBe(anchor + 86400);
  });
});

describe("snapToMinute", () => {
  it("snaps to nearest minute", () => {
    expect(snapToMinute(3629)).toBe(3600);
    expect(snapToMinute(3631)).toBe(3660);
  });
});

describe("pointerToUtc", () => {
  it("returns anchor at left edge", () => {
    const rect = { left: 0, width: 100 } as DOMRect;
    expect(pointerToUtc(0, rect, 1000)).toBe(1000);
  });

  it("returns anchor + 48h at right edge", () => {
    const rect = { left: 0, width: 100 } as DOMRect;
    expect(pointerToUtc(100, rect, 1000)).toBe(1000 + 172800);
  });

  it("clamps to bounds", () => {
    const rect = { left: 0, width: 100 } as DOMRect;
    expect(pointerToUtc(-50, rect, 1000)).toBe(1000);
    expect(pointerToUtc(200, rect, 1000)).toBe(1000 + 172800);
  });
});
