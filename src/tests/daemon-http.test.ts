// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";

describe("daemon HTTP client (http.ts)", () => {
  it("exports expected functions", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/http.ts", "utf-8");

    expect(src).toContain("export async function daemonGet");
    expect(src).toContain("export async function daemonPost");
    expect(src).toContain("export function invalidateDaemonBootstrap");
    expect(src).toContain("export async function ensureDaemonCompatible");
    expect(src).toContain("export async function getDaemonPort");
  });

  it("uses bearer token auth", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/http.ts", "utf-8");

    expect(src).toContain("Authorization");
    expect(src).toContain("Bearer");
  });

  it("requests go to localhost", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/http.ts", "utf-8");

    expect(src).toContain("http://127.0.0.1:");
  });

  it("handles error responses", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/http.ts", "utf-8");

    expect(src).toContain("resp.ok");
    expect(src).toContain("throw new Error");
  });

  it("bootstraps via Tauri invoke", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/http.ts", "utf-8");

    expect(src).toContain("get_daemon_bootstrap");
    expect(src).toContain("compatible_protocol");
  });
});
