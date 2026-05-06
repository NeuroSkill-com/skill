// SPDX-License-Identifier: GPL-3.0-only
// One-off daemon client functions that don't warrant their own module.

import { daemonGet, daemonPost } from "./http";

export interface TaskHeartbeat {
  lastTickUnixMs: number;
  lastDurationMs: number;
  tickCount: number;
}

export interface DaemonBackgroundTask {
  id: string;
  name: string;
  does: string;
  why: string;
  intervalSecs: number;
  cost: "low" | "medium" | "high";
  userToggleable: boolean;
  heartbeat: TaskHeartbeat;
  state?: { running: boolean; detail?: string };
}

export interface DaemonActivityResponse {
  tasks: DaemonBackgroundTask[];
}

interface RawHeartbeat {
  last_tick_unix_ms: number;
  last_duration_ms: number;
  tick_count: number;
}

interface RawTask {
  id: string;
  name: string;
  does: string;
  why: string;
  interval_secs: number;
  cost: "low" | "medium" | "high";
  user_toggleable: boolean;
  heartbeat: RawHeartbeat;
  state?: { running: boolean; detail?: string };
}

export async function getDaemonActivity(): Promise<DaemonActivityResponse> {
  const raw = await daemonGet<{ tasks: RawTask[] }>("/v1/activity");
  return {
    tasks: raw.tasks.map((t) => ({
      id: t.id,
      name: t.name,
      does: t.does,
      why: t.why,
      intervalSecs: t.interval_secs,
      cost: t.cost,
      userToggleable: t.user_toggleable,
      heartbeat: {
        lastTickUnixMs: t.heartbeat.last_tick_unix_ms,
        lastDurationMs: t.heartbeat.last_duration_ms,
        tickCount: t.heartbeat.tick_count,
      },
      state: t.state,
    })),
  };
}

export async function deleteSession(csvPath: string): Promise<void> {
  await daemonPost("/v1/history/sessions/delete", { csv_path: csvPath });
}

export async function submitLabel(args: Record<string, unknown>): Promise<void> {
  await daemonPost("/v1/labels", args);
}

export async function setExgInferenceDevice(device: string): Promise<void> {
  await daemonPost("/v1/settings/exg-inference-device", { value: device });
}

export async function downloadOcrModels(): Promise<void> {
  await daemonPost("/v1/settings/screenshot/download-ocr", {});
}
