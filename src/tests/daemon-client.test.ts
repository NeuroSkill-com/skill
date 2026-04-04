// SPDX-License-Identifier: GPL-3.0-only

import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";

describe("daemon client barrel (client.ts)", () => {
  const src = fs.readFileSync("src/lib/daemon/client.ts", "utf-8");

  it("re-exports all domain modules", () => {
    const expectedModules = ["skills", "lsl", "devices", "settings", "tools", "chat", "misc", "eeg-stream", "ws"];
    for (const mod of expectedModules) {
      expect(src).toContain(`from "./${mod}"`);
    }
  });

  it("does not re-export invoke-proxy or http (internal)", () => {
    expect(src).not.toContain("invoke-proxy");
    expect(src).not.toContain("./http");
  });
});

describe("daemon client layer file inventory", () => {
  const daemonDir = "src/lib/daemon";
  const files = fs
    .readdirSync(daemonDir)
    .filter((f: string) => f.endsWith(".ts"))
    .sort();

  it("has expected files", () => {
    expect(files).toContain("client.ts");
    expect(files).toContain("http.ts");
    expect(files).toContain("invoke-proxy.ts");
    expect(files).toContain("ws.ts");
    expect(files).toContain("eeg-stream.ts");
  });

  it("has no unexpectedly large files", () => {
    for (const f of files) {
      const lines = fs.readFileSync(path.join(daemonDir, f), "utf-8").split("\n").length;
      // invoke-proxy is the biggest at ~270 lines; nothing should exceed 300
      expect(lines).toBeLessThan(300);
    }
  });

  it("all files have license header", () => {
    for (const f of files) {
      const content = fs.readFileSync(path.join(daemonDir, f), "utf-8");
      expect(content).toContain("SPDX-License-Identifier");
    }
  });
});

describe("guard script coverage", () => {
  const guard = fs.readFileSync("scripts/check-daemon-invokes.js", "utf-8");
  const proxy = fs.readFileSync("src/lib/daemon/invoke-proxy.ts", "utf-8");

  it("guard blocks all proxy-routed commands", () => {
    // Extract command names from proxy ROUTES
    const routeKeys = [...proxy.matchAll(/^\s+(\w+):\s*\[/gm)].map((m) => m[1]);
    // Also special-cased commands
    const special = ["chat_completions_ipc", "stream_search_embeddings", "enqueue_umap_compare", "poll_job"];
    const allCmds = [...routeKeys, ...special];

    const missing: string[] = [];
    for (const cmd of allCmds) {
      if (!guard.includes(`"${cmd}"`)) {
        missing.push(cmd);
      }
    }
    expect(missing).toEqual([]);
  });

  it("guard regex excludes daemonInvoke calls", () => {
    expect(guard).toContain("(?<!daemon)invoke");
  });
});
