// SPDX-License-Identifier: GPL-3.0-only
// Shared E2E test helpers for daemon integration tests.
//
// Two modes:
//   1. Connect to the user's running daemon (default port 18445)
//   2. Spawn an isolated daemon on a random port with a temp skill dir

import { type ChildProcess, spawn as spawnProcess } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

// ── Default (user's running daemon) ─────────────────────────────────────────

/** Default daemon port (the one started by the Tauri app). */
export const DEFAULT_PORT = 18445;
const DEFAULT_TOKEN_PATH = join(homedir(), "Library/Application Support/skill/daemon/auth.token");

/** Read the default daemon auth token from disk. */
export function readToken(path = DEFAULT_TOKEN_PATH): string {
  try {
    return readFileSync(path, "utf-8").trim();
  } catch {
    return "";
  }
}

// ── Dynamic helpers (work with any port) ────────────────────────────────────

export function baseUrl(port: number): string {
  return `http://127.0.0.1:${port}`;
}

/** Check if a daemon is reachable and ready. */
export async function isDaemonReady(port = DEFAULT_PORT, timeoutMs = 1000): Promise<boolean> {
  try {
    const r = await fetch(`${baseUrl(port)}/readyz`, { signal: AbortSignal.timeout(timeoutMs) });
    if (!r.ok) return false;
    const body = await r.json();
    return body.ready === true;
  } catch {
    return false;
  }
}

/** Check if a daemon is reachable at all (even if not fully ready). */
export async function isDaemonAlive(port = DEFAULT_PORT, timeoutMs = 500): Promise<boolean> {
  try {
    const r = await fetch(`${baseUrl(port)}/healthz`, { signal: AbortSignal.timeout(timeoutMs) });
    return r.ok;
  } catch {
    return false;
  }
}

/** Enter test mode — pauses background work.
 *  Only available in debug builds. Silently no-ops in release builds. */
export async function testBegin(token: string, port = DEFAULT_PORT): Promise<void> {
  try {
    await fetch(`${baseUrl(port)}/v1/test/begin`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Release build or daemon not ready — that's fine
  }
}

/** Exit test mode — resumes background work. */
export async function testEnd(token: string, port = DEFAULT_PORT): Promise<void> {
  try {
    await fetch(`${baseUrl(port)}/v1/test/end`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2000),
    });
  } catch {
    // Best effort
  }
}

/** Make an authenticated API call to a daemon. */
export async function api<T>(
  token: string,
  path: string,
  method = "GET",
  body?: unknown,
  timeoutMs = 30_000,
  port = DEFAULT_PORT,
): Promise<T> {
  const resp = await fetch(`${baseUrl(port)}${path}`, {
    method,
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: body ? JSON.stringify(body) : undefined,
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!resp.ok) throw new Error(`${method} ${path} → ${resp.status}`);
  return resp.json() as Promise<T>;
}

// ── Isolated daemon spawner ─────────────────────────────────────────────────

/** Find a free TCP port. */
export async function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.listen(0, "127.0.0.1", () => {
      const addr = srv.address();
      if (typeof addr === "object" && addr) {
        const port = addr.port;
        srv.close(() => resolve(port));
      } else {
        srv.close(() => reject(new Error("Could not get port")));
      }
    });
    srv.on("error", reject);
  });
}

export interface IsolatedDaemon {
  port: number;
  token: string;
  skillDir: string;
  process: ChildProcess;
  /** Gracefully stop the daemon. */
  stop: () => Promise<void>;
}

const DAEMON_BIN = "src-tauri/target/debug/skill-daemon";

/** Check if the debug daemon binary exists. */
export function hasDaemonBinary(): boolean {
  return existsSync(DAEMON_BIN);
}

/**
 * Spawn an isolated daemon on a random port with a temporary skill dir.
 * Waits until the daemon is ready before returning.
 *
 * @param opts.token - Auth token (default: random 32-char hex)
 * @param opts.skillDir - Skill data dir (default: temp dir)
 * @param opts.port - Port (default: auto-assigned free port)
 * @param opts.readyTimeoutMs - Max wait for readiness (default: 15s)
 */
export async function spawnDaemon(
  opts: { token?: string; skillDir?: string; port?: number; readyTimeoutMs?: number } = {},
): Promise<IsolatedDaemon> {
  const port = opts.port ?? (await freePort());
  const token = opts.token ?? randomHex(32);
  const skillDir = opts.skillDir ?? mkdtempSync(join(tmpdir(), "skill-e2e-"));
  const readyTimeout = opts.readyTimeoutMs ?? 15_000;

  // Write auth token where the daemon expects it
  const authDir = join(skillDir, "daemon");
  const { mkdirSync } = await import("node:fs");
  mkdirSync(authDir, { recursive: true });
  writeFileSync(join(authDir, "auth.token"), token);

  const child = spawnProcess(DAEMON_BIN, [], {
    env: {
      ...process.env,
      SKILL_DAEMON_ADDR: `127.0.0.1:${port}`,
      SKILL_DATA_DIR: skillDir,
      SKILL_DAEMON_TOKEN: token,
      // Disable service installer so it doesn't modify launchd/systemd
      SKILL_DAEMON_SERVICE_AUTOINSTALL: "0",
    },
    stdio: "pipe",
  });

  // Collect stderr for debugging
  let stderr = "";
  child.stderr?.on("data", (chunk: Buffer) => {
    stderr += chunk.toString();
  });

  // Wait for readiness
  const deadline = Date.now() + readyTimeout;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(
        `Daemon exited with code ${child.exitCode} before becoming ready.\nstderr: ${stderr.slice(-2000)}`,
      );
    }
    if (await isDaemonAlive(port, 300)) {
      break;
    }
    await sleep(200);
  }

  if (!(await isDaemonAlive(port, 1000))) {
    child.kill("SIGTERM");
    throw new Error(
      `Daemon on port ${port} did not become ready within ${readyTimeout}ms.\nstderr: ${stderr.slice(-2000)}`,
    );
  }

  const stop = async () => {
    if (child.exitCode === null) {
      child.kill("SIGTERM");
      await new Promise<void>((resolve) => {
        const timer = setTimeout(() => {
          child.kill("SIGKILL");
          resolve();
        }, 3000);
        child.on("exit", () => {
          clearTimeout(timer);
          resolve();
        });
      });
    }
  };

  return { port, token, skillDir, process: child, stop };
}

function randomHex(len: number): string {
  const bytes = new Uint8Array(len / 2);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}
