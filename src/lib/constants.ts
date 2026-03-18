// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Single source of truth for every numeric and string constant shared across
 * the EEG visualisation pipeline.
 *
 * Signal-processing values (SAMPLE_RATE, FILTER_HOP, SPEC_N_FREQ, BANDS …)
 * must stay in sync with their Rust mirrors in `src-tauri/src/constants.rs`.
 */

// ── Hardware / signal ─────────────────────────────────────────────────────────

/** Maximum number of EEG channels in the DSP pipeline.  Mirrors `EEG_CHANNELS` in constants.rs. */
export const EEG_CHANNELS = 12;

/** Number of EEG channels for 4-channel devices (Muse, Ganglion). */
export const EEG_CHANNELS_4 = 4;

/** Muse channel labels (default), index-matched to all per-channel arrays. */
export const EEG_CH = ["TP9", "AF7", "AF8", "TP10"] as const;

/** Per-channel accent colours for the default 4-channel view. */
export const EEG_COLOR = ["#22c55e", "#60a5fa", "#c084fc", "#fb923c"] as const;

// ── OpenBCI Ganglion (4-channel) ──────────────────────────────────────────────

/** OpenBCI Ganglion channel count. */
export const GANGLION_EEG_CHANNELS = 4;

/** OpenBCI Ganglion channel labels (generic — user-configurable montage). */
export const GANGLION_CH = ["Ch1", "Ch2", "Ch3", "Ch4"] as const;

/** Ganglion per-channel colours. */
export const GANGLION_COLOR = ["#22c55e", "#60a5fa", "#c084fc", "#fb923c"] as const;

// ── Hermes V1 (8-channel) ────────────────────────────────────────────────────

/** Hermes V1 EEG channel count (8-channel ADS1299). */
export const HERMES_EEG_CHANNELS = 8;

/** Hermes V1 channel labels (10-20 positions — must match Rust HERMES_CHANNEL_NAMES). */
export const HERMES_CH = [
  "Fp1", "Fp2", "AF3", "AF4", "F3", "F4", "FC1", "FC2",
] as const;

/** Hermes V1 per-channel colours. */
export const HERMES_COLOR = [
  "#22c55e", "#60a5fa", "#c084fc", "#fb923c",
  "#16a34a", "#3b82f6", "#a855f7", "#f97316",
] as const;

// ── MW75 Neuro (12-channel) ──────────────────────────────────────────────────

/** MW75 total EEG channel count (6 per ear cup). */
export const MW75_EEG_CHANNELS = 12;

/** MW75 channel labels — approximate 10-20 extended positions.
 *  Left ear (Ch1-6): FT7, T7, TP7, CP5, P7, C5
 *  Right ear (Ch7-12): FT8, T8, TP8, CP6, P8, C6 */
export const MW75_CH = [
  "FT7", "T7", "TP7", "CP5", "P7", "C5",
  "FT8", "T8", "TP8", "CP6", "P8", "C6",
] as const;

/** MW75 per-channel colours — 6 greens (left) + 6 blues (right). */
export const MW75_COLOR = [
  "#22c55e", "#16a34a", "#15803d", "#a3e635", "#84cc16", "#65a30d",
  "#60a5fa", "#3b82f6", "#2563eb", "#c084fc", "#a855f7", "#7c3aed",
] as const;

// ── Emotiv EPOC (14-channel) ──────────────────────────────────────────────────

/** Emotiv EPOC X / EPOC+ hardware channel count (14 channels). */
export const EMOTIV_EEG_CHANNELS = 14;

/**
 * Emotiv EPOC X / EPOC+ channel labels — all 14 electrodes.
 *
 * Must match Rust `EMOTIV_EPOC_CHANNEL_NAMES` in `skill-constants`.
 * The DSP pipeline is capped at `EEG_CHANNELS` (12), so only the first
 * 12 channels are processed; the remaining 2 are shown for reference.
 */
export const EMOTIV_CH = [
  "AF3", "F7", "F3", "FC5", "T7", "P7", "O1",
  "O2", "P8", "T8", "FC6", "F4", "F8", "AF4",
] as const;

/** Emotiv per-channel colours — 7 warm (left) + 7 cool (right). */
export const EMOTIV_COLOR = [
  "#22c55e", "#16a34a", "#15803d", "#a3e635", "#84cc16", "#65a30d", "#4ade80",
  "#60a5fa", "#3b82f6", "#2563eb", "#c084fc", "#a855f7", "#7c3aed", "#818cf8",
] as const;

// ── IDUN Guardian (1-channel) ─────────────────────────────────────────────────

/** IDUN Guardian channel count (single bipolar). */
export const IDUN_EEG_CHANNELS = 1;

/** IDUN Guardian channel label. */
export const IDUN_CH = ["EEG"] as const;

/** IDUN Guardian channel colour. */
export const IDUN_COLOR = ["#22c55e"] as const;

/** Default EEG hardware sample rate (Hz) — used when device rate is unknown. */
export const SAMPLE_RATE = 256;

/** Display half-range (µV).  Signals are clamped to ±EEG_RANGE_UV. */
export const EEG_RANGE_UV = 1000;

// ── EegChart canvas layout ────────────────────────────────────────────────────

/** Total canvas height (CSS px). */
export const CHART_H = 172;

/** Time-axis strip height at the bottom of the chart (CSS px). */
export const TIME_H = 18;

/** Waveform area height = CHART_H − TIME_H (CSS px). */
export const WAVE_H = CHART_H - TIME_H;

/** Vertical inset per waveform row so peaks don't clip the row border (CSS px). */
export const ROW_PAD = 4;

// ── Waveform ring buffer ──────────────────────────────────────────────────────

/** Number of 5-second epochs shown simultaneously. */
export const N_EPOCHS = 3;

/** Duration of one epoch (seconds). */
export const EPOCH_S = 5;

/** Samples per epoch = EPOCH_S × SAMPLE_RATE (at default 256 Hz). */
export const EPOCH_SAMP = EPOCH_S * SAMPLE_RATE; // 1 280

/** Ring-buffer depth = N_EPOCHS × EPOCH_SAMP = 15 s of history (at default 256 Hz). */
export const BUF_SIZE = N_EPOCHS * EPOCH_SAMP; // 3 840

/**
 * Compute the ring-buffer depth for a given sample rate.
 * Always shows `N_EPOCHS × EPOCH_S` seconds of history regardless of device.
 */
export function bufSizeForRate(sampleRate: number): number {
  return N_EPOCHS * EPOCH_S * sampleRate;
}

// ── Signal processing (must match Rust constants.rs) ─────────────────────────

/** Filter hop size (samples).  Mirrors `FILTER_HOP`.  One spectrogram column
 *  is produced per hop: 32 / 256 Hz = 125 ms → 8 columns/s. */
export const FILTER_HOP = 32;

/** Spectrogram frequency bins (0 Hz … 50 Hz at 1 Hz/bin).
 *  Mirrors `SPEC_N_FREQ` in constants.rs. */
export const SPEC_N_FREQ = 51;

/** Spectrogram time columns in the rolling buffer = BUF_SIZE / FILTER_HOP
 *  = 120 columns = 15 s — matches the waveform window exactly. */
export const SPEC_COLS = BUF_SIZE / FILTER_HOP; // 120

/**
 * Compute spectrogram column count for a given sample rate.
 */
export function specColsForRate(sampleRate: number): number {
  return Math.ceil(bufSizeForRate(sampleRate) / FILTER_HOP);
}

// ── Spectrogram normalisation ─────────────────────────────────────────────────

/** Initial per-channel log₁₀(PSD) soft-max seed (µV²). */
export const SPEC_LOG_INIT = -4;

/** Exponential decay rate for the per-channel log-max tracker.
 *  τ ≈ 20 s at 8 columns/s: `1 − 1/(8 × 20)`. */
export const SPEC_LOG_DECAY = 1 - 1 / (8 * 20);

/** Dynamic range shown in the colormap (log₁₀ units = 60 dB). */
export const SPEC_LOG_RANGE = 3.0;

/** Absolute minimum log₁₀(PSD) — anything below this is mapped to zero. */
export const SPEC_LOG_FLOOR = -12;

// ── Spectrogram colormap (viridis-inspired) ───────────────────────────────────
//
// Six control points: [normalised_power, r, g, b, alpha].
// Low power → transparent near-black; high power → fully opaque yellow.
// Linearly interpolated into a 256-entry RGBA LUT at startup.

export const SPEC_CMAP_STOPS_DARK: readonly [number, number, number, number, number][] = [
  [0.00,  10,  10,  25,   0], // near-black, transparent
  [0.12,  68,   1,  84, 140], // dark purple
  [0.35,  59,  82, 139, 200], // blue
  [0.55,  33, 145, 140, 220], // teal
  [0.75,  94, 201,  98, 235], // green
  [1.00, 253, 231,  37, 255], // bright yellow, fully opaque
] as const;

/** @deprecated alias kept for backward compat */
export const SPEC_CMAP_STOPS = SPEC_CMAP_STOPS_DARK;

// Jet-inspired — high contrast, vibrant, clearly readable on white backgrounds.
export const SPEC_CMAP_STOPS_LIGHT: readonly [number, number, number, number, number][] = [
  [0.00, 240, 240, 250, 255], // pale blue-white (silence)
  [0.12,   0,  40, 210, 255], // strong blue
  [0.30,   0, 160, 235, 255], // cyan
  [0.45,   0, 200,  60, 255], // vivid green
  [0.60, 240, 220,   0, 255], // bright yellow
  [0.75, 240, 120,   0, 255], // hot orange
  [0.90, 210,  20,  20, 255], // red
  [1.00, 130,   0,  30, 255], // dark crimson (max power)
] as const;

// ── Waveform rendering ────────────────────────────────────────────────────────

/** DC-blocker one-pole coefficient.  τ ≈ 1 / (DC_BETA × SAMPLE_RATE) ≈ 780 ms. */
export const DC_BETA = 0.005;

/** Centred moving-average kernel width for waveform smoothing (must be odd). */
export const SMOOTH_K = 9;

/** EWMA time-constant for the write-head display position (ms).
 *  Must be large enough to smooth 48 ms BLE packet bursts (Muse), but small
 *  enough that the display feels live.  80 ms ≈ 2× the burst interval. */
export const WP_TAU_MS = 80;

// ── BandChart canvas layout ───────────────────────────────────────────────────

/** Height of one channel tile in the band power chart (CSS px). */
export const BAND_TILE_H = 48;

/** Gap between adjacent channel tiles (CSS px). */
export const BAND_TILE_GAP = 6;

/** Total band chart canvas height (CSS px) = 4 × TILE_H + 3 × TILE_GAP. */
export const BAND_CANVAS_H = EEG_CHANNELS * BAND_TILE_H + (EEG_CHANNELS - 1) * BAND_TILE_GAP; // 642

/** Left inner margin inside each tile (CSS px). */
export const BAND_TILE_ML = 12;

/** Right inner margin inside each tile (CSS px). */
export const BAND_TILE_MR = 12;

/** EWMA smoothing time-constant for band-power interpolation (ms). */
export const BAND_TAU_MS = 350;

// ── Band definitions (must match Rust constants.rs BANDS / BAND_COLORS / BAND_SYMBOLS) ──

/** Number of clinical EEG frequency bands.  Mirrors `NUM_BANDS`. */
export const NUM_BANDS = 6;

/**
 * Ordered band metadata.  Each entry mirrors a row in the Rust `BANDS`,
 * `BAND_COLORS`, and `BAND_SYMBOLS` constants.
 *
 * `key`   — field name on the `BandPowers` struct for the *relative* power.
 * `name`  — display name shown in the tile (all-caps).
 * `sym`   — Greek symbol shown as the large centrepiece.
 * `lo`    — lower bound of the band (Hz, inclusive).
 * `hi`    — upper bound of the band (Hz, exclusive).
 * `color` — hex accent colour; must match `BAND_COLORS[i]` in constants.rs.
 */
export const BANDS = [
  { key: "rel_delta",      name: "DELTA", sym: "δ",  lo: 0.5,  hi:   4, color: "#6366f1" },
  { key: "rel_theta",      name: "THETA", sym: "θ",  lo: 4,    hi:   8, color: "#8b5cf6" },
  { key: "rel_alpha",      name: "ALPHA", sym: "α",  lo: 8,    hi:  13, color: "#22c55e" },
  { key: "rel_beta",       name: "BETA",  sym: "β",  lo: 13,   hi:  30, color: "#3b82f6" },
  { key: "rel_gamma",      name: "GAMMA", sym: "γ",  lo: 30,   hi:  50, color: "#f59e0b" },
  { key: "rel_high_gamma", name: "Hγ",    sym: "γ+", lo: 50,   hi: 100, color: "#ef4444" },
] as const;

// ── EEG Embedding (must match constants.rs EMBEDDING_* values) ───────────────

/** Duration of each ZUNA embedding epoch (seconds).  Mirrors `EMBEDDING_EPOCH_SECS`. */
export const EMBEDDING_EPOCH_SECS = 5.0;

/** Default overlap between consecutive epochs (seconds).
 *  Mirrors `EMBEDDING_OVERLAP_SECS`. */
export const EMBEDDING_OVERLAP_SECS = 2.5;

/** Minimum configurable overlap (seconds).  Mirrors `EMBEDDING_OVERLAP_MIN_SECS`. */
export const EMBEDDING_OVERLAP_MIN_SECS = 0.0;

/** Maximum configurable overlap (seconds).  Mirrors `EMBEDDING_OVERLAP_MAX_SECS`. */
export const EMBEDDING_OVERLAP_MAX_SECS = EMBEDDING_EPOCH_SECS - 0.5; // 4.5

// ── Calibration defaults (must match constants.rs CALIBRATION_* values) ────────

/** Default label for the first calibration action. */
export const CALIBRATION_ACTION1_LABEL = "Eyes Open";

/** Default label for the second calibration action. */
export const CALIBRATION_ACTION2_LABEL = "Eyes Closed";

/** Default duration of each calibration action (seconds). */
export const CALIBRATION_ACTION_DURATION_SECS = 10;

/** Default duration of the break between actions (seconds). */
export const CALIBRATION_BREAK_DURATION_SECS = 5;

/** Default number of full loop iterations. */
export const CALIBRATION_LOOP_COUNT = 3;

/** Whether to auto-open calibration window on startup. */
export const CALIBRATION_AUTO_START = true;

// ── Updater (must match constants.rs UPDATER_* values and tauri.conf.json) ────

/** Ed25519 public key for verifying update signatures.
 *  Mirrors `UPDATER_PUBKEY` in constants.rs and `plugins.updater.pubkey`
 *  in tauri.conf.json.  All three MUST be identical. */
export const UPDATER_PUBKEY =
  "RWSusqj1BfOCzJrG0Zc2GVJfId2PbbkH0X8+z+VcJrea4Qu2qGittCpk";

/** URL template for the update manifest endpoint.
 *  Mirrors `UPDATER_ENDPOINT` in constants.rs and
 *  `plugins.updater.endpoints[0]` in tauri.conf.json. */
export const UPDATER_ENDPOINT =
  "https://releases.example.com/skill/{{target}}/{{arch}}/{{current_version}}";

/** Seconds between automatic background update checks (0 = disabled).
 *  Mirrors `UPDATER_CHECK_INTERVAL_SECS`. */
export const UPDATER_CHECK_INTERVAL_SECS = 3600; // 1 hour

/** Whether to check for updates automatically on startup.
 *  Mirrors `UPDATER_CHECK_ON_STARTUP`. */
export const UPDATER_CHECK_ON_STARTUP = true;

// ── Default filter config (must match FilterConfig::default() in eeg_filter.rs) ─

export const DEFAULT_FILTER_CONFIG = {
  sample_rate:        SAMPLE_RATE,
  low_pass_hz:        50,     // DEFAULT_LP_HZ
  high_pass_hz:       0.5,    // DEFAULT_HP_HZ
  notch:              "Hz60" as const,
  notch_bandwidth_hz: 1.0,    // DEFAULT_NOTCH_BW_HZ
} as const;

// ── UMAP 3D viewer ───────────────────────────────────────────────────────────

/** Colour for session A (query) points (hex integer for Three.js). */
export const UMAP_COLOR_A = 0x3b82f6; // blue-500

/** Colour for session B (neighbor) points (hex integer for Three.js). */
export const UMAP_COLOR_B = 0xf59e0b; // amber-500

/** UMAP point cloud normalisation scale. */
export const UMAP_SCALE = 15;

/** Animation duration (ms) for UMAP cloud transition. */
export const UMAP_ANIM_MS = 1800;

/** Link-line palette for labeled-point connections. */
export const UMAP_LINK_PALETTE = [
  0x22d3ee, 0xf472b6, 0xa3e635, 0xfbbf24,
  0xc084fc, 0xfb7185, 0x34d399, 0x60a5fa,
] as const;

/** UMAP scene background color (dark / light). */
export const UMAP_BG       = 0x1a1a2e;
export const UMAP_BG_LIGHT = 0xf1f5f9; // slate-100

/** Base point size for UMAP scatter cloud. */
export const UMAP_POINT_SIZE = 0.5;

/** Min/max multiplier for the node-scale slider. */
export const UMAP_SCALE_MIN = 0.2;
export const UMAP_SCALE_MAX = 3.0;

/** Time (ms) between adding successive chronological trace segments. */
export const UMAP_TRACE_INTERVAL_MS = 60;

/** Duration (ms) for each line-segment grow animation. */
export const UMAP_TRACE_GROW_MS = 400;

/** Chronological trace line color. */
export const UMAP_TRACE_COLOR = 0x22d3ee; // cyan-400

/** Chronological trace node sphere color. */
export const UMAP_TRACE_NODE_COLOR = 0xffffff;

/** Date palette HSL saturation & lightness for colorByDate mode. */
export const UMAP_DATE_SAT = 0.85;
export const UMAP_DATE_LIT = 0.55;

// ── Session detail chart colours ─────────────────────────────────────────────

export const C_DELTA   = "#6366f1";
export const C_THETA   = "#22c55e";
export const C_ALPHA   = "#3b82f6";
export const C_BETA    = "#f59e0b";
export const C_GAMMA   = "#ef4444";
export const C_FOCUS   = "#3b82f6";
export const C_RELAX   = "#10b981";
export const C_ENGAGE  = "#f59e0b";
export const C_MED     = "#8b5cf6";
export const C_COG     = "#0ea5e9";
export const C_DROW    = "#ef4444";
export const C_MOOD    = "#f59e0b";
export const C_HR      = "#ef4444";
export const C_HRV_G   = "#10b981"; // RMSSD
export const C_HRV_B   = "#3b82f6"; // SDNN
export const C_HRV_A   = "#f59e0b"; // pNN50
export const C_BLINK   = "#ec4899";
export const C_PITCH   = "#0ea5e9";
export const C_ROLL    = "#6366f1";
export const C_STILL   = "#22c55e";
export const C_STRESS  = "#f43f5e";

// ── Search page ──────────────────────────────────────────────────────────────

/** Paginated results per page in search. */
export const SEARCH_PAGE_SIZE = 15;

/** Job poll interval (ms) for search & UMAP. */
export const JOB_POLL_INTERVAL_MS = 300;

/** UMAP poll interval (ms). */
export const UMAP_POLL_INTERVAL_MS = 500;

// ── Session colors ───────────────────────────────────────────────────────────

/** Shared palette for session segments across history, compare, and timeline views. */
export const SESSION_COLORS = ['#3b82f6','#10b981','#8b5cf6','#f59e0b','#06b6d4','#f43f5e','#22d3ee','#84cc16'];

/** Look up a session color by index (wraps around). */
export function sessionColor(idx: number): string { return SESSION_COLORS[idx % SESSION_COLORS.length]; }
