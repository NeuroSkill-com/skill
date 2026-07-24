/**
 * Live-daemon UI E2E for Settings controls that load/run RLX:
 * Voice (TTS), Embeddings (rlx-embed + EEG reembed), Screenshots (vision),
 * EXG (encoder reembed), LLM Start.
 *
 * Talks to the *real* skill-daemon (default :18445) started by `npm run tauri --
 * dev`. Mocks only the Tauri IPC bootstrap + a few Tauri-only TTS/ASR commands,
 * bridging them to daemon HTTP so the Svelte UI exercises the same paths the
 * packaged app uses.
 *
 * Pair with `models-validate-api.spec.ts` for Rust-engine proofs
 * (`/v1/models/validate*`).
 *
 * Run (daemon must already be up):
 *   npm run test:e2e:models-ui
 *   npm run test:e2e:models-ui:headed
 */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { expect, type Page, test } from "@playwright/test";

function readToken(): string {
  const p =
    process.platform === "darwin"
      ? path.join(os.homedir(), "Library", "Application Support", "skill", "daemon", "auth.token")
      : path.join(os.homedir(), ".config", "skill", "daemon", "auth.token");
  return fs.readFileSync(p, "utf8").trim();
}

const TOKEN = readToken();
const PORT = Number(process.env.SKILL_DAEMON_PORT ?? "18445");

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

      // ── Tauri event bus (enough for listen/emit used by TTS/ASR UIs) ──────
      window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
      window.__TAURI_INTERNALS__.metadata = {
        currentWindow: { label: "main" },
        currentWebview: { label: "main", windowLabel: "main" },
        windows: [{ label: "main" }],
        webviews: [{ label: "main", windowLabel: "main" }],
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
      window.__TAURI_INTERNALS__.unregisterCallback = function (id) {
        _cbs.delete(id);
      };
      window.__TAURI_INTERNALS__.runCallback = function (id, data) {
        const fn = _cbs.get(id);
        if (fn) fn(data);
      };
      window.__TAURI_INTERNALS__.callbacks = _cbs;

      // listen() registers via plugin:event|listen → we emulate emit/listen.
      const _listeners = {};
      function fireTauriEvent(event, payload) {
        const ids = _listeners[event] || [];
        for (let i = 0; i < ids.length; i++) {
          window.__TAURI_INTERNALS__.runCallback(ids[i], {
            event,
            id: ids[i],
            payload,
          });
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

          case "get_tts_engine":
            return await daemon("GET", "/v1/settings/tts-engine");
          case "set_tts_engine": {
            const cfg = args.config || args;
            await daemon("POST", "/v1/settings/tts-engine", {
              engine: cfg.engine || "",
              model: cfg.model || "",
              voice: cfg.voice || "",
            });
            // Daemon warms in background; tell the UI the engine is ready.
            setTimeout(() => fireTauriEvent("tts-progress", {
              phase: "ready", step: 1, total: 1, label: "ready",
            }), 50);
            return { ok: true };
          }
          case "get_tts_engines":
            return await daemon("GET", "/v1/tts/engines");
          case "get_asr_engines":
            return await daemon("GET", "/v1/asr/engines");
          case "tts_init":
            setTimeout(() => fireTauriEvent("tts-progress", {
              phase: "ready", step: 1, total: 1, label: "ready",
            }), 30);
            return null;
          case "tts_unload":
            setTimeout(() => fireTauriEvent("tts-progress", {
              phase: "unloaded", step: 0, total: 0, label: "",
            }), 10);
            return null;
          case "tts_speak": {
            const text = args.text || "";
            const res = await daemon("POST", "/v1/say", { text });
            return res;
          }
          case "tts_list_voices": {
            const eng = await daemon("GET", "/v1/settings/tts-engine");
            return eng.voices || ["Jasper"];
          }
          case "tts_get_voice": {
            const eng = await daemon("GET", "/v1/settings/tts-engine");
            return eng.voice || "Jasper";
          }
          case "tts_set_voice":
            return null;
          case "tts_list_neutts_voices":
            return [];

          case "get_log_config":
            return {
              embedder: true,
              bluetooth: true,
              scanner: true,
              websocket: false,
              csv: false,
              filter: false,
              bands: false,
              tts: false,
              llm: false,
              chat_store: false,
              history: false,
              hooks: false,
              tools: false,
            };
          case "set_log_config":
            return null;

          case "asr_start": {
            const body = {
              trigger: args.trigger,
              routing: args.routing,
              language: args.language,
            };
            return await daemon("POST", "/v1/asr/start", body);
          }
          case "asr_stop":
            return await daemon("POST", "/v1/asr/stop", {});
          case "asr_status":
            return await daemon("GET", "/v1/asr/status");
          case "asr_set_speaking":
            return await daemon("POST", "/v1/asr/speaking", { active: !!args.active });
          case "asr_ptt":
            return await daemon("POST", "/v1/asr/ptt", { active: !!args.active });

          case "open_chat_window":
          case "open_downloads_window":
            return null;

          default:
            // Unknown Tauri command — soft-fail so pages don't crash.
            console.warn("[e2e-bridge] unhandled invoke:", cmd, args);
            return null;
        }
      };
    })();
  `;
}

async function openSettingsTab(page: Page, label: string) {
  await page.goto("/settings");
  await page.waitForLoadState("domcontentloaded");
  const tab = page.getByRole("tab", { name: label });
  await tab.click();
  await expect(page.getByRole("tabpanel")).toBeVisible({ timeout: 15_000 });
}

test.describe.configure({ mode: "serial" });

test.describe("Models UI E2E (live daemon)", () => {
  test.beforeEach(async ({ page }) => {
    // Fail fast if daemon is down.
    const r = await fetch(`http://127.0.0.1:${PORT}/healthz`, {
      headers: { Authorization: `Bearer ${TOKEN}` },
      signal: AbortSignal.timeout(2000),
    }).catch(() => null);
    test.skip(!r?.ok, `skill-daemon not reachable on :${PORT}`);

    // Pause idle re-embed / scanner so Settings clicks don't OOM the host.
    await fetch(`http://127.0.0.1:${PORT}/v1/test/begin`, {
      method: "POST",
      headers: { Authorization: `Bearer ${TOKEN}`, "Content-Type": "application/json" },
      body: "{}",
    }).catch(() => null);

    await page.addInitScript(liveDaemonBridgeScript(PORT, TOKEN));
  });

  test("Voice tab: shows Inflect-Nano, preload Ready, speak via /v1/say", async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("pageerror", (err) => consoleErrors.push(err.message));

    await openSettingsTab(page, "Voice");

    // Engine chip for Inflect-Nano should be selected (settings.tts_engine).
    const inflect = page.getByRole("button", { name: "Inflect-Nano" });
    await expect(inflect).toBeVisible({ timeout: 15_000 });
    await expect(inflect).toHaveClass(/border-indigo-500/);

    // Kick preload — bridge fires tts-progress ready.
    const preload = page.getByRole("button", { name: /Preload|Retry/i });
    await preload.click();
    await expect(page.getByText("Ready", { exact: true }).first()).toBeVisible({ timeout: 10_000 });

    // Speak through the test widget (bridged to POST /v1/say).
    const sayReq = page.waitForResponse((r) => r.url().includes("/v1/say") && r.request().method() === "POST", {
      timeout: 20_000,
    });
    // Prefer the Speak button — sample chips can race with "speaking" lock.
    const speakBtn = page.getByRole("button", { name: /^Speak$/i });
    await expect(speakBtn).toBeEnabled({ timeout: 5_000 });
    await speakBtn.click();
    const resp = await sayReq;
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.ok).toBeTruthy();
    expect(String(body.spoken || "").length).toBeGreaterThan(0);
    expect(consoleErrors.filter((e) => !/ResizeObserver|favicon/i.test(e))).toEqual([]);
  });

  test("LLM tab: Start reaches running when GGUF is downloaded", async ({ page }) => {
    test.setTimeout(420_000);

    const ensure = await fetch(`http://127.0.0.1:${PORT}/v1/models/ensure-llm-weights`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${TOKEN}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ include_mmproj: false }),
    }).then((r) => r.json());
    expect(ensure.ok, JSON.stringify(ensure)).toBeTruthy();

    await openSettingsTab(page, "LLM");
    await expect(page.getByText(/\.gguf/i).first()).toBeVisible({ timeout: 20_000 });

    const start = page.getByRole("button", { name: /^Start$/i });
    await expect(start).toBeVisible({ timeout: 15_000 });

    if (await start.isDisabled()) {
      await expect(page.getByText(/is not downloaded yet/i).first()).toBeVisible();
      test.skip(true, "active GGUF not downloaded yet");
      return;
    }

    const startReq = page.waitForResponse(
      (r) => r.url().includes("/v1/llm/server/start") && r.request().method() === "POST",
      { timeout: 30_000 },
    );
    await start.click();
    expect((await startReq).ok()).toBeTruthy();

    await expect
      .poll(
        async () => {
          const st = await fetch(`http://127.0.0.1:${PORT}/v1/llm/server/status`, {
            headers: { Authorization: `Bearer ${TOKEN}` },
          }).then((r) => r.json());
          return st.status;
        },
        { timeout: 300_000 },
      )
      .toBe("running");

    // Free RAM — long LLM loads have OOM-killed this host (exit 137).
    const stop = page.getByRole("button", { name: /^Stop$/i });
    if (await stop.isVisible().catch(() => false)) {
      await stop.click();
    } else {
      await fetch(`http://127.0.0.1:${PORT}/v1/llm/server/stop`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${TOKEN}`,
          "Content-Type": "application/json",
        },
        body: "{}",
      }).catch(() => null);
    }
  });

  test("Embeddings tab: Apply reloads rlx-embed; Benchmark runs text embed", async ({ page }) => {
    test.setTimeout(180_000);
    await openSettingsTab(page, "Embeddings");

    const applyReq = page.waitForResponse(
      (r) => r.url().includes("/v1/models/text-embedding") && r.request().method() === "POST",
      { timeout: 60_000 },
    );
    await page
      .getByRole("button", { name: /^Apply$/i })
      .first()
      .click();
    const applyResp = await applyReq;
    expect(applyResp.ok()).toBeTruthy();
    const applyBody = await applyResp.json();
    expect(applyBody.ok, JSON.stringify(applyBody)).toBeTruthy();
    await expect(page.getByText(/Embedding model applied/i).first()).toBeVisible({
      timeout: 15_000,
    });

    const query = page.getByLabel("Benchmark query");
    await expect(query).toBeVisible({ timeout: 10_000 });
    await query.fill("focused coding session");

    const benchReq = page.waitForResponse(
      (r) => r.url().includes("/v1/labels/index/benchmark") && r.request().method() === "POST",
      { timeout: 120_000 },
    );
    await page.getByRole("button", { name: /^Benchmark$/i }).click();
    const benchResp = await benchReq;
    expect(benchResp.ok()).toBeTruthy();
    const benchBody = await benchResp.json();
    expect(benchBody.ok !== false, JSON.stringify(benchBody)).toBeTruthy();
  });

  test("Embeddings tab: Re-embed kicks EEG trigger-reembed", async ({ page }) => {
    test.setTimeout(60_000);
    await openSettingsTab(page, "Embeddings");

    const reembedBtn = page.getByRole("button", { name: /^Re-embed$/i });
    await expect(reembedBtn).toBeVisible({ timeout: 15_000 });

    const reembedReq = page.waitForResponse(
      (r) => r.url().includes("/v1/models/trigger-reembed") && r.request().method() === "POST",
      { timeout: 30_000 },
    );
    await reembedBtn.click();
    expect((await reembedReq).ok()).toBeTruthy();
    // Cancel immediately — full batch re-embed OOMs this host (exit 137).
    await fetch(`http://127.0.0.1:${PORT}/v1/test/begin`, {
      method: "POST",
      headers: { Authorization: `Bearer ${TOKEN}`, "Content-Type": "application/json" },
      body: "{}",
    }).catch(() => null);
    await expect(page.getByRole("button", { name: /Embedding|Re-embed/i })).toBeVisible();
  });

  test("Screenshots tab: Re-embed & Reindex hits vision rebuild route", async ({ page }) => {
    test.setTimeout(300_000);

    const estimate = await fetch(`http://127.0.0.1:${PORT}/v1/settings/screenshot/estimate-reembed`, {
      headers: { Authorization: `Bearer ${TOKEN}` },
    })
      .then((r) => r.json())
      .catch(() => null);
    const unembedded = Number(estimate?.unembedded ?? estimate?.stale ?? estimate?.total ?? 0);

    await openSettingsTab(page, "Screenshots");
    const btn = page.getByRole("button", { name: /Re-embed & Reindex|Re-embed now/i }).first();
    await expect(btn).toBeVisible({ timeout: 15_000 });

    // Full vision rebuild on this machine has ~9k unembedded images (~40min / OOM).
    // Smoke: prove the button is wired; heavy path is covered by validate/embed-image.
    if (unembedded > 50) {
      await expect(btn).toBeEnabled();
      test.info().annotations.push({
        type: "note",
        description: `skipped full rebuild (${unembedded} unembedded) — button visible`,
      });
      return;
    }

    const rebuildReq = page.waitForResponse(
      (r) => r.url().includes("/v1/settings/screenshot/rebuild-embeddings") && r.request().method() === "POST",
      { timeout: 240_000 },
    );
    await btn.click();
    expect((await rebuildReq).ok()).toBeTruthy();
  });

  test("EXG tab: Re-embed all sessions when estimate has epochs", async ({ page }) => {
    test.setTimeout(120_000);

    const estimate = await fetch(`http://127.0.0.1:${PORT}/v1/models/estimate-reembed`, {
      headers: { Authorization: `Bearer ${TOKEN}` },
    })
      .then((r) => r.json())
      .catch(() => null);
    const missing = Number(estimate?.missing ?? 0);
    const hasData = Number(estimate?.embedded ?? 0) + missing > 0;

    await openSettingsTab(page, "EXG");
    const btn = page.getByRole("button", { name: /Re-embed all sessions/i });
    await expect(btn).toBeVisible({ timeout: 20_000 });

    test.skip(!hasData, "no EXG epochs to re-embed on this machine");
    // Encoder warm-up on tens of thousands of missing rows OOMs this host.
    if (missing > 5_000) {
      await expect(btn).toBeVisible();
      test.info().annotations.push({
        type: "note",
        description: `button visible; skipped click (${missing} missing epochs)`,
      });
      return;
    }

    await expect(btn).toBeEnabled({ timeout: 20_000 });
    const reembedReq = page.waitForResponse(
      (r) => r.url().includes("/v1/models/trigger-reembed") && r.request().method() === "POST",
      { timeout: 30_000 },
    );
    await btn.click();
    expect((await reembedReq).ok()).toBeTruthy();
    await fetch(`http://127.0.0.1:${PORT}/v1/test/begin`, {
      method: "POST",
      headers: { Authorization: `Bearer ${TOKEN}`, "Content-Type": "application/json" },
      body: "{}",
    }).catch(() => null);
  });

  test("Chat voice: mic gated on LLM; ASR start loads Whisper then stops without a mic", async ({ page }) => {
    await page.goto("/chat");
    await page.waitForLoadState("domcontentloaded");
    await expect(page.locator("body")).toBeVisible();

    // Voice controls require the LLM server to be running — with no downloaded
    // model the mic stays disabled. That gating is itself part of the E2E check.
    const mic = page.getByRole("button", { name: /start voice input|stop voice input|hold to talk/i });
    await expect(mic.first()).toBeVisible({ timeout: 15_000 });
    await expect(mic.first()).toBeDisabled();

    // Drive ASR through the same bridge the UI uses once the LLM is up, so we
    // still validate Whisper load + mic failure on this machine.
    const asrStart = page.waitForResponse((r) => r.url().includes("/v1/asr/start") && r.request().method() === "POST", {
      timeout: 20_000,
    });
    await page.evaluate(async () => {
      // @ts-expect-error test bridge
      await window.__TAURI_INTERNALS__.invoke("asr_start", {
        trigger: "push_to_talk",
        routing: "transcribe_only",
        language: "en",
      });
    });
    const resp = await asrStart;
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.ok).toBeTruthy();
    expect(body.mode?.engine || "whisper").toMatch(/whisper/i);

    // Engine thread exits after CoreAudio rejects the (missing) input device.
    await expect
      .poll(
        async () => {
          const st = await fetch(`http://127.0.0.1:${PORT}/v1/asr/status`, {
            headers: { Authorization: `Bearer ${TOKEN}` },
          }).then((r) => r.json());
          return st.status?.running === false;
        },
        { timeout: 20_000 },
      )
      .toBeTruthy();
  });
});
