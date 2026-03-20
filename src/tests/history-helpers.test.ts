// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import {
  fmtDurCompact, totalDurationSecs, labelsForDay,
  assignLabelRainbowColors, labelRelations,
  dateKey, fmtDuration, fmtSamples, dayPct,
  secToUtcDir, localDayBounds, buildLocalDays,
  GRID_COLS, GRID_ROWS, GRID_BIN,
  LABEL_PROXIMITY_SECS,
  type SessionEntry,
} from "$lib/history-helpers";
import type { LabelRow } from "$lib/types";

// ── Helper factory ────────────────────────────────────────────────────────────

function session(overrides: Partial<SessionEntry> = {}): SessionEntry {
  return {
    csv_file: "test.csv", csv_path: "/data/test.csv",
    session_start_utc: 1700000000, session_end_utc: 1700003600,
    device_name: "Muse S", serial_number: null,
    battery_pct: 80, total_samples: 256000,
    sample_rate_hz: 256, labels: [],
    file_size_bytes: 1024,
    ...overrides,
  };
}

function label(id: number, eeg_start: number, text = "test"): LabelRow {
  return {
    id, text, eeg_start, eeg_end: eeg_start + 5,
    wall_start: eeg_start, wall_end: eeg_start + 5,
    context: "", created_at: eeg_start,
  } as LabelRow;
}

// ── Constants ─────────────────────────────────────────────────────────────────

describe("grid constants", () => {
  it("GRID_COLS is 24 (hours)", () => expect(GRID_COLS).toBe(24));
  it("GRID_ROWS is 720 (5-sec bins per hour)", () => expect(GRID_ROWS).toBe(720));
  it("GRID_BIN is 5 (seconds per row)", () => expect(GRID_BIN).toBe(5));
  it("GRID_ROWS * GRID_BIN = 3600 (one hour)", () => expect(GRID_ROWS * GRID_BIN).toBe(3600));
  it("LABEL_PROXIMITY_SECS is 15", () => expect(LABEL_PROXIMITY_SECS).toBe(15));
});

// ── fmtDurCompact ─────────────────────────────────────────────────────────────

describe("fmtDurCompact", () => {
  it("0 → empty", () => expect(fmtDurCompact(0)).toBe(""));
  it("negative → empty", () => expect(fmtDurCompact(-10)).toBe(""));
  it("minutes only", () => expect(fmtDurCompact(300)).toBe("5m"));
  it("hours and minutes", () => expect(fmtDurCompact(5400)).toBe("1h 30m"));
  it("hours only", () => expect(fmtDurCompact(7200)).toBe("2h"));
  it("less than a minute → '0m'", () => expect(fmtDurCompact(30)).toBe("0m"));
});

// ── totalDurationSecs ─────────────────────────────────────────────────────────

describe("totalDurationSecs", () => {
  it("empty list → 0", () => expect(totalDurationSecs([])).toBe(0));
  it("one session", () => {
    expect(totalDurationSecs([session()])).toBe(3600);
  });
  it("multiple sessions", () => {
    const s1 = session({ session_start_utc: 1000, session_end_utc: 2000 });
    const s2 = session({ session_start_utc: 3000, session_end_utc: 4500 });
    expect(totalDurationSecs([s1, s2])).toBe(2500);
  });
  it("ignores sessions with null times", () => {
    const s1 = session({ session_start_utc: null });
    expect(totalDurationSecs([s1])).toBe(0);
  });
});

// ── labelsForDay ──────────────────────────────────────────────────────────────

describe("labelsForDay", () => {
  it("empty sessions → empty labels", () => {
    expect(labelsForDay("2026-01-01", [])).toEqual([]);
  });
  it("aggregates labels from multiple sessions", () => {
    const l1 = label(1, 100);
    const l2 = label(2, 200);
    const s1 = session({ labels: [l1] });
    const s2 = session({ labels: [l2] });
    const result = labelsForDay("2026-01-01", [s1, s2]);
    expect(result).toHaveLength(2);
    expect(result.map(l => l.id)).toEqual([1, 2]);
  });
});

// ── assignLabelRainbowColors ──────────────────────────────────────────────────

describe("assignLabelRainbowColors", () => {
  it("empty → empty map", () => {
    expect(assignLabelRainbowColors([]).size).toBe(0);
  });
  it("assigns unique HSL colors to each label", () => {
    const labels = [label(1, 100), label(2, 200), label(3, 300)];
    const colors = assignLabelRainbowColors(labels);
    expect(colors.size).toBe(3);
    const vals = [...colors.values()];
    // All should be HSL strings
    vals.forEach(c => expect(c).toMatch(/^hsl\(/));
    // All should be different
    expect(new Set(vals).size).toBe(3);
  });
  it("sorts by eeg_start before assigning hues", () => {
    const labels = [label(1, 300), label(2, 100), label(3, 200)];
    const colors = assignLabelRainbowColors(labels);
    // label 2 (earliest) gets hue 0
    expect(colors.get(2)).toMatch(/hsl\(0,/);
  });
});

// ── labelRelations ────────────────────────────────────────────────────────────

describe("labelRelations", () => {
  it("finds exact text matches", () => {
    const hovered = label(1, 100, "alpha");
    const all = [label(1, 100, "alpha"), label(2, 200, "alpha"), label(3, 300, "beta")];
    const { exactIds, closeIds } = labelRelations(hovered, all);
    expect(exactIds.has(2)).toBe(true);
    expect(exactIds.has(3)).toBe(false);
    expect(closeIds.size).toBe(0); // 200 is not within 15s of 100
  });
  it("finds temporally close labels", () => {
    const hovered = label(1, 100, "alpha");
    const all = [label(1, 100, "alpha"), label(2, 110, "beta")]; // 10s apart
    const { closeIds } = labelRelations(hovered, all);
    expect(closeIds.has(2)).toBe(true);
  });
  it("case-insensitive text matching", () => {
    const hovered = label(1, 100, "Alpha");
    const all = [label(1, 100, "Alpha"), label(2, 200, "alpha")];
    const { exactIds } = labelRelations(hovered, all);
    expect(exactIds.has(2)).toBe(true);
  });
});

// ── fmtDuration ───────────────────────────────────────────────────────────────

describe("fmtDuration (history)", () => {
  it("null start → dash", () => expect(fmtDuration(null, 100)).toBe("—"));
  it("null end → dash", () => expect(fmtDuration(100, null)).toBe("—"));
  it("end <= start → dash", () => expect(fmtDuration(100, 100)).toBe("—"));
  it("0 start is treated as missing → dash", () => expect(fmtDuration(0, 3600)).toBe("—"));
  it("1 hour", () => expect(fmtDuration(1000, 4600)).toBe("1h 0m 0s"));
  it("1 minute 30 seconds", () => expect(fmtDuration(1000, 1090)).toBe("1m 30s"));
  it("45 seconds", () => expect(fmtDuration(1000, 1045)).toBe("45s"));
});

// ── fmtSamples ────────────────────────────────────────────────────────────────

describe("fmtSamples", () => {
  it("null → dash", () => expect(fmtSamples(null)).toBe("—"));
  it("0 → dash", () => expect(fmtSamples(0)).toBe("—"));
  it("500 → '500'", () => expect(fmtSamples(500)).toBe("500"));
  it("1500 → '1.5K'", () => expect(fmtSamples(1500)).toBe("1.5K"));
  it("2500000 → '2.5M'", () => expect(fmtSamples(2500000)).toBe("2.5M"));
});

// ── dayPct ────────────────────────────────────────────────────────────────────

describe("dayPct", () => {
  it("start of day → 0%", () => expect(dayPct(1000, 1000)).toBe(0));
  it("end of day → 100%", () => expect(dayPct(1000 + 86400, 1000)).toBe(100));
  it("midday → ~50%", () => {
    const pct = dayPct(1000 + 43200, 1000);
    expect(pct).toBeCloseTo(50, 0);
  });
  it("clamps below 0", () => expect(dayPct(900, 1000)).toBe(0));
  it("clamps above 100", () => expect(dayPct(1000 + 90000, 1000)).toBe(100));
});

// ── secToUtcDir ───────────────────────────────────────────────────────────────

describe("secToUtcDir", () => {
  it("epoch → '19700101'", () => expect(secToUtcDir(0)).toBe("19700101"));
  it("known date", () => {
    // 2023-11-14 22:13:20 UTC = 1700000000
    expect(secToUtcDir(1700000000)).toBe("20231114");
  });
});

// ── localDayBounds ────────────────────────────────────────────────────────────

describe("localDayBounds", () => {
  it("returns startSec < endSec", () => {
    const { startSec, endSec } = localDayBounds("2026-03-15");
    expect(endSec - startSec).toBe(86400);
  });
  it("start is midnight local", () => {
    const { startSec } = localDayBounds("2026-03-15");
    const d = new Date(startSec * 1000);
    expect(d.getHours()).toBe(0);
    expect(d.getMinutes()).toBe(0);
  });
});

// ── buildLocalDays ────────────────────────────────────────────────────────────

describe("buildLocalDays", () => {
  it("empty input → empty output", () => {
    expect(buildLocalDays([], new Map())).toEqual([]);
  });
  it("converts UTC dirs to local day keys (sorted newest first)", () => {
    const dirs = ["20260315", "20260316"];
    const result = buildLocalDays(dirs, new Map());
    expect(result.length).toBeGreaterThanOrEqual(2);
    // Should be sorted descending
    for (let i = 1; i < result.length; i++) {
      expect(result[i - 1] >= result[i]).toBe(true);
    }
  });
  it("uses session_start_utc when available", () => {
    const s = session({ session_start_utc: 1700000000 });
    const store = new Map<string, SessionEntry[]>();
    store.set("2023-11-14", [s]);
    const result = buildLocalDays([], store);
    expect(result.length).toBeGreaterThanOrEqual(1);
  });
});

// ── dateKey ───────────────────────────────────────────────────────────────────

describe("dateKey", () => {
  it("formats unix timestamp as YYYY-MM-DD", () => {
    const result = dateKey(1700000000);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});
