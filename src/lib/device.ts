// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Device abstraction layer
 *
 * Single source of truth for EEG device families and their capability flags.
 * All UI decisions that depend on what hardware is connected should derive from
 * `DeviceCapabilities` rather than checking `device_name` strings directly.
 *
 * ## Adding a new device
 * 1. Add a new entry to the `DeviceKind` union.
 * 2. Add a `*_CAPS` constant below with accurate capability flags.
 * 3. Add a detection clause in `deviceCapabilities()`.
 * 4. Optionally add a matching entry in `src-tauri/src/device.rs`.
 */

// ── Device family ─────────────────────────────────────────────────────────────

/**
 * Known EEG device families.
 * `"unknown"` is used for unrecognised device names or while disconnected.
 */
export type DeviceKind =
  | "muse"       // Muse 1 / 2 / S / Monitor  — 4-ch frontal + temporal
  | "openbci"    // OpenBCI Cyton (8-ch) / Ganglion (4-ch) — configurable 10-20
  | "emotiv"     // Emotiv EPOC / Insight / Flex — 14/5/32-ch
  | "idun"       // IDUN Guardian — single-ch bipolar in-ear EEG earbud
  | "unknown";   // unrecognised or disconnected

// ── Capability flags ──────────────────────────────────────────────────────────

export interface DeviceCapabilities {
  /** Device family. */
  kind: DeviceKind;

  /** Advertised EEG channel count. */
  channelCount: number;

  /** Device has photoplethysmography (PPG / heart-rate) sensor. */
  hasPpg: boolean;

  /** Device has inertial measurement unit (accelerometer + gyroscope). */
  hasImu: boolean;

  /**
   * Device has electrodes at central scalp sites (C3, C4, Cz or equivalent).
   *
   * This flag gates the **mu-rhythm suppression** metric.  Mu (8–13 Hz)
   * originates from the motor cortex — central scalp only.  Devices whose
   * electrode montage covers only frontal / temporal sites (e.g. Muse) cannot
   * produce a meaningful mu suppression value and should hide that metric.
   */
  hasCentralElectrodes: boolean;

  /** Whether the device supports a full 10-20 montage (or superset). */
  hasFullMontage: boolean;

  /** Nominal sample rate in Hz. */
  sampleRateHz: number;

  /** Human-readable electrode labels in channel order (as reported by firmware). */
  electrodeNames: readonly string[];
}

// ── Device capability tables ──────────────────────────────────────────────────

const MUSE_CAPS: DeviceCapabilities = {
  kind:                 "muse",
  channelCount:         4,
  hasPpg:               true,
  hasImu:               true,
  hasCentralElectrodes: false,   // TP9 / AF7 / AF8 / TP10 — frontal + temporal
  hasFullMontage:       false,
  sampleRateHz:         256,
  electrodeNames:       ["TP9", "AF7", "AF8", "TP10"],
} as const;

const OPENBCI_CAPS: DeviceCapabilities = {
  kind:                 "openbci",
  channelCount:         8,       // Cyton; Ganglion = 4
  hasPpg:               false,
  hasImu:               false,
  hasCentralElectrodes: true,    // standard 10-20 includes C3, C4, Cz
  hasFullMontage:       true,
  sampleRateHz:         250,
  electrodeNames:       ["Fp1", "Fp2", "C3", "C4", "P7", "P8", "O1", "O2"],
} as const;

const EMOTIV_CAPS: DeviceCapabilities = {
  kind:                 "emotiv",
  channelCount:         14,      // EPOC; Insight = 5; Flex = 32
  hasPpg:               false,
  hasImu:               true,
  hasCentralElectrodes: true,    // EPOC includes FC5/FC6 (near-central)
  hasFullMontage:       false,
  sampleRateHz:         128,
  electrodeNames:       [
    "AF3","F7","F3","FC5","T7","P7","O1",
    "O2","P8","T8","FC6","F4","F8","AF4",
  ],
} as const;

const IDUN_CAPS: DeviceCapabilities = {
  kind:                 "idun",
  channelCount:         1,       // single bipolar in-ear montage
  hasPpg:               false,
  hasImu:               true,    // 6-DOF IMU (accel + gyro)
  hasCentralElectrodes: false,   // in-ear canal placement
  hasFullMontage:       false,
  sampleRateHz:         250,
  electrodeNames:       ["EEG"],
} as const;

const UNKNOWN_CAPS: DeviceCapabilities = {
  kind:                 "unknown",
  channelCount:         0,
  hasPpg:               false,
  hasImu:               false,
  hasCentralElectrodes: false,
  hasFullMontage:       false,
  sampleRateHz:         0,
  electrodeNames:       [],
} as const;

// ── Detection ─────────────────────────────────────────────────────────────────

/**
 * Derive `DeviceCapabilities` from the BLE / USB advertising name reported by
 * the firmware.  Name matching is case-insensitive prefix/substring detection.
 *
 * Returns `UNKNOWN_CAPS` for `null` (disconnected) or unrecognised names.
 */
export function deviceCapabilities(deviceName: string | null): DeviceCapabilities {
  if (!deviceName) return UNKNOWN_CAPS;
  const n = deviceName.toLowerCase();

  if (n.startsWith("muse"))                                        return MUSE_CAPS;
  if (n.startsWith("openbci") || n.startsWith("cyton")
      || n.startsWith("ganglion"))                                 return OPENBCI_CAPS;
  if (n.startsWith("emotiv") || n.startsWith("epoc")
      || n.startsWith("insight") || n.startsWith("flex"))         return EMOTIV_CAPS;
  if (n.startsWith("idun") || n.startsWith("ige")
      || n.startsWith("guardian"))                                 return IDUN_CAPS;

  return UNKNOWN_CAPS;
}

/** Convenience: true when the connected device is any Muse variant. */
export function isMuse(deviceName: string | null): boolean {
  return deviceCapabilities(deviceName).kind === "muse";
}
