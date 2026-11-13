// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

import { describe, expect, it } from "vitest";
import {
  appendTrace,
  arousalBar,
  arousalLevel,
  EDA_TRACE_WINDOW_S,
  EMPTY_TRACE,
  POOR_CONTACT_FRACTION,
  rejectedPercent,
} from "../lib/dashboard/eda-logic";

describe("appendTrace", () => {
  it("appends samples in order", () => {
    const a = appendTrace(EMPTY_TRACE, [
      [100, 2.0],
      [101, 2.1],
    ]);
    expect(a).toEqual({ t: [100, 101], v: [2.0, 2.1] });
  });

  it("does not mutate its input", () => {
    const base = { t: [1], v: [2] };
    appendTrace(base, [[2, 3]]);
    expect(base).toEqual({ t: [1], v: [2] });
  });

  it("drops samples older than the window, measured from the newest", () => {
    let tr = appendTrace(EMPTY_TRACE, [[0, 1]], 10);
    tr = appendTrace(tr, [[5, 2]], 10);
    tr = appendTrace(tr, [[12, 3]], 10);
    expect(tr.t).toEqual([5, 12]);
    expect(tr.v).toEqual([2, 3]);
  });

  it("keeps a sample exactly at the window edge", () => {
    const tr = appendTrace({ t: [0], v: [1] }, [[10, 2]], 10);
    expect(tr.t).toEqual([0, 10]);
  });

  it("skips non-finite samples", () => {
    const tr = appendTrace(EMPTY_TRACE, [
      [1, Number.NaN],
      [Number.POSITIVE_INFINITY, 2],
      [3, 4],
    ]);
    expect(tr).toEqual({ t: [3], v: [4] });
  });

  it("stays bounded over a long stream", () => {
    let tr = EMPTY_TRACE;
    for (let s = 0; s < 1000; s++) tr = appendTrace(tr, [[s, 2]]);
    expect(tr.t.length).toBe(EDA_TRACE_WINDOW_S + 1);
    expect(tr.t.length).toBe(tr.v.length);
  });

  it("returns an empty trace when nothing valid was ever added", () => {
    expect(appendTrace(EMPTY_TRACE, [[Number.NaN, 1]])).toEqual(EMPTY_TRACE);
  });
});

describe("arousalLevel", () => {
  it("is warming up, not typical, before the baseline exists", () => {
    expect(arousalLevel(null)).toBe("warmingUp");
    expect(arousalLevel(undefined)).toBe("warmingUp");
    expect(arousalLevel(Number.NaN)).toBe("warmingUp");
  });

  it("bands the z-score", () => {
    expect(arousalLevel(-1.5)).toBe("low");
    expect(arousalLevel(0)).toBe("typical");
    expect(arousalLevel(0.99)).toBe("typical");
    expect(arousalLevel(1)).toBe("elevated");
    expect(arousalLevel(1.99)).toBe("elevated");
    expect(arousalLevel(2)).toBe("high");
    expect(arousalLevel(8)).toBe("high");
  });

  it("treats exactly -1 as typical", () => {
    expect(arousalLevel(-1)).toBe("typical");
  });
});

describe("arousalBar", () => {
  it("maps -3..+3 onto 0..100 and clamps", () => {
    expect(arousalBar(-3)).toBe(0);
    expect(arousalBar(0)).toBe(50);
    expect(arousalBar(3)).toBe(100);
    expect(arousalBar(-10)).toBe(0);
    expect(arousalBar(10)).toBe(100);
  });

  it("is empty without a baseline", () => {
    expect(arousalBar(null)).toBe(0);
  });
});

describe("rejectedPercent", () => {
  it("converts a valid fraction to a rejected percentage", () => {
    expect(rejectedPercent(1)).toBe(0);
    expect(rejectedPercent(0.75)).toBe(25);
    expect(rejectedPercent(0)).toBe(100);
  });

  it("clamps out-of-range and non-finite input", () => {
    expect(rejectedPercent(1.5)).toBe(0);
    expect(rejectedPercent(-1)).toBe(100);
    expect(rejectedPercent(Number.NaN)).toBe(0);
  });

  it("warns below the poor-contact threshold", () => {
    expect(POOR_CONTACT_FRACTION).toBeGreaterThan(0.5);
    expect(POOR_CONTACT_FRACTION).toBeLessThan(1);
  });
});
