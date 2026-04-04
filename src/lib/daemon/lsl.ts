// SPDX-License-Identifier: GPL-3.0-only

import { daemonGet, daemonPost } from "./http";

export interface LslPairStreamRequest {
  sourceId: string;
  name: string;
  streamType: string;
  channels: number;
  sampleRate: number;
}

interface LslConfigResponse {
  auto_connect: boolean;
  paired_streams: Array<{
    source_id: string;
    name: string;
    stream_type: string;
    channels: number;
    sample_rate: number;
  }>;
}

interface LslDiscoverResponse {
  name: string;
  stream_type: string;
  channels: number;
  sample_rate: number;
  source_id: string;
  hostname: string;
}

export async function lslSetIdleTimeout(secs: number | null): Promise<void> {
  await daemonPost("/v1/lsl/idle-timeout", { secs });
}

export async function lslSetAutoConnect(enabled: boolean): Promise<void> {
  await daemonPost("/v1/lsl/auto-connect", { enabled });
}

export async function lslDiscover<T>(): Promise<T[]> {
  const [discovered, cfg] = await Promise.all([
    daemonGet<LslDiscoverResponse[]>("/v1/lsl/discover"),
    daemonGet<LslConfigResponse>("/v1/lsl/config").catch(() => ({ auto_connect: false, paired_streams: [] })),
  ]);
  const pairedIds = new Set(cfg.paired_streams.map((s) => s.source_id));
  const out = discovered.map((s) => ({
    name: s.name,
    type: s.stream_type,
    channels: s.channels,
    sample_rate: s.sample_rate,
    source_id: s.source_id,
    hostname: s.hostname,
    paired: pairedIds.has(s.source_id),
  }));
  return out as T[];
}

export async function lslConnect(name: string): Promise<void> {
  await daemonPost("/v1/control/start-session", { target: `lsl:${name}` });
}

export async function lslSwitchSession(name: string): Promise<void> {
  await daemonPost("/v1/control/switch-session", { target: `lsl:${name}` });
}

export async function lslStartSecondary(name: string): Promise<void> {
  await daemonPost("/v1/control/start-session", { target: `lsl:${name}` });
}

export async function lslCancelSecondary(_sessionId: string): Promise<void> {
  await daemonPost("/v1/control/cancel-session", {});
}

export async function lslPairStream(req: LslPairStreamRequest): Promise<void> {
  await daemonPost("/v1/lsl/pair", {
    source_id: req.sourceId,
    name: req.name,
    stream_type: req.streamType,
    channels: req.channels,
    sample_rate: req.sampleRate,
  });
}

export async function lslUnpairStream(sourceId: string): Promise<void> {
  await daemonPost("/v1/lsl/unpair", { source_id: sourceId });
}

export async function lslIrohStart<T>(): Promise<T> {
  const out = await daemonPost<T>("/v1/lsl/iroh/start", {});
  await daemonPost("/v1/control/start-session", { target: "lsl-iroh" }).catch(() => {});
  return out;
}

export async function lslIrohStop(): Promise<void> {
  await daemonPost("/v1/lsl/iroh/stop", {});
  await daemonPost("/v1/control/cancel-session", {}).catch(() => {});
}

export function lslIrohStatus<T>(): Promise<T> {
  return daemonGet<T>("/v1/lsl/iroh/status");
}

export async function lslVirtualSourceRunning(): Promise<boolean> {
  const v = await daemonGet<{ running: boolean }>("/v1/lsl/virtual-source/running");
  return !!v.running;
}

export async function lslStartVirtualSource(): Promise<boolean> {
  const v = await daemonPost<{ started?: boolean }>("/v1/lsl/virtual-source/start", {});
  return !!v.started;
}

export async function lslStopVirtualSource(): Promise<boolean> {
  const v = await daemonPost<{ was_running?: boolean }>("/v1/lsl/virtual-source/stop", {});
  return !!v.was_running;
}

export function lslGetConfig<T>(): Promise<T> {
  return daemonGet<T>("/v1/lsl/config");
}

export async function lslGetIdleTimeout(): Promise<number | null> {
  const v = await daemonGet<{ secs: number | null }>("/v1/lsl/idle-timeout");
  return v.secs ?? null;
}

export function getStatus<T>(): Promise<T> {
  return daemonGet<T>("/v1/status");
}

export async function listSecondarySessions<T>(): Promise<T[]> {
  return [] as T[];
}
