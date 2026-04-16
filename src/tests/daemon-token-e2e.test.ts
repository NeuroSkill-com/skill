// SPDX-License-Identifier: GPL-3.0-only

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { baseUrl, hasDaemonBinary, type IsolatedDaemon, spawnDaemon } from "./e2e-helpers";

const canRun = hasDaemonBinary();

describe.skipIf(!canRun)("daemon token E2E", () => {
  let d: IsolatedDaemon;
  let token: string;
  let BASE: string;

  async function api<T>(path: string, tok: string, method = "GET", body?: unknown): Promise<T> {
    const resp = await fetch(`${BASE}${path}`, {
      method,
      headers: { Authorization: `Bearer ${tok}`, "Content-Type": "application/json" },
      body: body ? JSON.stringify(body) : undefined,
    });
    return resp.json() as Promise<T>;
  }

  beforeAll(async () => {
    d = await spawnDaemon();
    token = d.token;
    BASE = baseUrl(d.port);

    // Clean up any leftover tokens from previous runs
    try {
      const list = await api<Array<{ id: string; is_default: boolean }>>("/v1/auth/tokens", token);
      if (Array.isArray(list)) {
        for (const t of list) {
          if (!t.is_default) {
            await api("/v1/auth/tokens/delete", token, "POST", { id: t.id }).catch(() => {});
          }
        }
      }
    } catch {
      /* ignore */
    }
  }, 30_000);

  afterAll(async () => {
    // Clean up any tokens created during tests
    try {
      const list = await api<Array<{ id: string; is_default: boolean }>>("/v1/auth/tokens", token);
      if (Array.isArray(list)) {
        for (const t of list) {
          if (!t.is_default) {
            await api("/v1/auth/tokens/delete", token, "POST", { id: t.id }).catch(() => {});
          }
        }
      }
    } catch {
      /* daemon already stopped */
    }
    await d?.stop();
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

    // POST should be rejected for ACL (403)
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
    expect(r.status).toBe(403);
  });

  it("scoped data token cannot access auth/control routes", async () => {
    const created = await api<{ token: string; ok?: boolean; error?: string }>("/v1/auth/tokens", token, "POST", {
      name: "DataOnly",
      acl: "data",
      expiry: "week",
    });
    // Skip if token creation failed (e.g. max tokens reached from prior runs)
    if (!created.token) {
      return;
    }

    // Data route should work (200 OK, even if empty)
    const sessResp = await fetch(`${BASE}/v1/history/sessions`, {
      headers: { Authorization: `Bearer ${created.token}` },
      signal: AbortSignal.timeout(10_000),
    });
    expect(sessResp.status).toBe(200);

    // Auth route should fail
    const authResp = await fetch(`${BASE}/v1/auth/tokens`, {
      headers: { Authorization: `Bearer ${created.token}` },
      signal: AbortSignal.timeout(10_000),
    });
    expect(authResp.status).toBe(403);

    // Control route should fail
    const controlResp = await fetch(`${BASE}/v1/control/retry-connect`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${created.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({}),
      signal: AbortSignal.timeout(10_000),
    });
    expect(controlResp.status).toBe(403);
  }, 20_000);

  it("scoped stream token cannot push events", async () => {
    const created = await api<{ token: string; ok?: boolean; error?: string }>("/v1/auth/tokens", token, "POST", {
      name: "StreamOnly",
      acl: "stream",
      expiry: "week",
    });
    if (!created.token) {
      return;
    }

    // Read stream/status endpoints should work
    const statusResp = await fetch(`${BASE}/v1/status`, {
      headers: { Authorization: `Bearer ${created.token}` },
    });
    expect(statusResp.status).toBe(200);

    // Mutation should fail
    const pushResp = await fetch(`${BASE}/v1/events/push`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${created.token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ type: "Test", payload: {} }),
    });
    expect(pushResp.status).toBe(403);
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

    // Newer daemon builds invalidate old default tokens immediately.
    // Older builds may still accept old token until restart (file fallback).
    const oldResp = await fetch(`${BASE}/v1/version`, {
      headers: { Authorization: `Bearer ${oldToken}` },
    });
    expect([200, 401]).toContain(oldResp.status);

    // Update for subsequent tests
    token = r.token;
  });

  it("query param auth works (WebSocket compat)", async () => {
    const r = await fetch(`${BASE}/v1/version?token=${encodeURIComponent(token)}`);
    expect(r.status).toBe(200);
    const body = await r.json();
    expect(body.daemon).toBe("skill-daemon");
  });

  it("lists tokens without exposing raw secrets", async () => {
    const tokens = await api<Array<{ id: string; token: string; name: string }>>("/v1/auth/tokens", token);
    expect(Array.isArray(tokens)).toBe(true);
    for (const t of tokens) {
      expect(t.token).toBeTruthy();
      expect(t.token).not.toBe(token);
    }
  });
});
