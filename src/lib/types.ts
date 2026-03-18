// SPDX-License-Identifier: GPL-3.0-only
// Shared TypeScript interfaces used across multiple pages/components.

// ── Muse / BCI device status ─────────────────────────────────────────────────

export interface PairedDevice {
  id: string;
  name: string;
  last_seen: number;
}

export interface DiscoveredDevice {
  id: string;
  name: string;
  last_rssi: number;
  is_paired: boolean;
  is_preferred: boolean;
}

export type PowerlineFreq = "Hz60" | "Hz50";

export interface FilterConfig {
  sample_rate:        number;
  low_pass_hz:        number | null;
  high_pass_hz:       number | null;
  notch:              PowerlineFreq | null;
  notch_bandwidth_hz: number;
}

export interface DeviceStatus {
  state:          "disconnected" | "scanning" | "connected" | "bt_off";
  device_name:    string | null;
  device_id:      string | null;
  /** Factory serial number from the headset ("sn" field), e.g. "AAAA-BBBB-CCCC". Arrives a few seconds after connect. */
  serial_number:  string | null;
  /** Hardware MAC address from the headset ("ma" field), e.g. "AA-BB-CC-DD-EE-FF". */
  mac_address:    string | null;
  csv_path:       string | null;
  sample_count:   number;
  battery:        number;
  eeg:            number[];
  paired_devices: PairedDevice[];
  bt_error:       string | null;
  target_name:    string | null;
  filter_config:   FilterConfig;
  /** Per-channel quality in electrode order [TP9, AF7, AF8, TP10].
   *  Values: "good" | "fair" | "poor" | "no_signal" */
  channel_quality: string[];
  /** Current auto-retry attempt (0 = not retrying). */
  retry_attempt: number;
  /** Seconds remaining until next auto-retry (0 = not counting down). */
  retry_countdown_secs: number;
  /** Latest raw PPG values [ambient, infrared, red]. */
  ppg: number[];
  /** Total PPG samples received this session. */
  ppg_sample_count: number;
  /** Latest accelerometer reading [x, y, z] in g. */
  accel: [number, number, number];
  /** Latest gyroscope reading [x, y, z] in °/s. */
  gyro: [number, number, number];
  /** Battery fuel-gauge voltage in mV (Classic only, 0 on Athena). */
  fuel_gauge_mv: number;
  /** Raw temperature ADC value (Classic only, 0 on Athena). */
  temperature_raw: number;
  /** Which device family is connected (see `DeviceKind` in device.rs). */
  device_kind: string;
  /** Hardware model code, e.g. "p50" = Muse S (Athena), "p21" = Muse 2. */
  hardware_version: string | null;
  /** Device has a PPG (heart-rate) sensor. */
  has_ppg: boolean;
  /** Device has an IMU (accelerometer + gyroscope). */
  has_imu: boolean;
  /** Device has electrodes at central scalp sites (C3/C4/Cz). */
  has_central_electrodes: boolean;
  /** Device supports a full 10-20 montage (or superset). */
  has_full_montage: boolean;
  /** Catch-all for future fields not yet typed. */
  [k: string]: unknown;
}

// ── Muse electrode constants ─────────────────────────────────────────────────

export const MUSE_CHANNELS  = ["TP9", "AF7", "AF8", "TP10"] as const;
export const MUSE_POSITIONS = ["Left ear", "Left forehead", "Right forehead", "Right ear"] as const;

// ── Sleep types ──────────────────────────────────────────────────────────────

export interface SleepEpoch {
  utc:       number;
  stage:     number;   // 0=Wake, 1=N1, 2=N2, 3=N3, 5=REM
  rel_delta: number;
  rel_theta: number;
  rel_alpha: number;
  rel_beta:  number;
}

export interface SleepSummary {
  total_epochs:  number;
  wake_epochs:   number;
  n1_epochs:     number;
  n2_epochs:     number;
  n3_epochs:     number;
  rem_epochs:    number;
  epoch_secs:    number;
}

export interface SleepStages {
  epochs:  SleepEpoch[];
  summary: SleepSummary;
}

// ── UMAP types ───────────────────────────────────────────────────────────────

export interface UmapPoint {
  x: number; y: number; z: number;
  session: number; utc: number; label?: string;
  /** Semantic distance from the query anchor (used in kNN graph tooltips). */
  dist?: number;
}

export interface UmapResult {
  points: UmapPoint[];
  n_a: number;
  n_b: number;
  dim: number;
  elapsed_ms?: number;
}

export interface UmapProgress {
  epoch: number;
  total_epochs: number;
  loss: number;
  best_loss: number;
  elapsed_secs: number;
  epoch_ms: number;
}

// ── Label types ──────────────────────────────────────────────────────────────

export interface LabelRow {
  id:          number;
  eeg_start:   number;
  eeg_end:     number;
  label_start: number;
  label_end:   number;
  text:        string;
  context:     string;
  created_at:  number;
}
