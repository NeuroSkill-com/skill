// SPDX-License-Identifier: GPL-3.0-only

import { daemonGet, daemonPost } from "./http";

export interface SkillInfo {
  name: string;
  description: string;
  source: string;
  enabled: boolean;
}

export async function getSkillsRefreshInterval(): Promise<number> {
  const r = await daemonGet<{ value: number }>("/v1/skills/refresh-interval");
  return r.value;
}

export async function setSkillsRefreshInterval(secs: number): Promise<void> {
  await daemonPost("/v1/skills/refresh-interval", { value: secs });
}

export async function getSkillsSyncOnLaunch(): Promise<boolean> {
  const r = await daemonGet<{ value: boolean }>("/v1/skills/sync-on-launch");
  return r.value;
}

export async function setSkillsSyncOnLaunch(enabled: boolean): Promise<void> {
  await daemonPost("/v1/skills/sync-on-launch", { value: enabled });
}

export async function getSkillsLastSync(): Promise<number | null> {
  const r = await daemonGet<{ value: number | null }>("/v1/skills/last-sync");
  return r.value;
}

export async function syncSkillsNow(): Promise<string> {
  const r = await daemonPost<{ message?: string; status?: string }>("/v1/skills/sync-now", {});
  return r.message ?? r.status ?? "ok";
}

export function listSkills(): Promise<SkillInfo[]> {
  return daemonGet<SkillInfo[]>("/v1/skills/list");
}

export async function getSkillsLicense(): Promise<string | null> {
  const r = await daemonGet<{ value: string | null }>("/v1/skills/license");
  return r.value;
}

export async function setDisabledSkills(names: string[]): Promise<void> {
  await daemonPost("/v1/skills/disabled", { values: names });
}
