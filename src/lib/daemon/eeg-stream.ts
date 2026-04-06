// SPDX-License-Identifier: GPL-3.0-only
// EEG/PPG/IMU streaming via daemon WebSocket events.
// Replaces Tauri IPC Channel subscriptions (subscribe_eeg, subscribe_ppg, subscribe_imu).

import { type DaemonEvent, disconnectDaemonWs, onDaemonEvent } from "./ws";

// Re-export the canonical types from chart components
export type { BandSnapshot } from "$lib/BandChart.svelte";
export type { ImuPacket } from "$lib/ImuChart.svelte";

// ── EEG/PPG packet types (same as old Tauri types) ─────────────────────────

export interface EegPacket {
  electrode: number;
  samples: number[];
  timestamp: number;
}

/** Batched frame: all channels in one event. */
interface EegFramePacket {
  channels: number[];
  timestamp: number;
}

export interface PpgPacket {
  channel: number;
  samples: number[];
  timestamp: number;
}

// ── Subscriptions ──────────────────────────────────────────────────────────

/** Subscribe to EEG sample packets. Returns unsubscribe function.
 *  The daemon sends batched frames (`{ channels, timestamp }`); this
 *  unpacks them into per-electrode `EegPacket` calls for backward compat. */
export function subscribeEeg(callback: (pkt: EegPacket) => void): () => void {
  return onDaemonEvent("EegSample", (ev: DaemonEvent) => {
    const p = ev.payload as Record<string, unknown>;
    if (Array.isArray(p.channels)) {
      // Batched frame — unpack into per-electrode callbacks
      const frame = p as unknown as EegFramePacket;
      for (let i = 0; i < frame.channels.length; i++) {
        callback({ electrode: i, samples: [frame.channels[i]], timestamp: frame.timestamp });
      }
    } else {
      // Legacy per-electrode format
      callback(p as unknown as EegPacket);
    }
  });
}

/** Subscribe to PPG sample packets. Returns unsubscribe function. */
export function subscribePpg(callback: (pkt: PpgPacket) => void): () => void {
  return onDaemonEvent("PpgSample", (ev: DaemonEvent) => {
    callback(ev.payload as unknown as PpgPacket);
  });
}

/** Subscribe to IMU packets. Returns unsubscribe function. */
export function subscribeImu(
  callback: (pkt: { sensor: "accel" | "gyro"; samples: [number, number, number][]; timestamp: number }) => void,
): () => void {
  return onDaemonEvent("ImuSample", (ev: DaemonEvent) => {
    callback(
      ev.payload as unknown as { sensor: "accel" | "gyro"; samples: [number, number, number][]; timestamp: number },
    );
  });
}

/** Subscribe to band power snapshots (~4 Hz). Returns unsubscribe function. */
export function subscribeBands(callback: (snap: import("$lib/BandChart.svelte").BandSnapshot) => void): () => void {
  return onDaemonEvent("EegBands", (ev: DaemonEvent) => {
    callback(ev.payload as unknown as import("$lib/BandChart.svelte").BandSnapshot);
  });
}

/** Get the latest band snapshot (one-shot fetch via daemon HTTP). */
export async function getLatestBands(): Promise<import("$lib/BandChart.svelte").BandSnapshot | null> {
  try {
    const { daemonGet } = await import("./http");
    return await daemonGet<import("$lib/BandChart.svelte").BandSnapshot | null>("/v1/activity/latest-bands");
  } catch {
    return null;
  }
}

/** Clean up all EEG streaming subscriptions. */
export function disconnectEegStream(): void {
  disconnectDaemonWs();
}
