// SPDX-License-Identifier: GPL-3.0-only
// Shared E2E test helpers for daemon integration tests.
//
// Provides:
//   - Daemon readiness detection (waits for /readyz)
//   - Test mode lifecycle (POST /v1/test/begin + /v1/test/end)
//   - Auth token resolution
//   - Typed API helper

import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

export const PORT = 18445;
export const BASE = `http://127.0.0.1:${PORT}`;
const TOKEN_PATH = join(homedir(), "Library/Application Support/skill/daemon/auth.token");

/** Read the daemon auth token from disk. */
export function readToken(): string {
  try {
    return readFileSync(TOKEN_PATH, "utf-8").trim();
  } catch {
    return "";
  }
}

/** Check if the daemon is reachable and ready. */
export async function isDaemonReady(timeoutMs = 1000): Promise<boolean> {
  try {
    const r = await fetch(`${BASE}/readyz`, { signal: AbortSignal.timeout(timeoutMs) });
    if (!r.ok) return false;
    const body = await r.json();
    return body.ready === true;
  } catch {
    return false;
  }
}

/** Check if the daemon is reachable at all (even if not fully ready). */
export async function isDaemonAlive(timeoutMs = 500): Promise<boolean> {
  try {
    const r = await fetch(`${BASE}/healthz`, { signal: AbortSignal.timeout(timeoutMs) });
    return r.ok;
  } catch {
    return false;
  }
}

/** Enter test mode — pauses background work (screenshots, OCR, re-embed).
 *  Only available in debug builds. Silently no-ops in release builds. */
export async function testBegin(token: string): Promise<void> {
  try {
    await fetch(`${BASE}/v1/test/begin`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Release build or daemon not ready — that's fine
  }
}

/** Exit test mode — resumes background work. */
export async function testEnd(token: string): Promise<void> {
  try {
    await fetch(`${BASE}/v1/test/end`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Best effort
  }
}

/** Make an authenticated API call to the daemon. */
export async function api<T>(
  token: string,
  path: string,
  method = "GET",
  body?: unknown,
  timeoutMs = 30_000,
): Promise<T> {
  const resp = await fetch(`${BASE}${path}`, {
    method,
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!resp.ok) throw new Error(`${method} ${path} → ${resp.status}`);
  return resp.json() as Promise<T>;
}
