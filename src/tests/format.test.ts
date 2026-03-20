// SPDX-License-Identifier: GPL-3.0-only
import { describe, it, expect } from "vitest";
import {
  pad, fromUnix, toUnix, fmtDuration, fmtDurationRange, fmtSecs,
  fmtDateIso, fmtDateTime, fmtDateTimeSecs, fmtCountdown,
  thresholdColor, fmtMs, fmtElapsed,
  dateToLocalKey, dateToCompactKey, unixToLocalKey, localKeyToUnix,
  fmtBytes, fmtGB, fmtDayKey,
  fmtDateTimeLocalInput, parseDateTimeLocalInput,
} from "$lib/format";

describe("pad", () => {
  it("pads single digit", () => expect(pad(5)).toBe("05"));
  it("does not pad two digits", () => expect(pad(12)).toBe("12"));
  it("pads zero", () => expect(pad(0)).toBe("00"));
  it("does not pad three digits", () => expect(pad(123)).toBe("123"));
});

describe("fromUnix / toUnix", () => {
  it("round-trips correctly", () => {
    const ts = 1700000000;
    expect(toUnix(fromUnix(ts))).toBe(ts);
  });
  it("fromUnix returns a Date", () => {
    expect(fromUnix(0)).toBeInstanceOf(Date);
  });
  it("epoch 0 is Jan 1 1970", () => {
    const d = fromUnix(0);
    expect(d.getUTCFullYear()).toBe(1970);
    expect(d.getUTCMonth()).toBe(0);
    expect(d.getUTCDate()).toBe(1);
  });
});

describe("fmtDuration", () => {
  it("0 → '0s'", () => expect(fmtDuration(0)).toBe("0s"));
  it("negative → '0s'", () => expect(fmtDuration(-5)).toBe("0s"));
  it("30s → '30s'", () => expect(fmtDuration(30)).toBe("30s"));
  it("90s → '1m 30s'", () => expect(fmtDuration(90)).toBe("1m 30s"));
  it("300s → '5m'", () => expect(fmtDuration(300)).toBe("5m"));
  it("3661s → '1h 1m'", () => expect(fmtDuration(3661)).toBe("1h 1m"));
  it("3600s → '1h'", () => expect(fmtDuration(3600)).toBe("1h"));
  it("7200s → '2h'", () => expect(fmtDuration(7200)).toBe("2h"));
});

describe("fmtDurationRange", () => {
  it("returns dash for null start", () => expect(fmtDurationRange(null, 100)).toBe("–"));
  it("returns dash for null end", () => expect(fmtDurationRange(100, null)).toBe("–"));
  it("computes duration from range", () => expect(fmtDurationRange(100, 190)).toBe("1m 30s"));
});

describe("fmtSecs", () => {
  it("30s → '30s'", () => expect(fmtSecs(30)).toBe("30s"));
  it("90s → '1m 30s'", () => expect(fmtSecs(90)).toBe("1m 30s"));
});

describe("fmtDateIso", () => {
  it("formats epoch as UTC date", () => {
    // 2023-11-14 in some timezone
    const ts = 1700000000;
    const result = fmtDateIso(ts);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("fmtDateTime", () => {
  it("formats as YYYY-MM-DD HH:MM", () => {
    const result = fmtDateTime(1700000000);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });
});

describe("fmtDateTimeSecs", () => {
  it("formats as YYYY-MM-DD HH:MM:SS", () => {
    const result = fmtDateTimeSecs(1700000000);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/);
  });
});

describe("fmtCountdown", () => {
  it("0 → '00:00'", () => expect(fmtCountdown(0)).toBe("00:00"));
  it("61 → '01:01'", () => expect(fmtCountdown(61)).toBe("01:01"));
  it("599 → '09:59'", () => expect(fmtCountdown(599)).toBe("09:59"));
  it("3600 → '60:00'", () => expect(fmtCountdown(3600)).toBe("60:00"));
});

describe("thresholdColor", () => {
  it("returns fallback for value below all thresholds", () => {
    expect(thresholdColor(10, [[60, "green"], [30, "yellow"]], "gray")).toBe("gray");
  });
  it("returns matching threshold color", () => {
    expect(thresholdColor(50, [[60, "green"], [30, "yellow"]], "gray")).toBe("yellow");
  });
  it("returns highest matching threshold", () => {
    expect(thresholdColor(80, [[60, "green"], [30, "yellow"]], "gray")).toBe("green");
  });
});

describe("fmtMs", () => {
  it("500 → '500ms'", () => expect(fmtMs(500)).toBe("500ms"));
  it("1500 → '1.5s'", () => expect(fmtMs(1500)).toBe("1.5s"));
  it("0 → '0ms'", () => expect(fmtMs(0)).toBe("0ms"));
});

describe("fmtElapsed", () => {
  it("30 → '30s'", () => expect(fmtElapsed(30)).toBe("30s"));
  it("90 → '1m 30s'", () => expect(fmtElapsed(90)).toBe("1m 30s"));
  it("60 → '1m 00s'", () => expect(fmtElapsed(60)).toBe("1m 00s"));
});

describe("dateToLocalKey / dateToCompactKey", () => {
  it("formats Date as YYYY-MM-DD", () => {
    const d = new Date(2026, 2, 15); // March 15, 2026
    expect(dateToLocalKey(d)).toBe("2026-03-15");
  });
  it("formats Date as YYYYMMDD", () => {
    const d = new Date(2026, 2, 15);
    expect(dateToCompactKey(d)).toBe("20260315");
  });
  it("pads single-digit months and days", () => {
    const d = new Date(2026, 0, 5); // Jan 5
    expect(dateToLocalKey(d)).toBe("2026-01-05");
    expect(dateToCompactKey(d)).toBe("20260105");
  });
});

describe("unixToLocalKey", () => {
  it("converts unix timestamp to local date key", () => {
    const result = unixToLocalKey(1700000000);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("localKeyToUnix", () => {
  it("converts local key to unix seconds", () => {
    const key = "2026-03-15";
    const unix = localKeyToUnix(key);
    const d = new Date(unix * 1000);
    expect(d.getFullYear()).toBe(2026);
    expect(d.getMonth()).toBe(2); // March
    expect(d.getDate()).toBe(15);
  });
  it("round-trips with dateToLocalKey", () => {
    const d = new Date(2026, 5, 20); // June 20
    const key = dateToLocalKey(d);
    const unix = localKeyToUnix(key);
    const back = new Date(unix * 1000);
    expect(back.getFullYear()).toBe(2026);
    expect(back.getMonth()).toBe(5);
    expect(back.getDate()).toBe(20);
  });
});

describe("fmtDateTimeLocalInput / parseDateTimeLocalInput", () => {
  it("formats for datetime-local input", () => {
    const result = fmtDateTimeLocalInput(1700000000);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}$/);
  });
  it("round-trips", () => {
    const ts = 1700000000;
    const formatted = fmtDateTimeLocalInput(ts);
    const parsed = parseDateTimeLocalInput(formatted);
    expect(parsed).toBe(ts);
  });
});

describe("fmtBytes", () => {
  it("formats bytes", () => expect(fmtBytes(500)).toBe("500 B"));
  it("formats kilobytes", () => expect(fmtBytes(1500)).toBe("1.5 KB"));
  it("formats megabytes", () => expect(fmtBytes(1_500_000)).toBe("1.5 MB"));
  it("formats gigabytes", () => expect(fmtBytes(1_500_000_000)).toBe("1.5 GB"));
});

describe("fmtGB", () => {
  it("formats GB", () => expect(fmtGB(3.2)).toBe("3.2 GB"));
  it("formats sub-GB as MB", () => expect(fmtGB(0.5)).toBe("512 MB"));
});

describe("fmtDayKey", () => {
  it("formats YYYY-MM-DD as human date", () => {
    const result = fmtDayKey("2026-03-15");
    // Locale-dependent, but should contain "2026" and "15"
    expect(result).toContain("2026");
    expect(result).toContain("15");
  });
  it("returns empty for empty string", () => expect(fmtDayKey("")).toBe(""));
});
