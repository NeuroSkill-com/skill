// SPDX-License-Identifier: GPL-3.0-only

import { daemonGet, daemonPost, invalidateDaemonBootstrap } from "./http";

export interface GpuStats {
  gpuName?: string | null;
  render: number;
  tiler: number;
  overall: number;
  isUnifiedMemory: boolean;
  totalMemoryBytes: number | null;
  freeMemoryBytes: number | null;
}

export interface ActiveWindowInfo {
  app_name: string;
  app_path: string;
  window_title: string;
  activated_at: number;
}

export function getGpuStats(): Promise<GpuStats | null> {
  return daemonGet<GpuStats | null>("/v1/settings/gpu-stats");
}

export async function getStorageFormat(): Promise<"csv" | "parquet" | "both"> {
  const r = await daemonGet<{ value: "csv" | "parquet" | "both" }>("/v1/settings/storage-format");
  return r.value;
}

export async function setStorageFormat(format: "csv" | "parquet" | "both"): Promise<void> {
  await daemonPost("/v1/settings/storage-format", { value: format });
}

export async function getWsConfig(): Promise<[string, number]> {
  const r = await daemonGet<{ host: string; port: number }>("/v1/settings/ws-config");
  return [r.host, r.port];
}

export async function setWsConfig(host: string, port: number): Promise<number> {
  const r = await daemonPost<{ port: number }>("/v1/settings/ws-config", { host, port });
  invalidateDaemonBootstrap();
  return r.port;
}

export async function getApiToken(): Promise<string> {
  const r = await daemonGet<{ value: string }>("/v1/settings/api-token");
  return r.value;
}

export async function setApiToken(token: string): Promise<void> {
  await daemonPost("/v1/settings/api-token", { value: token });
  invalidateDaemonBootstrap();
}

export async function getHfEndpoint(): Promise<string> {
  const r = await daemonGet<{ value: string }>("/v1/settings/hf-endpoint");
  return r.value;
}

export async function setHfEndpoint(endpoint: string): Promise<void> {
  await daemonPost("/v1/settings/hf-endpoint", { value: endpoint });
}

export async function getActiveWindowTracking(): Promise<boolean> {
  const r = await daemonGet<{ value: boolean }>("/v1/activity/tracking/active-window");
  return r.value;
}

export async function setActiveWindowTracking(enabled: boolean): Promise<void> {
  await daemonPost("/v1/activity/tracking/active-window", { value: enabled });
}

export function getActiveWindow(): Promise<ActiveWindowInfo | null> {
  return daemonGet<ActiveWindowInfo | null>("/v1/activity/current-window");
}

export async function getInputActivityTracking(): Promise<boolean> {
  const r = await daemonGet<{ value: boolean }>("/v1/activity/tracking/input");
  return r.value;
}

export async function setInputActivityTracking(enabled: boolean): Promise<void> {
  await daemonPost("/v1/activity/tracking/input", { value: enabled });
}

export async function getLastInputActivity(): Promise<[number, number]> {
  const r = await daemonGet<{ keyboard: number; mouse: number }>("/v1/activity/last-input");
  return [r.keyboard, r.mouse];
}

export async function getMainWindowAutoFit(): Promise<boolean> {
  const r = await daemonGet<{ value: boolean }>("/v1/ui/main-window-auto-fit");
  return r.value;
}

export async function setMainWindowAutoFit(enabled: boolean): Promise<void> {
  await daemonPost("/v1/ui/main-window-auto-fit", { value: enabled });
}

export async function getLocationEnabled(): Promise<boolean> {
  const r = await daemonGet<{ value: boolean }>("/v1/settings/location-enabled");
  return r.value;
}

export function setLocationEnabled(enabled: boolean): Promise<Record<string, unknown>> {
  return daemonPost<Record<string, unknown>>("/v1/settings/location-enabled", { value: enabled });
}

export async function getInferenceDevice(): Promise<"gpu" | "cpu"> {
  const r = await daemonGet<{ value: "gpu" | "cpu" }>("/v1/settings/inference-device");
  return r.value;
}

export async function setInferenceDevice(device: "gpu" | "cpu"): Promise<void> {
  await daemonPost("/v1/settings/inference-device", { value: device });
}
