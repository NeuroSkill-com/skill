// SPDX-License-Identifier: GPL-3.0-only

import { beforeEach, describe, expect, it, vi } from "vitest";

const daemonGet = vi.fn();
const daemonPost = vi.fn();
const tauriInvoke = vi.fn();

vi.mock("../lib/daemon/http", () => ({
  daemonGet,
  daemonPost,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriInvoke,
}));

describe("daemonInvoke runtime contract", () => {
  beforeEach(() => {
    daemonGet.mockReset();
    daemonPost.mockReset();
    tauriInvoke.mockReset();
  });

  it("routes known GET command to daemonGet with exact path", async () => {
    daemonGet.mockResolvedValue({ state: "connected" });
    const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");

    const v = await daemonInvoke<{ state: string }>("get_status");
    expect(v.state).toBe("connected");
    expect(daemonGet).toHaveBeenCalledWith("/v1/status");
    expect(daemonPost).not.toHaveBeenCalled();
    expect(tauriInvoke).not.toHaveBeenCalled();
  });

  it("routes known POST command to daemonPost with exact path+body", async () => {
    daemonPost.mockResolvedValue({ ok: true });
    const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");

    const req = { target: "muse" };
    const out = await daemonInvoke<{ ok: boolean }>("switch_session", req);
    expect(out.ok).toBe(true);
    expect(daemonPost).toHaveBeenCalledWith("/v1/control/switch-session", req);
    expect(daemonGet).not.toHaveBeenCalled();
  });

  it("falls back to Tauri invoke when daemon HTTP fails", async () => {
    daemonPost.mockRejectedValue(new Error("daemon down"));
    tauriInvoke.mockResolvedValue({ ok: true, via: "tauri" });
    const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");

    const out = await daemonInvoke<{ ok: boolean; via: string }>("cancel_session", { a: 1 });
    expect(out).toEqual({ ok: true, via: "tauri" });
    expect(tauriInvoke).toHaveBeenCalledWith("cancel_session", { a: 1 });
  });

  it("unknown command always uses Tauri invoke", async () => {
    tauriInvoke.mockResolvedValue({ ok: 1 });
    const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");

    const out = await daemonInvoke<{ ok: number }>("some_unknown_cmd", { x: 2 });
    expect(out.ok).toBe(1);
    expect(daemonGet).not.toHaveBeenCalled();
    expect(daemonPost).not.toHaveBeenCalled();
    expect(tauriInvoke).toHaveBeenCalledWith("some_unknown_cmd", { x: 2 });
  });

  it("channel command emits delta+done messages", async () => {
    daemonPost.mockResolvedValue({
      content: "hello",
      finish_reason: "stop",
      prompt_tokens: 1,
      completion_tokens: 2,
      n_ctx: 3,
    });
    const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");

    const messages: unknown[] = [];
    const channel = { onmessage: (m: unknown) => messages.push(m) };
    await daemonInvoke<void>("chat_completions_ipc", { channel, messages: [], params: {} });

    expect(daemonPost).toHaveBeenCalledWith("/v1/llm/chat-completions", { messages: [], params: {} });
    expect(messages).toEqual([
      { type: "delta", content: "hello" },
      { type: "done", finish_reason: "stop", prompt_tokens: 1, completion_tokens: 2, n_ctx: 3 },
    ]);
  });
});
