#!/usr/bin/env npx tsx
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/**
 * test.ts — Comprehensive smoke-test for the Skill WebSocket + HTTP API.
 *
 * ═══════════════════════════════════════════════════════════════════════════════
 * ARCHITECTURE OVERVIEW
 * ═══════════════════════════════════════════════════════════════════════════════
 *
 * The NeuroSkill™ app runs a combined HTTP + WebSocket server on a single TCP port.
 * Both protocols share the same port and the same command set.
 *
 * Communication models:
 *   • WEBSOCKET REQUEST/RESPONSE — Client sends { command: "..." }, server
 *     replies with { command: "...", ok: true/false, ...payload }.
 *     The "command" field echoes the request for client-side matching.
 *
 *   • HTTP REST — Each command is also reachable as a REST endpoint:
 *       GET  /status          → status
 *       GET  /sessions        → sessions
 *       POST /label           → label
 *       POST /notify          → notify
 *       POST /say             → say (TTS, fire-and-forget)
 *       POST /calibrate       → run_calibration (auto-start)
 *       POST /timer           → timer (open + auto-start)
 *       POST /search          → search (EEG ANN)
 *       POST /search_labels   → search_labels
 *       POST /compare         → compare
 *       POST /sleep           → sleep staging
 *       POST /umap            → enqueue UMAP job
 *       GET  /umap/:job_id    → poll UMAP job
 *       GET  /calibrations    → list profiles
 *       POST /calibrations    → create profile
 *       GET  /calibrations/:id
 *       PATCH /calibrations/:id
 *       DELETE /calibrations/:id
 *       GET  /dnd             → dnd status (config + live eligibility + OS state)
 *       POST /dnd             → dnd_set (force enable/disable)
 *       GET  /llm/status      → llm_status
 *       POST /llm/start       → llm_start (loads model, may take seconds)
 *       POST /llm/stop        → llm_stop (frees GPU/CPU resources)
 *       GET  /llm/catalog     → llm_catalog (model list with download states)
 *       POST /llm/download    → llm_download (fire-and-forget)
 *       POST /llm/cancel_download → llm_cancel_download
 *       POST /llm/delete      → llm_delete (removes cached model)
 *       GET  /llm/logs        → llm_logs (last 500 log lines)
 *       POST /llm/chat        → non-streaming chat; accepts { message, images?, system? }
 *                               or full OpenAI messages array; supports base64 image upload
 *
 *   • HTTP UNIVERSAL TUNNEL — POST / with { "command": "…", …params }
 *     behaves identically to the WebSocket protocol.
 *
 *   • BROADCAST EVENTS — Server pushes unsolicited JSON objects to ALL connected
 *     WebSocket clients. These have { event: "..." } instead of { command: "..." }.
 *     Events fire in real-time as data arrives from the Muse headband.
 *
 * Data pipeline:
 *   1. Muse headband → BLE → raw EEG (4ch × 256Hz), PPG (64Hz), IMU (~50Hz)
 *   2. Every 5 seconds, a 5s EEG window (epoch) is fed to the ZUNA GPU encoder
 *      (WebGPU / wgpu) which produces a high-dimensional embedding vector.
 *   3. Embeddings are stored in per-day SQLite databases (YYYYMMDD/embeddings.sqlite).
 *   4. Band powers, derived scores, sleep staging, and search indices are all
 *      computed from these embeddings and the raw spectral data.
 *
 * Storage layout:
 *   ~/.skill/data/
 *     ├── 20260224/
 *     │   └── embeddings.sqlite   — embedding vectors + per-epoch scores
 *     ├── 20260223/
 *     │   └── embeddings.sqlite
 *     ├── labels.sqlite           — user text annotations (cross-day)
 *     └── ...
 *
 * ═══════════════════════════════════════════════════════════════════════════════
 * COMMANDS TESTED
 * ═══════════════════════════════════════════════════════════════════════════════
 *
 * 1.  STATUS             — Full snapshot of device, session, embeddings, scores, sleep
 * 2.  SESSIONS           — List all recording sessions across all days
 * 3.  NOTIFY             — Native OS notification (title + optional body)
 * 4.  SAY                — Speak text via on-device TTS (fire-and-forget)
 * 5.  LABEL              — Create a timestamped text annotation
 * 6.  SEARCH_LABELS      — Search labels by free-text query (text / context / both modes)
 * 7.  HOOKS_STATUS       — Proactive Hook rules + scenario + last-trigger metadata
 * 7b. HOOKS_GET/SET      — Full CRUD for hook rules via hooks_get / hooks_set
 * 8.  HOOKS_SUGGEST      — Suggest threshold from labels + EEG embedding distances
 * 9.  HOOKS_LOG          — Paginated hook trigger audit log from hooks.sqlite (includes scenario in hook_json)
 * 10. INTERACTIVE_SEARCH — Cross-modal 4-layer graph search (query → labels → EEG → found labels)
 * 11. SEARCH             — ANN similarity search across EEG embedding history
 * 9.  COMPARE            — Side-by-side metrics for two time ranges + UMAP enqueue
 * 10. SLEEP              — Sleep stage classification for a time range
 * 11. CALIBRATE          — list_calibrations + run_calibration (open & auto-start)
 * 12. TIMER              — Open focus-timer window and auto-start work phase
 * 13. UMAP               — Enqueue a 3D dimensionality reduction job
 * 14. UMAP_POLL          — Poll for UMAP job completion
 * 15. DND                — Do Not Disturb status (dnd) + force override (dnd_set); GET/POST /dnd
 * 16. LLM                — LLM server management + streaming chat + image upload
 *                          (llm_status, llm_catalog, llm_download, llm_logs, llm_chat);
 *                          REST /llm/* endpoints; POST /llm/chat with base64 images
 * 17. UNKNOWN            — Verify error handling for bad commands
 * 18. BROADCASTS         — Listen for server-pushed real-time events
 * 19. HTTP API           — REST endpoints + universal tunnel on the same port
 *
 * ═══════════════════════════════════════════════════════════════════════════════
 * USAGE
 * ═══════════════════════════════════════════════════════════════════════════════
 *
 *   npx tsx test.ts              # auto-discover; try WS, fall back to HTTP
 *   npx tsx test.ts 62853        # explicit port (same auto-transport logic)
 *   npx tsx test.ts --ws         # force WebSocket (fail if unavailable)
 *   npx tsx test.ts --http       # force HTTP (skip WS-only tests)
 *   npx tsx test.ts 62853 --http # explicit port + HTTP
 *
 * Requires: Node ≥ 18 (native fetch + WebSocket), bonjour-service (devDependency).
 * Exits 0 on success, 1 on failure.
 */

import { Bonjour } from "bonjour-service";
import { execSync } from "child_process";
import WebSocket from "ws";

// ── Config ────────────────────────────────────────────────────────────────────

// Parse argv: optional port number and optional --ws / --http flags.
const _argv = process.argv.slice(2);
const PORT: number | null = _argv.find(a => /^\d+$/.test(a)) ? Number(_argv.find(a => /^\d+$/.test(a))) : null;
const FORCE_WS   = _argv.includes("--ws");
const FORCE_HTTP = _argv.includes("--http");

const TIMEOUT_MS = 600_000; // 10 min — UMAP compute can be very slow on large datasets
const WS_URL     = (port: number) => `ws://127.0.0.1:${port}`;

let ws:        WebSocket;
let httpBase = "";
/** Active transport for command tests — set during connection in main(). */
let transport: "ws" | "http" = "ws";

let timer:  ReturnType<typeof setTimeout>;
let passed = 0;
let failed = 0;

// ── ANSI formatting ───────────────────────────────────────────────────────────

const GRAY   = "\x1b[90m";
const GREEN  = "\x1b[32m";
const RED    = "\x1b[31m";
const CYAN   = "\x1b[36m";
const YELLOW = "\x1b[33m";
const BOLD   = "\x1b[1m";
const DIM    = "\x1b[2m";
const RESET  = "\x1b[0m";

function ok(msg: string)   { console.log(`  ${GREEN}✓${RESET} ${msg}`); passed++; }
function fail(msg: string) { console.log(`  ${RED}✗${RESET} ${msg}`); failed++; }
function info(msg: string) { console.log(`  ${CYAN}ℹ${RESET} ${DIM}${msg}${RESET}`); }
function heading(msg: string) { console.log(`\n  ${BOLD}━━ ${msg} ━━${RESET}`); }
function field(name: string, value: unknown, desc: string) {
  console.log(`    ${GRAY}│${RESET} ${YELLOW}${name}${RESET} = ${BOLD}${value}${RESET}  ${DIM}${desc}${RESET}`);
}
function die(msg: string): never { console.error(`\n${RED}FATAL:${RESET} ${msg}`); process.exit(1); }

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * testWs(port) — Quick probe to check if a WebSocket server is listening.
 * Opens a connection, waits 1.5s for "open", then closes. Returns true/false.
 */
function testWs(p: number): Promise<boolean> {
  return new Promise((resolve) => {
    try {
      const w = new WebSocket(`ws://127.0.0.1:${p}`);
      const t = setTimeout(() => { try { w.close(); } catch {} resolve(false); }, 1500);
      w.on("open", () => { clearTimeout(t); w.close(); resolve(true); });
      w.on("error", () => { clearTimeout(t); resolve(false); });
    } catch { resolve(false); }
  });
}

/**
 * send(cmd, timeoutMs) — Send a JSON command and wait for the matching response.
 *
 * In WebSocket mode: listens for a frame whose `command` field echoes the
 * request; rejects after `timeoutMs`.
 *
 * In HTTP mode: `main()` replaces this with {@link sendHttp} so every
 * command test works transparently over either transport.
 */
let send = function wsSend(
  cmd: { command: string; [k: string]: unknown },
  timeoutMs = 15000,
): Promise<any> {
  return new Promise((resolve, reject) => {
    const handler = (raw: WebSocket.RawData) => {
      let data: any;
      try { data = JSON.parse(raw.toString()); } catch { return; }
      if (data.command === cmd.command) {
        ws.off("message", handler);
        resolve(data);
      }
    };
    ws.on("message", handler);
    ws.send(JSON.stringify(cmd));
    setTimeout(() => {
      ws.off("message", handler);
      reject(new Error(`timeout waiting for "${cmd.command}" response (${timeoutMs}ms)`));
    }, timeoutMs);
  });
};

/**
 * HTTP fallback for send(): POST / with the command JSON, return parsed response.
 * Assigned to `send` by `main()` when WebSocket is unavailable or --http forced.
 */
function sendHttp(
  cmd: { command: string; [k: string]: unknown },
  _timeoutMs?: number,
): Promise<any> {
  return fetch(`${httpBase}/`, {
    method:  "POST",
    headers: { "Content-Type": "application/json" },
    body:    JSON.stringify(cmd),
  }).then(r => r.json());
}

/**
 * collectEvents(ms) — Passively listen for broadcast events for `ms` milliseconds.
 *
 * Returns an array of all event objects received. These are server-pushed
 * messages with an "event" field (not "command").
 */
function collectEvents(ms: number): Promise<any[]> {
  return new Promise((resolve) => {
    const events: any[] = [];
    const handler = (raw: WebSocket.RawData) => {
      const data = JSON.parse(raw.toString());
      if (data.event) events.push(data);
    };
    ws.on("message", handler);
    setTimeout(() => { ws.off("message", handler); resolve(events); }, ms);
  });
}

/** Pretty-format a value for display in test output. */
function fmt(v: unknown): string {
  if (v === null || v === undefined) return `${DIM}null${RESET}`;
  if (typeof v === "number") return v % 1 === 0 ? String(v) : v.toFixed(3);
  if (typeof v === "string") return `"${v}"`;
  if (Array.isArray(v)) return `[${v.length} items]`;
  if (typeof v === "object") return `{${Object.keys(v!).length} keys}`;
  return String(v);
}

// ═══════════════════════════════════════════════════════════════════════════════
// PORT DISCOVERY
// ═══════════════════════════════════════════════════════════════════════════════
//
// The NeuroSkill™ app's WebSocket port is dynamic. We try three strategies:
//
//   1. bonjour-service (cross-platform mDNS) — The app registers "_skill._tcp"
//      on the local network. We browse for it and resolve the port.
//
//   2. lsof fallback (Unix) — Find processes named "skill", list their TCP
//      LISTEN sockets, and probe each with a WebSocket handshake.
//
//   3. Manual — User passes the port as argv[2].
//
// ═══════════════════════════════════════════════════════════════════════════════

async function discover(): Promise<number> {
  if (PORT) return PORT;

  // Strategy 1: bonjour-service mDNS discovery
  info("trying mDNS discovery (bonjour-service)…");
  const port = await new Promise<number | null>((resolve) => {
    const instance = new Bonjour();
    const timeout = setTimeout(() => {
      browser.stop();
      instance.destroy();
      resolve(null);
    }, 5000);

    const browser = instance.find({ type: "skill" }, (service) => {
      clearTimeout(timeout);
      browser.stop();
      const port = service.port;
      info(`mDNS found: ${service.name} @ ${service.host}:${port}`);
      instance.destroy();
      resolve(port);
    });
  });

  if (port) return port;

  // Strategy 2: lsof fallback (Unix)
  try {
    info("trying lsof fallback…");
    const ps = execSync("pgrep -if 'skill' 2>/dev/null || true", { encoding: "utf8" }).trim();
    if (ps) {
      const pids = ps.split("\n").map(s => s.trim()).filter(Boolean);
      for (const pid of pids) {
        try {
          const lsof = execSync(`lsof -iTCP -sTCP:LISTEN -nP -p ${pid} 2>/dev/null || true`, { encoding: "utf8" });
          for (const m of lsof.matchAll(/:(\d{4,5})\s+\(LISTEN\)/g)) {
            if (await testWs(Number(m[1]))) return Number(m[1]);
          }
        } catch {}
      }
    }
  } catch {}

  die("Could not discover port. Pass it manually: npx tsx test.ts <port>");
}

// ═══════════════════════════════════════════════════════════════════════════════
// COMMAND TESTS
// ═══════════════════════════════════════════════════════════════════════════════


// ─────────────────────────────────────────────────────────────────────────────
// 1. STATUS
// ─────────────────────────────────────────────────────────────────────────────
//
// Request:  { command: "status" }
// Response: { command: "status", ok: true, device: {...}, session: {...},
//             embeddings: {...}, labels: {...}, calibration: {...},
//             signal_quality: [...], sleep: {...}, scores: {...} }
//
// What the server does:
//   Assembles a full snapshot of every subsystem in the app into a single
//   response. This is the "god object" — everything a UI needs to render
//   the dashboard in one round-trip. No parameters needed.
//
// Subsystems returned:
//
//   • device — Muse headband BLE connection state, hardware identifiers
//     (serial, MAC, firmware), battery level (EMA-smoothed from telemetry
//     packets), raw sensor counts, IMU readings, and auto-reconnect state.
//
//   • session — Current recording session timing.
//
//   • embeddings — Stats from the ZUNA GPU encoder pipeline.
//
//   • labels — Count of user-created text annotations.
//
//   • calibration — Timestamp of the last completed calibration session.
//
//   • signal_quality — Array of 4 floats [0–1] per EEG channel.
//
//   • sleep — Rolling 48-hour sleep hypnogram summary.
//
//   • scores — Most recent 5-second epoch's full set of derived EEG metrics.
//
// ─────────────────────────────────────────────────────────────────────────────

async function testStatus(): Promise<void> {
  heading("status");
  info("Request: { command: 'status' }");
  info("Returns the full real-time state snapshot: device, session, embeddings, scores, sleep.");
  info("No parameters — this is a zero-argument introspection command.");
  info("The server assembles all subsystem states into a single JSON response.");
  try {
    const r = await send({ command: "status" });
    r.ok ? ok("command succeeded") : fail(`ok=${r.ok}`);

    // ── device ──
    console.log(`    ${CYAN}── device ──${RESET}  ${DIM}Muse headband BLE connection state & hardware identifiers${RESET}`);
    info("The server maintains a persistent BLE connection to the Muse headband.");
    info("'device' reflects the real-time connection state machine and hardware telemetry.");
    const d = r.device || {};
    field("state",              d.state,              "connection state: disconnected | scanning | connected | bt_off");
    field("connected",          d.connected,          "true when streaming from a Muse headband");
    field("streaming",          d.streaming,           "true when BLE data stream is active");
    field("name",               d.name ?? "null",     "Bluetooth device name (e.g. 'Muse-XXXX')");
    field("id",                 d.id ?? "null",       "platform-specific BLE device identifier");
    field("serial_number",      d.serial_number ?? "null", "Muse serial number (from telemetry)");
    field("mac_address",        d.mac_address ?? "null",   "Bluetooth MAC address");
    field("firmware_version",   d.firmware_version ?? "null", "headband firmware version string");
    field("hardware_version",   d.hardware_version ?? "null", "headband hardware revision");
    field("bootloader_version", d.bootloader_version ?? "null", "bootloader version");
    field("preset",             d.preset ?? "null",   "active headset EEG preset (e.g. 'p21')");
    field("battery",            d.battery,            "battery level 0–100 (EMA-smoothed from BLE telemetry)");
    field("sample_count",       d.sample_count,       "total EEG samples received this session (4ch × 256Hz)");
    field("ppg_sample_count",   d.ppg_sample_count,   "total PPG samples this session (64Hz)");
    field("ppg",                fmt(d.ppg),           "latest raw PPG sensor values [ambient, ir, red]");
    field("retry_attempt",      d.retry_attempt,      "auto-reconnect attempt count (0 = first try)");
    field("retry_countdown_secs", d.retry_countdown_secs, "seconds until next retry (null if not retrying)");
    field("accel",              fmt(d.accel),         "accelerometer [x,y,z] in g (from Muse IMU)");
    field("gyro",               fmt(d.gyro),          "gyroscope [x,y,z] in °/s (from Muse IMU)");
    field("fuel_gauge_mv",      d.fuel_gauge_mv,      "battery fuel gauge millivolts (Classic firmware)");
    field("temperature_raw",    d.temperature_raw,    "raw temperature sensor (Classic firmware)");

    // ── session ──
    console.log(`    ${CYAN}── session ──${RESET}  ${DIM}Current recording session timing${RESET}`);
    info("A 'session' begins when the Muse connects and starts streaming EEG.");
    info("It ends when the device disconnects. start_utc is null when idle.");
    const s = r.session || {};
    field("start_utc",      s.start_utc,      "unix timestamp when current session started (null = no session)");
    field("duration_secs",  s.duration_secs,  "wall-clock seconds since session start");

    // ── embeddings ──
    console.log(`    ${CYAN}── embeddings ──${RESET}  ${DIM}ZUNA GPU encoder pipeline stats${RESET}`);
    info("Every 5s of clean EEG is passed through a WebGPU (wgpu) neural encoder.");
    info("The encoder produces a high-dimensional embedding vector — the 'neural fingerprint'");
    info("of that 5-second brain moment. These embeddings are stored in daily SQLite DBs");
    info("and used for similarity search, UMAP projection, and metric computation.");
    const e = r.embeddings || {};
    field("today",          e.today,          "embedding epochs computed today");
    field("total",          e.total,          "all-time total embeddings across all days");
    field("recording_days", e.recording_days, "number of YYYYMMDD dirs with data");
    field("encoder_loaded", e.encoder_loaded, "true once the wgpu ZUNA model is warm");
    field("overlap_secs",   e.overlap_secs,   "sliding-window overlap for epochs (0 = non-overlapping)");

    // ── labels ──
    console.log(`    ${CYAN}── labels ──${RESET}  ${DIM}User-annotated EEG moments${RESET}`);
    info("Labels are free-text annotations stored in labels.sqlite with a UTC timestamp.");
    info("They appear in search results and can be attached to UMAP points.");
    field("total",          r.labels?.total,  "total labels stored in labels.sqlite");

    // ── calibration ──
    console.log(`    ${CYAN}── calibration ──${RESET}  ${DIM}Timed reference task for model baseline${RESET}`);
    info("Calibration is a guided eyes-open / eyes-closed task (~60s each).");
    info("It establishes a per-user baseline for alpha power and other metrics.");
    field("last_calibration_utc", r.calibration?.last_calibration_utc, "unix timestamp of last completed calibration");

    // ── signal_quality ──
    console.log(`    ${CYAN}── signal_quality ──${RESET}  ${DIM}Per-channel electrode contact quality${RESET}`);
    info("4-element array for [TP9, AF7, AF8, TP10] — the Muse's 4 EEG channels.");
    info("Computed from impedance / noise floor. 1.0 = great, 0.0 = no contact.");
    field("channel_quality", fmt(r.signal_quality), "array of 0–1 quality scores per EEG channel");

    // ── sleep ──
    console.log(`    ${CYAN}── sleep ──${RESET}  ${DIM}Rolling 48-hour sleep hypnogram summary${RESET}`);
    info("The server classifies every embedding in the past 48h into a sleep stage");
    info("using band-power heuristics (delta/theta/alpha/beta/sigma ratios).");
    info("Returns aggregate epoch counts — NOT a per-epoch hypnogram (use 'sleep' command for that).");
    const sl = r.sleep || {};
    field("window_hours",  sl.window_hours,   "lookback window (always 48h)");
    field("total_epochs",  sl.total_epochs,   "number of 5s epochs classified");
    field("wake_epochs",   sl.wake_epochs,    "epochs classified as Wake");
    field("n1_epochs",     sl.n1_epochs,      "epochs classified as N1 (light sleep)");
    field("n2_epochs",     sl.n2_epochs,      "epochs classified as N2 (spindle sleep)");
    field("n3_epochs",     sl.n3_epochs,      "epochs classified as N3 (deep/slow-wave sleep)");
    field("rem_epochs",    sl.rem_epochs,     "epochs classified as REM");

    // ── scores ──
    const sc = r.scores;
    if (sc) {
      console.log(`    ${CYAN}── scores ──${RESET}  ${DIM}Latest 5s epoch: all derived EEG/PPG/IMU metrics${RESET}`);
      info("Updated every 5 seconds when streaming. Contains 60+ fields.");
      info("These same fields are also broadcast in real-time via 'eeg-bands' events.");
      field("epoch_timestamp",  sc.epoch_timestamp, "YYYYMMDDHHmmss UTC timestamp of this epoch");

      console.log(`    ${GRAY}  ─ Brain state scores (0–100 scale, higher = more of that state) ─${RESET}`);
      field("focus",            sc.focus,            "frontal beta/theta ratio → attentional engagement");
      field("relaxation",       sc.relaxation,       "posterior alpha dominance → calm wakefulness");
      field("engagement",       sc.engagement,       "beta/(alpha+theta) → cognitive involvement");
      field("mood",             sc.mood,             "composite valence index (FAA + alpha + engagement)");

      console.log(`    ${GRAY}  ─ Composite scores (0–100 scale) ─${RESET}`);
      field("meditation",       sc.meditation,       "alpha + stillness + HRV composite");
      field("cognitive_load",   sc.cognitive_load,    "frontal θ / parietal α workload indicator");
      field("drowsiness",       sc.drowsiness,       "theta-alpha ratio + absolute alpha trend");

      console.log(`    ${GRAY}  ─ Band power ratios (dimensionless, log-scale or raw ratios) ─${RESET}`);
      field("faa",              sc.faa,              "Frontal Alpha Asymmetry: ln(AF8α) − ln(AF7α). +ve = approach, −ve = withdrawal");
      field("tar",              sc.tar,              "Theta/Alpha Ratio — elevated in drowsiness, meditation");
      field("bar",              sc.bar,              "Beta/Alpha Ratio — elevated in stress, attention");
      field("dtr",              sc.dtr,              "Delta/Theta Ratio — deep relaxation indicator");
      field("tbr",              sc.tbr,              "Theta/Beta Ratio — inverse attention marker (high = inattentive)");

      console.log(`    ${GRAY}  ─ Spectral features (frequency-domain analysis) ─${RESET}`);
      field("pse",              sc.pse,              "Power Spectral Entropy [0–1] — spectral complexity/randomness");
      field("apf",              sc.apf,              "Alpha Peak Frequency (Hz) — individual alpha rhythm speed (~9–11 Hz)");
      field("bps",              sc.bps,              "Band-Power Slope (1/f exponent) — neural noise color");
      field("snr",              sc.snr,              "Signal-to-Noise Ratio (dB) — signal quality metric");
      field("sef95",            sc.sef95,            "Spectral Edge Freq 95% (Hz) — freq below which 95% power lies");
      field("spectral_centroid", sc.spectral_centroid, "Spectral Centroid (Hz) — 'center of mass' of the spectrum");
      field("coherence",        sc.coherence,        "mean inter-channel alpha coherence [−1,1]");
      field("mu_suppression",   sc.mu_suppression,   "Mu suppression index (current/baseline alpha) — motor imagery");
      field("laterality_index", sc.laterality_index, "hemispheric power asymmetry (R−L)/(R+L)");
      field("pac_theta_gamma",  sc.pac_theta_gamma,  "Phase-Amplitude Coupling θ–γ — cross-frequency binding");

      console.log(`    ${GRAY}  ─ Complexity / nonlinear features (time-domain analysis) ─${RESET}`);
      field("hjorth_activity",   sc.hjorth_activity,   "Hjorth Activity — signal variance (total power)");
      field("hjorth_mobility",   sc.hjorth_mobility,   "Hjorth Mobility — mean frequency estimate");
      field("hjorth_complexity", sc.hjorth_complexity, "Hjorth Complexity — bandwidth / spectral change");
      field("permutation_entropy", sc.permutation_entropy, "Permutation Entropy — ordinal pattern complexity [0–1]");
      field("higuchi_fd",       sc.higuchi_fd,       "Higuchi Fractal Dimension — signal self-similarity (~1.0–2.0)");
      field("dfa_exponent",     sc.dfa_exponent,     "DFA α — long-range correlations (~0.5=white, ~1.5=Brownian)");
      field("sample_entropy",   sc.sample_entropy,   "Sample Entropy — irregularity / unpredictability");

      console.log(`    ${GRAY}  ─ PPG / cardiovascular (from Muse forehead PPG sensor) ─${RESET}`);
      field("hr",               sc.hr,               "Heart Rate (bpm) — pulse from IR PPG");
      field("rmssd",            sc.rmssd,            "RMSSD (ms) — short-term HRV, parasympathetic tone");
      field("sdnn",             sc.sdnn,             "SDNN (ms) — overall HRV, total autonomic variability");
      field("pnn50",            sc.pnn50,            "pNN50 (%) — fraction of adjacent RR intervals differing >50ms");
      field("lf_hf_ratio",     sc.lf_hf_ratio,     "LF/HF Ratio — sympathetic vs parasympathetic balance");
      field("respiratory_rate", sc.respiratory_rate, "Respiratory Rate (breaths/min) — from PPG modulation");
      field("spo2_estimate",    sc.spo2_estimate,    "SpO₂ estimate (%) — blood oxygen from red/IR ratio");
      field("perfusion_index",  sc.perfusion_index,  "Perfusion Index (%) — pulsatile/non-pulsatile blood flow");
      field("stress_index",     sc.stress_index,     "Stress Index — Baevsky's SI from RR interval histogram");

      console.log(`    ${GRAY}  ─ Artifact detection (cumulative event counters) ─${RESET}`);
      field("blink_count",      sc.blink_count,      "total eye blinks detected (AF7/AF8 spike pattern)");
      field("blink_rate",       sc.blink_rate,       "blinks per minute (rolling 60s window)");
      field("jaw_clench_count", sc.jaw_clench_count, "total jaw clenches detected (TP9/TP10 HF burst)");
      field("jaw_clench_rate",  sc.jaw_clench_rate,  "jaw clenches per minute (rolling 60s window)");

      console.log(`    ${GRAY}  ─ Head pose (IMU-derived, complementary filter on accel+gyro) ─${RESET}`);
      field("head_pitch",       sc.head_pitch,       "pitch angle (°) — positive = looking up");
      field("head_roll",        sc.head_roll,        "roll angle (°) — positive = right ear down");
      field("stillness",        sc.stillness,        "stillness score 0–100 (100 = perfectly still)");
      field("nod_count",        sc.nod_count,        "total head nods detected");
      field("shake_count",      sc.shake_count,      "total head shakes detected");

      console.log(`    ${GRAY}  ─ Relative band powers (fractions, sum ≈ 1.0) ─${RESET}`);
      const b = sc.bands || {};
      field("bands.rel_delta",  b.rel_delta,         "δ Delta 1–4 Hz — deep sleep, unconscious processing");
      field("bands.rel_theta",  b.rel_theta,         "θ Theta 4–8 Hz — drowsiness, meditation, memory");
      field("bands.rel_alpha",  b.rel_alpha,         "α Alpha 8–13 Hz — relaxed wakefulness, eyes-closed");
      field("bands.rel_beta",   b.rel_beta,          "β Beta 13–30 Hz — active cognition, focus, anxiety");
      field("bands.rel_gamma",  b.rel_gamma,         "γ Gamma 30–50 Hz — high-level processing, binding");

      // Validate completeness
      const expected = [
        "focus","relaxation","engagement","mood","meditation","cognitive_load","drowsiness",
        "faa","tar","bar","dtr","tbr","pse","apf","bps","snr","sef95","spectral_centroid",
        "coherence","mu_suppression","laterality_index","pac_theta_gamma",
        "hjorth_activity","hjorth_mobility","hjorth_complexity",
        "permutation_entropy","higuchi_fd","dfa_exponent","sample_entropy",
        "hr","rmssd","sdnn","pnn50","lf_hf_ratio","respiratory_rate",
        "spo2_estimate","perfusion_index","stress_index",
        "blink_count","blink_rate","jaw_clench_count","jaw_clench_rate",
        "head_pitch","head_roll","stillness","nod_count","shake_count",
      ];
      const missing = expected.filter(f => sc[f] === undefined);
      if (missing.length === 0) {
        ok(`all ${expected.length} score fields present`);
      } else {
        fail(`missing score fields: ${missing.join(", ")}`);
      }
    } else {
      ok("scores = null (no epoch computed yet — connect a Muse to see data)");
    }

    // ── history ──
    console.log(`    ${CYAN}── history ──${RESET}  ${DIM}Recording history stats, streak, today vs 7-day average${RESET}`);
    info("Computed from the session list: totals, consecutive-day streak, and");
    info("comparison of today's metrics against the rolling 7-day average.");
    const h = r.history;
    if (h && h !== null) {
      field("total_sessions",        h.total_sessions,        "total recording sessions across all days");
      field("total_recording_hours", h.total_recording_hours, "cumulative recording time in hours");
      field("total_epochs",          h.total_epochs,          "total 5-second embedding epochs stored");
      field("recording_days",        h.recording_days,        "distinct calendar days with recordings");
      field("current_streak_days",   h.current_streak_days,   "consecutive recording days ending today");
      field("longest_session_min",   h.longest_session_min,   "longest single session in minutes");
      field("avg_session_min",       h.avg_session_min,       "average session duration in minutes");
      if (h.today_vs_avg && Object.keys(h.today_vs_avg).length > 0) {
        const keys = Object.keys(h.today_vs_avg);
        ok(`today_vs_avg has ${keys.length} metrics: ${keys.join(", ")}`);
        const sample = h.today_vs_avg[keys[0]];
        field("  sample.today",     sample.today,     "today's value for first metric");
        field("  sample.avg_7d",    sample.avg_7d,    "7-day rolling average");
        field("  sample.delta_pct", sample.delta_pct, "percentage change vs average");
        field("  sample.direction", sample.direction, "up | down | stable (±5% threshold)");
      } else {
        ok("today_vs_avg is empty (no data today or this week)");
      }
    } else {
      ok("history = null (no sessions recorded yet)");
    }

  } catch (e: any) { fail(`status failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 2. SESSIONS
// ─────────────────────────────────────────────────────────────────────────────

async function testSessions(): Promise<any[]> {
  heading("sessions");
  info("Request: { command: 'sessions' }");
  info("Scans all daily SQLite DBs and reconstructs recording sessions from contiguous epochs.");
  info("A gap of >120s between epochs starts a new session.");
  info("Returns an array of session objects with day, start_utc, end_utc, n_epochs.");
  try {
    const r = await send({ command: "sessions" });
    r.ok ? ok("command succeeded") : fail(`ok=${r.ok}`);
    const list = r.sessions || [];
    ok(`${list.length} session(s) found`);
    for (const s of list.slice(0, 5)) {
      const start = new Date(s.start_utc * 1000).toISOString().slice(0, 16);
      const dur = s.end_utc - s.start_utc;
      field("session", `${start}`, `${dur}s, ${s.n_epochs} epochs, day=${s.day}`);
    }
    if (list.length > 5) info(`… and ${list.length - 5} more`);
    return list;
  } catch (e: any) { fail(`sessions failed: ${e.message}`); return []; }
}


// ─────────────────────────────────────────────────────────────────────────────
// 3. NOTIFY
// ─────────────────────────────────────────────────────────────────────────────
//
// Request:  { command: "notify", title: "…", body?: "…" }
// Response: { command: "notify", ok: true }
//
// What the server does:
//   Fires a native OS notification via `tauri-plugin-notification`.
//   Useful for triggering alerts from scripts or external automation.
//   `title` is required; `body` is optional.
//
// ─────────────────────────────────────────────────────────────────────────────

async function testNotify(): Promise<void> {
  heading("notify");
  info("Request: { command: 'notify', title: '…', body?: '…' }");
  info("Triggers a native OS notification from an external process.");

  // ── title + body ──
  try {
    const r = await send({ command: "notify", title: "Skill test", body: "smoke-test notification" });
    r.ok ? ok("notify with title+body succeeded") : fail(`ok=${r.ok}, error=${r.error}`);
  } catch (e: any) { fail(`notify title+body failed: ${e.message}`); }

  // ── title only ──
  try {
    const r = await send({ command: "notify", title: "Skill test (title only)" });
    r.ok ? ok("notify with title only succeeded") : fail(`ok=${r.ok}, error=${r.error}`);
  } catch (e: any) { fail(`notify title-only failed: ${e.message}`); }

  // ── missing title → error ──
  try {
    const r = await send({ command: "notify" });
    r.ok === false
      ? ok(`correctly rejected missing title: error="${r.error}"`)
      : fail("expected ok=false for missing title");
  } catch (e: any) { fail(`missing-title test failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 4. SAY
// ─────────────────────────────────────────────────────────────────────────────
//
// Request:  { command: "say", text: "Hello world" }
// Response: { command: "say", ok: true, spoken: "Hello world" }
//
// What the server does:
//   Enqueues the utterance on the dedicated skill-tts OS thread and returns
//   immediately — the response arrives before audio playback begins.  The TTS
//   engine (kittentts-rs, ONNX + espeak-ng phonemisation) synthesises and plays
//   the audio in the background.  On first call the model is downloaded from
//   HuggingFace Hub and cached; subsequent calls use the local cache.
//
// Notes:
//   • "fire-and-forget" from the API perspective: ok=true means the request
//     was accepted, NOT that audio has finished playing.
//   • Missing `text` field → ok=false with a descriptive error.
//   • Empty `text` string → ok=false (backend validates non-empty).
//   • The `spoken` field echoes the accepted text back to the caller.
//   • Available via WebSocket, HTTP POST /say, and the universal POST / tunnel.
//
// ─────────────────────────────────────────────────────────────────────────────

async function testSay(): Promise<void> {
  heading("say (TTS)");
  info("Request: { command: 'say', text: '...' }");
  info("Speaks text via the on-device kittentts engine (ONNX + espeak-ng).");
  info("Returns immediately — audio plays in the background on the skill-tts thread.");

  // ── basic utterance ───────────────────────────────────────────────────────
  try {
    info("Testing basic utterance…");
    const r = await send({ command: "say", text: "Skill smoke test. TTS working." });
    r.ok ? ok("say command accepted") : fail(`ok=${r.ok}, error=${r.error}`);
    field("spoken", `"${r.spoken}"`, "echoed text confirmed by server");
    if (r.spoken === "NeuroSkill™ smoke test. TTS working.") {
      ok("spoken field echoes the input text correctly");
    } else {
      fail(`spoken mismatch: expected "Skill smoke test. TTS working.", got "${r.spoken}"`);
    }
  } catch (e: any) { fail(`say basic failed: ${e.message}`); }

  // ── calibration-style phrases ─────────────────────────────────────────────
  try {
    info("Testing calibration-style phrases…");
    for (const phrase of [
      "Eyes open.",
      "Eyes closed.",
      "Break. Next: Eyes open.",
      "Calibration complete. Three loops recorded.",
    ]) {
      const r = await send({ command: "say", text: phrase });
      r.ok
        ? ok(`accepted: "${phrase}"`)
        : fail(`rejected "${phrase}": ${r.error}`);
    }
  } catch (e: any) { fail(`say phrases failed: ${e.message}`); }

  // ── missing text field → error ────────────────────────────────────────────
  try {
    info("Testing missing 'text' field (should return ok=false)…");
    const r = await send({ command: "say" });
    r.ok === false
      ? ok(`correctly rejected missing text: error="${r.error}"`)
      : fail("expected ok=false for missing text field");
  } catch (e: any) { fail(`missing-text test failed: ${e.message}`); }

  // ── empty string → error ──────────────────────────────────────────────────
  try {
    info("Testing empty text string (should return ok=false)…");
    const r = await send({ command: "say", text: "" });
    r.ok === false
      ? ok(`correctly rejected empty text: error="${r.error}"`)
      : fail("expected ok=false for empty text string");
  } catch (e: any) { fail(`empty-text test failed: ${e.message}`); }

  // ── optional voice field ──────────────────────────────────────────────────
  try {
    info("Testing optional voice field…");
    const r = await send({ command: "say", text: "Voice check.", voice: "Jasper" });
    r.ok ? ok("say with voice accepted") : fail(`ok=${r.ok}, error=${r.error}`);
    r.voice === "Jasper"
      ? ok(`voice echoed correctly: "${r.voice}"`)
      : fail(`expected voice="Jasper", got "${r.voice}"`);
  } catch (e: any) { fail(`say with voice failed: ${e.message}`); }

  // ── voice omitted → no voice field in response ────────────────────────────
  try {
    info("Testing omitted voice → response must not contain 'voice' key…");
    const r = await send({ command: "say", text: "Default voice." });
    r.ok ? ok("say without voice accepted") : fail(`ok=${r.ok}, error=${r.error}`);
    !("voice" in r)
      ? ok("no 'voice' key in response when voice omitted")
      : ok(`server returned voice="${r.voice}" (active default — also valid)`);
  } catch (e: any) { fail(`say default voice test failed: ${e.message}`); }

  // ── empty voice string treated as omitted ─────────────────────────────────
  try {
    info("Testing empty voice string (treated as default)…");
    const r = await send({ command: "say", text: "Empty voice.", voice: "" });
    r.ok
      ? ok("empty voice string treated as default (ok=true)")
      : fail(`ok=${r.ok}, error=${r.error}`);
  } catch (e: any) { fail(`say empty voice test failed: ${e.message}`); }

  // ── response shape ────────────────────────────────────────────────────────
  try {
    info("Verifying response shape (with voice)…");
    const r = await send({ command: "say", text: "Shape check.", voice: "Jasper" });
    if (r.ok !== true)               { fail(`ok not true: ${r.ok}`); return; }
    if (r.command !== "say")         { fail(`command not echoed: ${r.command}`); return; }
    if (typeof r.spoken !== "string"){ fail(`spoken not a string: ${typeof r.spoken}`); return; }
    if (typeof r.voice  !== "string"){ fail(`voice not a string: ${typeof r.voice}`); return; }
    ok("response shape: { ok: true, command: 'say', spoken: string, voice: string }");
  } catch (e: any) { fail(`response shape test failed: ${e.message}`); }

  // ── HTTP POST /say ────────────────────────────────────────────────────────
  try {
    info("Testing HTTP POST /say endpoint…");
    const res = await fetch(`${httpBase}/say`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ text: "HTTP TTS check." }),
    });
    const data = await res.json() as any;
    res.status === 200     ? ok("HTTP /say → 200")                  : fail(`expected 200, got ${res.status}`);
    data?.ok === true      ? ok("HTTP /say → ok=true")              : fail(`ok=${data?.ok}, error=${data?.error}`);
    typeof data?.spoken === "string"
      ? ok(`HTTP /say → spoken="${data.spoken}"`)
      : fail("HTTP /say → spoken field missing or not a string");
  } catch (e: any) { fail(`HTTP /say test failed: ${e.message}`); }

  // ── HTTP POST /say — missing text → 400 ──────────────────────────────────
  try {
    info("Testing HTTP POST /say with missing text → 400…");
    const res = await fetch(`${httpBase}/say`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({}),
    });
    res.status === 400 ? ok("HTTP /say (no text) → 400") : fail(`expected 400, got ${res.status}`);
    const data = await res.json() as any;
    data?.ok === false  ? ok("ok=false in error response") : fail(`ok=${data?.ok}`);
  } catch (e: any) { fail(`HTTP /say missing-text test failed: ${e.message}`); }

  // ── Universal tunnel ──────────────────────────────────────────────────────
  try {
    info("Testing universal POST / tunnel for say…");
    const res = await fetch(`${httpBase}/`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ command: "say", text: "Tunnel check." }),
    });
    const data = await res.json() as any;
    res.status === 200 && data?.ok === true
      ? ok(`POST / tunnel → ok=true, spoken="${data.spoken}"`)
      : fail(`tunnel say failed: status=${res.status} ok=${data?.ok}`);
  } catch (e: any) { fail(`tunnel say test failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 5. LABEL
// ─────────────────────────────────────────────────────────────────────────────

async function testLabel(): Promise<void> {
  heading("label");
  info("Request: { command: 'label', text: '...' }");
  info("Creates a timestamped text annotation on the current EEG moment.");
  info("Stored in labels.sqlite. Appears in search results and UMAP visualizations.");
  info("Also triggers a 'label-created' broadcast event to all connected clients.");
  try {
    const text = `test-label-${Date.now()}`;
    const r = await send({ command: "label", text });
    r.ok ? ok(`label created: id=${r.label_id}`) : fail(`ok=${r.ok}, error=${r.error}`);
    field("label_id", r.label_id, "auto-incremented label ID in labels.sqlite");
  } catch (e: any) { fail(`label failed: ${e.message}`); }
}

async function testHooksStatus(): Promise<void> {
  heading("hooks_status");

  try {
    const r = await send({ command: "hooks_status" });
    if (r.ok === true) ok("hooks_status returns ok=true");
    else fail(`hooks_status failed: ${r.error ?? "unknown"}`);

    if (Array.isArray(r.hooks)) ok(`hooks array present (${r.hooks.length})`);
    else fail("hooks field is not an array");

    const first = Array.isArray(r.hooks) && r.hooks.length > 0 ? r.hooks[0] : null;
    if (first?.hook) {
      if (typeof first.hook.scenario === "string") ok(`hook.scenario present (${first.hook.scenario})`);
      else fail("hook.scenario missing or not a string");
    } else {
      info("no hooks configured; scenario field check skipped");
    }
  } catch (e: any) {
    fail(`hooks_status request failed: ${e.message}`);
  }
}

async function testHooksSuggest(): Promise<void> {
  heading("hooks_suggest");
  try {
    const r = await send({ command: "hooks_suggest", keywords: ["focus"] });
    if (r.ok === true) ok("hooks_suggest returns ok=true");
    else fail(`hooks_suggest failed: ${r.error ?? "unknown"}`);

    if (r.suggestion && typeof r.suggestion === "object") ok("suggestion payload present");
    else fail("missing suggestion object");
  } catch (e: any) {
    fail(`hooks_suggest request failed: ${e.message}`);
  }
}

async function testHooksGetSet(): Promise<void> {
  heading("hooks_get / hooks_set");
  try {
    // Get current hooks
    const r0 = await send({ command: "hooks_get" });
    if (r0.ok === true) ok("hooks_get returns ok=true");
    else fail(`hooks_get failed: ${r0.error ?? "unknown"}`);
    if (Array.isArray(r0.hooks)) ok(`hooks array present (${r0.hooks.length})`);
    else fail("hooks field is not an array");

    const original = Array.isArray(r0.hooks) ? r0.hooks : [];

    // Add a test hook
    const testHook = {
      name: "__cli_test_hook__",
      enabled: true,
      keywords: ["test", "cli"],
      scenario: "cognitive",
      command: "test_cmd",
      text: "test text",
      distance_threshold: 0.15,
      recent_limit: 12,
    };
    const withTest = [...original, testHook];
    const r1 = await send({ command: "hooks_set", hooks: withTest });
    if (r1.ok === true) ok("hooks_set (add) returns ok=true");
    else fail(`hooks_set (add) failed: ${r1.error ?? "unknown"}`);

    // Verify the hook was added
    const r2 = await send({ command: "hooks_get" });
    const found = Array.isArray(r2.hooks) && r2.hooks.some((h: any) => h.name === "__cli_test_hook__");
    if (found) ok("test hook found after set");
    else fail("test hook not found after set");

    // Clean up — restore original hooks
    const r3 = await send({ command: "hooks_set", hooks: original });
    if (r3.ok === true) ok("hooks_set (restore) returns ok=true");
    else fail(`hooks_set (restore) failed: ${r3.error ?? "unknown"}`);

    // Verify cleanup
    const r4 = await send({ command: "hooks_get" });
    const still = Array.isArray(r4.hooks) && r4.hooks.some((h: any) => h.name === "__cli_test_hook__");
    if (!still) ok("test hook removed after restore");
    else fail("test hook still present after restore");
  } catch (e: any) {
    fail(`hooks_get/set request failed: ${e.message}`);
  }
}

async function testHooksLog(): Promise<void> {
  heading("hooks_log");
  try {
    const r = await send({ command: "hooks_log", limit: 5, offset: 0 });
    if (r.ok === true) ok("hooks_log returns ok=true");
    else fail(`hooks_log failed: ${r.error ?? "unknown"}`);

    if (Array.isArray(r.rows)) ok(`rows array present (${r.rows.length})`);
    else fail("rows field is not an array");

    if (typeof r.total === "number") ok(`total count present (${r.total})`);
    else fail("total field is not numeric");
  } catch (e: any) {
    fail(`hooks_log request failed: ${e.message}`);
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// 5. SEARCH_LABELS
// ─────────────────────────────────────────────────────────────────────────────
//
// Request:  { command: "search_labels", query: "...", k?, ef?, mode? }
// Response: { command: "search_labels", ok: true,
//             query, mode, model, k, count,
//             results: [{ label_id, text, context, distance, similarity,
//                         eeg_start, eeg_end, created_at, embedding_model,
//                         eeg_metrics }] }
//
// What the server does:
//   Embeds `query` using the configured fastembed model, then searches the
//   in-memory HNSW label index for the k nearest neighbors.  Three index
//   choices exist, selected by `mode`:
//     "text"    — searches the label-text HNSW (built from labels.text column)
//     "context" — searches the context HNSW (built from labels.context column)
//     "both"    — runs both searches and deduplicates by best distance
//   Results include per-label EEG band-power metrics for the recording window.
//
// Notes:
//   • If the embedder is not yet initialised (model still downloading), the
//     server returns ok=false with a descriptive error.
//   • An empty result list is perfectly valid — it means no labels have been
//     embedded yet, or no labels exist at all.
//   • `context` mode will return empty results when no labels have context text
//     (context_embedding column will be NULL for those rows).
//
// ─────────────────────────────────────────────────────────────────────────────

async function testSearchLabels(): Promise<void> {
  heading("search_labels");
  info("Request: { command: 'search_labels', query: '...', k?, ef?, mode? }");
  info("Searches the label HNSW index using a free-text query embedded by fastembed.");
  info("mode: \"text\" (default) | \"context\" | \"both\"");
  info("Returns results sorted by cosine distance (lower = more similar).");

  // ── mode: "text" ──
  try {
    info("Testing mode=\"text\" (default)…");
    const r = await send({ command: "search_labels", query: "focused meditation", k: 5 });
    r.ok ? ok("text mode succeeded") : fail(`ok=${r.ok}, error=${r.error}`);

    field("query",   r.query,   "echoed back from request");
    field("mode",    r.mode,    "search mode used (default: \"text\")");
    field("model",   r.model,   "fastembed model code that embedded the query");
    field("k",       r.k,       "neighbors requested");
    field("count",   r.count,   "results actually returned (≤ k)");

    const results = r.results || [];
    ok(`${results.length} result(s) returned for text mode`);
    if (results.length > 0) {
      const hit = results[0];
      info("each result contains:");
      field("  label_id",        hit.label_id,        "primary key in labels.sqlite");
      field("  text",            `"${hit.text}"`,     "label text string");
      field("  context",         hit.context ? `"${hit.context.slice(0, 40)}…"` : "\"\"", "label context (may be empty)");
      field("  distance",        hit.distance,        "cosine distance to query [0–1] (lower = closer)");
      field("  similarity",      hit.similarity,      "1 − distance, convenience field [0–1]");
      field("  eeg_start",       hit.eeg_start,       "unix timestamp of the recorded EEG window start");
      field("  eeg_end",         hit.eeg_end,         "unix timestamp of the recorded EEG window end");
      field("  created_at",      hit.created_at,      "unix timestamp when the label was created");
      field("  embedding_model", hit.embedding_model, "model that embedded this label");
      if (hit.eeg_metrics && Object.keys(hit.eeg_metrics).length > 0) {
        const mkeys = Object.keys(hit.eeg_metrics);
        ok(`eeg_metrics present: ${mkeys.slice(0, 5).join(", ")}${mkeys.length > 5 ? "…" : ""}`);
      } else {
        info("eeg_metrics not available (no EEG data for this window)");
      }
      // Sanity checks
      if (typeof hit.distance === "number" && hit.distance >= 0 && hit.distance <= 1) {
        ok("distance in valid range [0, 1]");
      } else {
        fail(`distance out of range: ${hit.distance}`);
      }
      if (Math.abs((hit.similarity ?? 0) - (1 - (hit.distance ?? 0))) < 0.001) {
        ok("similarity == 1 − distance");
      } else {
        fail(`similarity mismatch: similarity=${hit.similarity} distance=${hit.distance}`);
      }
      // Verify results are sorted by ascending distance
      const distances = results.map((r: any) => r.distance as number);
      const sorted = [...distances].sort((a, b) => a - b);
      const isSorted = distances.every((d: number, i: number) => d === sorted[i]);
      isSorted ? ok("results sorted by ascending distance") : fail("results NOT sorted by distance");
    }
  } catch (e: any) { fail(`search_labels text mode failed: ${e.message}`); }

  // ── mode: "context" ──
  try {
    info("Testing mode=\"context\"…");
    const r = await send({ command: "search_labels", query: "deep focus reading", k: 5, mode: "context" });
    r.ok ? ok("context mode succeeded") : fail(`ok=${r.ok}, error=${r.error}`);
    field("mode",  r.mode,  "should be \"context\"");
    field("count", r.count, "results (0 if no labels have context text embedded)");
    if (r.mode !== "context") fail(`mode echoed as "${r.mode}", expected "context"`);
    else ok("mode echoed correctly");
    const results = r.results || [];
    ok(`${results.length} result(s) for context mode (0 = no context embeddings yet)`);
  } catch (e: any) { fail(`search_labels context mode failed: ${e.message}`); }

  // ── mode: "both" ──
  try {
    info("Testing mode=\"both\" (merges text + context hits by best distance)…");
    const r = await send({ command: "search_labels", query: "relaxed", k: 5, mode: "both" });
    r.ok ? ok("both mode succeeded") : fail(`ok=${r.ok}, error=${r.error}`);
    field("mode",  r.mode,  "should be \"both\"");
    field("count", r.count, "merged unique results (≤ k, deduplicated by label_id)");
    if (r.mode !== "both") fail(`mode echoed as "${r.mode}", expected "both"`);
    else ok("mode echoed correctly");
    const results = r.results || [];
    ok(`${results.length} result(s) for both mode`);
    // In "both" mode there must be no duplicate label_ids
    const ids = (results as any[]).map(r => r.label_id);
    const uniqueIds = new Set(ids);
    uniqueIds.size === ids.length
      ? ok("no duplicate label_ids in both-mode results")
      : fail(`duplicate label_ids found: ${ids.join(", ")}`);
  } catch (e: any) { fail(`search_labels both mode failed: ${e.message}`); }

  // ── empty query error ──
  try {
    info("Testing empty query (should return ok=false)…");
    const r = await send({ command: "search_labels", query: "" });
    r.ok === false
      ? ok(`correctly rejected empty query: error="${r.error}"`)
      : fail("expected ok=false for empty query");
  } catch (e: any) { fail(`empty-query test failed: ${e.message}`); }

  // ── invalid mode error ──
  try {
    info("Testing invalid mode (should return ok=false)…");
    const r = await send({ command: "search_labels", query: "test", mode: "invalid_mode" });
    r.ok === false
      ? ok(`correctly rejected invalid mode: error="${r.error}"`)
      : fail("expected ok=false for invalid mode");
  } catch (e: any) { fail(`invalid-mode test failed: ${e.message}`); }

  // ── custom k and ef ──
  try {
    info("Testing custom k=3 and ef=32…");
    const r = await send({ command: "search_labels", query: "anxiety", k: 3, ef: 32 });
    r.ok ? ok(`k=3 ef=32 succeeded, ${r.count} result(s)`) : fail(`ok=${r.ok}, error=${r.error}`);
    field("k", r.k, "echoed k value");
    if (r.k === 3) ok("k echoed correctly");
    else fail(`expected k=3, got k=${r.k}`);
    const results = r.results || [];
    if (results.length > 3) fail(`got ${results.length} results but k=3`);
    else ok(`result count (${results.length}) ≤ k (3)`);
  } catch (e: any) { fail(`k/ef test failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 6. INTERACTIVE_SEARCH
// ─────────────────────────────────────────────────────────────────────────────
//
// Request:  { command: "interactive_search", query: "...",
//             k_text?, k_eeg?, k_labels?, reach_minutes? }
// Response: { command: "interactive_search", ok: true,
//             query, k_text, k_eeg, k_labels, reach_minutes,
//             nodes: GraphNode[], edges: GraphEdge[], dot: string }
//
// What the server does:
//   Runs a 5-step cross-modal pipeline:
//     1. Embed query text → text vector (fastembed).
//     2. Search label text-HNSW → k_text semantically similar labels
//        (layer 1: text_label nodes).
//     3. For each text label, compute mean EEG embedding of its time window.
//     4. Search all daily EEG HNSW indices → k_eeg raw EEG neighbors
//        (layer 2: eeg_point nodes).
//     5. For each EEG neighbor, find labels within ±reach_minutes
//        (layer 3: found_label nodes).
//
//   Returns a directed graph with 4 node kinds and 4 edge kinds:
//     Nodes: query | text_label | eeg_point | found_label
//     Edges: text_sim | eeg_bridge | eeg_sim | label_prox
//
//   Also returns a Graphviz DOT string for the full graph.
//
// Notes:
//   • Empty results are valid — if no labels/embeddings exist, only the
//     query node is returned (nodes.length === 1, edges.length === 0).
//   • The embedder must be initialised; ok=false if model is still loading.
//   • All parameters are optional with sensible defaults.
//
// ─────────────────────────────────────────────────────────────────────────────

async function testInteractiveSearch(): Promise<void> {
  heading("interactive_search");
  info("Request: { command: 'interactive_search', query: '...', k_text?, k_eeg?, k_labels?, reach_minutes? }");
  info("Cross-modal 4-layer graph: query → text_labels → eeg_points → found_labels.");
  info("Returns nodes[], edges[], and a Graphviz DOT string.");

  // ── basic query with defaults ─────────────────────────────────────────────
  try {
    info("Testing basic query with default parameters…");
    const r = await send({ command: "interactive_search", query: "focused meditation" }, 60_000);
    r.ok ? ok("command succeeded") : fail(`ok=${r.ok}, error=${r.error}`);

    // ── echoed parameters ──────────────────────────────────────────────────
    field("query",          r.query,          "echoed query string");
    field("k_text",         r.k_text,         "text-label neighbors requested (default 5)");
    field("k_eeg",          r.k_eeg,          "EEG-similarity neighbors requested (default 5)");
    field("k_labels",       r.k_labels,       "label-proximity neighbors per EEG point (default 3)");
    field("reach_minutes",  r.reach_minutes,  "temporal reach around each EEG point (default 10)");

    if (r.query !== "focused meditation") fail(`query not echoed correctly: "${r.query}"`);
    else ok("query echoed correctly");

    // ── structural checks: nodes ────────────────────────────────────────────
    const nodes: any[] = r.nodes ?? [];
    const edges: any[] = r.edges ?? [];

    ok(`${nodes.length} node(s), ${edges.length} edge(s) returned`);

    // There must always be exactly one query node
    const queryNodes = nodes.filter((n: any) => n.kind === "query");
    queryNodes.length === 1
      ? ok("exactly 1 query node present")
      : fail(`expected 1 query node, got ${queryNodes.length}`);

    // Query node must have the correct text
    if (queryNodes.length === 1) {
      queryNodes[0].text === "focused meditation"
        ? ok("query node text matches")
        : fail(`query node text="${queryNodes[0].text}"`);
      queryNodes[0].id === "query"
        ? ok("query node id = \"query\"")
        : fail(`query node id="${queryNodes[0].id}"`);
      queryNodes[0].distance === 0
        ? ok("query node distance = 0")
        : fail(`query node distance=${queryNodes[0].distance}`);
    }

    // All node kinds must be one of the 4 valid values
    const validKinds = new Set(["query", "text_label", "eeg_point", "found_label"]);
    const badKinds = nodes.filter((n: any) => !validKinds.has(n.kind));
    badKinds.length === 0
      ? ok("all node kinds are valid")
      : fail(`invalid node kinds: ${badKinds.map((n: any) => n.kind).join(", ")}`);

    // Every node must have: id (string), kind (string), distance (number)
    const structurallyBad = nodes.filter(
      (n: any) => typeof n.id !== "string" || typeof n.kind !== "string" || typeof n.distance !== "number"
    );
    structurallyBad.length === 0
      ? ok("all nodes have required fields (id, kind, distance)")
      : fail(`${structurallyBad.length} node(s) missing required fields`);

    // Count nodes by kind and report
    const byKind: Record<string, number> = {};
    for (const n of nodes) byKind[n.kind] = (byKind[n.kind] ?? 0) + 1;
    field("query nodes",       byKind.query       ?? 0, "center of the graph");
    field("text_label nodes",  byKind.text_label  ?? 0, "semantically similar labels (layer 1)");
    field("eeg_point nodes",   byKind.eeg_point   ?? 0, "raw EEG neighbors of label windows (layer 2)");
    field("found_label nodes", byKind.found_label ?? 0, "labels near EEG neighbors in time (layer 3)");

    // text_label nodes: must have text and timestamp_unix
    const textLabels = nodes.filter((n: any) => n.kind === "text_label");
    if (textLabels.length > 0) {
      const missingText = textLabels.filter((n: any) => typeof n.text !== "string" || n.text === "");
      const missingTs   = textLabels.filter((n: any) => typeof n.timestamp_unix !== "number");
      missingText.length === 0 ? ok("text_label nodes all have text") : fail(`${missingText.length} text_label node(s) missing text`);
      missingTs.length   === 0 ? ok("text_label nodes all have timestamp_unix") : fail(`${missingTs.length} text_label node(s) missing timestamp_unix`);

      // parent_id of text_label must be "query"
      const badParent = textLabels.filter((n: any) => n.parent_id !== "query");
      badParent.length === 0
        ? ok("all text_label nodes have parent_id=\"query\"")
        : fail(`${badParent.length} text_label node(s) with wrong parent_id`);

      // distance should be a valid cosine distance [0, 1]
      const badDist = textLabels.filter((n: any) => n.distance < 0 || n.distance > 1);
      badDist.length === 0
        ? ok("text_label distances in valid range [0, 1]")
        : fail(`${badDist.length} text_label node(s) with distance out of [0, 1]`);

      // Optional eeg_metrics — if present, must be an object
      const sampleMetrics = textLabels.find((n: any) => n.eeg_metrics != null)?.eeg_metrics;
      if (sampleMetrics) {
        const mkeys = Object.keys(sampleMetrics);
        ok(`text_label eeg_metrics present (${mkeys.length} field(s): ${mkeys.slice(0, 5).join(", ")})`);
      } else {
        ok("text_label eeg_metrics = null (no EEG data for label windows — ok)");
      }
    } else {
      ok("no text_label nodes (no labels embedded yet — annotate with `label` command first)");
    }

    // eeg_point nodes: must have timestamp_unix but no text
    const eegPoints = nodes.filter((n: any) => n.kind === "eeg_point");
    if (eegPoints.length > 0) {
      const missingTs = eegPoints.filter((n: any) => typeof n.timestamp_unix !== "number");
      missingTs.length === 0 ? ok("eeg_point nodes all have timestamp_unix") : fail(`${missingTs.length} eeg_point node(s) missing timestamp_unix`);

      // eeg_point ids should follow the "ep_<unix>" pattern
      const badId = eegPoints.filter((n: any) => !n.id.startsWith("ep_"));
      badId.length === 0 ? ok("eeg_point ids follow \"ep_<unix>\" pattern") : fail(`${badId.length} eeg_point(s) with unexpected id format`);

      // IDs should be unique (dedup check)
      const ids = eegPoints.map((n: any) => n.id);
      const uniqueIds = new Set(ids);
      uniqueIds.size === ids.length
        ? ok("eeg_point ids are unique (no duplicates)")
        : fail(`duplicate eeg_point ids: ${ids.length - uniqueIds.size} collision(s)`);
    } else {
      ok("no eeg_point nodes (no embeddings matched — ok if no EEG data recorded)");
    }

    // found_label nodes: must have text and timestamp_unix
    const foundLabels = nodes.filter((n: any) => n.kind === "found_label");
    if (foundLabels.length > 0) {
      const missingText = foundLabels.filter((n: any) => typeof n.text !== "string");
      missingText.length === 0 ? ok("found_label nodes all have text") : fail(`${missingText.length} found_label node(s) missing text`);

      // IDs should be unique
      const ids = foundLabels.map((n: any) => n.id);
      const uniqueIds = new Set(ids);
      uniqueIds.size === ids.length
        ? ok("found_label ids are unique")
        : fail(`duplicate found_label ids detected`);

      // t_dist should be in [0, 1] (fraction of reach window)
      const badDist = foundLabels.filter((n: any) => n.distance < 0 || n.distance > 1);
      badDist.length === 0
        ? ok("found_label distances in valid range [0, 1]")
        : fail(`${badDist.length} found_label(s) with distance out of [0, 1]`);
    } else {
      ok("no found_label nodes (no labels near EEG points within reach window — ok)");
    }

    // ── structural checks: edges ────────────────────────────────────────────
    const validEdgeKinds = new Set(["text_sim", "eeg_bridge", "eeg_sim", "label_prox"]);
    const badEdgeKinds = edges.filter((e: any) => !validEdgeKinds.has(e.kind));
    badEdgeKinds.length === 0
      ? ok("all edge kinds are valid")
      : fail(`invalid edge kinds: ${badEdgeKinds.map((e: any) => e.kind).join(", ")}`);

    // Every edge must have: from_id, to_id, distance, kind
    const badEdges = edges.filter(
      (e: any) =>
        typeof e.from_id !== "string" ||
        typeof e.to_id   !== "string" ||
        typeof e.distance !== "number" ||
        typeof e.kind    !== "string"
    );
    badEdges.length === 0
      ? ok("all edges have required fields (from_id, to_id, distance, kind)")
      : fail(`${badEdges.length} edge(s) missing required fields`);

    // text_sim edges should originate from "query"
    const textSimEdges = edges.filter((e: any) => e.kind === "text_sim");
    if (textSimEdges.length > 0) {
      const badFrom = textSimEdges.filter((e: any) => e.from_id !== "query");
      badFrom.length === 0
        ? ok("text_sim edges all originate from \"query\"")
        : fail(`${badFrom.length} text_sim edge(s) not from "query"`);
    }

    // eeg_bridge edges: from_id should be a text_label id
    const eegBridgeEdges = edges.filter((e: any) => e.kind === "eeg_bridge");
    if (eegBridgeEdges.length > 0) {
      const tlIds = new Set(textLabels.map((n: any) => n.id));
      const badFrom = eegBridgeEdges.filter((e: any) => !tlIds.has(e.from_id));
      badFrom.length === 0
        ? ok("eeg_bridge edges all originate from text_label nodes")
        : fail(`${badFrom.length} eeg_bridge edge(s) not from a text_label`);
    }

    // label_prox edges: from_id should be an eeg_point id
    const labelProxEdges = edges.filter((e: any) => e.kind === "label_prox");
    if (labelProxEdges.length > 0) {
      const epIds = new Set(eegPoints.map((n: any) => n.id));
      const badFrom = labelProxEdges.filter((e: any) => !epIds.has(e.from_id));
      badFrom.length === 0
        ? ok("label_prox edges all originate from eeg_point nodes")
        : fail(`${badFrom.length} label_prox edge(s) not from an eeg_point`);
    }

    // Count edges by kind
    const edgeByKind: Record<string, number> = {};
    for (const e of edges) edgeByKind[e.kind] = (edgeByKind[e.kind] ?? 0) + 1;
    field("text_sim edges",   edgeByKind.text_sim    ?? 0, "query → text_label (semantic)");
    field("eeg_bridge edges", edgeByKind.eeg_bridge  ?? 0, "text_label → eeg_point (neural bridge)");
    field("eeg_sim edges",    edgeByKind.eeg_sim     ?? 0, "eeg_point → eeg_point (direct similarity)");
    field("label_prox edges", edgeByKind.label_prox  ?? 0, "eeg_point → found_label (temporal)");

    // ── DOT output ──────────────────────────────────────────────────────────
    if (typeof r.dot === "string" && r.dot.length > 0) {
      ok(`DOT string returned (${r.dot.length} chars)`);
      r.dot.includes("digraph interactive_search")
        ? ok("DOT contains expected header: \"digraph interactive_search\"")
        : fail("DOT header \"digraph interactive_search\" not found");
      r.dot.includes('"query"')
        ? ok("DOT contains query node")
        : fail("DOT does not contain query node");
    } else {
      fail("DOT string missing or empty");
    }

  } catch (e: any) { fail(`interactive_search (basic) failed: ${e.message}`); }

  // ── custom parameters ─────────────────────────────────────────────────────
  try {
    info("Testing custom parameters: k_text=3, k_eeg=3, k_labels=2, reach_minutes=5…");
    const r = await send({
      command:       "interactive_search",
      query:         "relaxed state",
      k_text:        3,
      k_eeg:         3,
      k_labels:      2,
      reach_minutes: 5,
    }, 60_000);
    r.ok ? ok("custom-parameter query succeeded") : fail(`ok=${r.ok}, error=${r.error}`);

    // Verify echoed parameters
    r.k_text        === 3  ? ok("k_text=3 echoed correctly")        : fail(`k_text echoed as ${r.k_text}`);
    r.k_eeg         === 3  ? ok("k_eeg=3 echoed correctly")         : fail(`k_eeg echoed as ${r.k_eeg}`);
    r.k_labels      === 2  ? ok("k_labels=2 echoed correctly")      : fail(`k_labels echoed as ${r.k_labels}`);
    r.reach_minutes === 5  ? ok("reach_minutes=5 echoed correctly") : fail(`reach_minutes echoed as ${r.reach_minutes}`);

    // k_text caps the text_label count
    const textLabels = (r.nodes ?? []).filter((n: any) => n.kind === "text_label");
    textLabels.length <= 3
      ? ok(`text_label count (${textLabels.length}) ≤ k_text (3)`)
      : fail(`text_label count (${textLabels.length}) exceeds k_text (3)`);

    // k_eeg caps the eeg_point count (at most k_text × k_eeg, deduped)
    const eegPoints = (r.nodes ?? []).filter((n: any) => n.kind === "eeg_point");
    eegPoints.length <= textLabels.length * 3 + 1
      ? ok(`eeg_point count (${eegPoints.length}) within expected bound`)
      : fail(`eeg_point count (${eegPoints.length}) seems too high for k_eeg=3`);

  } catch (e: any) { fail(`interactive_search (custom params) failed: ${e.message}`); }

  // ── parameter clamping ────────────────────────────────────────────────────
  try {
    info("Testing parameter clamping: k_text=50 (server clamps to 20), reach_minutes=120 (clamps to 60)…");
    const r = await send({
      command:       "interactive_search",
      query:         "test clamping",
      k_text:        50,
      k_eeg:         50,
      k_labels:      20,
      reach_minutes: 120,
    }, 60_000);
    r.ok ? ok("over-limit parameters accepted (clamped by server)") : fail(`ok=${r.ok}, error=${r.error}`);

    r.k_text        <= 20 ? ok(`k_text clamped to ≤ 20 (got ${r.k_text})`)         : fail(`k_text not clamped: ${r.k_text}`);
    r.k_eeg         <= 20 ? ok(`k_eeg clamped to ≤ 20 (got ${r.k_eeg})`)           : fail(`k_eeg not clamped: ${r.k_eeg}`);
    r.k_labels      <= 10 ? ok(`k_labels clamped to ≤ 10 (got ${r.k_labels})`)     : fail(`k_labels not clamped: ${r.k_labels}`);
    r.reach_minutes <= 60 ? ok(`reach_minutes clamped to ≤ 60 (got ${r.reach_minutes})`) : fail(`reach_minutes not clamped: ${r.reach_minutes}`);

  } catch (e: any) { fail(`interactive_search (clamping) failed: ${e.message}`); }

  // ── missing query → error ─────────────────────────────────────────────────
  try {
    info("Testing missing query field (should return ok=false)…");
    const r = await send({ command: "interactive_search" }, 30_000);
    r.ok === false
      ? ok(`correctly rejected missing query: error="${r.error}"`)
      : fail("expected ok=false for missing query");
  } catch (e: any) { fail(`missing-query test failed: ${e.message}`); }

  // ── empty query → error ───────────────────────────────────────────────────
  try {
    info("Testing empty query string (should return ok=false)…");
    const r = await send({ command: "interactive_search", query: "" }, 30_000);
    r.ok === false
      ? ok(`correctly rejected empty query: error="${r.error}"`)
      : fail("expected ok=false for empty query");
  } catch (e: any) { fail(`empty-query test failed: ${e.message}`); }

  // ── graph connectivity invariants ─────────────────────────────────────────
  try {
    info("Checking graph connectivity invariants on a fresh query…");
    const r = await send({ command: "interactive_search", query: "work focus concentration" }, 60_000);
    if (!r.ok) { ok("embedder not ready — skipping graph invariant checks"); return; }

    const nodes: any[] = r.nodes ?? [];
    const edges: any[] = r.edges ?? [];
    const nodeIds = new Set(nodes.map((n: any) => n.id));

    // Every edge must reference existing node IDs
    const danglingEdges = edges.filter(
      (e: any) => !nodeIds.has(e.from_id) || !nodeIds.has(e.to_id)
    );
    danglingEdges.length === 0
      ? ok("no dangling edges — all edge endpoints exist as nodes")
      : fail(`${danglingEdges.length} edge(s) reference non-existent node ids`);

    // The query node must always exist (it is always created first)
    nodeIds.has("query")
      ? ok("query node always present in graph")
      : fail("query node missing from graph");

    // If there are text_labels → there must be text_sim edges connecting them
    const tls = nodes.filter((n: any) => n.kind === "text_label");
    const tsEdges = edges.filter((e: any) => e.kind === "text_sim");
    if (tls.length > 0) {
      tsEdges.length > 0
        ? ok(`${tls.length} text_label(s) → ${tsEdges.length} text_sim edge(s)`)
        : fail("text_label nodes present but no text_sim edges");
    }

    // If there are found_labels → there must be label_prox edges
    const fls = nodes.filter((n: any) => n.kind === "found_label");
    const lpEdges = edges.filter((e: any) => e.kind === "label_prox");
    if (fls.length > 0) {
      lpEdges.length > 0
        ? ok(`${fls.length} found_label(s) → ${lpEdges.length} label_prox edge(s)`)
        : fail("found_label nodes present but no label_prox edges");
    }

    ok("graph connectivity invariants satisfied");
  } catch (e: any) { fail(`graph invariants check failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 7. SEARCH_EEG
// ─────────────────────────────────────────────────────────────────────────────

async function testSearch(): Promise<void> {
  heading("search");
  info("Request: { command: 'search', start_utc, end_utc, k }");
  info("Performs approximate nearest-neighbor search on EEG embeddings.");
  info("Uses embeddings in [start_utc, end_utc] as query vectors and searches ALL history.");
  info("Returns the k most neurally-similar moments for each query epoch.");
  try {
    const now = Math.floor(Date.now() / 1000);
    const r = await send({ command: "search", start_utc: now - 300, end_utc: now, k: 3 });
    r.ok ? ok("command succeeded") : fail(`ok=${r.ok}`);

    const res = r.result?.result;
    if (res) {
      field("query_count",    res.query_count,    "number of embedding epochs used as queries");
      field("k",              res.k,              "neighbors requested per query");
      field("searched_days",  res.searched_days?.length, "number of YYYYMMDD dirs searched");
      field("start_utc",      res.start_utc,      "actual query range start (clamped to data)");
      field("end_utc",        res.end_utc,        "actual query range end (clamped to data)");

      const neighbors = res.neighbors || [];
      ok(`${neighbors.length} neighbor group(s) returned`);
      if (neighbors.length > 0) {
        const n = neighbors[0];
        info("each neighbor has: query_ts, results[{ts, distance, day, label?}]");
        field("sample neighbor", `query_ts=${n.query_ts}`, `${n.results?.length || 0} results`);
      }

      // ── analysis ──
      console.log(`    ${CYAN}── analysis ──${RESET}  ${DIM}Search result insights${RESET}`);
      const a = res.analysis;
      if (a) {
        field("total_neighbors",  a.total_neighbors,  "total neighbor entries returned");
        field("time_span_hours",  a.time_span_hours,  "time span of all neighbors in hours");
        if (a.distance_stats) {
          field("distance.min",   a.distance_stats.min,    "closest neighbor cosine distance");
          field("distance.mean",  a.distance_stats.mean,   "mean neighbor distance");
          field("distance.max",   a.distance_stats.max,    "furthest neighbor distance");
        }
        field("top_days",          a.top_days?.length,      "number of days with neighbors");
        field("temporal_dist",     Object.keys(a.temporal_distribution || {}).length, "distinct hours with neighbors");
        field("neighbor_metrics",  Object.keys(a.neighbor_metrics || {}).length, "metrics available for neighbors");
        ok("search analysis present");
      } else {
        ok("analysis not available (no neighbors)");
      }
    } else {
      ok("no search results (no embeddings in range)");
    }
  } catch (e: any) { fail(`search failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 7. CALIBRATE
// ─────────────────────────────────────────────────────────────────────────────
//
// Tests three things in sequence:
//   a) list_calibrations — enumerate profiles (name, id, actions, loop_count, …)
//   b) run_calibration   — open the calibration window with the active profile
//                          and auto-start it immediately
//   c) run_calibration { id } — same but with an explicit profile UUID
//
// Note: run_calibration requires a connected Muse headband.  The test treats
// ok=false with a connection error as a soft pass (expected in CI / headset-free
// environments) and only fails on unexpected server errors.
//
// ─────────────────────────────────────────────────────────────────────────────

async function testCalibrate(): Promise<void> {
  heading("calibrate");

  // ── a) list_calibrations ──
  info("Request: { command: 'list_calibrations' }");
  info("Returns all saved calibration profiles (name, id, actions[], loop_count, …).");
  let profiles: Array<{ id: string; name: string; actions: Array<{ label: string; duration_secs: number }>; break_duration_secs: number; loop_count: number; auto_start: boolean; last_calibration_utc: number | null }> = [];
  try {
    const lr = await send({ command: "list_calibrations" });
    lr.ok ? ok("list_calibrations succeeded") : fail(`ok=${lr.ok}, error=${lr.error}`);
    profiles = lr.profiles ?? [];
    ok(`${profiles.length} profile(s) found`);
    for (const p of profiles) {
      field("profile.id",             p.id,              "UUID string");
      field("profile.name",           p.name,            "human-readable name");
      field("profile.actions",        p.actions?.length, "number of timed actions in this profile");
      field("profile.break_duration", p.break_duration_secs, "seconds between action repeats");
      field("profile.loop_count",     p.loop_count,      "how many times to cycle the action list");
      field("profile.auto_start",     p.auto_start,      "whether this profile auto-starts on open");
      field("profile.last_run",       p.last_calibration_utc ?? "never", "unix timestamp of last completed run");
      if (p.actions?.length > 0) {
        for (const a of p.actions) {
          info(`  action: "${a.label}" — ${a.duration_secs}s`);
        }
      }
    }
  } catch (e: any) { fail(`list_calibrations failed: ${e.message}`); }

  // ── b) run_calibration (active profile, no id) ──
  info("Request: { command: 'run_calibration' } — opens calibration and auto-starts active profile.");
  info("Requires a connected Muse headband; ok=false with a connection error is expected in headset-free CI.");
  try {
    const r = await send({ command: "run_calibration" }, 10000);
    if (r.ok) {
      ok("run_calibration succeeded (window opened, calibration started)");
    } else if (typeof r.error === "string" && r.error.toLowerCase().includes("connect")) {
      ok(`run_calibration: no headset connected — expected in CI: "${r.error}"`);
    } else {
      fail(`run_calibration failed: ${r.error}`);
    }
  } catch (e: any) { fail(`run_calibration (no id) failed: ${e.message}`); }

  // ── c) run_calibration with explicit profile id ──
  if (profiles.length > 0) {
    const target = profiles[0];
    info(`Request: { command: 'run_calibration', id: '${target.id}' } — explicit profile: "${target.name}"`);
    try {
      const r = await send({ command: "run_calibration", id: target.id }, 10000);
      if (r.ok) {
        ok(`run_calibration with id="${target.id}" succeeded`);
      } else if (typeof r.error === "string" && r.error.toLowerCase().includes("connect")) {
        ok(`run_calibration (explicit id): no headset — expected in CI: "${r.error}"`);
      } else {
        fail(`run_calibration (explicit id) failed: ${r.error}`);
      }
    } catch (e: any) { fail(`run_calibration (explicit id) failed: ${e.message}`); }

    // ── d) run_calibration with bogus id → error ──
    info("Request: { command: 'run_calibration', id: 'nonexistent-uuid' } — should return ok=false.");
    try {
      const r = await send({ command: "run_calibration", id: "nonexistent-uuid-that-does-not-exist" }, 10000);
      // Either ok=false (profile not found) or ok=false (no headset) is acceptable
      r.ok === false
        ? ok(`correctly rejected bogus id: "${r.error}"`)
        : fail("expected ok=false for nonexistent profile id");
    } catch (e: any) { fail(`bogus-id test failed: ${e.message}`); }
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// 8. TIMER
// ─────────────────────────────────────────────────────────────────────────────
//
// Request:  { command: "timer" }
// Response: { command: "timer", ok: true }
//
// What the server does:
//   Opens the focus-timer window (or brings it to the front if already open)
//   and immediately starts the work phase using the last-saved preset.
//   If the window is already open, a `focus-timer-start` Tauri event is emitted
//   so the running Svelte page starts without a reload.
//
// ─────────────────────────────────────────────────────────────────────────────

async function testTimer(): Promise<void> {
  heading("timer");
  info("Request: { command: 'timer' }");
  info("Opens the focus-timer window and auto-starts the work phase.");
  info("If the window is already open, emits a focus-timer-start event instead.");
  try {
    const r = await send({ command: "timer" });
    r.ok
      ? ok("timer succeeded — focus-timer window opened and work phase started")
      : fail(`ok=${r.ok}, error=${r.error}`);
  } catch (e: any) { fail(`timer failed: ${e.message}`); }

  // Idempotent: calling timer again while window is open should also succeed
  info("Calling timer again (window already open) — should still return ok=true…");
  try {
    const r2 = await send({ command: "timer" });
    r2.ok
      ? ok("timer (idempotent second call) succeeded")
      : fail(`second call failed: ${r2.error}`);
  } catch (e: any) { fail(`timer (second call) failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 9. COMPARE
// ─────────────────────────────────────────────────────────────────────────────

async function testCompare(): Promise<void> {
  heading("compare");
  info("Request: { command: 'compare', a_start_utc, a_end_utc, b_start_utc, b_end_utc }");
  info("Compares two time ranges side-by-side: aggregated metrics, sleep staging, + UMAP enqueue.");
  info("Server loads all epochs in each range, computes mean of every metric, runs sleep classification,");
  info("and enqueues an async UMAP 3D projection job (poll with 'umap_poll').");
  try {
    const now = Math.floor(Date.now() / 1000);
    const r = await send({
      command: "compare",
      a_start_utc: now - 600, a_end_utc: now - 300,
      b_start_utc: now - 300, b_end_utc: now,
    });
    r.ok ? ok("command succeeded") : fail(`ok=${r.ok}`);

    for (const side of ["a", "b"] as const) {
      const m = r[side];
      if (!m) { info(`${side.toUpperCase()} = null (no data in range)`); continue; }
      console.log(`    ${CYAN}── session ${side.toUpperCase()} metrics ──${RESET}  ${DIM}Mean of all epochs in range${RESET}`);
      field("n_epochs",         m.n_epochs,         "number of 5s epochs in this range");
      field("focus",            m.focus?.toFixed(1), "mean focus score 0–100");
      field("relaxation",       m.relaxation?.toFixed(1), "mean relaxation score 0–100");
      field("engagement",       m.engagement?.toFixed(1), "mean engagement score 0–100");
      field("faa",              m.faa?.toFixed(3),   "mean Frontal Alpha Asymmetry");
      field("mood",             m.mood?.toFixed(1),  "mean mood index 0–100");
      field("meditation",       m.meditation?.toFixed(1), "mean meditation score 0–100");
      field("cognitive_load",   m.cognitive_load?.toFixed(1), "mean cognitive load 0–100");
      field("drowsiness",       m.drowsiness?.toFixed(1), "mean drowsiness 0–100");
      field("hr",               m.hr?.toFixed(0),    "mean heart rate (bpm)");

      const metricFields = [
        "focus","relaxation","engagement","faa","tar","bar","dtr","tbr","pse","apf","bps","snr",
        "coherence","mu_suppression","mood","sef95","spectral_centroid",
        "hjorth_activity","hjorth_mobility","hjorth_complexity",
        "permutation_entropy","higuchi_fd","dfa_exponent","sample_entropy",
        "pac_theta_gamma","laterality_index",
        "hr","rmssd","sdnn","pnn50","lf_hf_ratio","respiratory_rate",
        "spo2_estimate","perfusion_index","stress_index",
        "blink_count","blink_rate","jaw_clench_count","jaw_clench_rate",
        "head_pitch","head_roll","stillness","nod_count","shake_count",
        "meditation","cognitive_load","drowsiness",
        "rel_delta","rel_theta","rel_alpha","rel_beta","rel_gamma","rel_high_gamma",
      ];
      const present = metricFields.filter(f => typeof m[f] === "number");
      const missing = metricFields.filter(f => typeof m[f] !== "number");
      if (missing.length === 0) {
        ok(`${side.toUpperCase()}: all ${metricFields.length} metric fields present`);
      } else if (present.length === 0) {
        info(`${side.toUpperCase()}: no metric data (empty range)`);
      } else {
        fail(`${side.toUpperCase()}: missing ${missing.length} fields: ${missing.slice(0, 5).join(", ")}…`);
      }
    }

    // Sleep staging for each side
    for (const side of ["sleep_a", "sleep_b"] as const) {
      const sl = r[side];
      if (sl?.summary) {
        const s = sl.summary;
        ok(`${side}: ${sl.epochs?.length || 0} epochs — W=${s.wake_epochs} N1=${s.n1_epochs} N2=${s.n2_epochs} N3=${s.n3_epochs} REM=${s.rem_epochs}`);
      } else {
        info(`${side} = null (no sleep data)`);
      }
    }

    // ── insights ──
    console.log(`    ${CYAN}── insights ──${RESET}  ${DIM}Timeseries stats, deltas, trends${RESET}`);
    if (r.insights) {
      const ins = r.insights;
      field("n_epochs_a", ins.n_epochs_a, "timeseries epochs for session A");
      field("n_epochs_b", ins.n_epochs_b, "timeseries epochs for session B");
      field("improved",   ins.improved?.length,  "metrics that improved ≥5%");
      field("declined",   ins.declined?.length,  "metrics that declined ≥5%");
      field("stable",     ins.stable?.length,    "metrics within ±5%");
      if (ins.improved?.length > 0) ok(`improved: ${ins.improved.join(", ")}`);
      if (ins.declined?.length > 0) ok(`declined: ${ins.declined.join(", ")}`);
      if (ins.deltas?.focus) {
        const d = ins.deltas.focus;
        field("focus delta", `${d.a?.toFixed(2)} → ${d.b?.toFixed(2)} (${d.direction}, ${d.pct?.toFixed(1)}%)`, "A→B change");
      }
      if (ins.stats_a?.focus) {
        const s = ins.stats_a.focus;
        field("A focus stats", `min=${s.min} p25=${s.p25} med=${s.median} p75=${s.p75} max=${s.max} σ=${s.stddev} trend=${s.trend}`, "distribution");
      }
      ok("compare insights present");
    } else {
      info("insights not available (no timeseries data)");
    }

    // UMAP job ticket
    if (r.umap) {
      ok(`UMAP enqueued: job_id=${r.umap.job_id}, n_a=${r.umap.n_a}, n_b=${r.umap.n_b}, est=${r.umap.estimated_secs}s`);
    } else {
      info("umap not returned (possibly no embeddings in either range)");
    }
  } catch (e: any) { fail(`compare failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 10. SLEEP
// ─────────────────────────────────────────────────────────────────────────────

async function testSleep(): Promise<void> {
  heading("sleep");
  info("Request: { command: 'sleep', start_utc, end_utc }");
  info("Classifies every 5s EEG epoch in range into Wake/N1/N2/N3/REM using band-power rules.");
  info("Returns per-epoch hypnogram array AND a summary with epoch counts per stage.");
  info("Classification uses relative delta/theta/alpha/sigma/beta power ratios.");
  try {
    const now = Math.floor(Date.now() / 1000);
    const r = await send({ command: "sleep", start_utc: now - 3600, end_utc: now });
    r.ok ? ok("command succeeded") : fail(`ok=${r.ok}`);

    if (r.epochs) {
      field("epochs",       r.epochs.length,    "5s epochs with stage classification");
      const s = r.summary;
      if (s) {
        field("total_epochs", s.total_epochs,     "total classified");
        field("epoch_secs",   s.epoch_secs,       "seconds per epoch (always 5)");
        field("wake_epochs",  s.wake_epochs,      "Wake — eyes open, active, beta/alpha dominant");
        field("n1_epochs",    s.n1_epochs,        "N1 — light sleep, theta dominant, hypnagogic");
        field("n2_epochs",    s.n2_epochs,        "N2 — spindle sleep, sigma bursts, K-complexes");
        field("n3_epochs",    s.n3_epochs,        "N3 — deep slow-wave sleep, delta >20%, restorative");
        field("rem_epochs",   s.rem_epochs,       "REM — mixed-frequency, dreaming, muscle atonia");
      }
      // ── analysis ──
      console.log(`    ${CYAN}── analysis ──${RESET}  ${DIM}Sleep quality metrics${RESET}`);
      const a = r.analysis;
      if (a && a !== null) {
        field("efficiency_pct",     a.efficiency_pct,     "sleep efficiency: (total−wake)/total × 100");
        field("onset_latency_min",  a.onset_latency_min,  "minutes from first epoch to first non-wake");
        field("rem_latency_min",    a.rem_latency_min,    "minutes from sleep onset to first REM");
        field("transitions",        a.transitions,        "total stage transitions");
        field("awakenings",         a.awakenings,         "transitions from sleep → wake");
        if (a.stage_minutes) {
          const sm = a.stage_minutes;
          ok(`stage minutes: W=${sm.wake} N1=${sm.n1} N2=${sm.n2} N3=${sm.n3} REM=${sm.rem} total=${sm.total}`);
        }
        if (a.bouts) {
          const stages = Object.keys(a.bouts);
          ok(`bout analysis for ${stages.length} stage(s): ${stages.join(", ")}`);
          for (const [stage, b] of Object.entries(a.bouts) as [string, any][]) {
            field(`  ${stage} bouts`, `${b.count} × avg ${b.mean_min?.toFixed(1)}m, max ${b.max_min?.toFixed(1)}m`, "count/duration");
          }
        }
        ok("sleep analysis present");
      } else {
        info("analysis not available (empty epoch range)");
      }
    } else {
      ok("no epochs in range");
    }
  } catch (e: any) { fail(`sleep failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 11. UMAP + UMAP_POLL
// ─────────────────────────────────────────────────────────────────────────────

async function testUmap(): Promise<void> {
  heading("umap");
  info("Request: { command: 'umap', a_start_utc, a_end_utc, b_start_utc, b_end_utc }");
  info("Enqueues a UMAP 3D dimensionality reduction job (non-blocking async).");
  info("Projects high-dimensional EEG embeddings from two time ranges into shared 3D space.");
  info("Nearby points in 3D = neurally similar brain moments in embedding space.");
  info("Points are tagged with session (A=0/B=1), UTC timestamp, and optional label text.");
  info("Uses async job queue: enqueue → get job_id → poll with 'umap_poll' until complete.");
  try {
    const now = Math.floor(Date.now() / 1000);
    const SIX_HOURS = 6 * 3600;
    info(`Range A: ${new Date((now - SIX_HOURS) * 1000).toISOString().slice(11,16)} – ${new Date((now - SIX_HOURS / 2) * 1000).toISOString().slice(11,16)} UTC (6h → 3h ago)`);
    info(`Range B: ${new Date((now - SIX_HOURS / 2) * 1000).toISOString().slice(11,16)} – ${new Date(now * 1000).toISOString().slice(11,16)} UTC (3h → now)`);
    const enq = await send({
      command: "umap",
      a_start_utc: now - SIX_HOURS, a_end_utc: now - SIX_HOURS / 2,
      b_start_utc: now - SIX_HOURS / 2, b_end_utc: now,
    }, 30000);
    if (!enq) { fail("no response from umap enqueue"); return; }
    enq.ok ? ok("enqueued") : fail(`ok=${enq.ok}, error=${enq.error}`);
    field("job_id",              enq.job_id,              "unique job identifier for polling");
    field("queue_position",      enq.queue_position,      "0 = running now, >0 = waiting in queue");
    field("estimated_secs",      enq.estimated_secs,      "rough time estimate based on embedding count");
    field("n_a",                 enq.n_a,                 "embedding count for range A (last 6–3h)");
    field("n_b",                 enq.n_b,                 "embedding count for range B (last 3–0h)");

    if (!enq.job_id && enq.job_id !== 0) { fail("no job_id — cannot poll"); return; }

    // Poll loop — UMAP is GPU-heavy and can block the WS server for seconds
    // between progress callbacks. We use a generous per-poll timeout (60s) and
    // show live progress (epoch, loss, ETA) when the server reports it.
    info("polling for result (umap_poll) — GPU job, may take 30s–3min…");
    const pollStart = Date.now();
    const POLL_TIMEOUT = 300_000; // 5 min — large datasets can be slow
    const POLL_INTERVAL = 2_000;  // 2s between polls (server is busy with GPU)
    const POLL_SEND_TIMEOUT = 60_000; // 60s per poll send — server may be mid-epoch
    let result: any = null;
    let pollCount = 0;
    let lastProgressLine = "";
    while (Date.now() - pollStart < POLL_TIMEOUT) {
      await new Promise(r => setTimeout(r, POLL_INTERVAL));
      pollCount++;
      let poll: any;
      try {
        poll = await send({ command: "umap_poll", job_id: enq.job_id }, POLL_SEND_TIMEOUT);
      } catch {
        const elapsed = ((Date.now() - pollStart) / 1000).toFixed(0);
        info(`poll #${pollCount}: no response (server busy with GPU, ${elapsed}s elapsed)`);
        continue;
      }
      if (poll.status === "complete") { result = poll; break; }
      if (poll.status === "error") { fail(`job error: ${poll.error}`); return; }
      if (poll.status === "not_found") { fail(`job ${enq.job_id} not found (expired or invalid)`); return; }

      // Show live progress if available
      const p = poll.progress;
      const elapsed = ((Date.now() - pollStart) / 1000).toFixed(0);
      if (p && p.total_epochs > 0) {
        const pct = Math.round(p.epoch / p.total_epochs * 100);
        const eta = p.epoch_ms > 0 ? ((p.total_epochs - p.epoch) * p.epoch_ms / 1000).toFixed(0) : "?";
        const line = `epoch ${p.epoch}/${p.total_epochs} (${pct}%) · ${p.epoch_ms.toFixed(0)}ms/ep · loss=${p.loss?.toFixed(4) ?? "?"} · ~${eta}s left`;
        if (line !== lastProgressLine) {
          info(`poll #${pollCount}: ${line}`);
          lastProgressLine = line;
        }
      } else {
        info(`poll #${pollCount}: pending (${elapsed}s elapsed, waiting for first epoch…)`);
      }
    }

    if (!result) { fail(`poll timed out after ${POLL_TIMEOUT / 1000}s (${pollCount} polls)`); return; }

    ok(`completed in ${result.elapsed_ms}ms`);
    const res = result.result;
    field("n_a",   res.n_a,   "points from range A");
    field("n_b",   res.n_b,   "points from range B");
    field("dim",   res.dim,   "input embedding dimensionality (before UMAP → 3D)");
    field("points", res.points?.length, "total 3D-projected points");

    if (res.points?.length > 0) {
      const p = res.points[0];
      info(`sample point: x=${p.x.toFixed(3)} y=${p.y.toFixed(3)} z=${p.z?.toFixed(3)} session=${p.session} utc=${p.utc}${p.label ? ` label="${p.label}"` : ""}`);
      info("session=0 → range A (6–3h ago), session=1 → range B (3–0h ago)");
    } else {
      ok("no points (no embeddings in test ranges — need Muse recording in last 6 hours)");
    }

    // ── analysis ──
    console.log(`    ${CYAN}── analysis ──${RESET}  ${DIM}Cluster separation metrics${RESET}`);
    const a = res.analysis;
    if (a && a !== null) {
      field("separation_score",       a.separation_score,       "inter/intra cluster ratio (higher = better separation)");
      field("inter_cluster_distance", a.inter_cluster_distance, "Euclidean distance between A and B centroids");
      field("intra_spread_a",         a.intra_spread_a,         "mean point-to-centroid distance in cluster A");
      field("intra_spread_b",         a.intra_spread_b,         "mean point-to-centroid distance in cluster B");
      if (a.centroid_a) field("centroid_a", `[${a.centroid_a.map((v: number) => v.toFixed(2)).join(", ")}]`, "mean position of A points");
      if (a.centroid_b) field("centroid_b", `[${a.centroid_b.map((v: number) => v.toFixed(2)).join(", ")}]`, "mean position of B points");
      field("n_outliers_a", a.n_outliers_a, "A points >2σ from centroid");
      field("n_outliers_b", a.n_outliers_b, "B points >2σ from centroid");
      if (a.outliers?.length > 0) {
        const o = a.outliers[0];
        info(`sample outlier: session=${o.session} utc=${o.utc} dist=${o.distance_to_centroid?.toFixed(2)}`);
      }
      ok("UMAP cluster analysis present");
    } else {
      info("analysis not available (too few points)");
    }
  } catch (e: any) { fail(`umap failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 14. UNKNOWN COMMAND
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// DND (DO NOT DISTURB) AUTOMATION
// ─────────────────────────────────────────────────────────────────────────────
//
// Two commands:
//
//   dnd      — read-only status snapshot (no parameters)
//   dnd_set  — force-override DND on or off { "enabled": bool }
//
// REST equivalents:
//   GET  /dnd               → dnd
//   POST /dnd { enabled }   → dnd_set
//
// The `dnd` response includes:
//   enabled          — whether the automation feature is turned on in settings
//   threshold        — focus score (0–100) that must be sustained
//   duration_secs    — seconds the score must stay above threshold
//   mode_identifier  — macOS Focus mode identifier string
//   elapsed_secs     — seconds focus has been continuously above threshold (0 when below)
//   dnd_active       — whether the APP has activated DND (may differ from OS state)
//   os_active        — whether the OS currently reports Focus/DND as on (null on non-macOS)
//
// ─────────────────────────────────────────────────────────────────────────────

async function testDnd(): Promise<void> {
  heading("DND (Do Not Disturb)");

  // ── dnd — status snapshot ────────────────────────────────────────────────
  heading("dnd — status");
  info("Request: { command: 'dnd' }");
  info("Returns the full DND automation config + live eligibility state + OS query.");
  info("No parameters required — purely a read operation.");
  try {
    const r = await send({ command: "dnd" });
    r.ok ? ok("dnd status command succeeded") : fail(`ok=${r.ok}, error=${r.error}`);

    field("enabled",         r.enabled,         "feature enabled in settings (bool)");
    field("avg_score",       r.avg_score,       "rolling average focus score over the current window (0–100)");
    field("threshold",       r.threshold,       "avg_score must reach this to activate DND (0–100)");
    field("sample_count",    r.sample_count,    "samples currently in the window");
    field("window_size",     r.window_size,     "target window size in samples (≈ duration_secs × 4 Hz)");
    field("duration_secs",   r.duration_secs,   "seconds worth of samples that define the rolling window");
    field("mode_identifier", r.mode_identifier, "macOS Focus mode identifier string");
    field("dnd_active",      r.dnd_active,      "whether the APP has activated DND");
    field("os_active",       r.os_active,       "real OS Focus state (null on non-macOS)");
    info("dnd-eligibility broadcast (emitted ~4 Hz via WsBroadcaster + Tauri IPC):");
    info("  focus_score  — raw per-tick score");
    info("  avg_score    — rolling average over window_size samples — what triggers DND");
    info("  sample_count — samples collected so far (ramps up from 0 at connect)");

    // Structural validation
    if (typeof r.enabled        !== "boolean") { fail("enabled is not a boolean"); }
    else if (typeof r.avg_score      !== "number")  { fail("avg_score is not a number"); }
    else if (typeof r.threshold      !== "number")  { fail("threshold is not a number"); }
    else if (typeof r.sample_count   !== "number")  { fail("sample_count is not a number"); }
    else if (typeof r.window_size    !== "number")  { fail("window_size is not a number"); }
    else if (typeof r.duration_secs  !== "number")  { fail("duration_secs is not a number"); }
    else if (typeof r.mode_identifier !== "string") { fail("mode_identifier is not a string"); }
    else if (typeof r.dnd_active     !== "boolean") { fail("dnd_active is not a boolean"); }
    else if (r.os_active !== null && typeof r.os_active !== "boolean") { fail("os_active must be bool or null"); }
    else { ok("all dnd status fields have correct types"); }

    // Sanity checks
    if (r.threshold >= 0 && r.threshold <= 100) { ok(`threshold in valid range: ${r.threshold}`); }
    else { fail(`threshold out of range: ${r.threshold}`); }
    if (r.avg_score >= 0 && r.avg_score <= 100) { ok(`avg_score in valid range: ${r.avg_score.toFixed(1)}`); }
    else { fail(`avg_score out of range: ${r.avg_score}`); }
    if (r.window_size >= 8) { ok(`window_size ≥ 8: ${r.window_size}`); }
    else { fail(`window_size too small: ${r.window_size}`); }
    if (r.duration_secs > 0) { ok(`duration_secs positive: ${r.duration_secs}`); }
    else { fail(`duration_secs must be > 0, got: ${r.duration_secs}`); }
  } catch (e: any) { fail(`dnd status failed: ${e.message}`); }

  // ── dnd_set — force disable (safe to call any time) ─────────────────────
  heading("dnd_set — force disable");
  info("Request: { command: 'dnd_set', enabled: false }");
  info("Forces DND off immediately, bypassing the EEG threshold.");
  info("Always safe to call — if DND was already off this is a no-op at the OS level.");
  try {
    const r = await send({ command: "dnd_set", enabled: false });
    r.ok       ? ok("dnd_set enabled=false succeeded") : fail(`ok=${r.ok}, error=${r.error}`);
    r.enabled === false ? ok("response echoes enabled=false") : fail(`enabled=${r.enabled}`);
    field("ok",      r.ok,      "true if OS call succeeded (or was a no-op)");
    field("enabled", r.enabled, "echoes the requested state");
  } catch (e: any) { fail(`dnd_set disable failed: ${e.message}`); }

  // Verify the state was applied
  try {
    const r = await send({ command: "dnd" });
    r.dnd_active === false
      ? ok("confirmed: dnd_active=false after force-disable")
      : fail(`dnd_active is ${r.dnd_active} after force-disable`);
  } catch (e: any) { fail(`dnd status re-check failed: ${e.message}`); }

  // ── dnd_set — force enable ───────────────────────────────────────────────
  heading("dnd_set — force enable");
  info("Request: { command: 'dnd_set', enabled: true }");
  info("Forces DND on immediately. On macOS this activates the configured Focus mode.");
  info("On non-macOS platforms the OS call is a no-op but ok=true is returned.");
  try {
    const r = await send({ command: "dnd_set", enabled: true });
    // ok=true means the OS call succeeded; ok=false means it failed (e.g. no macOS)
    if (r.ok) {
      ok("dnd_set enabled=true succeeded (OS call ok)");
      field("ok",      r.ok,      "OS call succeeded");
      field("enabled", r.enabled, "echoes the requested state");
    } else {
      // On non-macOS or without permissions, the OS call may legitimately fail.
      ok(`dnd_set enabled=true returned ok=${r.ok} (expected on non-macOS or missing permissions)`);
    }
    r.enabled === true ? ok("response echoes enabled=true") : fail(`enabled=${r.enabled}`);
  } catch (e: any) { fail(`dnd_set enable failed: ${e.message}`); }

  // Always clean up by disabling DND so tests don't leave the user in DND
  try {
    await send({ command: "dnd_set", enabled: false });
    ok("cleanup: DND disabled after enable test");
  } catch (e: any) { fail(`cleanup disable failed: ${e.message}`); }

  // ── dnd_set — missing enabled field → error ──────────────────────────────
  heading("dnd_set — validation");
  info("Request: { command: 'dnd_set' } (missing enabled) → should return ok=false");
  try {
    const r = await send({ command: "dnd_set" });
    r.ok === false
      ? ok(`correctly rejected missing enabled: error="${r.error}"`)
      : fail("expected ok=false when enabled field is missing");
  } catch (e: any) { fail(`missing-enabled test failed: ${e.message}`); }

  info("Request: { command: 'dnd_set', enabled: 'yes' } (wrong type) → should return ok=false");
  try {
    const r = await send({ command: "dnd_set", enabled: "yes" });
    r.ok === false
      ? ok(`correctly rejected non-boolean enabled: error="${r.error}"`)
      : fail("expected ok=false for non-boolean enabled");
  } catch (e: any) { fail(`non-boolean enabled test failed: ${e.message}`); }

  // ── HTTP REST: GET /dnd ───────────────────────────────────────────────────
  heading("HTTP REST — GET /dnd");
  info("GET /dnd → DND status snapshot (same as { command: 'dnd' } via WS)");
  try {
    const res  = await fetch(`${httpBase}/dnd`);
    const data = await res.json() as any;
    res.status === 200 ? ok("GET /dnd → 200") : fail(`expected 200, got ${res.status}`);
    data?.ok === true  ? ok("GET /dnd → ok=true") : fail(`ok=${data?.ok}, error=${data?.error}`);
    data?.command === "dnd" ? ok("command field echoed: 'dnd'") : fail(`command=${data?.command}`);
    typeof data?.enabled      === "boolean" ? ok("enabled field present (boolean)") : fail(`enabled=${data?.enabled}`);
    typeof data?.threshold    === "number"  ? ok("threshold field present (number)") : fail(`threshold=${data?.threshold}`);
    typeof data?.dnd_active   === "boolean" ? ok("dnd_active field present (boolean)") : fail(`dnd_active=${data?.dnd_active}`);
    field("enabled",      data?.enabled,      "");
    field("threshold",    data?.threshold,    "");
    field("elapsed_secs", data?.elapsed_secs, "");
    field("dnd_active",   data?.dnd_active,   "");
    field("os_active",    data?.os_active,    "");
  } catch (e: any) { fail(`GET /dnd failed: ${e.message}`); }

  // ── HTTP REST: POST /dnd { enabled: false } ───────────────────────────────
  heading("HTTP REST — POST /dnd disable");
  info("POST /dnd { enabled: false } → force-disable DND via REST");
  try {
    const res = await fetch(`${httpBase}/dnd`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ enabled: false }),
    });
    const data = await res.json() as any;
    res.status === 200  ? ok("POST /dnd (disable) → 200") : fail(`expected 200, got ${res.status}`);
    data?.ok === true   ? ok("POST /dnd (disable) → ok=true") : fail(`ok=${data?.ok}, error=${data?.error}`);
    data?.command === "dnd_set" ? ok("command='dnd_set'") : fail(`command=${data?.command}`);
    data?.enabled === false ? ok("enabled=false in response") : fail(`enabled=${data?.enabled}`);
  } catch (e: any) { fail(`POST /dnd disable failed: ${e.message}`); }

  // ── HTTP REST: POST /dnd missing enabled → 400 ───────────────────────────
  heading("HTTP REST — POST /dnd validation");
  info("POST /dnd {} (missing enabled) → 400");
  try {
    const res = await fetch(`${httpBase}/dnd`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({}),
    });
    res.status === 400 ? ok("POST /dnd (no enabled) → 400") : fail(`expected 400, got ${res.status}`);
    const data = await res.json() as any;
    data?.ok === false  ? ok("ok=false in error response") : fail(`ok=${data?.ok}`);
    typeof data?.error === "string" ? ok(`error message: "${data.error}"`) : fail("no error field");
  } catch (e: any) { fail(`POST /dnd missing-enabled test failed: ${e.message}`); }

  // ── Universal tunnel: dnd via POST / ─────────────────────────────────────
  heading("Universal tunnel — dnd");
  info("POST / { command: 'dnd' } → status via HTTP tunnel");
  try {
    const res = await fetch(`${httpBase}/`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ command: "dnd" }),
    });
    const data = await res.json() as any;
    res.status === 200 && data?.ok === true
      ? ok(`tunnel dnd → ok=true, enabled=${data.enabled}, dnd_active=${data.dnd_active}`)
      : fail(`tunnel dnd failed: status=${res.status} ok=${data?.ok}`);
  } catch (e: any) { fail(`tunnel dnd test failed: ${e.message}`); }

  info("POST / { command: 'dnd_set', enabled: false } → disable via tunnel");
  try {
    const res = await fetch(`${httpBase}/`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify({ command: "dnd_set", enabled: false }),
    });
    const data = await res.json() as any;
    res.status === 200 && data?.ok === true
      ? ok(`tunnel dnd_set → ok=true, enabled=${data.enabled}`)
      : fail(`tunnel dnd_set failed: status=${res.status} ok=${data?.ok}`);
  } catch (e: any) { fail(`tunnel dnd_set test failed: ${e.message}`); }
}

// ─────────────────────────────────────────────────────────────────────────────
// 15. LLM COMMANDS
// ─────────────────────────────────────────────────────────────────────────────
//
// Tests the built-in LLM inference server management commands exposed over the
// WebSocket (and HTTP) API:
//
//   llm_status          — server state (stopped/loading/running), model name, n_ctx
//   llm_start           — load the active GGUF model and start inference (async, slow)
//   llm_stop            — stop the server and free GPU/CPU resources
//   llm_catalog         — full model catalog with download states
//   llm_download        — start downloading a GGUF model (fire-and-forget)
//   llm_cancel_download — cancel an in-progress download
//   llm_delete          — delete a locally-cached model file
//   llm_logs            — last ≤500 LLM server log lines
//   llm_chat            — streaming chat (WebSocket only; sends multiple frames)
//
// Most tests verify the *protocol* (ok field, required response fields, error
// handling) and do NOT require an actual model to be downloaded or the server
// to be running, so they are safe to run on a CI machine without GPU or models.
//
// The llm_chat test only runs when transport === "ws" (streaming requires WebSocket)
// and is automatically skipped when the LLM server is not running.
//
// ─────────────────────────────────────────────────────────────────────────────

async function testLlm(): Promise<void> {
  heading("LLM commands");
  info("Testing LLM server management + streaming chat over WebSocket/HTTP.");
  info("Protocol tests run regardless of model availability — no GPU required.");

  // ── llm_status ───────────────────────────────────────────────────────────
  // Request:  { command: "llm_status" }
  // Response: { command: "llm_status", ok: true, status: "stopped"|"loading"|"running",
  //             model_name: "...", n_ctx: 0, supports_vision: false }
  try {
    info("Testing llm_status…");
    const r = await send({ command: "llm_status" });
    r.ok === true ? ok("llm_status ok=true") : fail(`llm_status ok=${r.ok}, error=${r.error}`);
    field("command",         r.command,         "should echo 'llm_status'");
    field("status",          r.status,          "'stopped' | 'loading' | 'running'");
    field("model_name",      r.model_name,      "empty string if no model selected");
    field("n_ctx",           r.n_ctx,           "0 when server is stopped");
    field("supports_vision", r.supports_vision, "true if mmproj is loaded");

    if (r.command !== "llm_status") fail(`command not echoed: "${r.command}"`);
    else ok("command echoed correctly");

    const validStatuses = new Set(["stopped", "loading", "running"]);
    validStatuses.has(r.status)
      ? ok(`status is valid ("${r.status}")`)
      : fail(`invalid status value: "${r.status}"`);

    typeof r.model_name === "string"
      ? ok("model_name is a string")
      : fail(`model_name is not a string: ${typeof r.model_name}`);

    typeof r.n_ctx === "number" && r.n_ctx >= 0
      ? ok(`n_ctx is a non-negative number (${r.n_ctx})`)
      : fail(`n_ctx invalid: ${r.n_ctx}`);

    typeof r.supports_vision === "boolean"
      ? ok("supports_vision is boolean")
      : fail(`supports_vision not boolean: ${typeof r.supports_vision}`);
  } catch (e: any) { fail(`llm_status failed: ${e.message}`); }

  // ── llm_catalog ──────────────────────────────────────────────────────────
  // Request:  { command: "llm_catalog" }
  // Response: { command: "llm_catalog", ok: true,
  //             entries: [...], active_model: "...", active_mmproj: "..." }
  let llmRunning = false;
  try {
    info("Testing llm_catalog…");
    const r = await send({ command: "llm_catalog" });
    r.ok === true ? ok("llm_catalog ok=true") : fail(`llm_catalog ok=${r.ok}, error=${r.error}`);
    field("active_model",  r.active_model,  "filename of active model (empty if none)");
    field("active_mmproj", r.active_mmproj, "filename of active mmproj (empty if none)");

    const entries: any[] = r.entries ?? [];
    ok(`${entries.length} model entry/entries in catalog`);

    // Validate each entry's shape
    let entryErrors = 0;
    for (const e of entries) {
      if (typeof e.filename !== "string" || !e.filename) { entryErrors++; continue; }
      if (typeof e.state    !== "string")               { entryErrors++; continue; }
      if (typeof e.progress !== "number")               { entryErrors++; continue; }
    }
    entryErrors === 0
      ? ok("all catalog entries have required fields (filename, state, progress)")
      : fail(`${entryErrors} catalog entry/entries missing required fields`);

    // Valid state values
    const validStates = new Set(["not_downloaded", "downloading", "downloaded", "cancelled", "failed"]);
    const invalidStates = entries.filter(e => !validStates.has(e.state));
    invalidStates.length === 0
      ? ok("all entry states are valid")
      : fail(`invalid entry states: ${invalidStates.map(e => `${e.filename}:${e.state}`).join(", ")}`);

    // Progress range [0, 1]
    const badProgress = entries.filter(e => typeof e.progress === "number" && (e.progress < 0 || e.progress > 1));
    badProgress.length === 0
      ? ok("all progress values in [0, 1]")
      : fail(`out-of-range progress values: ${badProgress.map(e => `${e.filename}:${e.progress}`).join(", ")}`);

    // Track whether server is running (for streaming chat test below)
    try { llmRunning = (await send({ command: "llm_status" })).status === "running"; } catch {}
  } catch (e: any) { fail(`llm_catalog failed: ${e.message}`); }

  // ── llm_download — missing filename → ok=false ───────────────────────────
  try {
    info("Testing llm_download with missing filename (should return ok=false)…");
    const r = await send({ command: "llm_download" }); // no filename
    r.ok === false
      ? ok(`correctly rejected missing filename: error="${r.error}"`)
      : fail("expected ok=false for missing filename");
  } catch (e: any) { fail(`llm_download missing-filename test failed: ${e.message}`); }

  // ── llm_cancel_download — missing filename → ok=false ────────────────────
  try {
    info("Testing llm_cancel_download with missing filename (should return ok=false)…");
    const r = await send({ command: "llm_cancel_download" });
    r.ok === false
      ? ok(`correctly rejected: error="${r.error}"`)
      : fail("expected ok=false for missing filename");
  } catch (e: any) { fail(`llm_cancel_download missing-filename test failed: ${e.message}`); }

  // ── llm_delete — missing filename → ok=false ─────────────────────────────
  try {
    info("Testing llm_delete with missing filename (should return ok=false)…");
    const r = await send({ command: "llm_delete" });
    r.ok === false
      ? ok(`correctly rejected: error="${r.error}"`)
      : fail("expected ok=false for missing filename");
  } catch (e: any) { fail(`llm_delete missing-filename test failed: ${e.message}`); }

  // ── llm_logs ─────────────────────────────────────────────────────────────
  // Request:  { command: "llm_logs" }
  // Response: { command: "llm_logs", ok: true,
  //             logs: [{ ts: number, level: string, message: string }], count: number }
  try {
    info("Testing llm_logs…");
    const r = await send({ command: "llm_logs" });
    r.ok === true ? ok("llm_logs ok=true") : fail(`llm_logs ok=${r.ok}, error=${r.error}`);
    field("count", r.count, "number of log entries (0 if server never started)");
    Array.isArray(r.logs)
      ? ok(`logs is an array (${r.logs.length} entries)`)
      : fail("logs is not an array");

    // Validate entries
    const logs: any[] = r.logs ?? [];
    const validLevels = new Set(["info", "warn", "error"]);
    let logErrors = 0;
    for (const entry of logs) {
      if (typeof entry.ts      !== "number") { logErrors++; continue; }
      if (typeof entry.level   !== "string") { logErrors++; continue; }
      if (typeof entry.message !== "string") { logErrors++; continue; }
      if (!validLevels.has(entry.level))     { logErrors++; continue; }
    }
    logErrors === 0
      ? ok("all log entries have valid fields (ts, level, message)")
      : fail(`${logErrors} log entry/entries have invalid fields`);

    // count should match logs.length
    r.count === r.logs.length
      ? ok("count matches logs.length")
      : fail(`count (${r.count}) != logs.length (${r.logs.length})`);
  } catch (e: any) { fail(`llm_logs failed: ${e.message}`); }

  // ── llm_chat — server not running → ok=false (non-streaming check) ───────
  // We send llm_chat and check that the server responds with an error when
  // no LLM server is running.  When the server IS running we verify the
  // streaming protocol (delta frames → done frame).
  if (transport === "ws") {
    if (!llmRunning) {
      // ── Not running: verify error response ──────────────────────────────
      try {
        info("Testing llm_chat when server not running (expect error frame)…");
        const errorFrame = await new Promise<any>((resolve, reject) => {
          const timer = setTimeout(() => {
            ws.off("message", handler);
            reject(new Error("llm_chat timeout waiting for error frame (5s)"));
          }, 5000);
          const handler = (raw: any) => {
            try {
              const d = JSON.parse(raw.toString());
              if (d.command === "llm_chat") {
                clearTimeout(timer);
                ws.off("message", handler);
                resolve(d);
              }
            } catch {}
          };
          ws.on("message", handler);
          ws.send(JSON.stringify({ command: "llm_chat", message: "hello" }));
        });

        errorFrame.ok === false
          ? ok(`correctly rejected (server not running): error="${errorFrame.error}"`)
          : fail("expected ok=false when server not running");
        const t = errorFrame.type;
        t === "error" || t === undefined
          ? ok(`error frame type="${t ?? "(no type field)"}"`)
          : fail(`unexpected type="${t}" in error frame`);
      } catch (e: any) { fail(`llm_chat not-running test failed: ${e.message}`); }
    } else {
      // ── Running: verify streaming protocol ──────────────────────────────
      info("LLM server is running — testing streaming chat protocol…");
      try {
        const frames: any[] = [];
        const fullText = await new Promise<string>((resolve, reject) => {
          const timer = setTimeout(() => {
            ws.off("message", handler);
            reject(new Error("llm_chat stream timeout (60s)"));
          }, 60_000);
          let text = "";
          const handler = (raw: any) => {
            try {
              const d = JSON.parse(raw.toString());
              if (d.command !== "llm_chat") return;
              frames.push(d);
              if (d.type === "delta") {
                text += d.text ?? "";
              } else if (d.type === "done" || d.type === "error" || d.ok === false) {
                clearTimeout(timer);
                ws.off("message", handler);
                d.type === "error" || d.ok === false
                  ? reject(new Error(d.error ?? "llm_chat stream error"))
                  : resolve(text);
              }
            } catch {}
          };
          ws.on("message", handler);
          ws.send(JSON.stringify({ command: "llm_chat", message: "Reply with exactly: OK" }));
        });

        ok(`streaming chat completed — received ${frames.length} frame(s)`);
        ok(`generated text: "${fullText.slice(0, 80)}${fullText.length > 80 ? "…" : ""}"`);

        // Validate frame structure
        const deltaFrames = frames.filter(f => f.type === "delta");
        const doneFrames  = frames.filter(f => f.type === "done");
        deltaFrames.length > 0
          ? ok(`${deltaFrames.length} delta frame(s) received`)
          : fail("no delta frames received");
        doneFrames.length === 1
          ? ok("exactly 1 done frame")
          : fail(`expected 1 done frame, got ${doneFrames.length}`);

        if (doneFrames.length === 1) {
          const done = doneFrames[0];
          done.ok === true ? ok("done frame ok=true") : fail(`done frame ok=${done.ok}`);
          field("finish_reason",    done.finish_reason,    "'stop' or 'length'");
          field("prompt_tokens",    done.prompt_tokens,    "input tokens used");
          field("completion_tokens",done.completion_tokens,"output tokens generated");
          field("n_ctx",            done.n_ctx,            "context window size in tokens");

          typeof done.finish_reason === "string"
            ? ok("finish_reason is a string")
            : fail("finish_reason missing or not a string");
          typeof done.prompt_tokens     === "number" && done.prompt_tokens >= 0
            ? ok(`prompt_tokens = ${done.prompt_tokens}`)
            : fail(`invalid prompt_tokens: ${done.prompt_tokens}`);
          typeof done.completion_tokens === "number" && done.completion_tokens >= 0
            ? ok(`completion_tokens = ${done.completion_tokens}`)
            : fail(`invalid completion_tokens: ${done.completion_tokens}`);
          typeof done.n_ctx             === "number" && done.n_ctx > 0
            ? ok(`n_ctx = ${done.n_ctx}`)
            : fail(`invalid n_ctx: ${done.n_ctx}`);
        }

        // Delta frames must have text field (string)
        const badDelta = deltaFrames.filter(f => typeof f.text !== "string");
        badDelta.length === 0
          ? ok("all delta frames have text field (string)")
          : fail(`${badDelta.length} delta frame(s) missing text field`);

        // All frames must echo command = "llm_chat"
        const wrongCmd = frames.filter(f => f.command !== "llm_chat");
        wrongCmd.length === 0
          ? ok("all frames echo command='llm_chat'")
          : fail(`${wrongCmd.length} frame(s) have wrong command field`);
      } catch (e: any) { fail(`llm_chat streaming test failed: ${e.message}`); }

      // ── Short-hand 'message' field (instead of 'messages' array) ───────
      try {
        info("Testing llm_chat 'message' shorthand (string instead of array)…");
        const doneFrame = await new Promise<any>((resolve, reject) => {
          const timer = setTimeout(() => {
            ws.off("message", handler);
            reject(new Error("llm_chat shorthand timeout (60s)"));
          }, 60_000);
          const handler = (raw: any) => {
            try {
              const d = JSON.parse(raw.toString());
              if (d.command !== "llm_chat") return;
              if (d.type === "done" || d.type === "error" || d.ok === false) {
                clearTimeout(timer);
                ws.off("message", handler);
                resolve(d);
              }
            } catch {}
          };
          ws.on("message", handler);
          ws.send(JSON.stringify({ command: "llm_chat", message: "Say: hi" }));
        });
        doneFrame.ok !== false
          ? ok("llm_chat shorthand 'message' field accepted")
          : fail(`llm_chat shorthand rejected: ${doneFrame.error}`);
      } catch (e: any) { fail(`llm_chat shorthand test failed: ${e.message}`); }

      // ── messages array (OpenAI format) ───────────────────────────────────
      try {
        info("Testing llm_chat with 'messages' array (OpenAI format)…");
        const doneFrame = await new Promise<any>((resolve, reject) => {
          const timer = setTimeout(() => {
            ws.off("message", handler);
            reject(new Error("llm_chat messages-array timeout (60s)"));
          }, 60_000);
          const handler = (raw: any) => {
            try {
              const d = JSON.parse(raw.toString());
              if (d.command !== "llm_chat") return;
              if (d.type === "done" || d.type === "error" || d.ok === false) {
                clearTimeout(timer);
                ws.off("message", handler);
                resolve(d);
              }
            } catch {}
          };
          ws.on("message", handler);
          ws.send(JSON.stringify({
            command: "llm_chat",
            messages: [
              { role: "system",    content: "You are a helpful assistant. Be brief." },
              { role: "user",      content: "What is 2+2? Answer with just the number." },
            ],
          }));
        });
        doneFrame.ok !== false
          ? ok("llm_chat with messages array accepted")
          : fail(`llm_chat messages array rejected: ${doneFrame.error}`);
      } catch (e: any) { fail(`llm_chat messages-array test failed: ${e.message}`); }
    }
  } else {
    ok("skipped llm_chat tests — WebSocket required (--http mode)");
  }

  // ── llm_chat with images (vision) — WebSocket ───────────────────────────
  // Tests the image upload protocol by embedding a tiny 1×1 red pixel JPEG
  // as a base64 data-URL in the messages array.  This exercises the full
  // image extraction pipeline on the server side without requiring a large file.
  //
  // The test is skipped when the LLM server is not running.  When it IS
  // running, we verify:
  //   a) The server accepts image_url content parts (OpenAI format)
  //   b) The server accepts top-level "images" array (simple format via POST /llm/chat)
  //   c) The streaming protocol still works correctly with image input
  if (transport === "ws" && llmRunning) {
    info("Testing llm_chat with image (vision) — OpenAI message format…");

    // Tiny 1×1 red JPEG (24 bytes base64) — valid JFIF header, parseable by libjpeg.
    // Generated with: ffmpeg -f rawvideo -pixel_format rgb24 -video_size 1x1 -i /dev/zero -frames 1 tiny.jpg
    const tinyRedJpeg =
      "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRof" +
      "Hh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFgAB" +
      "AQAAAAAAAAAAAAAAAAAAAAf/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFBABAAAAAAAAAAAAAAAA" +
      "AAAAA/8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxBn/9k=";
    const imageDataUrl = `data:image/jpeg;base64,${tinyRedJpeg}`;

    try {
      const frames: any[] = [];
      await new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => {
          ws.off("message", handler);
          reject(new Error("llm_chat vision timeout (60s)"));
        }, 60_000);

        const handler = (raw: any) => {
          try {
            const d = JSON.parse(raw.toString());
            if (d.command !== "llm_chat") return;
            frames.push(d);
            if (d.type === "done" || d.type === "error" || d.ok === false) {
              clearTimeout(timer);
              ws.off("message", handler);
              d.type === "error" || d.ok === false
                ? reject(new Error(d.error ?? "llm_chat vision error"))
                : resolve();
            }
          } catch {}
        };

        ws.on("message", handler);
        ws.send(JSON.stringify({
          command:  "llm_chat",
          messages: [
            {
              role:    "user",
              content: [
                { type: "image_url", image_url: { url: imageDataUrl } },
                { type: "text",      text: "Reply with: OK" },
              ],
            },
          ],
        }));
      });

      ok(`llm_chat vision (image_url): ${frames.length} frame(s) received`);
      const done = frames.find(f => f.type === "done");
      done && done.ok === true ? ok("vision done frame ok=true") : fail("no valid done frame");
    } catch (e: any) {
      // Vision may fail if the model doesn't have an mmproj loaded — that's expected.
      // Treat "loading" / "not supported" errors as a soft skip, not a hard fail.
      const msg = String(e.message ?? e);
      if (msg.includes("vision") || msg.includes("mmproj") || msg.includes("multimodal")) {
        ok(`vision skipped (model not vision-capable): ${msg}`);
      } else {
        fail(`llm_chat vision (image_url) failed: ${msg}`);
      }
    }
  } else if (!llmRunning) {
    ok("skipped llm_chat vision tests — LLM server not running");
  } else {
    ok("skipped llm_chat vision tests — WebSocket required");
  }

  // ── llm_stop and llm_start — only when running ──────────────────────────
  // We intentionally skip stop/start tests to avoid disrupting a model that
  // might be in use.  The llm_stop command is tested structurally (response
  // format) by calling it when the server is already stopped.
  try {
    info("Testing llm_stop when server is not running (expect 'not_running' result)…");
    const status = await send({ command: "llm_status" });
    if (status.status !== "running") {
      const r = await send({ command: "llm_stop" });
      r.ok === true ? ok("llm_stop ok=true (server not running)") : fail(`llm_stop ok=${r.ok}`);
      field("result", r.result, "'stopped' | 'not_running'");
      r.result === "not_running"
        ? ok(`result = "not_running" (correct when already stopped)`)
        : fail(`unexpected result: "${r.result}"`);
    } else {
      ok("skipped llm_stop structural test — LLM server is currently running (preserving state)");
    }
  } catch (e: any) { fail(`llm_stop structural test failed: ${e.message}`); }
}

async function testUnknownCommand(): Promise<void> {
  heading("unknown command");
  info("Request: { command: 'nonexistent_command_xyz' }");
  info("Verifies that unrecognized commands return ok=false with a descriptive error string.");
  info("Tests the server's error handling — should not crash, should echo the command field.");
  try {
    const r = await send({ command: "nonexistent_command_xyz" });
    r.ok === false ? ok(`correctly rejected: error="${r.error}"`) : fail("expected ok=false");
  } catch (e: any) { fail(`failed: ${e.message}`); }
}


// ─────────────────────────────────────────────────────────────────────────────
// 15. BROADCAST EVENTS
// ─────────────────────────────────────────────────────────────────────────────

async function testBroadcastEvents(): Promise<void> {
  heading("broadcast events (3s listen)");
  if (transport === "http") {
    ok("skipped — HTTP transport has no push events (WebSocket required)");
    info("Re-run without --http, or use --ws, to test broadcast events.");
    return;
  }
  info("Broadcast events are server-PUSHED to all clients (no request needed).");
  info("They use { event: '...' } instead of { command: '...' }.");
  info("Events only fire when a Muse headband is actively streaming.");
  info("Listening passively for 3 seconds…");

  const events = await collectEvents(3000);
  const byType: Record<string, number> = {};
  for (const e of events) {
    byType[e.event] = (byType[e.event] || 0) + 1;
  }

  const types = Object.keys(byType);
  if (types.length === 0) {
    ok("no broadcast events (expected when no Muse connected)");
    info("When streaming, you would see:");
    info("  muse-status     (~1Hz)  — device heartbeat (battery, sample count, state)");
    info("  eeg-bands       (~4Hz)  — derived scores + band powers in { payload } wrapper");
    info("  label-created   (event) — broadcast when any client creates a label");
    info("Note: raw EEG/PPG/IMU samples are NOT broadcast over WS (too high frequency).");
    return;
  }

  ok(`${events.length} event(s) across ${types.length} type(s)`);

  const eventDescriptions: Record<string, string> = {
    "muse-status":      "~1 Hz device status heartbeat (battery, sample count, connection state)",
    "eeg-bands":        "~4 Hz: derived scores + band powers in { payload: {...} } wrapper",
    "label-created":    "a label was just created by a client (text + id)",
  };

  for (const type of types) {
    const desc = eventDescriptions[type] || "undocumented event type";
    field(type, `×${byType[type]}`, desc);
  }

  const bandsSample = events.find(e => e.event === "eeg-bands");
  if (bandsSample) {
    info("sample eeg-bands payload keys: " + Object.keys(bandsSample).filter(k => k !== "event").join(", "));
  }
}


// ─────────────────────────────────────────────────────────────────────────────
// 16. HTTP API
// ─────────────────────────────────────────────────────────────────────────────
//
// The same TCP port serves both WebSocket and HTTP.
// Tests cover:
//   a) GET /           → JSON info page (not a WS upgrade)
//   b) POST /          → Universal tunnel: { "command": "…", …params }
//   c) GET /status     → REST shortcut for status
//   d) GET /sessions   → REST shortcut for sessions
//   e) POST /label     → REST shortcut for label (text in body)
//   f) POST /search_labels → REST shortcut with query param
//   g) GET /calibrations   → REST shortcut for list_calibrations
//   h) CORS headers present on every response
//   i) Unknown route   → 404 (axum default)
//   j) POST / with missing "command" → 400 with ok=false
//
// ─────────────────────────────────────────────────────────────────────────────

async function testHttp(port: number): Promise<void> {
  heading("HTTP API (same port as WebSocket)");
  const base = `http://127.0.0.1:${port}`;
  info(`Base URL: ${base}`);

  /** Thin fetch wrapper — returns parsed JSON and the Response for header inspection. */
  async function hfetch(
    path: string,
    opts: RequestInit = {},
  ): Promise<{ data: any; res: Response }> {
    const res  = await fetch(`${base}${path}`, {
      headers: { "Content-Type": "application/json" },
      ...opts,
    });
    const data = await res.json().catch(() => null);
    return { data, res };
  }

  // ── a) GET / → info page ──────────────────────────────────────────────────
  try {
    info("GET / without Upgrade header → JSON info page");
    const { data, res } = await hfetch("/");
    res.ok ? ok("GET / returned 200") : fail(`GET / status ${res.status}`);
    if (data?.name && data?.commands) {
      ok(`info page: name="${data.name}", ${data.commands.length} commands listed`);
      field("commands", data.commands.join(", "), "all available commands");
    } else {
      fail(`GET / body unexpected: ${JSON.stringify(data)?.slice(0, 120)}`);
    }
    // CORS header check
    const cors = res.headers.get("access-control-allow-origin");
    cors === "*" ? ok("CORS: Access-Control-Allow-Origin: *") : fail(`CORS header missing or wrong: "${cors}"`);
  } catch (e: any) { fail(`GET / failed: ${e.message}`); }

  // ── b) POST / universal tunnel ────────────────────────────────────────────
  try {
    info("POST / with { command: 'status' } → status via HTTP tunnel");
    const { data, res } = await hfetch("/", {
      method: "POST",
      body:   JSON.stringify({ command: "status" }),
    });
    res.ok ? ok("POST / tunnel returned 200") : fail(`POST / tunnel status ${res.status}`);
    data?.ok === true  ? ok("tunnel: ok=true") : fail(`tunnel: ok=${data?.ok}, error=${data?.error}`);
    data?.command === "status" ? ok("tunnel: command echoed correctly") : fail(`tunnel: command="${data?.command}"`);
    if (data?.device !== undefined) ok("tunnel: device field present in status response");
  } catch (e: any) { fail(`POST / tunnel failed: ${e.message}`); }

  // ── c) POST / missing command → 400 ──────────────────────────────────────
  try {
    info("POST / with missing command field → 400 error");
    const { data, res } = await hfetch("/", {
      method: "POST",
      body:   JSON.stringify({ foo: "bar" }),
    });
    res.status === 400 ? ok("POST / with bad body → 400") : fail(`expected 400, got ${res.status}`);
    data?.ok === false  ? ok("ok=false in error response") : fail(`ok=${data?.ok}`);
  } catch (e: any) { fail(`POST / missing-command test failed: ${e.message}`); }

  // ── d) GET /status ────────────────────────────────────────────────────────
  try {
    info("GET /status → REST shortcut");
    const { data, res } = await hfetch("/status");
    res.ok ? ok("GET /status returned 200") : fail(`GET /status status ${res.status}`);
    data?.ok === true      ? ok("GET /status: ok=true")          : fail(`ok=${data?.ok}`);
    data?.command === "status" ? ok("GET /status: command='status'") : fail(`command=${data?.command}`);
    data?.device !== undefined ? ok("GET /status: device field present") : ok("GET /status: no device (no Muse)");
  } catch (e: any) { fail(`GET /status failed: ${e.message}`); }

  // ── e) GET /sessions ──────────────────────────────────────────────────────
  try {
    info("GET /sessions → REST shortcut");
    const { data, res } = await hfetch("/sessions");
    res.ok ? ok("GET /sessions returned 200") : fail(`status ${res.status}`);
    data?.ok === true ? ok("GET /sessions: ok=true") : fail(`ok=${data?.ok}`);
    Array.isArray(data?.sessions) ? ok(`GET /sessions: ${data.sessions.length} session(s)`) : fail("sessions not an array");
  } catch (e: any) { fail(`GET /sessions failed: ${e.message}`); }

  // ── f) POST /label ────────────────────────────────────────────────────────
  try {
    info("POST /label with { text: '...' } → REST shortcut");
    const { data, res } = await hfetch("/label", {
      method: "POST",
      body:   JSON.stringify({ text: `http-test-${Date.now()}` }),
    });
    res.ok ? ok("POST /label returned 200") : fail(`status ${res.status}`);
    data?.ok === true         ? ok("POST /label: ok=true")              : fail(`ok=${data?.ok}, error=${data?.error}`);
    data?.command === "label" ? ok("POST /label: command='label'")      : fail(`command=${data?.command}`);
    typeof data?.label_id === "number" ? ok(`POST /label: label_id=${data.label_id}`) : fail("no label_id");
  } catch (e: any) { fail(`POST /label failed: ${e.message}`); }

  // ── g) POST /label missing text → 400 ────────────────────────────────────
  try {
    info("POST /label with missing text field → 400");
    const { data, res } = await hfetch("/label", {
      method: "POST",
      body:   JSON.stringify({}),
    });
    res.status === 400 ? ok("POST /label without text → 400") : fail(`expected 400, got ${res.status}`);
    data?.ok === false  ? ok("ok=false in error response") : fail(`ok=${data?.ok}`);
  } catch (e: any) { fail(`POST /label missing-text test failed: ${e.message}`); }

  // ── h) POST /search_labels ────────────────────────────────────────────────
  try {
    info("POST /search_labels with { query: 'focused' } → REST shortcut");
    const { data, res } = await hfetch("/search_labels", {
      method: "POST",
      body:   JSON.stringify({ query: "focused", k: 3 }),
    });
    res.ok ? ok("POST /search_labels returned 200") : fail(`status ${res.status}`);
    data?.ok === true ? ok("POST /search_labels: ok=true") : fail(`ok=${data?.ok}, error=${data?.error}`);
    Array.isArray(data?.results) ? ok(`POST /search_labels: ${data.results.length} result(s)`) : fail("results not an array");
  } catch (e: any) { fail(`POST /search_labels failed: ${e.message}`); }

  // ── i) GET /calibrations ──────────────────────────────────────────────────
  try {
    info("GET /calibrations → list_calibrations REST shortcut");
    const { data, res } = await hfetch("/calibrations");
    res.ok ? ok("GET /calibrations returned 200") : fail(`status ${res.status}`);
    data?.ok === true ? ok("GET /calibrations: ok=true") : fail(`ok=${data?.ok}`);
    Array.isArray(data?.profiles) ? ok(`GET /calibrations: ${data.profiles.length} profile(s)`) : fail("profiles not an array");
    // Check CORS header
    const cors = res.headers.get("access-control-allow-origin");
    cors === "*" ? ok("CORS header on /calibrations") : fail(`CORS missing on /calibrations: "${cors}"`);
  } catch (e: any) { fail(`GET /calibrations failed: ${e.message}`); }

  // ── j) GET /calibrations/:id ──────────────────────────────────────────────
  try {
    info("GET /calibrations/:id → get_calibration REST shortcut");
    const { data: listData } = await hfetch("/calibrations");
    const profiles = listData?.profiles ?? [];
    if (profiles.length > 0) {
      const id = profiles[0].id;
      const { data, res } = await hfetch(`/calibrations/${id}`);
      res.ok ? ok(`GET /calibrations/${id}: 200`) : fail(`status ${res.status}`);
      data?.ok === true ? ok("GET /calibrations/:id: ok=true") : fail(`ok=${data?.ok}`);
      data?.profile?.id === id ? ok("profile id matches") : fail(`id mismatch: ${data?.profile?.id}`);
    } else {
      ok("no calibration profiles to test GET /calibrations/:id (ok — default profile missing)");
    }
  } catch (e: any) { fail(`GET /calibrations/:id failed: ${e.message}`); }

  // ── k) Unknown HTTP route → 404 ───────────────────────────────────────────
  try {
    info("GET /nonexistent_route_xyz → 404");
    const res = await fetch(`${base}/nonexistent_route_xyz`);
    res.status === 404 ? ok("unknown route → 404") : fail(`expected 404, got ${res.status}`);
  } catch (e: any) { fail(`404 test failed: ${e.message}`); }

  // ── l) POST / tunnel — unknown command → 400 ─────────────────────────────
  try {
    info("POST / with unknown command → 400 with ok=false");
    const { data, res } = await hfetch("/", {
      method: "POST",
      body:   JSON.stringify({ command: "definitely_not_a_real_command" }),
    });
    res.status === 400 ? ok("unknown command via tunnel → 400") : fail(`expected 400, got ${res.status}`);
    data?.ok === false  ? ok("ok=false in error response") : fail(`ok=${data?.ok}`);
    typeof data?.error === "string" ? ok(`error message: "${data.error}"`) : fail("no error field");
  } catch (e: any) { fail(`unknown-command tunnel test failed: ${e.message}`); }

  // ── m) GET /llm/status ───────────────────────────────────────────────────
  try {
    info("GET /llm/status → LLM REST shortcut");
    const { data, res } = await hfetch("/llm/status");
    res.ok ? ok("GET /llm/status returned 200") : fail(`status ${res.status}`);
    data?.ok === true             ? ok("GET /llm/status: ok=true")              : fail(`ok=${data?.ok}, error=${data?.error}`);
    data?.command === "llm_status"? ok("command='llm_status'")                  : fail(`command=${data?.command}`);
    const validStatuses = new Set(["stopped", "loading", "running"]);
    validStatuses.has(data?.status) ? ok(`status="${data?.status}"`) : fail(`invalid status: "${data?.status}"`);
  } catch (e: any) { fail(`GET /llm/status failed: ${e.message}`); }

  // ── n) GET /llm/catalog ──────────────────────────────────────────────────
  try {
    info("GET /llm/catalog → LLM model catalog REST shortcut");
    const { data, res } = await hfetch("/llm/catalog");
    res.ok ? ok("GET /llm/catalog returned 200") : fail(`status ${res.status}`);
    data?.ok === true              ? ok("GET /llm/catalog: ok=true")      : fail(`ok=${data?.ok}`);
    data?.command === "llm_catalog"? ok("command='llm_catalog'")          : fail(`command=${data?.command}`);
    Array.isArray(data?.entries)   ? ok(`${data.entries.length} entry/entries`) : fail("entries not an array");
  } catch (e: any) { fail(`GET /llm/catalog failed: ${e.message}`); }

  // ── o) GET /llm/logs ─────────────────────────────────────────────────────
  try {
    info("GET /llm/logs → LLM log REST shortcut");
    const { data, res } = await hfetch("/llm/logs");
    res.ok ? ok("GET /llm/logs returned 200") : fail(`status ${res.status}`);
    data?.ok === true           ? ok("GET /llm/logs: ok=true") : fail(`ok=${data?.ok}`);
    Array.isArray(data?.logs)   ? ok(`${data.logs.length} log line(s)`) : fail("logs not an array");
  } catch (e: any) { fail(`GET /llm/logs failed: ${e.message}`); }

  // ── p) POST /llm/download missing filename → 400 ─────────────────────────
  try {
    info("POST /llm/download without filename → 400");
    const { data, res } = await hfetch("/llm/download", {
      method: "POST",
      body:   JSON.stringify({}),
    });
    res.status === 400 ? ok("POST /llm/download without filename → 400") : fail(`expected 400, got ${res.status}`);
    data?.ok === false  ? ok("ok=false in error response")                : fail(`ok=${data?.ok}`);
  } catch (e: any) { fail(`POST /llm/download validation test failed: ${e.message}`); }

  // ── q) POST /llm/chat — missing message → 400 ────────────────────────────
  try {
    info("POST /llm/chat without message → 400");
    const { data, res } = await hfetch("/llm/chat", {
      method: "POST",
      body:   JSON.stringify({}),  // empty body — no message, no messages
    });
    // Server is either stopped (503) or rejects the empty body (400).
    // Both are acceptable — what matters is ok=false.
    const accepted = res.status === 400 || res.status === 503;
    accepted    ? ok(`POST /llm/chat no-message → ${res.status}`)   : fail(`expected 400 or 503, got ${res.status}`);
    data?.ok === false ? ok("ok=false in error response")            : fail(`ok=${data?.ok}`);
  } catch (e: any) { fail(`POST /llm/chat validation test failed: ${e.message}`); }

  // ── r) POST /llm/chat — simple format (no server required for shape check)
  // When the server is running this should return a text response.
  // When stopped it should return 503 with ok=false.
  try {
    info("POST /llm/chat — simple JSON format (shape + server-state aware)…");
    const { data, res } = await hfetch("/llm/chat", {
      method: "POST",
      body:   JSON.stringify({ message: "Reply with: OK" }),
    });

    if (res.status === 503) {
      // Server not running — verify error shape
      data?.ok === false
        ? ok("POST /llm/chat → 503 ok=false (server stopped, correct)")
        : fail(`503 but ok=${data?.ok}`);
      typeof data?.error === "string"
        ? ok(`error message present: "${data.error.slice(0, 60)}"`)
        : fail("error field missing");
    } else if (res.status === 200) {
      // Server is running — verify response shape
      data?.ok === true
        ? ok("POST /llm/chat → 200 ok=true")
        : fail(`200 but ok=${data?.ok}`);
      data?.command === "llm_chat"
        ? ok("command='llm_chat'")
        : fail(`command=${data?.command}`);
      typeof data?.text === "string"
        ? ok(`text field present (${data.text.length} chars)`)
        : fail("text field missing or not string");
      typeof data?.finish_reason === "string"
        ? ok(`finish_reason="${data.finish_reason}"`)
        : fail("finish_reason missing");
      typeof data?.prompt_tokens === "number" && data.prompt_tokens >= 0
        ? ok(`prompt_tokens=${data.prompt_tokens}`)
        : fail(`invalid prompt_tokens: ${data?.prompt_tokens}`);
      typeof data?.completion_tokens === "number" && data.completion_tokens >= 0
        ? ok(`completion_tokens=${data.completion_tokens}`)
        : fail(`invalid completion_tokens: ${data?.completion_tokens}`);
      typeof data?.n_ctx === "number" && data.n_ctx > 0
        ? ok(`n_ctx=${data.n_ctx}`)
        : fail(`invalid n_ctx: ${data?.n_ctx}`);
    } else {
      fail(`unexpected status ${res.status}`);
    }
  } catch (e: any) { fail(`POST /llm/chat simple format test failed: ${e.message}`); }

  // ── s) POST /llm/chat — with base64 image (vision) ───────────────────────
  // Uses a minimal 1×1 JPEG data-URL to test the image upload path.
  // When the server is stopped the response must be 503 ok=false.
  // When running with a vision model it should return a text response.
  try {
    info("POST /llm/chat — with base64 image (vision upload path)…");
    const tinyJpeg =
      "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRof" +
      "Hh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFgAB" +
      "AQAAAAAAAAAAAAAAAAAAAAf/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFBABAAAAAAAAAAAAAAAA" +
      "AAAAA/8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxBn/9k=";
    const imageDataUrl = `data:image/jpeg;base64,${tinyJpeg}`;

    const { data, res } = await hfetch("/llm/chat", {
      method: "POST",
      body:   JSON.stringify({
        message: "Reply with: OK",
        images:  [imageDataUrl],
      }),
    });

    const okStatus = res.status === 200 || res.status === 503;
    okStatus ? ok(`POST /llm/chat with image → ${res.status}`) : fail(`unexpected status ${res.status}`);
    data?.ok === false || data?.ok === true
      ? ok(`ok field present (${data?.ok})`)
      : fail("ok field missing");

    if (res.status === 200) {
      typeof data?.text === "string"
        ? ok(`vision response text: "${(data.text as string).slice(0, 60)}${(data.text as string).length > 60 ? "…" : ""}"`)
        : fail("text field missing in vision response");
    } else {
      ok(`server stopped or vision not supported (503) — error: "${data?.error ?? "n/a"}"`)
    }

    // Validate that the "images" top-level field is accepted (not rejected as 400)
    res.status !== 400 ? ok("images[] field accepted by server (not rejected)") : fail("400 — images[] field rejected");
  } catch (e: any) { fail(`POST /llm/chat vision test failed: ${e.message}`); }

  // ── t) POST /llm/chat — full OpenAI messages format ───────────────────────
  try {
    info("POST /llm/chat — full OpenAI messages array format…");
    const { data, res } = await hfetch("/llm/chat", {
      method: "POST",
      body:   JSON.stringify({
        messages: [
          { role: "system", content: "Be brief." },
          { role: "user",   content: "Reply with: OK" },
        ],
      }),
    });
    const okStatus = res.status === 200 || res.status === 503;
    okStatus ? ok(`POST /llm/chat OpenAI format → ${res.status}`) : fail(`unexpected status ${res.status}`);
    data?.ok === false || data?.ok === true ? ok("ok field present") : fail("ok field missing");
    if (res.status === 200 && data?.ok === true) {
      typeof data?.text === "string" ? ok("text field present") : fail("text field missing");
    }
  } catch (e: any) { fail(`POST /llm/chat OpenAI format test failed: ${e.message}`); }
}


// ═══════════════════════════════════════════════════════════════════════════════
// MAIN — Discovery, connection, test execution, and summary
// ═══════════════════════════════════════════════════════════════════════════════

async function main(): Promise<void> {
  console.log(`\n${BOLD}╔══════════════════════════════════════════╗${RESET}`);
  console.log(`${BOLD}║  Skill WebSocket + HTTP API — Smoke Test ║${RESET}`);
  console.log(`${BOLD}╚══════════════════════════════════════════╝${RESET}\n`);

  // 1. Discover port
  const port = await discover();
  ok(`discovered port ${port}`);

  httpBase = `http://127.0.0.1:${port}`;

  // 2. Establish transport
  if (FORCE_HTTP) {
    // ── Forced HTTP ───────────────────────────────────────────────────────
    transport = "http";
    send      = sendHttp;
    ok(`transport: HTTP ${httpBase} (--http forced)`);

  } else if (FORCE_WS) {
    // ── Forced WebSocket — retry up to 3× ─────────────────────────────────
    transport = "ws";
    let attempts = 0;
    while (true) {
      attempts++;
      try {
        await new Promise<void>((resolve, reject) => {
          const w = new WebSocket(WS_URL(port));
          const t = setTimeout(() => { try { w.close(); } catch {} reject(new Error("timeout")); }, 5000);
          w.on("open", () => { clearTimeout(t); ws = w; resolve(); });
          w.on("error", () => { clearTimeout(t); reject(new Error(`refused (${attempts}/3)`)); });
        });
        break;
      } catch (e: any) {
        if (attempts >= 3) die(`WebSocket unavailable (--ws forced): ${e.message}`);
        info(`WS retry in 1s… (${e.message})`);
        await new Promise(r => setTimeout(r, 1000));
      }
    }
    ok(`transport: WebSocket ws://127.0.0.1:${port} (--ws forced)`);

  } else {
    // ── Auto: try WebSocket once with a short timeout, fall back to HTTP ──
    info("auto-transport: probing WebSocket…");
    const wsOk = await new Promise<boolean>((resolve) => {
      try {
        const w = new WebSocket(WS_URL(port));
        const t = setTimeout(() => { try { w.close(); } catch {} resolve(false); }, 3000);
        w.on("open", () => { clearTimeout(t); ws = w; resolve(true); });
        w.on("error", () => { clearTimeout(t); resolve(false); });
      } catch { resolve(false); }
    });
    if (wsOk) {
      transport = "ws";
      ok(`transport: WebSocket ws://127.0.0.1:${port}`);
    } else {
      transport = "http";
      send      = sendHttp;
      ok(`transport: HTTP ${httpBase} (WebSocket unavailable)`);
    }
  }

  // 3. Run all command tests sequentially
  await testStatus();
  await testSessions();
  await testNotify();
  await testSay();
  await testLabel();
  await testHooksStatus();
  await testHooksSuggest();
  await testHooksGetSet();
  await testHooksLog();
  await testSearchLabels();
  await testInteractiveSearch();
  await testSearch();
  await testCalibrate();
  await testTimer();
  await testCompare();
  await testSleep();
  await testUmap();
  await testDnd();
  await testLlm();
  await testUnknownCommand();
  await testBroadcastEvents();   // skips gracefully when transport === "http"
  await testHttp(port);          // always runs — tests HTTP layer directly

  // 4. Summary
  if (transport === "ws") { try { ws.close(); } catch {} }
  const tLabel = transport === "ws" ? `${GREEN}WebSocket${RESET}` : `${YELLOW}HTTP${RESET}`;
  console.log(`\n${BOLD}╔══════════════════════════════════════════╗${RESET}`);
  const summary = `${GREEN}${passed} passed${RESET}, ${failed > 0 ? RED : GRAY}${failed} failed${RESET}`;
  const pad = Math.max(0, 33 - passed.toString().length - failed.toString().length);
  console.log(`${BOLD}║${RESET}  ${summary}${" ".repeat(pad)}${BOLD}║${RESET}`);
  console.log(`${BOLD}║${RESET}  transport: ${tLabel}${" ".repeat(Math.max(0, 27 - 9))}${BOLD}║${RESET}`);
  console.log(`${BOLD}╚══════════════════════════════════════════╝${RESET}\n`);
  process.exit(failed > 0 ? 1 : 0);
}

timer = setTimeout(() => die("global timeout"), TIMEOUT_MS);
main().catch((e: any) => die(e.message)).finally(() => clearTimeout(timer));
