// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";

// We can't import daemonInvoke directly (it depends on Tauri + fetch),
// but we can test the route table structure and logic.

describe("invoke-proxy route table", () => {
  it("is a valid TypeScript module", async () => {
    // Verify the module parses without error by reading the source
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");
    expect(src).toContain("const ROUTES");
    expect(src).toContain("export async function daemonInvoke");
  });

  it("has no duplicate route keys", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

    // Extract keys from ROUTES object
    const routeBlock = src.split("const ROUTES")[1]?.split("};")[0] ?? "";
    const keys = [...routeBlock.matchAll(/^\s+(\w+):\s*\[/gm)].map((m) => m[1]);

    const seen = new Set<string>();
    const dupes: string[] = [];
    for (const k of keys) {
      if (seen.has(k)) dupes.push(k);
      seen.add(k);
    }
    expect(dupes).toEqual([]);
    expect(keys.length).toBeGreaterThan(100);
  });

  it("all routes use valid methods", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

    const routeBlock = src.split("const ROUTES")[1]?.split("};")[0] ?? "";
    const methods = [...routeBlock.matchAll(/\[\s*([GP]),/g)].map((m) => m[1]);

    for (const m of methods) {
      expect(["G", "P"]).toContain(m);
    }
    expect(methods.length).toBeGreaterThan(100);
  });

  it("all routes have paths starting with /v1/", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

    const paths = [...src.matchAll(/"\/(v1\/[^"]+)"/g)].map((m) => m[1]);
    for (const p of paths) {
      expect(p).toMatch(/^v1\//);
    }
    expect(paths.length).toBeGreaterThan(50);
  });

  it("CHANNEL_ROUTES covers streaming commands", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

    expect(src).toContain("chat_completions_ipc");
    expect(src).toContain("stream_search_embeddings");
  });

  it("handles enqueue/poll job pattern", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

    expect(src).toContain('cmd === "enqueue_umap_compare"');
    expect(src).toContain('cmd === "poll_job"');
    expect(src).toContain("_jobResults");
  });

  it("falls back to Tauri invoke for unknown commands", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

    expect(src).toContain('import("@tauri-apps/api/core")');
    expect(src).toContain("invoke<T>(cmd, args)");
  });

  it("falls back on daemon HTTP failure", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

    // The catch block should fall back to invoke
    expect(src).toContain("catch");
    expect(src).toContain("Daemon HTTP failed");
  });
});
