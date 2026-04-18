// SPDX-License-Identifier: GPL-3.0-only

import { daemonInvoke } from "./invoke-proxy";

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
  await daemonInvoke("lsl_set_idle_timeout", { secs });
}

export async function lslSetAutoConnect(enabled: boolean): Promise<void> {
  await daemonInvoke("lsl_set_auto_connect", { enabled });
}

export async function lslDiscover<T>(): Promise<T[]> {
  const [discovered, cfg] = await Promise.all([
    daemonInvoke<LslDiscoverResponse[]>("lsl_discover"),
    daemonInvoke<LslConfigResponse>("lsl_get_config").catch(() => ({ auto_connect: false, paired_streams: [] })),
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
  await daemonInvoke("start_session", { target: `lsl:${name}` });
}

export async function lslSwitchSession(name: string): Promise<void> {
  await daemonInvoke("switch_session", { target: `lsl:${name}` });
}

export async function lslStartSecondary(name: string): Promise<void> {
  await daemonInvoke("start_session", { target: `lsl:${name}` });
}

export async function lslCancelSecondary(_sessionId: string): Promise<void> {
  await daemonInvoke("cancel_session", {});
}

export async function lslPairStream(req: LslPairStreamRequest): Promise<void> {
  await daemonInvoke("lsl_pair_stream", {
    sourceId: req.sourceId,
    name: req.name,
    streamType: req.streamType,
    channels: req.channels,
    sampleRate: req.sampleRate,
  });
}

export async function lslUnpairStream(sourceId: string): Promise<void> {
  await daemonInvoke("lsl_unpair_stream", { sourceId: sourceId });
}

export async function lslIrohStart<T>(): Promise<T> {
  const out = await daemonInvoke<T>("lsl_iroh_start", {});
  await daemonInvoke("start_session", { target: "lsl-iroh" }).catch(() => {});
  return out;
}

export async function lslIrohStop(): Promise<void> {
  await daemonInvoke("lsl_iroh_stop", {});
  await daemonInvoke("cancel_session", {}).catch(() => {});
}

export function lslIrohStatus<T>(): Promise<T> {
  return daemonInvoke<T>("lsl_iroh_status");
}

export async function lslVirtualSourceRunning(): Promise<boolean> {
  const v = await daemonInvoke<{ running: boolean }>("lsl_virtual_source_running");
  return !!v.running;
}

export async function lslStartVirtualSource(): Promise<boolean> {
  const v = await daemonInvoke<{ started?: boolean }>("lsl_virtual_source_start", {});
  return !!v.started;
}

export interface VirtualSourceConfig {
  channels: number;
  sampleRate: number;
  template: string;
  quality: string;
  amplitudeUv: number;
  noiseUv: number;
  lineNoise: string;
  dropoutProb: number;
}

export async function lslStartVirtualSourceConfigured(cfg: VirtualSourceConfig): Promise<boolean> {
  const v = await daemonInvoke<{ started?: boolean }>("lsl_virtual_source_start_configured", {
    channels: cfg.channels,
    sample_rate: cfg.sampleRate,
    template: cfg.template,
    quality: cfg.quality,
    amplitude_uv: cfg.amplitudeUv,
    noise_uv: cfg.noiseUv,
    line_noise: cfg.lineNoise,
    dropout_prob: cfg.dropoutProb,
  });
  return !!v.started;
}

export async function lslStopVirtualSource(): Promise<boolean> {
  const v = await daemonInvoke<{ was_running?: boolean }>("lsl_virtual_source_stop", {});
  return !!v.was_running;
}

export function lslGetConfig<T>(): Promise<T> {
  return daemonInvoke<T>("lsl_get_config");
}

export async function lslGetIdleTimeout(): Promise<number | null> {
  const v = await daemonInvoke<{ secs: number | null }>("lsl_get_idle_timeout");
  return v.secs ?? null;
}

export function getStatus<T>(): Promise<T> {
  return daemonInvoke<T>("get_status");
}

export async function listSecondarySessions<T>(): Promise<T[]> {
  return [] as T[];
}
