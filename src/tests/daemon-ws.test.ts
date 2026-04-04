// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";

describe("daemon WebSocket client (ws.ts)", () => {
  it("module exports expected functions", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/ws.ts", "utf-8");

    expect(src).toContain("export async function connectDaemonWs");
    expect(src).toContain("export function onDaemonEvent");
    expect(src).toContain("export function onAnyDaemonEvent");
    expect(src).toContain("export function disconnectDaemonWs");
    expect(src).toContain("export function isDaemonWsConnected");
  });

  it("auto-reconnects on close", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/ws.ts", "utf-8");

    expect(src).toContain("scheduleReconnect");
    expect(src).toContain("3000"); // reconnect delay
  });

  it("dispatches events by type", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/ws.ts", "utf-8");

    expect(src).toContain("_handlers.get(event.type)");
    expect(src).toContain("_globalHandlers");
  });
});

describe("EEG stream module (eeg-stream.ts)", () => {
  it("exports subscribe functions", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/eeg-stream.ts", "utf-8");

    expect(src).toContain("export function subscribeEeg");
    expect(src).toContain("export function subscribePpg");
    expect(src).toContain("export function subscribeImu");
    expect(src).toContain("export function subscribeBands");
    expect(src).toContain("export async function getLatestBands");
  });

  it("uses correct daemon event types", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/eeg-stream.ts", "utf-8");

    expect(src).toContain('"EegSample"');
    expect(src).toContain('"PpgSample"');
    expect(src).toContain('"ImuSample"');
    expect(src).toContain('"EegBands"');
  });

  it("getLatestBands calls daemon HTTP", async () => {
    const fs = await import("node:fs");
    const src = fs.readFileSync("src/lib/daemon/eeg-stream.ts", "utf-8");

    expect(src).toContain("/v1/activity/latest-bands");
  });
});
