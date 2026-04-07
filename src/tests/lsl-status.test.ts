// SPDX-License-Identifier: GPL-3.0-only
/**
 * Unit tests for LSL status handling logic:
 * - Status merge (refreshStatus behavior)
 * - EEG buffer growth
 * - State transitions
 * - Signal quality summary computation
 */
import { describe, expect, it } from "vitest";

// ── Status merge logic ─────────────────────────────────────────────────────

describe("status merge", () => {
  /** Simulates the dashboard refreshStatus merge: { ...existing, ...daemon } */
  function mergeStatus(existing: Record<string, unknown>, daemon: Record<string, unknown>): Record<string, unknown> {
    return { ...existing, ...daemon };
  }

  it("daemon connected state overrides local disconnected", () => {
    const local = { state: "disconnected", eeg: [NaN, NaN, NaN, NaN], filter_config: { sample_rate: 256 } };
    const daemon = { state: "connected", device_name: "TestLSL", device_kind: "lsl" };
    const merged = mergeStatus(local, daemon);

    expect(merged.state).toBe("connected");
    expect(merged.device_name).toBe("TestLSL");
    expect(merged.device_kind).toBe("lsl");
  });

  it("preserves local fields not in daemon response", () => {
    const local = {
      state: "disconnected",
      eeg: [1.0, 2.0, 3.0, 4.0],
      filter_config: { sample_rate: 256, notch: 50 },
      accel: [0.1, 0.2, 0.3],
    };
    const daemon = { state: "connected", device_name: "TestLSL" };
    const merged = mergeStatus(local, daemon);

    // Local-only fields preserved
    expect(merged.eeg).toEqual([1.0, 2.0, 3.0, 4.0]);
    expect(merged.filter_config).toEqual({ sample_rate: 256, notch: 50 });
    expect(merged.accel).toEqual([0.1, 0.2, 0.3]);
  });

  it("daemon fields override local fields", () => {
    const local = { state: "connected", sample_count: 100, battery: 50 };
    const daemon = { sample_count: 200, battery: 75 };
    const merged = mergeStatus(local, daemon);

    expect(merged.sample_count).toBe(200);
    expect(merged.battery).toBe(75);
    expect(merged.state).toBe("connected"); // not in daemon, stays
  });

  it("handles empty daemon response gracefully", () => {
    const local = { state: "connected", device_name: "Test" };
    const merged = mergeStatus(local, {});

    expect(merged.state).toBe("connected");
    expect(merged.device_name).toBe("Test");
  });
});

// ── EEG buffer growth ──────────────────────────────────────────────────────

describe("EEG buffer growth", () => {
  it("grows Float64Array when higher electrode index arrives", () => {
    let eegLatest = new Float64Array(0);

    // Simulate electrode 0
    if (0 >= eegLatest.length) {
      const next = new Float64Array(1);
      next.set(eegLatest);
      eegLatest = next;
    }
    eegLatest[0] = 42.5;

    expect(eegLatest.length).toBe(1);
    expect(eegLatest[0]).toBe(42.5);

    // Simulate electrode 31 (32-channel device)
    if (31 >= eegLatest.length) {
      const next = new Float64Array(32);
      next.set(eegLatest);
      eegLatest = next;
    }
    eegLatest[31] = -10.3;

    expect(eegLatest.length).toBe(32);
    expect(eegLatest[0]).toBe(42.5); // preserved
    expect(eegLatest[31]).toBe(-10.3);
  });

  it("handles 1024-channel device", () => {
    let eegLatest = new Float64Array(0);

    if (1023 >= eegLatest.length) {
      const next = new Float64Array(1024);
      next.set(eegLatest);
      eegLatest = next;
    }
    eegLatest[1023] = 99.9;

    expect(eegLatest.length).toBe(1024);
    expect(eegLatest[1023]).toBe(99.9);
    expect(eegLatest[0]).toBe(0); // zero-initialized
  });
});

// ── Signal quality summary ─────────────────────────────────────────────────

describe("signal quality summary", () => {
  function qualitySummary(qualities: string[]) {
    const good = qualities.filter((q) => q === "good").length;
    const fair = qualities.filter((q) => q === "fair").length;
    const poor = qualities.filter((q) => q === "poor").length;
    const none = qualities.length - good - fair - poor;
    return { good, fair, poor, none };
  }

  it("all good for 32 channels", () => {
    const q = qualitySummary(Array(32).fill("good"));
    expect(q).toEqual({ good: 32, fair: 0, poor: 0, none: 0 });
  });

  it("mixed quality", () => {
    const qualities = ["good", "good", "fair", "poor", "no_signal", "good", "fair", "poor"];
    const q = qualitySummary(qualities);
    expect(q).toEqual({ good: 3, fair: 2, poor: 2, none: 1 });
  });

  it("empty quality array", () => {
    const q = qualitySummary([]);
    expect(q).toEqual({ good: 0, fair: 0, poor: 0, none: 0 });
  });
});

// ── State transition guards ────────────────────────────────────────────────

describe("state transitions", () => {
  const VALID_STATES = ["disconnected", "connecting", "scanning", "connected", "bt_off"];

  it("all valid states are recognized", () => {
    const STATE_COLORS: Record<string, unknown> = {
      connected: { ring: "#22c55e" },
      connecting: { ring: "#eab308" },
      scanning: { ring: "#eab308" },
      bt_off: { ring: "#ef4444" },
      disconnected: { ring: "#6b7280" },
    };

    for (const state of VALID_STATES) {
      expect(STATE_COLORS[state]).toBeDefined();
    }
  });

  it("connecting is treated as scanning in main content", () => {
    // The dashboard shows the scanning UI for both "scanning" and "connecting"
    const showScanningUi = (state: string) => state === "scanning" || state === "connecting";

    expect(showScanningUi("connecting")).toBe(true);
    expect(showScanningUi("scanning")).toBe(true);
    expect(showScanningUi("connected")).toBe(false);
    expect(showScanningUi("disconnected")).toBe(false);
  });

  it("device kind detection for source label", () => {
    const sourceLabel = (kind: string): string | null => {
      switch (kind) {
        case "lsl":
          return "LSL";
        case "lsl-iroh":
          return "LSL · iroh";
        case "ganglion":
          return "USB";
        case "emotiv":
          return "Cortex";
        case "muse":
        case "mw75":
        case "hermes":
          return "BLE";
        default:
          return null;
      }
    };

    expect(sourceLabel("lsl")).toBe("LSL");
    expect(sourceLabel("muse")).toBe("BLE");
    expect(sourceLabel("emotiv")).toBe("Cortex");
    expect(sourceLabel("unknown")).toBeNull();
  });
});

// ── fmtEeg helper ──────────────────────────────────────────────────────────

describe("fmtEeg", () => {
  const fmtEeg = (v: number | null | undefined) =>
    v != null && Number.isFinite(v) ? `${(v >= 0 ? "+" : "") + v.toFixed(1)} µV` : "—";

  it("formats positive values", () => {
    expect(fmtEeg(42.5)).toBe("+42.5 µV");
  });

  it("formats negative values", () => {
    expect(fmtEeg(-10.3)).toBe("-10.3 µV");
  });

  it("formats zero", () => {
    expect(fmtEeg(0)).toBe("+0.0 µV");
  });

  it("returns dash for NaN", () => {
    expect(fmtEeg(NaN)).toBe("—");
  });

  it("returns dash for null", () => {
    expect(fmtEeg(null)).toBe("—");
  });

  it("returns dash for undefined", () => {
    expect(fmtEeg(undefined)).toBe("—");
  });

  it("returns dash for Infinity", () => {
    expect(fmtEeg(Infinity)).toBe("—");
  });
});
