/**
 * Backend model validation via the daemon's Rust engines.
 *
 * Complements models-ui-e2e (frontend only): these tests call
 * `/v1/models/validate*` which synthesize TTS, round-trip ASR,
 * ensure/download/start the LLM, run rlx-embed text+image checks, and
 * project synthetic EEG embeddings with rlx-umap — never an external HF CLI.
 *
 * Prerequisites: live skill-daemon (`npm run tauri -- dev` or equivalent).
 *
 *   npm run test:e2e:models-validate
 *   DOWNLOAD_TIMEOUT_SECS=900 npm run test:e2e:models-validate
 */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { expect, test } from "@playwright/test";

function readToken(): string {
  const p =
    process.platform === "darwin"
      ? path.join(os.homedir(), "Library", "Application Support", "skill", "daemon", "auth.token")
      : path.join(os.homedir(), ".config", "skill", "daemon", "auth.token");
  return fs.readFileSync(p, "utf8").trim();
}

const TOKEN = readToken();
const PORT = Number(process.env.SKILL_DAEMON_PORT ?? "18445");
const BASE = `http://127.0.0.1:${PORT}`;
const DOWNLOAD_TIMEOUT_SECS = Number(process.env.DOWNLOAD_TIMEOUT_SECS ?? "900");

async function daemonPost(route: string, body: Record<string, unknown> = {}) {
  const resp = await fetch(`${BASE}${route}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  const json = await resp.json();
  expect(resp.ok, `HTTP ${resp.status} for ${route}: ${JSON.stringify(json)}`).toBeTruthy();
  return json;
}

test.describe.configure({ mode: "serial" });

test.describe("daemon model validation (Rust API)", () => {
  test("TTS: Inflect-Nano synthesizes audible PCM", async () => {
    const json = await daemonPost("/v1/models/validate/tts", {
      text: "NeuroSkill voice check one two three.",
    });
    expect(json.ok, JSON.stringify(json)).toBeTruthy();
    expect(json.engine).toMatch(/inflect|kitten|qwen|orpheus/i);
    expect(json.seconds).toBeGreaterThan(0.15);
    expect(json.rms).toBeGreaterThan(1e-3);
  });

  test("ASR: TTS→Whisper round-trip loads weights", async () => {
    const json = await daemonPost("/v1/models/validate/asr", {
      text: "NeuroSkill voice check one two three.",
    });
    expect(json.ok, JSON.stringify(json)).toBeTruthy();
    expect(json.engine).toBe("whisper");
  });

  test("LLM: ensure weights via internal HF catalog download", async () => {
    const ensure = await daemonPost("/v1/models/ensure-llm-weights", {
      include_mmproj: false,
    });
    expect(ensure.ok, JSON.stringify(ensure)).toBeTruthy();
    expect(["already_downloaded", "download_started"]).toContain(ensure.result);

    test.setTimeout((DOWNLOAD_TIMEOUT_SECS + 60) * 1000);
    const weights = await daemonPost("/v1/models/validate/llm", {
      weights_only: true,
      download_timeout_secs: DOWNLOAD_TIMEOUT_SECS,
      include_mmproj: false,
    });
    expect(weights.ok, JSON.stringify(weights)).toBeTruthy();
    expect(weights.result).toBe("weights_present");
    expect(String(weights.filename)).toMatch(/\.gguf$/i);
  });

  test("LLM: start server reaches running (load_only)", async () => {
    test.setTimeout((DOWNLOAD_TIMEOUT_SECS + 300) * 1000);
    const json = await daemonPost("/v1/models/validate/llm", {
      download_timeout_secs: DOWNLOAD_TIMEOUT_SECS,
      stop_after: true,
      include_mmproj: false,
      load_only: true,
    });
    expect(json.ok, JSON.stringify(json)).toBeTruthy();
    expect(json.result).toBe("running");
  });

  test("embed-text: rlx-embed semantic consistency", async () => {
    test.setTimeout(300_000);
    const json = await daemonPost("/v1/models/validate/embed-text", {});
    expect(json.ok, JSON.stringify(json)).toBeTruthy();
    expect(json.backend).toBe("rlx-embed");
    expect(json.dim).toBeGreaterThanOrEqual(256);
    expect(json.cosine_similar).toBeGreaterThan(json.cosine_unrelated + 0.05);
  });

  test("embed-image: rlx-embed vision produces 768-d vectors", async () => {
    test.setTimeout(300_000);
    const json = await daemonPost("/v1/models/validate/embed-image", {});
    expect(json.ok, JSON.stringify(json)).toBeTruthy();
    expect(json.backend).toBe("rlx-embed");
    expect(json.dim).toBe(768);
    expect(Math.abs(json.l2_norm - 1)).toBeLessThan(0.05);
  });

  test("UMAP: synthetic EEG embeddings project via rlx-umap", async () => {
    test.setTimeout(180_000);
    const json = await daemonPost("/v1/models/validate/umap", { umap_n: 24 });
    expect(json.ok, JSON.stringify(json)).toBeTruthy();
    expect(json.points).toBeGreaterThanOrEqual(5);
    expect(json.dim).toBeGreaterThan(0);
  });
});
