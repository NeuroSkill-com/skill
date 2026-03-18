// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * Device abstraction layer — **thin TypeScript types only**.
 *
 * The single source of truth for device families, capabilities, and the
 * supported-devices catalog lives in `crates/skill-data/src/device.rs`.
 *
 * Capability flags are pushed to the frontend as part of `DeviceStatus`
 * (fields `has_ppg`, `has_imu`, `has_central_electrodes`, `has_full_montage`).
 *
 * For one-off capability lookups (e.g. onboarding before a device is
 * connected), use the `get_device_capabilities` Tauri command.
 */

import { invoke } from "@tauri-apps/api/core";

// ── Device family ─────────────────────────────────────────────────────────────

/**
 * Known EEG device families.
 * `"unknown"` is used for unrecognised device names or while disconnected.
 */
export type DeviceKind =
  | "muse"
  | "ganglion"
  | "open_bci"
  | "mw75"
  | "hermes"
  | "emotiv"
  | "idun"
  | "unknown";

// ── Capability flags ──────────────────────────────────────────────────────────

/** Mirrors `skill_data::device::DeviceCapabilities` (serialised via serde). */
export interface DeviceCapabilities {
  kind: DeviceKind;
  channel_count: number;
  has_ppg: boolean;
  has_imu: boolean;
  has_central_electrodes: boolean;
  has_full_montage: boolean;
  sample_rate_hz: number;
  electrode_names: string[];
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/**
 * Fetch device capabilities from Rust for an arbitrary device name.
 * Use this when you don't have a live `DeviceStatus` (e.g. onboarding).
 */
export function getDeviceCapabilities(deviceName: string | null): Promise<DeviceCapabilities> {
  return invoke<DeviceCapabilities>("get_device_capabilities", {
    deviceName: deviceName ?? null,
  });
}
