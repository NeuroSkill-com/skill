// SPDX-License-Identifier: GPL-3.0-only

import { type ChildProcess, spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const TEST_PORT = 18544; // Use a different port to avoid conflicts
const BASE = `http://127.0.0.1:${TEST_PORT}`;
const TOKEN_PATH = join(homedir(), "Library/Application Support/skill/daemon/auth.token");

// Skip if daemon binary doesn't exist (CI without full build)
const DAEMON_BIN = "src-tauri/target/debug/skill-daemon";
let canRun = false;
try {
  const { statSync } = await import("node:fs");
  canRun = statSync(DAEMON_BIN).isFile();
} catch {
  canRun = false;
}

async function api<T>(path: string, token: string, method = "GET", body?: unknown): Promise<T> {
  const resp = await fetch(`${BASE}${path}`, {
    method,
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  return resp.json() as Promise<T>;
}

describe.skipIf(!canRun)("daemon token E2E", () => {
  let daemon: ChildProcess;
  let token: string;

  beforeAll(async () => {
    // Start daemon (binary must already be built)
    daemon = spawn(DAEMON_BIN, [], {
      env: {
        ...process.env,
        SKILL_DAEMON_ADDR: `127.0.0.1:${TEST_PORT}`,
        RUST_LOG: "error",
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    daemon.stderr?.on("data", (d: Buffer) => process.stderr.write(d));

    // Wait for readiness
    let ready = false;
    for (let i = 0; i < 50; i++) {
      try {
        const r = await fetch(`${BASE}/healthz`, {
          signal: AbortSignal.timeout(200),
        });
        if (r.ok) {
          ready = true;
          break;
        }
      } catch {
        /* not ready */
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    if (!ready) throw new Error("Daemon did not become ready in 10s");

    token = readFileSync(TOKEN_PATH, "utf-8").trim();
  }, 30_000);

  afterAll(() => {
    daemon?.kill();
  });

  it("healthz responds", async () => {
    const r = await fetch(`${BASE}/healthz`);
    const body = await r.json();
    expect(body).toEqual({ ok: true });
  });

  it("auth with default token works", async () => {
    const v = await api<{ daemon: string }>("/v1/version", token);
    expect(v.daemon).toBe("skill-daemon");
  });

  it("rejects invalid token", async () => {
    const r = await fetch(`${BASE}/v1/version`, {
      headers: { Authorization: "Bearer invalid-token-xyz" },
    });
    expect(r.status).toBe(401);
  });

  it("cannot delete default token", async () => {
    const r = await api<{ ok: boolean; error?: string }>("/v1/auth/tokens/delete", token, "POST", { id: "default" });
    expect(r.ok).toBe(false);
    expect(r.error).toContain("cannot delete");
  });

  it("creates a scoped token", async () => {
    const r = await api<{
      id: string;
      token: string;
      acl: string;
      expires_at: number;
    }>("/v1/auth/tokens", token, "POST", {
      name: "E2E Test",
      acl: "read_only",
      expiry: "week",
    });
    expect(r.id).toBeTruthy();
    expect(r.token).toMatch(/^sk-/);
    expect(r.acl).toBe("read_only");
    expect(r.expires_at).toBeGreaterThan(Date.now() / 1000);
  });

  it("scoped read_only token can GET but not POST", async () => {
    // Create read_only token
    const created = await api<{ token: string }>("/v1/auth/tokens", token, "POST", {
      name: "ReadOnly",
      acl: "read_only",
      expiry: "week",
    });

    // GET should work
    const version = await api<{ daemon: string }>("/v1/version", created.token);
    expect(version.daemon).toBe("skill-daemon");

    // POST should be rejected (403 or 401)
    const r = await fetch(`${BASE}/v1/auth/tokens`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${created.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: "Nope",
        acl: "admin",
        expiry: "week",
      }),
    });
    expect(r.status).toBe(401);
  });

  it("revokes a token", async () => {
    const created = await api<{ id: string; token: string }>("/v1/auth/tokens", token, "POST", {
      name: "ToRevoke",
      acl: "admin",
      expiry: "week",
    });

    // Works before revoke
    const v1 = await api<{ daemon: string }>("/v1/version", created.token);
    expect(v1.daemon).toBe("skill-daemon");

    // Revoke
    await api("/v1/auth/tokens/revoke", token, "POST", {
      id: created.id,
    });

    // Fails after revoke
    const r = await fetch(`${BASE}/v1/version`, {
      headers: { Authorization: `Bearer ${created.token}` },
    });
    expect(r.status).toBe(401);
  });

  it("deletes a token", async () => {
    const created = await api<{ id: string }>("/v1/auth/tokens", token, "POST", {
      name: "ToDelete",
      acl: "admin",
      expiry: "week",
    });

    const r = await api<{ ok: boolean }>("/v1/auth/tokens/delete", token, "POST", { id: created.id });
    expect(r.ok).toBe(true);
  });

  it("refreshes default token", async () => {
    const oldToken = token;
    const r = await api<{ ok: boolean; token: string }>("/v1/auth/default-token/refresh", token, "POST");
    expect(r.ok).toBe(true);
    expect(r.token).toBeTruthy();
    expect(r.token).not.toBe(oldToken);

    // New token works
    const v = await api<{ daemon: string }>("/v1/version", r.token);
    expect(v.daemon).toBe("skill-daemon");

    // Old token still works (file-based fallback)
    const v2 = await api<{ daemon: string }>("/v1/version", oldToken);
    expect(v2.daemon).toBe("skill-daemon");

    // Update for subsequent tests
    token = r.token;
  });

  it("query param auth works (WebSocket compat)", async () => {
    const r = await fetch(`${BASE}/v1/version?token=${encodeURIComponent(token)}`);
    expect(r.status).toBe(200);
    const body = await r.json();
    expect(body.daemon).toBe("skill-daemon");
  });

  it("lists tokens (redacted)", async () => {
    const tokens = await api<Array<{ id: string; token: string; name: string }>>("/v1/auth/tokens", token);
    expect(Array.isArray(tokens)).toBe(true);
    // Tokens in list should be redacted (contain …)
    for (const t of tokens) {
      expect(t.token).toContain("…");
    }
  });
});
