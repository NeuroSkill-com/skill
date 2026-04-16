// SPDX-License-Identifier: GPL-3.0-only

import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { hasDaemonBinary, type IsolatedDaemon, spawnDaemon } from "./e2e-helpers";

const canRun = hasDaemonBinary();

let bootstrap = {
  port: 0,
  token: "",
  compatible_protocol: true,
  daemon_version: "",
  protocol_version: 1,
};

const tauriInvoke = vi.fn(async (cmd: string) => {
  if (cmd === "get_daemon_bootstrap") return bootstrap;
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauriInvoke }));

describe.skipIf(!canRun)("tauri runtime e2e via real daemon", () => {
  let d: IsolatedDaemon;

  beforeAll(async () => {
    d = await spawnDaemon();
    bootstrap = { ...bootstrap, port: d.port, token: d.token };
  }, 30_000);

  afterAll(async () => {
    await d?.stop();
  });

  it("http.ts can talk to real daemon", async () => {
    const { ensureDaemonCompatible, daemonGet, daemonPost } = await import("../lib/daemon/http");

    await ensureDaemonCompatible();
    const version = await daemonGet<{ daemon: string; protocol_version: number }>("/v1/version");
    expect(version.daemon).toBe("skill-daemon");
    expect(typeof version.protocol_version).toBe("number");

    const status = await daemonGet<{ state: string }>("/v1/status");
    expect(typeof status.state).toBe("string");

    const wsClients = await daemonGet<unknown[]>("/v1/ws-clients");
    expect(Array.isArray(wsClients)).toBe(true);

    const wsLog = await daemonGet<unknown[]>("/v1/ws-request-log");
    expect(Array.isArray(wsLog)).toBe(true);

    const dnd = await daemonPost<{ ok: boolean; value: boolean }>("/v1/settings/dnd/test", { enabled: false });
    expect(typeof dnd.ok).toBe("boolean");
  });

  it("schema snapshots for critical daemon payload keys", async () => {
    const { daemonGet } = await import("../lib/daemon/http");

    const version = await daemonGet<Record<string, unknown>>("/v1/version");
    expect(Object.keys(version).sort()).toMatchInlineSnapshot(`
      [
        "daemon",
        "daemon_version",
        "protocol_version",
      ]
    `);

    const status = await daemonGet<Record<string, unknown>>("/v1/status");
    expect(Object.keys(status).sort()).toMatchInlineSnapshot(`
      [
        "battery",
        "channel_names",
        "channel_quality",
        "csv_path",
        "device_error",
        "device_id",
        "device_kind",
        "device_name",
        "eeg_channel_count",
        "eeg_sample_rate_hz",
        "firmware_version",
        "fnirs_channel_names",
        "hardware_version",
        "has_central_electrodes",
        "has_full_montage",
        "has_imu",
        "has_ppg",
        "imu_channel_names",
        "iroh_client_name",
        "iroh_connected_peers",
        "iroh_eeg_streaming_active",
        "iroh_remote_device_connected",
        "iroh_streaming_active",
        "iroh_tunnel_online",
        "mac_address",
        "paired_devices",
        "phone_info",
        "ppg_channel_names",
        "ppg_sample_count",
        "retry_attempt",
        "retry_countdown_secs",
        "sample_count",
        "serial_number",
        "state",
        "target_display_name",
        "target_id",
        "target_name",
      ]
    `);
  });

  it("invoke-proxy routes to real daemon endpoints", async () => {
    const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");

    const status = await daemonInvoke<{ state: string }>("get_status");
    expect(typeof status.state).toBe("string");

    const hooks = await daemonInvoke<unknown[]>("get_hooks");
    expect(Array.isArray(hooks)).toBe(true);

    const sessions = await daemonInvoke<unknown[]>("list_sessions");
    expect(Array.isArray(sessions)).toBe(true);

    const port = await daemonInvoke<{ port: number }>("get_ws_port");
    expect(typeof port.port).toBe("number");
  }, 15_000);
});
