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
  document_path?: string | null;
  activated_at: number;
}

export interface FileInteractionRow {
  id: number;
  file_path: string;
  app_name: string;
  project: string;
  language: string;
  category: string;
  git_branch: string;
  seen_at: number;
  duration_secs: number | null;
  was_modified: boolean;
  size_delta: number;
  lines_added: number;
  lines_removed: number;
  words_delta: number;
  eeg_focus: number | null;
  eeg_mood: number | null;
}

export interface FileUsageRow {
  file_path: string;
  interactions: number;
  edits: number;
  total_secs: number;
  last_seen: number;
}

export interface ProjectUsageRow {
  project: string;
  interactions: number;
  total_secs: number;
  last_seen: number;
}

export interface EditChunkRow {
  id: number;
  interaction_id: number;
  chunk_at: number;
  lines_added: number;
  lines_removed: number;
  size_delta: number;
}

export interface LanguageBreakdownRow {
  language: string;
  interactions: number;
  edits: number;
  total_secs: number;
}

export interface CoEditRow {
  file_a: string;
  file_b: string;
  co_occurrences: number;
}

export interface DailySummaryRow {
  day_start: number;
  interactions: number;
  edits: number;
  total_secs: number;
  lines_added: number;
  lines_removed: number;
  distinct_projects: number;
  distinct_files: number;
  avg_eeg_focus: number | null;
}

export interface HourlyEditRow {
  hour: number;
  interactions: number;
  total_churn: number;
  avg_eeg_focus: number | null;
}

export interface FocusSessionRow {
  id: number;
  start_at: number;
  end_at: number;
  project: string;
  file_count: number;
  edit_count: number;
  total_lines_added: number;
  total_lines_removed: number;
  avg_eeg_focus: number | null;
  avg_eeg_mood: number | null;
}

export interface BuildEventRow {
  id: number;
  command: string;
  outcome: string;
  project: string;
  detected_at: number;
}

export interface FilePatternRule {
  app: string;
  title: string;
  comment?: string;
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

// ── File activity ────────────────────────────────────────────────────────────

export async function getFileActivityTracking(): Promise<boolean> {
  const r = await daemonGet<{ value: boolean }>("/v1/activity/tracking/files");
  return r.value;
}

export async function setFileActivityTracking(enabled: boolean): Promise<void> {
  await daemonPost("/v1/activity/tracking/files", { value: enabled });
}

export function getRecentFiles(limit?: number, since?: number): Promise<FileInteractionRow[]> {
  return daemonPost<FileInteractionRow[]>("/v1/activity/recent-files", {
    limit,
    since,
  });
}

export function getTopFiles(limit?: number, since?: number): Promise<FileUsageRow[]> {
  return daemonPost<FileUsageRow[]>("/v1/activity/top-files", {
    limit,
    since,
  });
}

export function getTopProjects(limit?: number, since?: number): Promise<ProjectUsageRow[]> {
  return daemonPost<ProjectUsageRow[]>("/v1/activity/top-projects", {
    limit,
    since,
  });
}

export function getEditChunks(interactionId: number): Promise<EditChunkRow[]> {
  return daemonPost<EditChunkRow[]>("/v1/activity/edit-chunks", {
    interactionId,
  });
}

export function getEditChunksRange(fromTs: number, toTs: number): Promise<EditChunkRow[]> {
  return daemonPost<EditChunkRow[]>("/v1/activity/edit-chunks", {
    fromTs,
    toTs,
  });
}

export function getLanguageBreakdown(since?: number): Promise<LanguageBreakdownRow[]> {
  return daemonPost<LanguageBreakdownRow[]>("/v1/activity/language-breakdown", { since });
}

export async function getContextSwitchRate(fromTs?: number, toTs?: number): Promise<number> {
  const r = await daemonPost<{ switches_per_minute: number }>("/v1/activity/context-switch-rate", { fromTs, toTs });
  return r.switches_per_minute;
}

export function getCoeditedFiles(windowSecs?: number, limit?: number, since?: number): Promise<CoEditRow[]> {
  return daemonPost<CoEditRow[]>("/v1/activity/coedited-files", { windowSecs, limit, since });
}

export function getDailySummary(dayStart: number): Promise<DailySummaryRow> {
  return daemonPost<DailySummaryRow>("/v1/activity/daily-summary", { dayStart });
}

export function getHourlyHeatmap(since?: number): Promise<HourlyEditRow[]> {
  return daemonPost<HourlyEditRow[]>("/v1/activity/hourly-heatmap", { since });
}

export function getFocusSessions(limit?: number, since?: number): Promise<FocusSessionRow[]> {
  return daemonPost<FocusSessionRow[]>("/v1/activity/focus-sessions", { limit, since });
}

export function getForgottenFiles(since?: number): Promise<string[]> {
  return daemonPost<string[]>("/v1/activity/forgotten-files", { since });
}

export function getRecentBuilds(): Promise<BuildEventRow[]> {
  return daemonGet<BuildEventRow[]>("/v1/activity/recent-builds");
}

export function getFilePatterns(): Promise<FilePatternRule[]> {
  return daemonGet<FilePatternRule[]>("/v1/activity/file-patterns");
}

export async function setFilePatterns(patterns: FilePatternRule[]): Promise<void> {
  await daemonPost("/v1/activity/file-patterns", patterns);
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

export async function getIrohLogs(): Promise<boolean> {
  const r = await daemonGet<{ value: boolean }>("/v1/settings/iroh-logs");
  return r.value;
}

export async function setIrohLogs(enabled: boolean): Promise<void> {
  await daemonPost("/v1/settings/iroh-logs", { value: enabled });
}

export async function getInferenceDevice(): Promise<"gpu" | "cpu"> {
  const r = await daemonGet<{ value: "gpu" | "cpu" }>("/v1/settings/inference-device");
  return r.value;
}

export async function setInferenceDevice(device: "gpu" | "cpu"): Promise<void> {
  await daemonPost("/v1/settings/inference-device", { value: device });
}
