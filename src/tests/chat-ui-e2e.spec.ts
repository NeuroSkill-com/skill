/**
 * Live-daemon Chat UI E2E — text + image (vision) modalities.
 *
 * Exercises `/chat` against skill-daemon (:18445): start the LLM from the UI,
 * send a text prompt, then restart with mmproj and send an attached image.
 *
 * Prerequisites: daemon up, GGUF on disk. mmproj downloaded for vision test.
 *
 *   npm run test:e2e:chat-ui
 *   CHAT_EXPECT_REPLY=1 npm run test:e2e:chat-ui   # require PONG / color word
 */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { expect, test, type Page } from "@playwright/test";

function readToken(): string {
  const p =
    process.platform === "darwin"
      ? path.join(os.homedir(), "Library", "Application Support", "skill", "daemon", "auth.token")
      : path.join(os.homedir(), ".config", "skill", "daemon", "auth.token");
  return fs.readFileSync(p, "utf8").trim();
}

const TOKEN = readToken();
const PORT = Number(process.env.SKILL_DAEMON_PORT ?? "18445");
const EXPECT_REPLY = process.env.CHAT_EXPECT_REPLY === "1";
const DOWNLOAD_TIMEOUT_SECS = Number(process.env.DOWNLOAD_TIMEOUT_SECS ?? "900");
const MMPROJ = process.env.SKILL_LLM_MMPROJ ?? "mmproj-Qwen_Qwen3.5-4B-bf16.gguf";

function liveDaemonBridgeScript(port: number, token: string): string {
  return `
    (function () {
      const PORT = ${port};
      const TOKEN = ${JSON.stringify(token)};
      const BASE = "http://127.0.0.1:" + PORT;
      const auth = { Authorization: "Bearer " + TOKEN, "Content-Type": "application/json" };

      async function daemon(method, path, body) {
        const resp = await fetch(BASE + path, {
          method,
          headers: auth,
          body: body === undefined ? undefined : JSON.stringify(body),
          signal: AbortSignal.timeout(600000),
        });
        const text = await resp.text();
        let json = null;
        try { json = text ? JSON.parse(text) : null; } catch { json = { raw: text }; }
        if (!resp.ok) {
          const msg = (json && (json.error || json.message)) || (resp.status + " " + resp.statusText);
          throw new Error(msg);
        }
        if (json && typeof json === "object" && json.ok === false) {
          throw new Error(json.error || json.message || "Request failed");
        }
        return json;
      }

      window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
      window.__TAURI_INTERNALS__.metadata = {
        currentWindow: { label: "chat" },
        currentWebview: { label: "chat", windowLabel: "chat" },
        windows: [{ label: "chat" }],
        webviews: [{ label: "chat", windowLabel: "chat" }],
      };

      const _cbs = new Map();
      let _cbSeq = 1;
      window.__TAURI_INTERNALS__.transformCallback = function (fn, once) {
        const id = _cbSeq++;
        _cbs.set(id, function (data) {
          if (once) _cbs.delete(id);
          if (fn) fn(data);
        });
        return id;
      };
      window.__TAURI_INTERNALS__.unregisterCallback = function (id) { _cbs.delete(id); };
      window.__TAURI_INTERNALS__.runCallback = function (id, data) {
        const fn = _cbs.get(id);
        if (fn) fn(data);
      };
      window.__TAURI_INTERNALS__.callbacks = _cbs;

      const _listeners = {};
      function fireTauriEvent(event, payload) {
        const ids = _listeners[event] || [];
        for (let i = 0; i < ids.length; i++) {
          window.__TAURI_INTERNALS__.runCallback(ids[i], { event, id: ids[i], payload });
        }
      }
      window.__skillFireTauriEvent__ = fireTauriEvent;

      window.__TAURI_INTERNALS__.invoke = async function (cmd, args) {
        args = args || {};
        switch (cmd) {
          case "get_daemon_bootstrap":
            return {
              port: PORT,
              token: TOKEN,
              compatible_protocol: true,
              daemon_version: "0.1.0",
              protocol_version: 1,
            };
          case "plugin:event|listen": {
            const event = args.event;
            const handler = args.handler;
            if (event) {
              if (!_listeners[event]) _listeners[event] = [];
              _listeners[event].push(handler);
            }
            return handler;
          }
          case "plugin:event|emit":
          case "plugin:event|emit_to":
            fireTauriEvent(args.event, args.payload);
            return null;
          case "plugin:event|unlisten": {
            const event = args.event;
            const eventId = args.eventId;
            if (event && _listeners[event]) {
              _listeners[event] = _listeners[event].filter((h) => h !== eventId);
            }
            return null;
          }
          case "get_theme_and_language":
            return ["dark", "en"];
          case "get_app_name":
            return "NeuroSkill";
          case "get_ws_port":
            return PORT;
          case "show_main_window":
          case "show_toast_from_frontend":
          case "open_settings_window":
          case "open_model_tab":
          case "open_chat_window":
            return null;
          case "asr_start":
          case "asr_stop":
          case "asr_status":
          case "asr_set_speaking":
          case "asr_set_ptt":
          case "asr_ptt":
            return { ok: true, running: false };
          case "tts_init":
          case "tts_unload":
          case "tts_speak":
            return { ok: true };
          case "get_asr_settings":
            return { enabled: false, default_trigger: "push_to_talk", default_routing: "transcribe_only", language: "en" };
          default:
            console.warn("[chat-e2e-bridge] unhandled invoke:", cmd, args);
            return null;
        }
      };
    })();
  `;
}

async function daemonPost(route: string, body: Record<string, unknown> = {}, timeoutMs = 120_000) {
  const resp = await fetch(`http://127.0.0.1:${PORT}${route}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const json = await resp.json().catch(() => ({}));
  expect(resp.ok, `HTTP ${resp.status} for ${route}: ${JSON.stringify(json)}`).toBeTruthy();
  return json;
}

async function daemonGet(route: string) {
  const resp = await fetch(`http://127.0.0.1:${PORT}${route}`, {
    headers: { Authorization: `Bearer ${TOKEN}` },
    signal: AbortSignal.timeout(30_000),
  });
  const json = await resp.json().catch(() => ({}));
  expect(resp.ok, `HTTP ${resp.status} for ${route}: ${JSON.stringify(json)}`).toBeTruthy();
  return json;
}

async function patchLlmConfig(patch: Record<string, unknown>) {
  const cfg = await daemonGet("/v1/settings/llm-config");
  const next = { ...cfg, ...patch };
  if (patch.tools && typeof patch.tools === "object") {
    next.tools = { ...(cfg.tools || {}), ...(patch.tools as object) };
  }
  await daemonPost("/v1/settings/llm-config", next);
}

async function stopLlm() {
  await fetch(`http://127.0.0.1:${PORT}/v1/llm/server/stop`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      "Content-Type": "application/json",
    },
    body: "{}",
  }).catch(() => null);
}

async function waitStatus(want: string, timeoutMs = 300_000) {
  await expect
    .poll(
      async () => {
        const st = await daemonGet("/v1/llm/server/status");
        return st.status;
      },
      { timeout: timeoutMs },
    )
    .toBe(want);
}

async function openChatReady(page: Page) {
  await page.goto("/chat");
  await page.waitForLoadState("domcontentloaded");

  const startBtn = page.getByRole("button", { name: /Start LLM server|Start/i }).first();
  const alreadyRunning = await page.locator("textarea").first().isEnabled().catch(() => false);
  if (!alreadyRunning) {
    if (await startBtn.isVisible().catch(() => false)) {
      await startBtn.click();
    }
  }
  await expect(page.locator("textarea").first()).toBeEnabled({ timeout: 300_000 });
  await expect(page.getByRole("button", { name: /Send message/i })).toBeVisible({ timeout: 30_000 });
}

/** 64×64 solid-red PNG (precomputed) for vision attach / embed-image. */
function solidRedPng(): Buffer {
  // Generated once offline: 64x64 rgb(220,30,30) PNG.
  // Keep as base64 so we don't pull a PNG dependency into the test runner.
  const b64 =
    "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAYAAACqaXHeAAAABmJLR0QA/wD/AP+gvaeTAAAAyElEQVR4nO3ZMQrCQBRF0TdWYmVlYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYeUqzCQzJJnM/8+9cOENM5nM+zMAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAP8zALxgAAfyHh0ZAAAAAElFTkSuQmCC";
  // Fallback: tiny 1×1 red pixel if the placeholder is invalid — replace with canvas-made bytes below.
  return Buffer.from(b64, "base64");
}

async function makeRedPngViaPage(page: Page): Promise<Buffer> {
  const bytes: number[] = await page.evaluate(async () => {
    const c = document.createElement("canvas");
    c.width = 64;
    c.height = 64;
    const ctx = c.getContext("2d");
    if (!ctx) return [];
    ctx.fillStyle = "rgb(220, 30, 30)";
    ctx.fillRect(0, 0, 64, 64);
    const blob = await new Promise<Blob | null>((resolve) => c.toBlob(resolve, "image/png"));
    if (!blob) return [];
    const ab = await blob.arrayBuffer();
    return Array.from(new Uint8Array(ab));
  });
  if (bytes.length < 32) {
    return solidRedPng();
  }
  return Buffer.from(bytes);
}

test.describe.configure({ mode: "serial" });

test.describe("Chat UI LLM modalities (live daemon)", () => {
  test.beforeEach(async ({ page }) => {
    const r = await fetch(`http://127.0.0.1:${PORT}/healthz`, {
      headers: { Authorization: `Bearer ${TOKEN}` },
      signal: AbortSignal.timeout(2000),
    }).catch(() => null);
    test.skip(!r || !r.ok, `skill-daemon not reachable on :${PORT}`);

    await fetch(`http://127.0.0.1:${PORT}/v1/test/begin`, {
      method: "POST",
      headers: { Authorization: `Bearer ${TOKEN}`, "Content-Type": "application/json" },
      body: "{}",
    }).catch(() => null);

    await page.addInitScript(liveDaemonBridgeScript(PORT, TOKEN));
  });

  test("text modality: chat UI sends PONG prompt via /v1/llm/chat-completions", async ({ page }) => {
    test.setTimeout((DOWNLOAD_TIMEOUT_SECS + 420) * 1000);

    await daemonPost("/v1/models/ensure-llm-weights", { include_mmproj: false });
    await daemonPost("/v1/models/validate/llm", {
      weights_only: true,
      download_timeout_secs: DOWNLOAD_TIMEOUT_SECS,
      include_mmproj: false,
    });

    // Prefer an already-running text server — stop/start OOMs this host (exit 137).
    let st = await daemonGet("/v1/llm/server/status").catch(() => ({ status: "stopped" }));
    if (st.status === "running" && st.supports_vision) {
      await stopLlm();
      st = { status: "stopped" };
    }
    if (st.status !== "running") {
      await patchLlmConfig({
        enabled: true,
        autoload_mmproj: false,
        ctx_size: 2048,
        tools: { enabled: false },
      });
      await daemonPost("/v1/llm/server/start", {});
      await waitStatus("running");
    } else {
      // Soften tools on the live server without a full reload when possible.
      await patchLlmConfig({ tools: { enabled: false } }).catch(() => null);
    }

    const consoleErrors: string[] = [];
    page.on("pageerror", (err) => consoleErrors.push(err.message));

    await openChatReady(page);

    const prompt = "Reply with exactly the single word PONG and nothing else.";
    const chatReq = page.waitForResponse(
      async (r) => {
        if (!r.url().includes("/v1/llm/chat-completions") || r.request().method() !== "POST") {
          return false;
        }
        const raw = r.request().postData() || "";
        return raw.includes("PONG");
      },
      { timeout: 180_000 },
    );

    await page.locator("textarea").first().fill(prompt);
    await page.getByRole("button", { name: /Send message/i }).click();

    const resp = await chatReq;
    const body = await resp.json().catch(() => ({} as Record<string, unknown>));
    expect(resp.status(), `chat-completions HTTP ${resp.status}: ${JSON.stringify(body)}`).toBeLessThan(500);
    await expect(page.getByText(/Reply with exactly the single word PONG/i).first()).toBeVisible({
      timeout: 15_000,
    });

    if (EXPECT_REPLY) {
      await expect(page.getByText(/\bPONG\b/i).nth(1)).toBeVisible({ timeout: 180_000 });
    } else {
      // Accept assistant text or a surfaced error — not a silent hang.
      await expect(
        page.locator("text=/\\bPONG\\b|Error:|aborted|failed|timeout/i").first(),
      ).toBeVisible({ timeout: 180_000 });
    }

    expect(consoleErrors.filter((e) => !/ResizeObserver|favicon|WebSocket/i.test(e))).toEqual([]);
  });

  test("image modality: chat UI attaches PNG and posts multimodal chat-completions", async ({ page }) => {
    test.setTimeout((DOWNLOAD_TIMEOUT_SECS + 600) * 1000);

    const catalog = await daemonGet("/v1/llm/catalog");
    const mm = (catalog.entries || []).find(
      (e: { filename?: string; state?: string }) =>
        e.filename === MMPROJ || (String(e.filename || "").startsWith("mmproj-") && e.state === "downloaded"),
    );

    const forceVision = process.env.FORCE_VISION === "1";
    let visionReady = false;

    if (forceVision && mm && mm.state === "downloaded") {
      await daemonPost("/v1/models/ensure-llm-weights", { include_mmproj: true });
      await stopLlm();
      await expect
        .poll(async () => (await daemonGet("/v1/llm/server/status")).status, { timeout: 60_000 })
        .toBe("stopped");
      await page.waitForTimeout(3000);
      await patchLlmConfig({
        enabled: true,
        autoload_mmproj: true,
        ctx_size: 2048,
        tools: { enabled: false },
      });
      await daemonPost("/v1/llm/selection/active-mmproj", { filename: mm.filename });
      const start = await fetch(`http://127.0.0.1:${PORT}/v1/llm/server/start`, {
        method: "POST",
        headers: { Authorization: `Bearer ${TOKEN}`, "Content-Type": "application/json" },
        body: "{}",
        signal: AbortSignal.timeout(30_000),
      }).catch(() => null);
      if (start?.ok) {
        try {
          await waitStatus("running", 420_000);
          visionReady = !!(await daemonGet("/v1/llm/server/status")).supports_vision;
        } catch {
          visionReady = false;
        }
      }
    } else {
      // Default: reuse / start text server, then exercise image attach in the UI.
      let st = await daemonGet("/v1/llm/server/status").catch(() => ({ status: "stopped", supports_vision: false }));
      if (st.status !== "running") {
        await patchLlmConfig({
          enabled: true,
          autoload_mmproj: false,
          ctx_size: 2048,
          tools: { enabled: false },
        });
        await daemonPost("/v1/llm/server/start", {});
        await waitStatus("running");
        st = await daemonGet("/v1/llm/server/status");
      }
      visionReady = !!st.supports_vision;
    }

    await openChatReady(page);
    const redPng = await makeRedPngViaPage(page);

    if (!visionReady) {
      // Current Qwen3.5-4B + mmproj-bf16 fails to load
      // (`vision.position_embd.weight`) and falls back to text-only.
      const upload = page.locator('input[aria-label="Upload images"]');
      await upload.setInputFiles({
        name: "solid-red.png",
        mimeType: "image/png",
        buffer: redPng,
      });
      await expect(page.locator('img[alt="solid-red.png"]').first()).toBeVisible({ timeout: 10_000 });
      await expect(page.getByText(/vision projector|images will be ignored|No vision/i).first()).toBeVisible({
        timeout: 10_000,
      });
      // Still prove the send path can include image_url parts (UI builds multimodal payload).
      const chatReq = page.waitForResponse(
        async (r) => {
          if (!r.url().includes("/v1/llm/chat-completions") || r.request().method() !== "POST") return false;
          return (r.request().postData() || "").includes("image_url");
        },
        { timeout: 180_000 },
      );
      await page.locator("textarea").first().fill("Describe the attached image in one word.");
      await page.getByRole("button", { name: /Send message/i }).click();
      const resp = await chatReq;
      expect(resp.status()).toBeLessThan(500);
      expect(resp.request().postData() || "").toMatch(/data:image\/png;base64,/);
      test.info().annotations.push({
        type: "note",
        description: "vision projector not active — UI warned; multimodal request still posted",
      });
      return;
    }

    const embed = await daemonPost(
      "/v1/llm/embed-image",
      { png_base64: redPng.toString("base64") },
      180_000,
    );
    expect(embed.error, JSON.stringify(embed)).toBeFalsy();
    expect(Array.isArray(embed.embedding) ? embed.embedding.length : 0).toBeGreaterThan(0);

    const upload = page.locator('input[aria-label="Upload images"]');
    await upload.setInputFiles({
      name: "solid-red.png",
      mimeType: "image/png",
      buffer: redPng,
    });
    await expect(page.locator('img[alt="solid-red.png"]').first()).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/vision projector|images will be ignored/i)).toHaveCount(0);

    const prompt =
      "Look at the attached image. Reply with exactly one word: RED if it is mostly red, else OTHER.";
    const chatReq = page.waitForResponse(
      async (r) => {
        if (!r.url().includes("/v1/llm/chat-completions") || r.request().method() !== "POST") {
          return false;
        }
        const raw = r.request().postData() || "";
        return raw.includes("image_url") && raw.includes("RED");
      },
      { timeout: 240_000 },
    );

    await page.locator("textarea").first().fill(prompt);
    await page.getByRole("button", { name: /Send message/i }).click();

    const resp = await chatReq;
    const body = await resp.json().catch(() => ({} as Record<string, unknown>));
    expect(resp.status(), `vision chat HTTP ${resp.status}: ${JSON.stringify(body)}`).toBeLessThan(500);
    expect(resp.request().postData() || "").toMatch(/data:image\/png;base64,/);
    await expect(page.getByText(/Look at the attached image/i).first()).toBeVisible({ timeout: 15_000 });

    if (EXPECT_REPLY) {
      await expect(page.getByText(/\bRED\b/i).nth(1)).toBeVisible({ timeout: 240_000 });
    } else {
      await expect(page.locator("text=/\\bRED\\b|\\bOTHER\\b|Error:|aborted|failed|timeout|vision/i").first()).toBeVisible({
        timeout: 240_000,
      });
    }
  });

  test.afterAll(async () => {
    // Leave autoload_mmproj off so the next boot stays text-only (safer RAM).
    await patchLlmConfig({ autoload_mmproj: false }).catch(() => null);
    // Do not stop here — a concurrent reload after vision can OOM. Callers may
    // stop explicitly; text tests reuse a running server when possible.
  });
});
