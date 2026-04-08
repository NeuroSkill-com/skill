// SPDX-License-Identifier: GPL-3.0-only

import { beforeEach, describe, expect, it, vi } from "vitest";

const tauriInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriInvoke,
}));

describe("daemon http negative-path contract", () => {
  beforeEach(() => {
    tauriInvoke.mockReset();
    vi.restoreAllMocks();
  });

  it("rejects bootstrap protocol mismatch", async () => {
    tauriInvoke.mockResolvedValue({
      port: 18444,
      token: "t",
      compatible_protocol: false,
      daemon_version: "0.0.1",
      protocol_version: 999,
    });

    const { ensureDaemonCompatible, invalidateDaemonBootstrap } = await import("../lib/daemon/http");
    invalidateDaemonBootstrap();
    await expect(ensureDaemonCompatible()).rejects.toThrow("Daemon protocol mismatch");
  });

  it("throws API error message on non-2xx response", async () => {
    tauriInvoke.mockResolvedValue({
      port: 18444,
      token: "t",
      compatible_protocol: true,
    });

    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ error: "forbidden" }), {
          status: 403,
          statusText: "Forbidden",
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );

    const { daemonGet, invalidateDaemonBootstrap } = await import("../lib/daemon/http");
    invalidateDaemonBootstrap();
    await expect(daemonGet("/v1/status")).rejects.toThrow("forbidden");
  });

  it("throws when payload has ok=false even with 200", async () => {
    tauriInvoke.mockResolvedValue({
      port: 18444,
      token: "t",
      compatible_protocol: true,
    });

    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ ok: false, error: "bad payload" }), {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );

    const { daemonPost, invalidateDaemonBootstrap } = await import("../lib/daemon/http");
    invalidateDaemonBootstrap();
    await expect(daemonPost("v1/control/start-session", {})).rejects.toThrow("bad payload");
  });

  it("sends bearer token and normalized URL", async () => {
    tauriInvoke.mockResolvedValue({
      port: 19999,
      token: "abc",
      compatible_protocol: true,
    });

    const fetchSpy = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchSpy);

    const { daemonPost, invalidateDaemonBootstrap } = await import("../lib/daemon/http");
    invalidateDaemonBootstrap();
    await daemonPost("v1/ping", { x: 1 });

    const [url, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("http://127.0.0.1:19999/v1/ping");
    expect((init.headers as Record<string, string>).Authorization).toBe("Bearer abc");
  });
});
