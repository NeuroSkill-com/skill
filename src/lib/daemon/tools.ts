// SPDX-License-Identifier: GPL-3.0-only

import { daemonGet, daemonPost } from "./http";

export function getLlmConfig<T>(): Promise<T> {
  return daemonGet<T>("/v1/settings/llm-config");
}

export async function setLlmConfig(config: unknown): Promise<void> {
  await daemonPost("/v1/settings/llm-config", config);
}

export function webCacheStats<T>(): Promise<T> {
  return daemonGet<T>("/v1/settings/web-cache/stats");
}

export function webCacheList<T>(): Promise<T[]> {
  return daemonGet<T[]>("/v1/settings/web-cache/list");
}

export async function webCacheClear(): Promise<void> {
  await daemonPost("/v1/settings/web-cache/clear", {});
}

export async function webCacheRemoveDomain(domain: string): Promise<void> {
  await daemonPost("/v1/settings/web-cache/remove-domain", { value: domain });
}

export async function webCacheRemoveEntry(key: string): Promise<void> {
  await daemonPost("/v1/settings/web-cache/remove-entry", { key });
}
