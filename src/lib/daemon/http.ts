// SPDX-License-Identifier: GPL-3.0-only

import { invoke } from "@tauri-apps/api/core";

interface DaemonBootstrap {
  port: number;
  token: string;
  compatible_protocol: boolean;
  daemon_version?: string | null;
  protocol_version?: number | null;
}

let bootstrapPromise: Promise<DaemonBootstrap> | null = null;

async function getBootstrap(): Promise<DaemonBootstrap> {
  if (!bootstrapPromise) {
    bootstrapPromise = invoke<DaemonBootstrap>("get_daemon_bootstrap").then((b) => {
      if (!b.compatible_protocol) {
        throw new Error(
          `Daemon protocol mismatch (daemon=${b.daemon_version ?? "unknown"}, protocol=${b.protocol_version ?? "unknown"})`,
        );
      }
      return b;
    });
  }
  return bootstrapPromise;
}

export function invalidateDaemonBootstrap(): void {
  bootstrapPromise = null;
}

export async function ensureDaemonCompatible(): Promise<void> {
  await getBootstrap();
}

export async function getDaemonPort(): Promise<number> {
  const b = await getBootstrap();
  return b.port;
}

export async function getDaemonBaseUrl(): Promise<{ port: number; token: string }> {
  const b = await getBootstrap();
  return { port: b.port, token: b.token };
}

export async function daemonGet<T>(path: string): Promise<T> {
  return daemonRequest<T>("GET", path);
}

export async function daemonPost<T>(path: string, body?: unknown, timeoutMs?: number): Promise<T> {
  return daemonRequest<T>("POST", path, body, timeoutMs);
}

export async function daemonPut<T>(path: string, body?: unknown): Promise<T> {
  return daemonRequest<T>("PUT", path, body);
}

export async function daemonDelete<T>(path: string, body?: unknown): Promise<T> {
  return daemonRequest<T>("DELETE", path, body);
}

async function daemonRequest<T>(
  method: "GET" | "POST" | "PUT" | "DELETE",
  path: string,
  body?: unknown,
  timeoutMs = 10_000,
): Promise<T> {
  let port: number;
  let token: string;
  try {
    const b = await getBootstrap();
    port = b.port;
    token = b.token;
  } catch (e) {
    import("./status.svelte").then(({ notifyDaemonError }) => notifyDaemonError("bootstrap failed")).catch(() => {});
    throw e;
  }
  const url = `http://127.0.0.1:${port}${path.startsWith("/") ? path : `/${path}`}`;

  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (token) headers.Authorization = `Bearer ${token}`;

  const resp = await fetch(url, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });

  const text = await resp.text();
  const json = text ? JSON.parse(text) : null;

  if (!resp.ok) {
    const msg = json?.error || json?.message || `${resp.status} ${resp.statusText}`;
    if (resp.status === 401) {
      // Token may be stale after daemon restart — force re-bootstrap on next request.
      invalidateDaemonBootstrap();
      import("./status.svelte")
        .then(({ notifyDaemonError }) => notifyDaemonError("authentication failed"))
        .catch(() => {});
    }
    throw new Error(msg);
  }

  // Mark connected on success
  import("./status.svelte").then(({ setDaemonConnected }) => setDaemonConnected()).catch(() => {});

  if (json && typeof json === "object" && json.ok === false) {
    throw new Error(json.error || json.message || "Request failed");
  }

  return json as T;
}
