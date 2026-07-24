#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
/**
 * CI check: skill-tts catalog stays wired to rlx-tts-bench + UI fallback.
 *
 *   node scripts/check-tts-catalog-parity.js
 *   SKILL_DAEMON_URL=http://127.0.0.1:18445 node scripts/check-tts-catalog-parity.js
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function mustContain(file, needle, msg) {
  const src = fs.readFileSync(path.join(ROOT, file), "utf8");
  if (!src.includes(needle)) {
    console.error(`FAIL: ${msg}\n  missing ${JSON.stringify(needle)} in ${file}`);
    process.exit(1);
  }
}

mustContain(
  "crates/skill-tts/src/engines.rs",
  "all_model_ids()",
  "skill-tts engines.rs must call rlx_tts_bench::adapters::all_model_ids()",
);
mustContain(
  "crates/skill-tts/src/catalog.rs",
  "list_engine_ids()",
  "skill-tts catalog must build from engines::list_engine_ids()",
);
mustContain("src/lib/llm/voice-catalog.ts", "fetchTtsEngines", "UI voice-catalog must fetch daemon TTS engines");
mustContain(
  "src/lib/llm/VoiceEnginePicker.svelte",
  "available",
  "VoiceEnginePicker must honour available / capability flags",
);

// Fallback list should include at least the core + FunASR/SenseVoice ASR defaults.
const voiceCat = fs.readFileSync(path.join(ROOT, "src/lib/llm/voice-catalog.ts"), "utf8");
for (const id of ["kitten", "funasr", "SenseVoiceSmall", "rlx-asr"]) {
  if (!voiceCat.includes(id)) {
    console.error(`FAIL: voice-catalog.ts missing expected id/token ${id}`);
    process.exit(1);
  }
}

const daemonUrl = process.env.SKILL_DAEMON_URL;
if (daemonUrl) {
  const tokenPaths = [
    path.join(process.env.HOME || "", "Library/Application Support/skill/daemon/auth.token"),
    path.join(process.env.HOME || "", ".config/skill/daemon/auth.token"),
  ];
  let token = "";
  for (const p of tokenPaths) {
    try {
      token = fs.readFileSync(p, "utf8").trim();
      if (token) break;
    } catch {
      /* try next */
    }
  }
  if (token) {
    const res = await fetch(`${daemonUrl.replace(/\/$/, "")}/v1/tts/engines`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!res.ok) {
      console.error(`FAIL: live /v1/tts/engines HTTP ${res.status}`);
      process.exit(1);
    }
    const body = await res.json();
    const engines = Array.isArray(body.engines) ? body.engines : [];
    if (engines.length < 2) {
      console.error("FAIL: live /v1/tts/engines returned too few engines");
      process.exit(1);
    }
    if (!engines.every((e) => typeof e.id === "string" && "available" in e)) {
      console.error("FAIL: live engines missing id/available");
      process.exit(1);
    }
    console.log(`OK live /v1/tts/engines (${engines.length} engines)`);
    process.exit(0);
  }
  console.warn("WARN: SKILL_DAEMON_URL set but no auth token; source-only check");
}

console.log("OK TTS catalog parity (source wiring)");
process.exit(0);
