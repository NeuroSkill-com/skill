// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Tests for src/lib/constants.ts
 *
 * Verifies that:
 *  1. All derived constants equal the formula used to compute them.
 *  2. Array-valued constants have the correct length and valid elements.
 *  3. TypeScript-side values agree with the Rust mirrors documented in
 *     constants.rs (hard-coded expected values guard against accidental
 *     divergence).
 *
 * **Sync guard:** If you change a shared constant in
 * `crates/skill-constants/src/lib.rs`, update `src/lib/constants.ts` AND
 * update the hard-coded expectations below.  Running `npm test` will catch
 * any drift between the two sides.
 */
import { describe, expect, it } from "vitest";
import {
  BAND_CANVAS_H,
  BAND_TILE_GAP,
  // BandChart layout
  BAND_TILE_H,
  BANDS,
  BUF_SIZE,
  // Calibration
  CALIBRATION_ACTION_DURATION_SECS,
  CALIBRATION_ACTION1_LABEL,
  CALIBRATION_ACTION2_LABEL,
  CALIBRATION_AUTO_START,
  CALIBRATION_BREAK_DURATION_SECS,
  CALIBRATION_LOOP_COUNT,
  // Canvas layout
  CHART_H,
  // Waveform rendering
  DC_BETA,
  // Filter defaults
  DEFAULT_FILTER_CONFIG,
  EEG_CH,
  // Hardware
  EEG_CHANNELS,
  EEG_COLOR,
  EEG_RANGE_UV,
  // Embedding
  EMBEDDING_EPOCH_SECS,
  EMBEDDING_OVERLAP_MAX_SECS,
  EMBEDDING_OVERLAP_MIN_SECS,
  EMBEDDING_OVERLAP_SECS,
  EPOCH_S,
  EPOCH_SAMP,
  // Signal processing
  FILTER_HOP,
  JOB_POLL_INTERVAL_MS,
  // Waveform ring buffer
  N_EPOCHS,
  // Band definitions
  NUM_BANDS,
  ROW_PAD,
  SAMPLE_RATE,
  // Search
  SEARCH_PAGE_SIZE,
  SMOOTH_K,
  SPEC_CMAP_STOPS,
  // Colormaps
  SPEC_CMAP_STOPS_DARK,
  SPEC_CMAP_STOPS_LIGHT,
  SPEC_COLS,
  SPEC_LOG_DECAY,
  SPEC_LOG_FLOOR,
  // Spectrogram normalisation
  SPEC_LOG_INIT,
  SPEC_LOG_RANGE,
  SPEC_N_FREQ,
  TIME_H,
  UMAP_POINT_SIZE,
  UMAP_SCALE_MAX,
  // UMAP
  UMAP_SCALE_MIN,
  WAVE_H,
  WP_TAU_MS,
} from "../../src/lib/constants";

// ── Hardware / signal ─────────────────────────────────────────────────────────

describe("hardware constants", () => {
  it("EEG_CHANNELS is 32 (mirrors Rust EEG_CHANNELS)", () => {
    expect(EEG_CHANNELS).toBe(32);
  });

  it("EEG_CH has one label per Muse channel", () => {
    expect(EEG_CH).toHaveLength(4);
  });

  it("EEG_CH labels match Rust CHANNEL_NAMES order", () => {
    expect(EEG_CH[0]).toBe("TP9");
    expect(EEG_CH[1]).toBe("AF7");
    expect(EEG_CH[2]).toBe("AF8");
    expect(EEG_CH[3]).toBe("TP10");
  });

  it("EEG_COLOR has one colour per Muse channel", () => {
    expect(EEG_COLOR).toHaveLength(4);
  });

  it("EEG_COLOR entries are valid hex strings", () => {
    for (const c of EEG_COLOR) {
      expect(c).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  it("SAMPLE_RATE is 256 Hz (mirrors Rust MUSE_SAMPLE_RATE)", () => {
    expect(SAMPLE_RATE).toBe(256);
  });

  it("EEG_RANGE_UV is 1000 µV", () => {
    expect(EEG_RANGE_UV).toBe(1000);
  });
});

// ── Canvas layout (EegChart) ──────────────────────────────────────────────────

describe("EegChart canvas layout", () => {
  it("WAVE_H = CHART_H − TIME_H", () => {
    expect(WAVE_H).toBe(CHART_H - TIME_H);
  });

  it("CHART_H is 172 px", () => {
    expect(CHART_H).toBe(172);
  });

  it("TIME_H is 18 px", () => {
    expect(TIME_H).toBe(18);
  });

  it("ROW_PAD is a positive number", () => {
    expect(ROW_PAD).toBeGreaterThan(0);
  });
});

// ── Waveform ring buffer ──────────────────────────────────────────────────────

describe("waveform ring buffer", () => {
  it("EPOCH_SAMP = EPOCH_S × SAMPLE_RATE", () => {
    expect(EPOCH_SAMP).toBe(EPOCH_S * SAMPLE_RATE);
  });

  it("EPOCH_SAMP is 1280 (5 s × 256 Hz)", () => {
    expect(EPOCH_SAMP).toBe(1280);
  });

  it("BUF_SIZE = N_EPOCHS × EPOCH_SAMP", () => {
    expect(BUF_SIZE).toBe(N_EPOCHS * EPOCH_SAMP);
  });

  it("BUF_SIZE is 3840 (3 epochs × 1280 samples)", () => {
    expect(BUF_SIZE).toBe(3840);
  });
});

// ── Signal processing ─────────────────────────────────────────────────────────

describe("signal processing constants", () => {
  it("FILTER_HOP is 32 (mirrors Rust FILTER_HOP)", () => {
    expect(FILTER_HOP).toBe(32);
  });

  it("SPEC_N_FREQ is 51 (0–50 Hz inclusive, mirrors Rust SPEC_N_FREQ)", () => {
    expect(SPEC_N_FREQ).toBe(51);
  });

  it("SPEC_COLS = BUF_SIZE / FILTER_HOP", () => {
    expect(SPEC_COLS).toBe(BUF_SIZE / FILTER_HOP);
  });

  it("SPEC_COLS is 120 (15 s × 8 columns/s)", () => {
    expect(SPEC_COLS).toBe(120);
  });
});

// ── Spectrogram normalisation ─────────────────────────────────────────────────

describe("spectrogram normalisation constants", () => {
  it("SPEC_LOG_INIT is negative (soft-max seed below real signals)", () => {
    expect(SPEC_LOG_INIT).toBeLessThan(0);
  });

  it("SPEC_LOG_DECAY is strictly between 0 and 1", () => {
    expect(SPEC_LOG_DECAY).toBeGreaterThan(0);
    expect(SPEC_LOG_DECAY).toBeLessThan(1);
  });

  it("SPEC_LOG_RANGE is 3.0 (= 60 dB dynamic range)", () => {
    expect(SPEC_LOG_RANGE).toBe(3.0);
  });

  it("SPEC_LOG_FLOOR is below SPEC_LOG_INIT", () => {
    expect(SPEC_LOG_FLOOR).toBeLessThan(SPEC_LOG_INIT);
  });
});

// ── Colormaps ─────────────────────────────────────────────────────────────────

describe("spectrogram colormap stops", () => {
  it("dark colormap has at least 2 stops", () => {
    expect(SPEC_CMAP_STOPS_DARK.length).toBeGreaterThanOrEqual(2);
  });

  it("every dark stop is a 5-element tuple", () => {
    for (const stop of SPEC_CMAP_STOPS_DARK) {
      expect(stop).toHaveLength(5);
    }
  });

  it("dark colormap normalised positions span [0, 1]", () => {
    const positions = SPEC_CMAP_STOPS_DARK.map((s) => s[0]);
    expect(Math.min(...positions)).toBe(0);
    expect(Math.max(...positions)).toBe(1);
  });

  it("dark colormap positions are non-decreasing", () => {
    const positions = SPEC_CMAP_STOPS_DARK.map((s) => s[0]);
    for (let i = 1; i < positions.length; i++) {
      expect(positions[i]).toBeGreaterThanOrEqual(positions[i - 1]);
    }
  });

  it("dark colormap RGB channels are in [0, 255]", () => {
    for (const [, r, g, b] of SPEC_CMAP_STOPS_DARK) {
      expect(r).toBeGreaterThanOrEqual(0);
      expect(r).toBeLessThanOrEqual(255);
      expect(g).toBeGreaterThanOrEqual(0);
      expect(g).toBeLessThanOrEqual(255);
      expect(b).toBeGreaterThanOrEqual(0);
      expect(b).toBeLessThanOrEqual(255);
    }
  });

  it("dark colormap alpha channels are in [0, 255]", () => {
    for (const [, , , , a] of SPEC_CMAP_STOPS_DARK) {
      expect(a).toBeGreaterThanOrEqual(0);
      expect(a).toBeLessThanOrEqual(255);
    }
  });

  it("light colormap has at least 2 stops", () => {
    expect(SPEC_CMAP_STOPS_LIGHT.length).toBeGreaterThanOrEqual(2);
  });

  it("light colormap positions span [0, 1]", () => {
    const positions = SPEC_CMAP_STOPS_LIGHT.map((s) => s[0]);
    expect(Math.min(...positions)).toBe(0);
    expect(Math.max(...positions)).toBe(1);
  });

  it("SPEC_CMAP_STOPS is the dark alias", () => {
    expect(SPEC_CMAP_STOPS).toBe(SPEC_CMAP_STOPS_DARK);
  });
});

// ── Waveform rendering ────────────────────────────────────────────────────────

describe("waveform rendering constants", () => {
  it("SMOOTH_K is odd (required for centred moving average)", () => {
    expect(SMOOTH_K % 2).toBe(1);
  });

  it("SMOOTH_K is a positive integer", () => {
    expect(SMOOTH_K).toBeGreaterThan(0);
    expect(Number.isInteger(SMOOTH_K)).toBe(true);
  });

  it("DC_BETA is a small positive coefficient (0 < DC_BETA < 0.1)", () => {
    expect(DC_BETA).toBeGreaterThan(0);
    expect(DC_BETA).toBeLessThan(0.1);
  });

  it("WP_TAU_MS is a positive integer (EWMA write-head tau)", () => {
    expect(WP_TAU_MS).toBeGreaterThan(0);
  });
});

// ── BandChart layout ──────────────────────────────────────────────────────────

describe("BandChart canvas layout", () => {
  it("BAND_CANVAS_H = default 4-channel layout (4 × TILE_H + 3 × TILE_GAP)", () => {
    expect(BAND_CANVAS_H).toBe(4 * BAND_TILE_H + 3 * BAND_TILE_GAP);
  });

  it("BAND_CANVAS_H is 210 px (4 × 48 + 3 × 6)", () => {
    expect(BAND_CANVAS_H).toBe(210);
  });

  it("BAND_TILE_H is positive", () => {
    expect(BAND_TILE_H).toBeGreaterThan(0);
  });

  it("BAND_TILE_GAP is non-negative", () => {
    expect(BAND_TILE_GAP).toBeGreaterThanOrEqual(0);
  });
});

// ── Band definitions ──────────────────────────────────────────────────────────

describe("band definitions", () => {
  it("NUM_BANDS is 6 (mirrors Rust NUM_BANDS)", () => {
    expect(NUM_BANDS).toBe(6);
  });

  it("BANDS array has NUM_BANDS entries", () => {
    expect(BANDS).toHaveLength(NUM_BANDS);
  });

  it("band order is delta→theta→alpha→beta→gamma→high_gamma", () => {
    expect(BANDS[0].name).toBe("DELTA");
    expect(BANDS[1].name).toBe("THETA");
    expect(BANDS[2].name).toBe("ALPHA");
    expect(BANDS[3].name).toBe("BETA");
    expect(BANDS[4].name).toBe("GAMMA");
    expect(BANDS[5].name).toBe("Hγ");
  });

  it("band keys match Rust BandPowers field names (rel_ prefix)", () => {
    for (const band of BANDS) {
      expect(band.key).toMatch(/^rel_/);
    }
  });

  it("every band has a positive-width frequency range (hi > lo)", () => {
    for (const band of BANDS) {
      expect(band.hi).toBeGreaterThan(band.lo);
    }
  });

  it("band ranges are contiguous (each hi equals next lo)", () => {
    for (let i = 0; i < BANDS.length - 1; i++) {
      expect(BANDS[i].hi).toBe(BANDS[i + 1].lo);
    }
  });

  it("delta starts at 0.5 Hz and high_gamma ends at 100 Hz", () => {
    expect(BANDS[0].lo).toBe(0.5);
    expect(BANDS[NUM_BANDS - 1].hi).toBe(100);
  });

  it("band colors are valid hex strings", () => {
    for (const band of BANDS) {
      expect(band.color).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  it("band colors match Rust BAND_COLORS", () => {
    // Hard-coded Rust values from constants.rs
    const rustColors = ["#6366f1", "#8b5cf6", "#22c55e", "#3b82f6", "#f59e0b", "#ef4444"];
    for (let i = 0; i < NUM_BANDS; i++) {
      expect(BANDS[i].color).toBe(rustColors[i]);
    }
  });

  it("band Greek symbols are non-empty strings", () => {
    for (const band of BANDS) {
      expect(typeof band.sym).toBe("string");
      expect(band.sym.length).toBeGreaterThan(0);
    }
  });
});

// ── EEG Embedding ─────────────────────────────────────────────────────────────

describe("embedding constants", () => {
  it("EMBEDDING_EPOCH_SECS is 5.0 (mirrors Rust)", () => {
    expect(EMBEDDING_EPOCH_SECS).toBe(5.0);
  });

  it("EMBEDDING_OVERLAP_SECS is 2.5 (50 % overlap)", () => {
    expect(EMBEDDING_OVERLAP_SECS).toBe(2.5);
  });

  it("EMBEDDING_OVERLAP_MIN_SECS is 0.0", () => {
    expect(EMBEDDING_OVERLAP_MIN_SECS).toBe(0.0);
  });

  it("EMBEDDING_OVERLAP_MAX_SECS = EMBEDDING_EPOCH_SECS − 0.5", () => {
    expect(EMBEDDING_OVERLAP_MAX_SECS).toBeCloseTo(EMBEDDING_EPOCH_SECS - 0.5, 6);
  });

  it("EMBEDDING_OVERLAP_MAX_SECS is 4.5", () => {
    expect(EMBEDDING_OVERLAP_MAX_SECS).toBe(4.5);
  });

  it("overlap bounds are ordered: min < default < max < epoch", () => {
    expect(EMBEDDING_OVERLAP_MIN_SECS).toBeLessThan(EMBEDDING_OVERLAP_SECS);
    expect(EMBEDDING_OVERLAP_SECS).toBeLessThan(EMBEDDING_OVERLAP_MAX_SECS);
    expect(EMBEDDING_OVERLAP_MAX_SECS).toBeLessThan(EMBEDDING_EPOCH_SECS);
  });
});

// ── Calibration defaults ──────────────────────────────────────────────────────

describe("calibration defaults", () => {
  it("CALIBRATION_ACTION1_LABEL is 'Eyes Open'", () => {
    expect(CALIBRATION_ACTION1_LABEL).toBe("Eyes Open");
  });

  it("CALIBRATION_ACTION2_LABEL is 'Eyes Closed'", () => {
    expect(CALIBRATION_ACTION2_LABEL).toBe("Eyes Closed");
  });

  it("CALIBRATION_ACTION_DURATION_SECS is 10 s", () => {
    expect(CALIBRATION_ACTION_DURATION_SECS).toBe(10);
  });

  it("CALIBRATION_BREAK_DURATION_SECS is 5 s", () => {
    expect(CALIBRATION_BREAK_DURATION_SECS).toBe(5);
  });

  it("CALIBRATION_LOOP_COUNT is 3", () => {
    expect(CALIBRATION_LOOP_COUNT).toBe(3);
  });

  it("CALIBRATION_AUTO_START is true", () => {
    expect(CALIBRATION_AUTO_START).toBe(true);
  });
});

// ── Default filter config ─────────────────────────────────────────────────────

describe("default filter config", () => {
  it("sample_rate matches SAMPLE_RATE", () => {
    expect(DEFAULT_FILTER_CONFIG.sample_rate).toBe(SAMPLE_RATE);
  });

  it("low_pass_hz is 50 Hz (mirrors Rust DEFAULT_LP_HZ)", () => {
    expect(DEFAULT_FILTER_CONFIG.low_pass_hz).toBe(50);
  });

  it("high_pass_hz is 0.5 Hz (mirrors Rust DEFAULT_HP_HZ)", () => {
    expect(DEFAULT_FILTER_CONFIG.high_pass_hz).toBe(0.5);
  });

  it("notch_bandwidth_hz is 1.0 (mirrors Rust DEFAULT_NOTCH_BW_HZ)", () => {
    expect(DEFAULT_FILTER_CONFIG.notch_bandwidth_hz).toBe(1.0);
  });

  it("low_pass_hz > high_pass_hz (valid bandpass range)", () => {
    expect(DEFAULT_FILTER_CONFIG.low_pass_hz).toBeGreaterThan(DEFAULT_FILTER_CONFIG.high_pass_hz);
  });
});

// ── UMAP ─────────────────────────────────────────────────────────────────────

describe("UMAP constants", () => {
  it("UMAP_SCALE_MIN < UMAP_SCALE_MAX", () => {
    expect(UMAP_SCALE_MIN).toBeLessThan(UMAP_SCALE_MAX);
  });

  it("UMAP_SCALE_MIN is positive", () => {
    expect(UMAP_SCALE_MIN).toBeGreaterThan(0);
  });

  it("UMAP_POINT_SIZE is a positive number", () => {
    expect(UMAP_POINT_SIZE).toBeGreaterThan(0);
  });
});

// ── Search / poll intervals ───────────────────────────────────────────────────

describe("search and polling constants", () => {
  it("SEARCH_PAGE_SIZE is a positive integer", () => {
    expect(SEARCH_PAGE_SIZE).toBeGreaterThan(0);
    expect(Number.isInteger(SEARCH_PAGE_SIZE)).toBe(true);
  });

  it("JOB_POLL_INTERVAL_MS is a positive integer", () => {
    expect(JOB_POLL_INTERVAL_MS).toBeGreaterThan(0);
    expect(Number.isInteger(JOB_POLL_INTERVAL_MS)).toBe(true);
  });
});
