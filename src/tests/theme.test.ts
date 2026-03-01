// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Tests for src/lib/theme.ts
 *
 * Covers every exported pure function and constant:
 *   colorForLevel  — "higher is better" (battery %, signal quality)
 *   colorForLoad   — "lower is better" (CPU %, GPU %)
 *   colorForRssi   — RSSI dBm → colour
 *   QUALITY_COLORS — signal-quality label map
 *   STATE_COLORS   — BLE connection state map
 */
import { describe, it, expect } from "vitest";
import {
  C_GOOD, C_WARN, C_BAD, C_NEUTRAL,
  colorForLevel, colorForLoad, colorForRssi,
  QUALITY_COLORS, STATE_COLORS,
} from "../../src/lib/theme";

// ── Base palette sanity ───────────────────────────────────────────────────────

describe("base palette constants", () => {
  it("C_GOOD is a valid hex colour", () => {
    expect(C_GOOD).toMatch(/^#[0-9a-fA-F]{6}$/);
  });

  it("C_WARN is a valid hex colour", () => {
    expect(C_WARN).toMatch(/^#[0-9a-fA-F]{6}$/);
  });

  it("C_BAD is a valid hex colour", () => {
    expect(C_BAD).toMatch(/^#[0-9a-fA-F]{6}$/);
  });

  it("C_NEUTRAL is a valid hex colour", () => {
    expect(C_NEUTRAL).toMatch(/^#[0-9a-fA-F]{6}$/);
  });

  it("all base colours are distinct", () => {
    const palette = [C_GOOD, C_WARN, C_BAD, C_NEUTRAL];
    const unique = new Set(palette);
    expect(unique.size).toBe(palette.length);
  });
});

// ── colorForLevel ─────────────────────────────────────────────────────────────

describe("colorForLevel (higher is better, default thresholds 30/60)", () => {
  it("100 % → C_GOOD (fully charged)", () => {
    expect(colorForLevel(100)).toBe(C_GOOD);
  });

  it("60 % → C_GOOD (at goodAt boundary)", () => {
    expect(colorForLevel(60)).toBe(C_GOOD);
  });

  it("61 % → C_GOOD (above goodAt)", () => {
    expect(colorForLevel(61)).toBe(C_GOOD);
  });

  it("59 % → C_WARN (just below goodAt)", () => {
    expect(colorForLevel(59)).toBe(C_WARN);
  });

  it("30 % → C_WARN (at warnAt boundary)", () => {
    expect(colorForLevel(30)).toBe(C_WARN);
  });

  it("29 % → C_BAD (just below warnAt)", () => {
    expect(colorForLevel(29)).toBe(C_BAD);
  });

  it("0 % → C_BAD (empty)", () => {
    expect(colorForLevel(0)).toBe(C_BAD);
  });

  it("custom thresholds: pct=50, warnAt=40, goodAt=70 → C_WARN", () => {
    expect(colorForLevel(50, 40, 70)).toBe(C_WARN);
  });

  it("custom thresholds: pct=80, warnAt=40, goodAt=70 → C_GOOD", () => {
    expect(colorForLevel(80, 40, 70)).toBe(C_GOOD);
  });

  it("custom thresholds: pct=10, warnAt=40, goodAt=70 → C_BAD", () => {
    expect(colorForLevel(10, 40, 70)).toBe(C_BAD);
  });
});

// ── colorForLoad ──────────────────────────────────────────────────────────────

describe("colorForLoad (lower is better, default thresholds 0.50/0.80)", () => {
  it("0.0 → C_GOOD (idle)", () => {
    expect(colorForLoad(0.0)).toBe(C_GOOD);
  });

  it("0.49 → C_GOOD (below warnAt)", () => {
    expect(colorForLoad(0.49)).toBe(C_GOOD);
  });

  it("0.50 → C_WARN (at warnAt boundary)", () => {
    expect(colorForLoad(0.50)).toBe(C_WARN);
  });

  it("0.79 → C_WARN (below badAt)", () => {
    expect(colorForLoad(0.79)).toBe(C_WARN);
  });

  it("0.80 → C_BAD (at badAt boundary)", () => {
    expect(colorForLoad(0.80)).toBe(C_BAD);
  });

  it("1.0 → C_BAD (fully loaded)", () => {
    expect(colorForLoad(1.0)).toBe(C_BAD);
  });

  it("custom thresholds: ratio=0.6, warnAt=0.7, badAt=0.9 → C_GOOD", () => {
    expect(colorForLoad(0.6, 0.7, 0.9)).toBe(C_GOOD);
  });

  it("custom thresholds: ratio=0.8, warnAt=0.7, badAt=0.9 → C_WARN", () => {
    expect(colorForLoad(0.8, 0.7, 0.9)).toBe(C_WARN);
  });

  it("custom thresholds: ratio=0.95, warnAt=0.7, badAt=0.9 → C_BAD", () => {
    expect(colorForLoad(0.95, 0.7, 0.9)).toBe(C_BAD);
  });
});

// ── colorForRssi ──────────────────────────────────────────────────────────────

describe("colorForRssi", () => {
  it("0 dBm → C_NEUTRAL (absent / not connected)", () => {
    expect(colorForRssi(0)).toBe(C_NEUTRAL);
  });

  it("-30 dBm → C_GOOD (excellent signal)", () => {
    expect(colorForRssi(-30)).toBe(C_GOOD);
  });

  it("-64 dBm → C_GOOD (still above -65 boundary)", () => {
    expect(colorForRssi(-64)).toBe(C_GOOD);
  });

  it("-65 dBm → C_WARN (at -65 boundary, not strictly greater)", () => {
    // dBm > -65 is false at exactly -65, so it falls to the next tier
    expect(colorForRssi(-65)).toBe(C_WARN);
  });

  it("-70 dBm → C_WARN (fair signal)", () => {
    expect(colorForRssi(-70)).toBe(C_WARN);
  });

  it("-80 dBm → C_WARN (at -80 boundary, not strictly greater)", () => {
    expect(colorForRssi(-80)).toBe(C_BAD);
  });

  it("-90 dBm → C_BAD (very weak)", () => {
    expect(colorForRssi(-90)).toBe(C_BAD);
  });

  it("-100 dBm → C_BAD (at noise floor)", () => {
    expect(colorForRssi(-100)).toBe(C_BAD);
  });
});

// ── QUALITY_COLORS ────────────────────────────────────────────────────────────

describe("QUALITY_COLORS", () => {
  it("contains all expected signal-quality labels", () => {
    const expected = ["good", "fair", "poor", "no_signal"];
    for (const key of expected) {
      expect(QUALITY_COLORS).toHaveProperty(key);
    }
  });

  it("'good' maps to C_GOOD", () => {
    expect(QUALITY_COLORS["good"]).toBe(C_GOOD);
  });

  it("'fair' maps to C_WARN", () => {
    expect(QUALITY_COLORS["fair"]).toBe(C_WARN);
  });

  it("'poor' maps to C_BAD", () => {
    expect(QUALITY_COLORS["poor"]).toBe(C_BAD);
  });

  it("'no_signal' maps to C_NEUTRAL", () => {
    expect(QUALITY_COLORS["no_signal"]).toBe(C_NEUTRAL);
  });

  it("all values are valid hex or rgba strings", () => {
    for (const val of Object.values(QUALITY_COLORS)) {
      const isHex  = /^#[0-9a-fA-F]{6}$/.test(val);
      const isRgba = /^rgba\(/.test(val);
      expect(isHex || isRgba, `unexpected colour value: ${val}`).toBe(true);
    }
  });
});

// ── STATE_COLORS ──────────────────────────────────────────────────────────────

describe("STATE_COLORS", () => {
  const expectedStates = ["connected", "scanning", "bt_off", "disconnected"] as const;

  it("contains all BLE connection states", () => {
    for (const state of expectedStates) {
      expect(STATE_COLORS).toHaveProperty(state);
    }
  });

  it("each state has ring, badge, text, border sub-keys", () => {
    for (const state of expectedStates) {
      const entry = STATE_COLORS[state];
      expect(entry).toHaveProperty("ring");
      expect(entry).toHaveProperty("badge");
      expect(entry).toHaveProperty("text");
      expect(entry).toHaveProperty("border");
    }
  });

  it("'connected' ring is C_GOOD", () => {
    expect(STATE_COLORS.connected.ring).toBe(C_GOOD);
  });

  it("'bt_off' ring is C_BAD", () => {
    expect(STATE_COLORS.bt_off.ring).toBe(C_BAD);
  });

  it("'scanning' ring is C_WARN", () => {
    expect(STATE_COLORS.scanning.ring).toBe(C_WARN);
  });

  it("'disconnected' ring is C_NEUTRAL", () => {
    expect(STATE_COLORS.disconnected.ring).toBe(C_NEUTRAL);
  });

  it("ring colours are all valid hex strings", () => {
    for (const state of expectedStates) {
      expect(STATE_COLORS[state].ring).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });
});
