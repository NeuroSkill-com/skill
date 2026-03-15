// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
#!/usr/bin/env npx tsx
/** Current CLI version — bump when breaking changes are made. */
const CLI_VERSION = "1.1.0";

/**
 * cli.ts — Command-line interface for the Skill WebSocket API.
 *
 * Supported devices: Muse (4ch), OpenBCI Ganglion (4ch), Neurable MW75 Neuro (12ch), Hermes V1 (8ch).
 * All commands work identically regardless of which device is connected.
 *
 * Usage:
 *   npx tsx cli.ts <command> [options]
 *
 * Commands:
 *   status                         Full device/session/embeddings/scores snapshot
 *   session [index]                All metrics + trends for one session (0=latest, 1=prev, …)
 *   sessions                       List all recording sessions across all days
 *   say "text"                      Speak text aloud via on-device TTS (fire-and-forget)
 *   notify "title" ["body"]        Show a native OS notification
 *   label "text"                   Create a timestamped annotation on the current moment
 *   search-labels "query"          Search labels by free text (text/context/both modes)
 *   interactive "keyword"          Cross-modal 4-layer graph search (labels → EEG → found labels)
 *   search                         ANN EEG-similarity search (auto: last session, k=5)
 *   compare                        Side-by-side A/B metrics (auto: last 2 sessions)
 *   sleep [index]                  Sleep staging — index selects session (0=latest, 1=prev, …)
 *   calibrate                      Open calibration window and start immediately
 *   timer                          Open focus-timer window and start work phase immediately
 *   umap                           3D UMAP projection with live progress bar
 *   listen                         Stream broadcast events for N seconds
 *   hooks                          List Proactive Hook rules, scenarios, and last-trigger metadata
 *   hooks list                     List raw hook rules (name, keywords, threshold, …)
 *   hooks add <name> [opts]        Add a new hook rule
 *   hooks remove <name>            Delete a hook by name
 *   hooks enable <name>            Enable a hook
 *   hooks disable <name>           Disable a hook
 *   hooks update <name> [opts]     Update fields on an existing hook
 *   hooks suggest "kw1,kw2"        Suggest threshold from real EEG/label data
 *   hooks log [--limit N --offset M]  View paginated hook trigger audit log rows
 *   dnd                            Show DND automation status (config + live eligibility + OS state)
 *   dnd on                         Force-enable DND immediately (bypass EEG threshold)
 *   dnd off                        Force-disable DND immediately
 *   llm status                     LLM server status (stopped/loading/running)
 *   llm start                      Load active model and start LLM inference server
 *   llm stop                       Stop LLM inference server and free GPU memory
 *   llm catalog                    Show model catalog with download states
 *   llm add <repo> <filename>      Add an external HF model to the catalog and download it
 *   llm add <hf-url>               Add from a full HuggingFace URL
 *   llm add ... --mmproj <file>    Also add and download a vision projector from the same repo
 *   llm select <filename>          Set the active text model
 *   llm mmproj <filename|none>     Set the active vision projector (or "none" to disable)
 *   llm autoload-mmproj <on|off>   Toggle auto-loading of vision projector on start
 *   llm download <filename>        Download a GGUF model (fire-and-forget; poll catalog for progress)
 *   llm pause <filename>           Pause an in-progress model download
 *   llm resume <filename>          Resume a paused model download
 *   llm cancel <filename>          Cancel an in-progress model download
 *   llm delete <filename>          Delete a locally-cached model file
 *   llm downloads                  List all downloads with status and progress
 *   llm refresh                    Re-probe the HF Hub cache for externally downloaded models
 *   llm fit                        Check which models fit in available RAM/VRAM
 *   llm logs                       Print last 500 LLM server log lines
 *   llm chat                       Interactive multi-turn chat REPL (WebSocket only)
 *   llm chat "message"             Single-shot: send one message and stream the reply
 *   llm chat "describe" --image a.jpg --image b.png   Vision: attach images to message
 *
 *   The LLM supports built-in tool calling (bash, read/write/edit, web search/fetch)
 *   which are executed server-side and results fed back into the conversation.
 *
 *   raw '{"command":"..."}'        Send arbitrary JSON, print full response
 *
 * Transport selection (default: try WebSocket, fall back to HTTP):
 *   --ws            Force WebSocket (error if unavailable)
 *   --http          Force HTTP REST (no live events)
 *   (neither)       Auto: try WebSocket, silently fall back to HTTP
 *
 * All time-range commands auto-select from your actual session history when
 * no --start/--end flags are given. The resolved parameters are printed as a
 * rerun command you can copy-paste for reproducible results.
 *
 * Options:
 *   --port <n>      Connect to explicit port (skips mDNS discovery)
 *   --json          Output raw JSON (no colors, pipeable to jq)
 *   --full          Print full JSON response in addition to the human-readable summary
 *   --dot           (interactive) Output Graphviz DOT format (pipe to dot -Tsvg)
 *   --mode <m>      Search mode for search-labels: text|context|both (default: text)
 *   --k-text <n>    (interactive) k for text-label search (default: 5)
 *   --k-eeg <n>     (interactive) k for EEG-similarity search (default: 5)
 *   --k-labels <n>  (interactive) k for label-proximity search (default: 3)
 *   --reach <n>     (interactive) temporal reach in minutes around EEG points (default: 10)
 *   --help          Show full help with examples
 *   --version       Print CLI version and exit
 *   --no-color      Disable ANSI colors (also honours NO_COLOR env var)
 *   --poll <n>      (status) Re-poll every N seconds and print fresh snapshots
 *
 * When parameters are omitted, ranges are auto-selected from your session
 * history. A `rerun:` line is printed so you can copy-paste it later.
 *
 * Examples:
 *   npx tsx cli.ts status                           # → device, scores, sleep, embeddings
 *   npx tsx cli.ts status --json | jq '.scores'     # → pipe to jq
 *   npx tsx cli.ts sessions                         # → 3 session(s) with timestamps
 *   npx tsx cli.ts sessions --json | jq '.sessions[0]'
 *   npx tsx cli.ts say "Eyes open. Starting calibration."
 *   npx tsx cli.ts say "Break time. Next: Eyes Closed." --voice Jasper
 *   npx tsx cli.ts say "Break time. Next: Eyes Closed." --http
 *   npx tsx cli.ts notify "Session done" "Great work!"
 *   npx tsx cli.ts label "meditation start"         # → { label_id: 42 }
 *   npx tsx cli.ts label "eyes closed" --context "4-7-8 breathing" --at 1740412800
 *   npx tsx cli.ts calibrations                     # → list all profiles
 *   npx tsx cli.ts calibrations get <id>            # → full profile JSON
 *   npx tsx cli.ts calibrations create "name" --actions "L1:20,L2:20" [--loops 3] [--break 5] [--auto-start]
 *   npx tsx cli.ts calibrations update <id-or-name> [--name ...] [--actions ...] [--loops N] [--break N] [--auto-start]
 *   npx tsx cli.ts calibrations delete <id-or-name>
 *   npx tsx cli.ts search-labels "focused reading"  # → semantic label search
 *   npx tsx cli.ts search-labels "deep work" --mode context --k 5
 *   npx tsx cli.ts interactive "deep focus"         # → cross-modal graph (summary)
 *   npx tsx cli.ts interactive "meditation" --json  # → raw JSON (nodes + edges + dot)
 *   npx tsx cli.ts interactive "flow state" --dot | dot -Tsvg > graph.svg
 *   npx tsx cli.ts interactive "anxiety" --full --k-text 8 --k-eeg 8 --reach 15
 *   npx tsx cli.ts search                           # auto: last session, k=5
 *   npx tsx cli.ts search --start 1740412800 --end 1740415500 --k 10
 *   npx tsx cli.ts compare                          # auto: last 2 sessions as A/B
 *   npx tsx cli.ts compare --a-start 1740380100 --a-end 1740382665 \
 *                          --b-start 1740412800 --b-end 1740415510
 *   npx tsx cli.ts sleep                            # auto: last 24h → sleep summary
 *   npx tsx cli.ts sleep --start 1740380100 --end 1740415510
 *   npx tsx cli.ts calibrate                         # → opens calibration + auto-starts
 *   npx tsx cli.ts timer                            # → opens focus-timer + auto-starts
 *   npx tsx cli.ts umap                             # auto: last 2 sessions → 3D points
 *   npx tsx cli.ts umap --json | jq '.points | length'
 *   npx tsx cli.ts listen --seconds 30              # 30s event stream
 *   npx tsx cli.ts hooks --json | jq '.hooks[] | {name: .hook.name, scenario: .hook.scenario, last: .last_trigger.triggered_at_utc}'
 *   npx tsx cli.ts hooks list --json
 *   npx tsx cli.ts hooks add "Deep Work Guard" --keywords "focus,deep work,flow" --scenario cognitive --threshold 0.14
 *   npx tsx cli.ts hooks update "Deep Work Guard" --keywords "focus,flow" --threshold 0.12
 *   npx tsx cli.ts hooks enable "Deep Work Guard"
 *   npx tsx cli.ts hooks disable "Deep Work Guard"
 *   npx tsx cli.ts hooks remove "Deep Work Guard"
 *   npx tsx cli.ts hooks suggest "focus,deep work"
 *   npx tsx cli.ts hooks log --limit 10 --offset 0
 *   npx tsx cli.ts raw '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":3}'
 *
 * Requires: Node ≥ 18, bonjour-service + ws (devDependencies).
 */

import { Bonjour } from "bonjour-service";
import { execSync } from "child_process";
import WebSocket from "ws";

// ── ANSI colors ───────────────────────────────────────────────────────────────
// These are module-level `let`s so that `applyNoColor()` can zero them all
// out when --no-color / NO_COLOR / non-TTY mode is active.

let GRAY   = "\x1b[90m";
let GREEN  = "\x1b[32m";
let RED    = "\x1b[31m";
let CYAN   = "\x1b[36m";
let YELLOW = "\x1b[33m";
let BLUE   = "\x1b[34m";
let MAGENTA= "\x1b[35m";
let BOLD   = "\x1b[1m";
let DIM    = "\x1b[2m";
let RESET  = "\x1b[0m";

/**
 * Zero-out all ANSI escape constants so no color codes reach stdout/stderr.
 * Called once at startup when NO_COLOR env var is set, stdout is not a TTY,
 * or the user passes `--no-color`.
 */
function applyNoColor(): void {
  GRAY = GREEN = RED = CYAN = YELLOW = BLUE = MAGENTA = BOLD = DIM = RESET = "";
}

let jsonMode = false;
let fullMode = false;
let globalTimer: ReturnType<typeof setTimeout>;

// Honour NO_COLOR (https://no-color.org/) and non-TTY stdout.
// Checked before parseArgs() so that color output is never emitted when the
// caller has opted out via environment convention.
let noColorMode = !!process.env.NO_COLOR || !process.stdout.isTTY;

// ── Colorized JSON printer ────────────────────────────────────────────────────

/**
 * Render any JS value as a colorized (or plain) JSON string.
 *
 * In `--json` mode returns standard `JSON.stringify` output (no ANSI codes).
 * Otherwise delegates to {@link colorizeValue} for recursive ANSI coloring:
 * keys in blue, strings green, numbers cyan, booleans yellow, null magenta.
 *
 * @param obj    - The value to serialize.
 * @param indent - Current nesting depth (used for pretty-print indentation).
 * @returns A printable string (may contain ANSI escape codes).
 */
function colorizeJson(obj: unknown, indent = 0): string {
  if (jsonMode) return JSON.stringify(obj, null, 2);
  return colorizeValue(obj, indent);
}

/**
 * Recursively colorize a single JS value with ANSI escape codes.
 *
 * Type → color mapping:
 * - `null`      → magenta
 * - `undefined` → dim
 * - `boolean`   → yellow
 * - `number`    → cyan
 * - `string`    → green (with JSON escaping via {@link escapeStr})
 * - `Array`     → delegates to {@link colorizeArray}
 * - `object`    → delegates to {@link colorizeObject}
 *
 * @param val    - The value to colorize.
 * @param indent - Current nesting depth for indentation.
 */
function colorizeValue(val: unknown, indent: number): string {
  if (val === null)      return `${MAGENTA}null${RESET}`;
  if (val === undefined) return `${DIM}undefined${RESET}`;
  if (typeof val === "boolean") return `${YELLOW}${val}${RESET}`;
  if (typeof val === "number")  return `${CYAN}${val}${RESET}`;
  if (typeof val === "string")  return `${GREEN}"${escapeStr(val)}"${RESET}`;
  if (Array.isArray(val))       return colorizeArray(val, indent);
  if (typeof val === "object")  return colorizeObject(val as Record<string, unknown>, indent);
  return String(val);
}

/**
 * Escape special characters for JSON string display.
 * Handles backslash, double-quote, newline, and tab.
 */
function escapeStr(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n").replace(/\t/g, "\\t");
}

/**
 * Colorize a JSON array with ANSI codes.
 *
 * Short primitive arrays (≤8 elements, no nested objects) are rendered on a
 * single line for compactness: `[1, 2, 3]`.  Longer or nested arrays use
 * one-element-per-line formatting with indentation.
 *
 * @param arr    - The array to render.
 * @param indent - Current nesting depth.
 */
function colorizeArray(arr: unknown[], indent: number): string {
  if (arr.length === 0) return `${DIM}[]${RESET}`;
  // Compact for short primitive arrays
  if (arr.length <= 8 && arr.every(v => typeof v !== "object" || v === null)) {
    const items = arr.map(v => colorizeValue(v, 0)).join(`${DIM},${RESET} `);
    return `${DIM}[${RESET}${items}${DIM}]${RESET}`;
  }
  const pad = "  ".repeat(indent + 1);
  const endPad = "  ".repeat(indent);
  const items = arr.map(v => `${pad}${colorizeValue(v, indent + 1)}`).join(`${DIM},${RESET}\n`);
  return `${DIM}[${RESET}\n${items}\n${endPad}${DIM}]${RESET}`;
}

/**
 * Colorize a JSON object with ANSI codes.
 *
 * Keys are rendered in blue, values recursively colorized.
 * Each key-value pair occupies its own line, indented by depth.
 *
 * @param obj    - The object to render.
 * @param indent - Current nesting depth.
 */
function colorizeObject(obj: Record<string, unknown>, indent: number): string {
  const keys = Object.keys(obj);
  if (keys.length === 0) return `${DIM}{}${RESET}`;
  const pad = "  ".repeat(indent + 1);
  const endPad = "  ".repeat(indent);
  const entries = keys.map(k => {
    const colorKey = `${BLUE}"${k}"${RESET}`;
    const colorVal = colorizeValue(obj[k], indent + 1);
    return `${pad}${colorKey}${DIM}:${RESET} ${colorVal}`;
  }).join(`${DIM},${RESET}\n`);
  return `${DIM}{${RESET}\n${entries}\n${endPad}${DIM}}${RESET}`;
}

// ── Output helpers ────────────────────────────────────────────────────────────

/**
 * Print a decorative/informational line to stdout.
 * Suppressed in `--json` mode so only machine-readable JSON reaches stdout.
 */
function print(msg: string) {
  if (!jsonMode) console.log(msg);
}

/**
 * Print the final command result to stdout.
 *
 * - `--json` mode: plain JSON only (no summary, no colors — pipe-safe).
 * - `--full` mode: colorized JSON printed after the human-readable summary.
 * - default: suppressed — only the human-readable summary is shown.
 */
function printResult(data: unknown) {
  if (jsonMode || fullMode) console.log(colorizeJson(data));
}

/**
 * Print a fatal error message, clean up resources, and exit with code 1.
 * In `--json` mode outputs `{"error":"..."}` to stdout for programmatic
 * consumption; otherwise prints a red-bold error to stderr.
 */
function printError(msg: string): never {
  if (jsonMode) {
    console.log(JSON.stringify({ error: msg }));
  } else {
    console.error(`${RED}${BOLD}error:${RESET} ${msg}`);
  }
  try { ws?.terminate(); } catch {}
  clearTimeout(globalTimer);
  process.exit(1);
}

/**
 * Print a dim informational message to stderr (e.g. "connected to ...").
 * Suppressed in `--json` mode. Uses stderr so it never pollutes JSON output.
 */
function printInfo(msg: string) {
  if (!jsonMode) console.error(`${DIM}${msg}${RESET}`);
}

/**
 * Overwrite the current terminal line on stderr with a progress message.
 * Used for UMAP epoch progress bars and GPU wait indicators.
 * The `\r` carriage return keeps the cursor on the same line.
 */
function printProgress(msg: string) {
  if (!jsonMode) process.stderr.write(`\r${DIM}${msg}${RESET}`);
}

/**
 * Clear the current progress line on stderr.
 * Called after a progress sequence finishes (e.g. UMAP completes).
 */
function clearProgress() {
  if (!jsonMode) process.stderr.write("\r\x1b[K");
}

// ── WebSocket helpers ─────────────────────────────────────────────────────────

/** The active WebSocket connection to the Skill server. */
let ws: WebSocket;

/**
 * Send a JSON command to the Skill WebSocket server and wait for its response.
 *
 * The server echoes back `{ "command": "<same>" }` in every response, so we
 * match on `data.command === cmd.command` to pair request→response.  Any
 * broadcast events (which have `"event"` instead of `"command"`) are ignored.
 *
 * @param cmd       - The command payload, e.g. `{ command: "status" }`.
 *                    Additional fields are forwarded as command parameters.
 * @param timeoutMs - How long to wait before rejecting (default 30 s).
 *                    UMAP poll uses 60 s because GPU work can block the WS thread.
 * @returns The parsed JSON response from the server.
 * @throws  On timeout (no matching response within `timeoutMs`).
 */
/**
 * Send a command and return the parsed response.
 * In WebSocket mode this is the WS request/response loop.
 * In `--http` mode `main()` replaces this with {@link sendHttp} so all
 * command handlers work transparently over either transport.
 */
let send = function wsSend(cmd: { command: string; [k: string]: unknown }, timeoutMs = 30000): Promise<any> {
  return new Promise((resolve, reject) => {
    let timer: ReturnType<typeof setTimeout>;
    const handler = (raw: WebSocket.RawData) => {
      let data: any;
      try { data = JSON.parse(raw.toString()); } catch { return; }
      if (data.command === cmd.command) {
        clearTimeout(timer);
        ws.off("message", handler);
        resolve(data);
      }
    };
    ws.on("message", handler);
    ws.send(JSON.stringify(cmd));
    timer = setTimeout(() => {
      ws.off("message", handler);
      reject(new Error(`timeout after ${timeoutMs}ms`));
    }, timeoutMs);
  });
};

/**
 * Passively collect broadcast events from the server for a fixed duration.
 *
 * The Skill server pushes real-time events (EEG packets, PPG, scores, IMU,
 * label-created, etc.) as JSON messages with an `"event"` field.  This
 * function accumulates them into an array without sending any commands.
 *
 * @param ms - How long to listen, in milliseconds.
 * @returns Array of event objects received during the window.
 */
function collectEvents(ms: number): Promise<any[]> {
  return new Promise((resolve) => {
    const events: any[] = [];
    const handler = (raw: WebSocket.RawData) => {
      try {
        const data = JSON.parse(raw.toString());
        if (data.event) events.push(data);
      } catch {}
    };
    ws.on("message", handler);
    const t = setTimeout(() => { ws.off("message", handler); resolve(events); }, ms);
    t.unref();  // don't keep the process alive just for this timer
  });
}

// ── Transport state ───────────────────────────────────────────────────────────

/** Active transport: set in `main()` after connection negotiation. */
let transport: "ws" | "http" = "ws";

/**
 * The resolved HTTP base URL once the port is known.
 * Set by `main()` after port discovery when HTTP transport is active.
 */
let httpBase = "";

/**
 * Send a command via HTTP POST to the universal tunnel (`POST /`).
 *
 * Used when `--http` is passed.  Mirrors the WS `send()` API so command
 * handlers don't need to care which transport is active.
 *
 * @param cmd       - The command payload (must have `command` field).
 * @param _timeout  - Ignored for HTTP (native fetch has its own timeout).
 */
async function sendHttp(cmd: { command: string; [k: string]: unknown }, _timeout?: number): Promise<any> {
  let res: Response;
  try {
    res = await fetch(`${httpBase}/`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body:    JSON.stringify(cmd),
    });
  } catch (e: any) {
    throw new Error(
      `could not reach Skill at ${httpBase}/ — is it running? (${e.message})\n` +
      `  Tip: use --port <n> to specify the port manually, or omit --http for auto-transport.`
    );
  }
  return res.json();
}

// ── Port discovery ────────────────────────────────────────────────────────────

/**
 * Discover the Skill WebSocket server port.
 *
 * Resolution order:
 * 1. If `--port <n>` was given, return it immediately (no discovery).
 * 2. mDNS via `bonjour-service` — looks for `_skill._tcp` services on the
 *    local network.  Times out after 5 s.
 * 3. `lsof` fallback (macOS/Linux) — finds processes named "skill" with
 *    TCP LISTEN sockets, then probes each port with a test WebSocket
 *    handshake (1.5 s timeout per port).
 * 4. If all strategies fail, prints an error and exits.
 *
 * @param explicitPort - Port from `--port` flag, or `null` for auto-discovery.
 * @returns The resolved TCP port number.
 */
async function discover(explicitPort: number | null): Promise<number> {
  // Use `!= null` rather than truthiness so that an explicit --port 0 would
  // be caught (port 0 is not valid for our use case, but the intent is clear).
  if (explicitPort != null) return explicitPort;

  // mDNS via bonjour-service
  printInfo("discovering Skill via mDNS…");
  const port = await new Promise<number | null>((resolve) => {
    const instance = new Bonjour();
    const timeout = setTimeout(() => { browser.stop(); instance.destroy(); resolve(null); }, 5000);
    const browser = instance.find({ type: "skill" }, (service) => {
      clearTimeout(timeout);
      browser.stop();
      printInfo(`found: ${service.name} @ ${service.host}:${service.port}`);
      instance.destroy();
      resolve(service.port);
    });
  });
  if (port) return port;

  // lsof fallback
  printInfo("trying lsof fallback…");
  try {
    const ps = execSync("pgrep -if 'skill' 2>/dev/null || true", { encoding: "utf8" }).trim();
    if (ps) {
      for (const pid of ps.split("\n").filter(Boolean)) {
        try {
          const lsof = execSync(`lsof -iTCP -sTCP:LISTEN -nP -p ${pid} 2>/dev/null || true`, { encoding: "utf8" });
          for (const m of lsof.matchAll(/:(\d{4,5})\s+\(LISTEN\)/g)) {
            const p = Number(m[1]);
            const ok = await new Promise<boolean>((resolve) => {
              try {
                const w = new WebSocket(`ws://127.0.0.1:${p}`);
                const t = setTimeout(() => { try { w.close(); } catch {} resolve(false); }, 1500);
                w.on("open", () => { clearTimeout(t); w.close(); resolve(true); });
                w.on("error", () => { clearTimeout(t); resolve(false); });
              } catch { resolve(false); }
            });
            if (ok) return p;
          }
        } catch {}
      }
    }
  } catch {}

  printError("could not discover Skill. Is it running? Use --port <n> to specify manually.");
}

/**
 * Open a WebSocket connection to `ws://127.0.0.1:<port>`.
 *
 * Retries up to 3 times with a 1 s delay between attempts (handles the
 * case where the server is still starting up).  Each attempt has a 5 s
 * handshake timeout.  On success, stores the socket in the module-level
 * `ws` variable.
 *
 * @param port - The TCP port to connect to.
 * @throws Exits the process after 3 failed attempts.
 */
async function connect(port: number): Promise<void> {
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      await new Promise<void>((resolve, reject) => {
        const w = new WebSocket(`ws://127.0.0.1:${port}`);
        const t = setTimeout(() => { try { w.close(); } catch {} reject(new Error("timeout")); }, 5000);
        w.on("open", () => { clearTimeout(t); ws = w; resolve(); });
        w.on("error", () => { clearTimeout(t); reject(new Error("connection refused")); });
      });
      printInfo(`connected to ws://127.0.0.1:${port}`);
      return;
    } catch (e: any) {
      if (attempt >= 3) printError(`failed to connect after 3 attempts: ${e.message}`);
      await new Promise(r => setTimeout(r, 1000));
    }
  }
}

/**
 * Try a single WebSocket connection attempt with a short timeout.
 * Returns `true` and stores the open socket in `ws` on success.
 * Returns `false` silently on any failure — no side-effects.
 * Used by auto-transport mode to probe WS before falling back to HTTP.
 */
async function tryConnectOnce(port: number, timeoutMs = 3000): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    try {
      const w = new WebSocket(`ws://127.0.0.1:${port}`);
      const t = setTimeout(() => { try { w.close(); } catch {} resolve(false); }, timeoutMs);
      w.on("open", () => { clearTimeout(t); ws = w; resolve(true); });
      w.on("error", () => { clearTimeout(t); resolve(false); });
    } catch { resolve(false); }
  });
}

// ── Argument parsing ──────────────────────────────────────────────────────────

/** Parsed command-line arguments. */
interface Args {
  /** The subcommand name: "status", "sessions", "search", etc. */
  command: string;
  /** Explicit WebSocket port, or `null` for auto-discovery. */
  port: number | null;
  /** If true, output raw JSON without ANSI colors. */
  json: boolean;
  /** If true, print full JSON response after the human-readable summary. */
  full: boolean;
  /** Force HTTP transport (no WebSocket). */
  http: boolean;
  /** Force WebSocket transport (error if unavailable). */
  ws: boolean;
  /** Annotation text for the `label` command, or title for `notify`. */
  text?: string;
  /** Notification body for the `notify` command (second positional arg). */
  body?: string;
  /** Start of time range (unix seconds) for `search` / `sleep`. */
  start?: number;
  /** End of time range (unix seconds) for `search` / `sleep`. */
  end?: number;
  /** Start of session A (unix seconds) for `compare` / `umap`. */
  aStart?: number;
  /** End of session A (unix seconds) for `compare` / `umap`. */
  aEnd?: number;
  /** Start of session B (unix seconds) for `compare` / `umap`. */
  bStart?: number;
  /** End of session B (unix seconds) for `compare` / `umap`. */
  bEnd?: number;
  /** Number of nearest neighbors for `search` / `search-labels` (default varies). */
  k?: number;
  /** HNSW ef parameter for `search-labels` (default: max(k×4, 64)). */
  ef?: number;
  /** Search mode for `search-labels`: "text" | "context" | "both" (default "text"). */
  mode?: string;
  /** Calibration profile name or UUID for the `calibrate` command. */
  profile?: string;
  /** Duration in seconds for `listen` (default 5). */
  seconds?: number;
  /** Raw JSON string for the `raw` command. */
  rawJson?: string;
  /** Show per-session metric trends when listing sessions. */
  trends: boolean;
  /** Index for `session` / `sleep` commands: 0 = latest, 1 = previous, … Null = not specified. */
  sessionIndex: number | null;
  /** Keyword / query string for the `interactive` command. */
  keyword?: string;
  /** If true, emit Graphviz DOT format to stdout (interactive command only). */
  dot: boolean;
  /** k for text-label HNSW search in `interactive` (default 5). */
  kText?: number;
  /** k for EEG-similarity HNSW search in `interactive` (default 5). */
  kEeg?: number;
  /** k for label-proximity search in `interactive` (default 3). */
  kLabels?: number;
  /** Temporal reach in minutes around each EEG point in `interactive` (default 10). */
  reach?: number;
  /**
   * Re-poll interval in seconds for `status --poll N`.
   * When set, `cmdStatus` loops indefinitely, printing a fresh snapshot every
   * N seconds over the same open WebSocket connection.
   */
  poll?: number;
  /**
   * Long-form annotation body for `label --context "..."`.
   * Stored alongside the short label text; used in `search-labels` context mode.
   */
  context?: string;
  /**
   * Backdate timestamp (unix seconds) for `label --at <utc>`.
   * Overrides the default "now" for both `label_start_utc` and `label_end_utc`.
   */
  at?: number;
  /** Voice name for the `say` command (e.g. "Jasper"). Uses server default when omitted. */
  voice?: string;
  /** Subaction for the `calibrations` command: "list" | "get" | "create" | "update" | "delete". */
  subAction?: string;
  /** Numeric ID for `calibrations get <id>`. */
  id?: number;
  /**
   * Calibration profile UUID for `calibrations update/delete`.
   * Populated from the first positional arg after the subcommand.
   */
  profileId?: string;
  /**
   * Calibration profile name for `calibrations create` / `calibrations update --name`.
   * For `create`: first positional arg after subcommand.
   * For `update`: via `--name` flag.
   */
  calName?: string;
  /**
   * Calibration actions string for `calibrations create` / `update --actions`.
   * Compact format: `"Eyes Open:20,Eyes Closed:20"` (label:duration_secs pairs).
   */
  calActions?: string;
  /** Loop count for `calibrations create` / `update --loops`. */
  calLoops?: number;
  /** Break duration (seconds) for `calibrations create` / `update --break`. */
  calBreak?: number;
  /** Auto-start flag for `calibrations create` / `update --auto-start`. */
  calAutoStart?: boolean;
  /** Generic pagination limit for subcommands that support it (e.g. hooks log). */
  limit?: number;
  /** Generic pagination offset for subcommands that support it (e.g. hooks log). */
  offset?: number;
  /** Hook keywords (comma-separated) for `hooks add` / `hooks update --keywords`. */
  hookKeywords?: string;
  /** Hook scenario for `hooks add` / `hooks update --scenario`. */
  hookScenario?: string;
  /** Hook command for `hooks add` / `hooks update --command`. */
  hookCommand?: string;
  /** Hook text payload for `hooks add` / `hooks update --text`. */
  hookText?: string;
  /** Hook distance threshold for `hooks add` / `hooks update --threshold`. */
  hookThreshold?: number;
  /** Hook recent-refs limit for `hooks add` / `hooks update --recent`. */
  hookRecent?: number;
  /** Hook name captured as second positional arg for hooks add/remove/enable/disable/update. */
  hookName?: string;
  /**
   * Vision projector filename for `llm add --mmproj <filename>`.
   * When specified alongside `llm add`, both the model and the mmproj are
   * added to the catalog and downloaded from the same repo.
   */
  mmproj?: string;
  /**
   * One or more image file paths for `llm chat`.
   * Each file is base64-encoded and embedded as an `image_url` content part.
   * Can be specified multiple times: `--image a.jpg --image b.png`.
   * Requires the LLM server to be loaded with a vision-capable model (mmproj).
   */
  images?: string[];
  /**
   * System prompt for `llm chat` (prepended as a `{ role: "system" }` message).
   * Example: `--system "You are a concise EEG assistant."`.
   * Omit to let the model use its built-in defaults.
   */
  system?: string;
  /**
   * Maximum tokens to generate per llm_chat turn.
   * Passed as `max_tokens` in GenParams.  Default: model default (2048).
   */
  maxTokens?: number;
  /**
   * Sampling temperature for llm_chat (0 = deterministic, 1 = creative).
   * Passed as `temperature` in GenParams.  Default: 0.8.
   */
  temperature?: number;
}

/**
 * Parse `process.argv` into a typed {@link Args} object.
 *
 * Supports `--flag value` style for all options.  The first positional
 * argument becomes the command name; subsequent positional args are
 * interpreted contextually (e.g. label text, raw JSON body).
 *
 * Numeric flags are validated: a non-numeric or missing value after the flag
 * name is a fatal error rather than a silent `NaN`.
 *
 * Unrecognized flags (anything starting with `--` that is not listed below)
 * are rejected with a clear error message to catch typos early.
 *
 * @returns Parsed arguments with defaults (`port: null`, `json: false`).
 */
function parseArgs(): Args {
  const argv = process.argv.slice(2);
  const args: Args = { command: "", port: null, json: false, full: false, http: false, ws: false, trends: false, sessionIndex: null, dot: false };

  /**
   * Read and validate the next argv token as a positive integer.
   * Exits with a helpful message if the value is missing or not a number.
   */
  function nextInt(flag: string): number {
    const raw = argv[++i];
    const n   = Number(raw);
    if (raw == null || raw.trim() === "" || isNaN(n)) {
      // printError not yet callable (jsonMode not set), so use process.exit directly.
      console.error(`error: ${flag} requires a numeric value (got: ${JSON.stringify(raw)})`);
      process.exit(1);
    }
    return n;
  }

  // Known flags — used to catch typos (see "unrecognized flag" guard below).
  const KNOWN_FLAGS = new Set([
    "--port", "--json", "--full", "--trends", "--http", "--ws", "--dot",
    "--help", "-h", "--version", "-v", "--no-color",
    "--start", "--end", "--a-start", "--a-end", "--b-start", "--b-end",
    "--k", "--k-text", "--k-eeg", "--k-labels", "--reach", "--ef",
    "--mode", "--profile", "--seconds", "--poll",
    "--limit", "--offset",
    "--context", "--at", "--voice",
    "--system", "--max-tokens", "--temperature", "--image", "--mmproj",
    "--keywords", "--scenario", "--command", "--threshold", "--recent", "--hook-text",
    "--actions", "--loops", "--break", "--auto-start", "--name",
  ]);

  let i = 0;
  while (i < argv.length) {
    const a = argv[i];

    if (a === "--port")          { args.port    = nextInt("--port");    }
    else if (a === "--json")     { args.json    = true; }
    else if (a === "--full")     { args.full    = true; }
    else if (a === "--trends")   { args.trends  = true; }
    else if (a === "--http")     { args.http    = true; }
    else if (a === "--ws")       { args.ws      = true; }
    else if (a === "--dot")      { args.dot     = true; }
    else if (a === "--no-color") { noColorMode  = true; }
    else if (a === "--help" || a === "-h")       { args.command = "help"; }
    else if (a === "--version"   || a === "-v")  { args.command = "version"; }
    else if (a === "--start")    { args.start   = nextInt("--start");   }
    else if (a === "--end")      { args.end     = nextInt("--end");     }
    else if (a === "--a-start")  { args.aStart  = nextInt("--a-start"); }
    else if (a === "--a-end")    { args.aEnd    = nextInt("--a-end");   }
    else if (a === "--b-start")  { args.bStart  = nextInt("--b-start"); }
    else if (a === "--b-end")    { args.bEnd    = nextInt("--b-end");   }
    else if (a === "--k")        { args.k       = nextInt("--k");       }
    else if (a === "--k-text")   { args.kText   = nextInt("--k-text");  }
    else if (a === "--k-eeg")    { args.kEeg    = nextInt("--k-eeg");   }
    else if (a === "--k-labels") { args.kLabels = nextInt("--k-labels");}
    else if (a === "--reach")    { args.reach   = nextInt("--reach");   }
    else if (a === "--ef")       { args.ef      = nextInt("--ef");      }
    else if (a === "--seconds")  { args.seconds = nextInt("--seconds"); }
    else if (a === "--poll")     { args.poll    = nextInt("--poll");    }
    else if (a === "--limit")    { args.limit   = nextInt("--limit");   }
    else if (a === "--offset")   { args.offset  = nextInt("--offset");  }
    else if (a === "--at")          { args.at          = nextInt("--at");          }
    else if (a === "--max-tokens")  { args.maxTokens   = nextInt("--max-tokens");   }
    else if (a === "--mode")        { args.mode        = argv[++i]; }
    else if (a === "--profile")     { args.profile     = argv[++i]; }
    else if (a === "--context")     { args.context     = argv[++i]; }
    else if (a === "--voice")       { args.voice       = argv[++i]; }
    else if (a === "--system")      { args.system      = argv[++i]; }
    else if (a === "--image")       { (args.images ??= []).push(argv[++i]); }
    else if (a === "--mmproj")      { args.mmproj   = argv[++i]; }
    else if (a === "--keywords")    { args.hookKeywords  = argv[++i]; }
    else if (a === "--scenario")    { args.hookScenario  = argv[++i]; }
    else if (a === "--command")     { args.hookCommand   = argv[++i]; }
    else if (a === "--hook-text")   { args.hookText      = argv[++i]; }
    else if (a === "--threshold")   {
      const raw = argv[++i];
      const n   = Number(raw);
      if (raw == null || raw.trim() === "" || isNaN(n)) {
        console.error(`error: --threshold requires a numeric value (got: ${JSON.stringify(raw)})`);
        process.exit(1);
      }
      args.hookThreshold = n;
    }
    else if (a === "--recent")      { args.hookRecent    = nextInt("--recent"); }
    else if (a === "--temperature") {
      const raw = argv[++i];
      const n   = Number(raw);
      if (raw == null || raw.trim() === "" || isNaN(n)) {
        console.error(`error: --temperature requires a numeric value (got: ${JSON.stringify(raw)})`);
        process.exit(1);
      }
      args.temperature = n;
    }
    else if (a === "--actions")     { args.calActions   = argv[++i]; }
    else if (a === "--loops")       { args.calLoops     = nextInt("--loops"); }
    else if (a === "--break")       { args.calBreak     = nextInt("--break"); }
    else if (a === "--auto-start")  { args.calAutoStart = true; }
    else if (a === "--name")        { args.calName      = argv[++i]; }
    // ── Positional arguments ─────────────────────────────────────────────
    else if (!args.command)      { args.command = a.toLowerCase(); }
    else if (args.command === "label"         && !args.text)    { args.text    = a; }
    else if (args.command === "search-labels" && !args.text)    { args.text    = a; }
    else if (args.command === "interactive"   && !args.keyword) { args.keyword = a; }
    else if (args.command === "say"           && !args.text)    { args.text    = a; }
    else if (args.command === "notify"        && !args.text)    { args.text    = a; }
    else if (args.command === "notify"        && !args.body)    { args.body    = a; }
    else if (args.command === "raw"           && !args.rawJson) { args.rawJson = a; }
    else if (args.command === "llm" && !args.subAction) {
      // llm <subAction> [arg]
      args.subAction = a.toLowerCase();
    }
    else if (args.command === "llm" && args.subAction === "download" && !args.text) {
      args.text = a; // model filename
    }
    else if (args.command === "llm" && args.subAction === "cancel" && !args.text) {
      args.text = a; // model filename
    }
    else if (args.command === "llm" && args.subAction === "pause" && !args.text) {
      args.text = a; // model filename
    }
    else if (args.command === "llm" && args.subAction === "resume" && !args.text) {
      args.text = a; // model filename
    }
    else if (args.command === "llm" && args.subAction === "delete" && !args.text) {
      args.text = a; // model filename
    }
    else if (args.command === "llm" && args.subAction === "select" && !args.text) {
      args.text = a; // model filename
    }
    else if (args.command === "llm" && args.subAction === "mmproj" && !args.text) {
      args.text = a; // mmproj filename (or "none" to disable)
    }
    else if (args.command === "llm" && args.subAction === "add" && !args.text) {
      args.text = a; // repo (first positional) or repo/filename combined
    }
    else if (args.command === "llm" && args.subAction === "add" && args.text && !args.body) {
      args.body = a; // filename (second positional)
    }
    else if (args.command === "llm" && args.subAction === "autoload-mmproj" && !args.text) {
      args.text = a; // on/off
    }
    else if (args.command === "llm" && args.subAction === "fit" && !args.text) {
      args.text = a; // optional filename filter
    }
    else if (args.command === "llm" && args.subAction === "chat" && !args.text) {
      args.text = a; // user message
    }
    else if (args.command === "dnd" && !args.subAction && (a === "on" || a === "off")) {
      args.subAction = a; // "on" or "off" → maps to dnd_set { enabled: true/false }
    }
    else if (args.command === "calibrations"  && !args.subAction) {
      // calibrations [list|get|create|update|delete] [<id-or-name>]
      args.subAction = a.toLowerCase();
    }
    else if (args.command === "calibrations"  && args.subAction === "get" && args.id == null) {
      const n = Number(a);
      if (isNaN(n)) { console.error(`error: calibrations get requires a numeric id (got: ${JSON.stringify(a)})`); process.exit(1); }
      args.id = n;
    }
    else if (args.command === "calibrations" && args.subAction === "create" && !args.calName) {
      args.calName = a; // profile name
    }
    else if (args.command === "calibrations" && args.subAction === "update" && !args.profileId) {
      args.profileId = a; // profile UUID or name
    }
    else if (args.command === "calibrations" && args.subAction === "delete" && !args.profileId) {
      args.profileId = a; // profile UUID or name
    }
    else if (args.command === "hooks" && !args.subAction) {
      args.subAction = a.toLowerCase();
    }
    else if (args.command === "hooks" && args.subAction === "suggest" && !args.text) {
      args.text = a;
    }
    else if (args.command === "hooks" && ["add", "remove", "enable", "disable", "update"].includes(args.subAction ?? "") && !args.hookName) {
      args.hookName = a;
    }
    else if (args.command === "session" || args.command === "sleep") {
      const n = Number(a);
      if (!isNaN(n) && a.trim() !== "") args.sessionIndex = n;
    }
    // ── Unrecognized flag guard ──────────────────────────────────────────
    else if (a.startsWith("--") && !KNOWN_FLAGS.has(a)) {
      console.error(`error: unrecognized option "${a}". Run with --help to see available options.`);
      process.exit(1);
    }

    i++;
  }

  return args;
}

/**
 * Print the full help text (commands, options, examples with sample output)
 * to stdout and exit with code 0.
 */
function showHelp(): never {
  const m = (cmd: string, desc: string) => `  ${CYAN}${cmd.padEnd(50)}${RESET} ${desc}`;
  console.log(`
${BOLD}skill cli${RESET} — command-line interface for the Skill WebSocket API

${BOLD}USAGE${RESET}
  npx tsx cli.ts <command> [options]

${BOLD}COMMANDS${RESET}
${m("status [--poll <secs>]",                         "full device, session, embeddings, scores snapshot; re-poll every N secs")}
${m('say "text"',                                    "speak text aloud via on-device TTS (fire-and-forget; returns immediately)")}
${m("session [index]",                               "all metrics + trends for one session (0=latest, 1=prev, …)")}
${m("sessions",                                      "list all recording sessions across all days")}
${m('notify "title" ["body"]',                       "show a native OS notification")}
${m('label "my annotation" [--context "..."] [--at <utc>]', "create a timestamped text annotation")}
${m('search-labels "query" [--mode text|context|both] [--k <n>] [--ef <n>]', "search labels by free text")}
${m('interactive "keyword" [--k-text <n>] [--k-eeg <n>] [--k-labels <n>] [--reach <n>]', "cross-modal 4-layer graph search")}
${m("search [--start <utc>] [--end <utc>] [--k <n>]", "ANN EEG-similarity search on embeddings")}
${m("compare --a-start .. --a-end .. --b-start .. --b-end ..", "side-by-side metrics + UMAP")}
${m("sleep [index] [--start <utc>] [--end <utc>]",   "sleep staging — index selects session (0=latest, 1=prev)")}
${m("calibrations [list|get <id>]",                  "list calibration profiles or inspect one by ID")}
${m('calibrations create "name" --actions "L1:20,L2:20"', "create a new calibration profile")}
${m("calibrations update <id-or-name> [opts]",       "update an existing calibration profile")}
${m("calibrations delete <id-or-name>",              "delete a calibration profile")}
${m("calibrate [--profile <name-or-id>]",            "open calibration window and start profile immediately")}
${m("timer",                                         "open focus-timer window and start work phase immediately")}
${m("umap [--a-start .. --a-end .. --b-start .. --b-end ..]", "3D UMAP projection (waits for result)")}
${m("dnd [on|off]",                                    "show DND automation status; 'on'/'off' force-overrides immediately")}
${m("llm status",                                      "LLM server status (stopped/loading/running)")}
${m("llm start",                                     "load active model and start LLM inference server")}
${m("llm stop",                                      "stop LLM inference server and free GPU memory")}
${m("llm catalog",                                   "show model catalog with download states")}
${m("llm add <repo> <filename>",                      "add an external HF model to the catalog and download it")}
${m("llm add <hf-url>",                               "add from a full HuggingFace URL")}
${m("llm add ... --mmproj <file>",                    "also add and download a vision projector from the same repo")}
${m("llm select <filename>",                         "set the active text model")}
${m("llm mmproj <filename|none>",                    "set the active vision projector (or 'none' to disable)")}
${m("llm autoload-mmproj <on|off>",                  "toggle auto-loading of vision projector on start")}
${m("llm download <filename>",                       "download a GGUF model by filename (fire-and-forget)")}
${m("llm pause <filename>",                          "pause an in-progress model download")}
${m("llm resume <filename>",                         "resume a paused model download")}
${m("llm cancel <filename>",                         "cancel an in-progress model download")}
${m("llm delete <filename>",                         "delete a locally-cached model file")}
${m("llm downloads",                                 "list all downloads with status and progress")}
${m("llm refresh",                                   "re-probe HF Hub cache for externally downloaded models")}
${m("llm fit",                                       "check which models fit in available RAM/VRAM")}
${m("llm logs",                                      "print last 500 LLM server log lines")}
${m("llm chat",                                       "interactive multi-turn chat REPL; type /help inside for commands")}
${m('llm chat "message"',                            "single-shot: send one message, stream the reply, and exit")}
${m("listen [--seconds <n>]",                        "listen for broadcast events (default: 5s)")}
${m("hooks",                                         "list Proactive Hooks (scenario + last trigger metadata)")}
${m("hooks list",                                    "list raw hook rules (name, keywords, threshold, …)")}
${m("hooks add <name> [--keywords …] [opts]",       "add a new hook rule")}
${m("hooks remove <name>",                           "delete a hook by name")}
${m("hooks enable <name>",                           "enable a hook")}
${m("hooks disable <name>",                          "disable a hook")}
${m("hooks update <name> [--keywords …] [opts]",    "update fields on an existing hook")}
${m('hooks suggest "kw1,kw2"',                       "suggest threshold from matching labels + recent EEG embeddings")}
${m("hooks log [--limit <n>] [--offset <n>]",       "show hook trigger audit history from hooks.sqlite")}
${m("raw '{\"command\":\"status\"}'",                "send raw JSON, print full response")}

${BOLD}OPTIONS${RESET}
  ${YELLOW}--port <n>${RESET}        connect to explicit port (skips mDNS discovery)
  ${YELLOW}--ws${RESET}              force WebSocket transport (error if unavailable)
  ${YELLOW}--http${RESET}            force HTTP transport (no live-event commands)
  ${DIM}(neither)${RESET}         auto: try WebSocket, silently fall back to HTTP
  ${YELLOW}--json${RESET}            output raw JSON (no colors, machine-readable)
  ${YELLOW}--full${RESET}            print full JSON response after the human-readable summary
  ${YELLOW}--no-color${RESET}        disable ANSI color output (also honoured via NO_COLOR env var)
  ${YELLOW}--poll <n>${RESET}        (status) re-poll every N seconds; keeps the socket open
  ${YELLOW}--limit <n>${RESET}       (hooks log) page size (default: 20)
  ${YELLOW}--offset <n>${RESET}      (hooks log) row offset (default: 0)
  ${YELLOW}--keywords <csv>${RESET}  (hooks add/update) comma-separated keywords
  ${YELLOW}--scenario <s>${RESET}    (hooks add/update) any | cognitive | emotional | physical
  ${YELLOW}--command <cmd>${RESET}   (hooks add/update) command to run on trigger
  ${YELLOW}--hook-text <txt>${RESET} (hooks add/update) payload text
  ${YELLOW}--threshold <f>${RESET}   (hooks add/update) distance threshold (0.01–1.0)
  ${YELLOW}--recent <n>${RESET}      (hooks add/update) recent-refs limit (10–20)
  ${YELLOW}--trends${RESET}          (sessions) show per-session metric trends and first/second-half deltas
  ${YELLOW}--context "..."${RESET}   (label) long-form annotation body; used by search-labels --mode context
  ${YELLOW}--at <utc>${RESET}        (label) backdate to a specific unix second (default: now)
  ${YELLOW}--mode <m>${RESET}        search-labels mode: text | context | both (default: text)
  ${YELLOW}--ef <n>${RESET}          HNSW ef parameter for search-labels (default: max(k×4, 64))
  ${YELLOW}--dot${RESET}             (interactive) output Graphviz DOT to stdout — pipe to ${DIM}dot -Tsvg > out.svg${RESET}
  ${YELLOW}--k-text <n>${RESET}      (interactive) k for text-label HNSW search (default: 5)
  ${YELLOW}--k-eeg <n>${RESET}       (interactive) k for EEG-similarity HNSW search (default: 5)
  ${YELLOW}--k-labels <n>${RESET}    (interactive) k for label-proximity step (default: 3)
  ${YELLOW}--reach <n>${RESET}       (interactive) temporal window in minutes around each EEG point (default: 10)
  ${YELLOW}--voice <name>${RESET}    say: voice name to use (e.g. ${GREEN}Jasper${RESET}); omit to use the server default
  ${YELLOW}--profile <p>${RESET}     calibrate: profile name or UUID to run (default: active profile)
  ${YELLOW}--actions "L:s,…"${RESET} (calibrations create/update) actions as "Label:secs" pairs (e.g. ${GREEN}"Eyes Open:20,Eyes Closed:20"${RESET})
  ${YELLOW}--loops <n>${RESET}       (calibrations create/update) loop count (default: 3)
  ${YELLOW}--break <n>${RESET}       (calibrations create/update) break duration in seconds (default: 5)
  ${YELLOW}--auto-start${RESET}     (calibrations create/update) auto-start when opened
  ${YELLOW}--name "…"${RESET}       (calibrations update) rename the profile
  ${YELLOW}--mmproj <file>${RESET}    llm add: also download a vision projector from the same repo
  ${YELLOW}--image <path>${RESET}     llm chat: attach an image (can be repeated: --image a.jpg --image b.png)
  ${YELLOW}--system "..."${RESET}    llm chat: prepend a system prompt (e.g. ${GREEN}"You are a concise EEG assistant."${RESET})
  ${YELLOW}--temperature <f>${RESET} llm chat: sampling temperature 0–2 (default 0.8; 0 = deterministic)
  ${YELLOW}--max-tokens <n>${RESET}  llm chat: maximum tokens to generate per turn (default 2048)
  ${YELLOW}--help${RESET}            show this help
  ${YELLOW}--version${RESET}         print CLI version and exit

${BOLD}EXAMPLES${RESET}
  When parameters are omitted, the CLI auto-selects ranges from your session
  history and prints a ${YELLOW}rerun:${RESET} line you can copy-paste for reproducible results.

  ${BOLD}status${RESET} — device, session, embeddings, scores, sleep
  ${DIM}$${RESET} npx tsx cli.ts status
  ${DIM}$${RESET} npx tsx cli.ts status --json
  ${DIM}$${RESET} npx tsx cli.ts status --json | jq '.scores.focus'
  ${DIM}$${RESET} npx tsx cli.ts status --port 62853
  ${DIM}$${RESET} npx tsx cli.ts status --poll 5             ${DIM}# refresh every 5 s${RESET}
  ${DIM}$${RESET} npx tsx cli.ts status --poll 10 --json     ${DIM}# JSON snapshot every 10 s${RESET}
  ${DIM}# Output:${RESET}
  ${DIM}#   { "command": "status", "ok": true,${RESET}
  ${DIM}#     "device": { "state": "connected", "name": "Muse-A1B2", "battery": 73, ... },${RESET}
  ${DIM}#     "session": { "start_utc": 1740412800, "duration_secs": 1847 },${RESET}
  ${DIM}#     "embeddings": { "today": 342, "total": 14820, "recording_days": 31 },${RESET}
  ${DIM}#     "scores": { "focus": 0.7, "relaxation": 0.4, "engagement": 0.6,${RESET}
  ${DIM}#       "hr": 68.2, "meditation": 0.52, "drowsiness": 0.1, ... },${RESET}
  ${DIM}#     "signal_quality": { "tp9": 0.95, "af7": 0.88, "af8": 0.91, "tp10": 0.97 },${RESET}
  ${DIM}#     "sleep": { "total_epochs": 420, "wake_epochs": 38, "n2_epochs": 210, ... } }${RESET}

  ${BOLD}sessions${RESET} — list all recordings
  ${DIM}$${RESET} npx tsx cli.ts sessions
  ${DIM}$${RESET} npx tsx cli.ts sessions --json | jq '.sessions | length'
  ${DIM}$${RESET} npx tsx cli.ts sessions --json | jq '.sessions[0]'
  ${DIM}# Output:${RESET}
  ${DIM}#   3 session(s)${RESET}
  ${DIM}#     20260223  2/23/2026, 9:15:00 AM → 10:02:33 AM  47m 33s  570 epochs${RESET}
  ${DIM}#     20260223  2/23/2026, 2:30:00 PM → 3:12:45 PM   42m 45s  513 epochs${RESET}
  ${DIM}#     20260224  2/24/2026, 8:00:00 AM → 8:45:10 AM   45m 10s  541 epochs${RESET}

  ${BOLD}say${RESET} — speak text aloud via on-device TTS (fire-and-forget)
  ${DIM}$${RESET} npx tsx cli.ts say "Eyes open. Starting calibration."
  ${DIM}$${RESET} npx tsx cli.ts say "Break time. Next: Eyes Closed." ${YELLOW}--voice Jasper${RESET}
  ${DIM}$${RESET} npx tsx cli.ts say "Calibration complete." --http
  ${DIM}$${RESET} npx tsx cli.ts say "Hello!" --json
  ${DIM}# Output (no --voice):${RESET}
  ${DIM}#   { "command": "say", "ok": true, "spoken": "Eyes open. Starting calibration." }${RESET}
  ${DIM}# Output (with --voice Jasper):${RESET}
  ${DIM}#   { "command": "say", "ok": true, "spoken": "Break time. Next: Eyes Closed.", "voice": "Jasper" }${RESET}
  ${DIM}# Note: --voice is optional; omitting it uses the voice last selected in Settings → Voice.${RESET}
  ${DIM}# Requires espeak-ng on PATH. First run downloads the TTS model (~30 MB).${RESET}

  ${BOLD}notify${RESET} — send a native OS notification
  ${DIM}$${RESET} npx tsx cli.ts notify "Session complete"
  ${DIM}$${RESET} npx tsx cli.ts notify "Focus done" "Take a 5-minute break"
  ${DIM}# Output:${RESET}
  ${DIM}#   { "command": "notify", "ok": true }${RESET}

  ${BOLD}label${RESET} — annotate the current EEG moment
  ${DIM}$${RESET} npx tsx cli.ts label "eyes closed relaxation"
  ${DIM}$${RESET} npx tsx cli.ts label "meditation start"
  ${DIM}$${RESET} npx tsx cli.ts label "feeling anxious"
  ${DIM}$${RESET} npx tsx cli.ts label "breathwork" --context "box breathing 4-4-4-4, 10 min"
  ${DIM}$${RESET} npx tsx cli.ts label "retrospective note" --at 1740412800
  ${DIM}# Output:${RESET}
  ${DIM}#   { "command": "label", "ok": true, "label_id": 42 }${RESET}

  ${BOLD}calibrations${RESET} — manage calibration profiles
  ${DIM}$${RESET} npx tsx cli.ts calibrations                         ${DIM}# list all profiles${RESET}
  ${DIM}$${RESET} npx tsx cli.ts calibrations list                    ${DIM}# same as above${RESET}
  ${DIM}$${RESET} npx tsx cli.ts calibrations get 3                   ${DIM}# full detail for profile id=3${RESET}
  ${DIM}$${RESET} npx tsx cli.ts calibrations --json | jq '.profiles[].name'
  ${DIM}# Output (list):${RESET}
  ${DIM}#   id     name                           actions  loop${RESET}
  ${DIM}#   ──────────────────────────────────────────────────────────${RESET}
  ${DIM}#   1      Eyes Open/Closed               4        2${RESET}
  ${DIM}#   2      Relaxation                     3        1${RESET}
  ${DIM}#   3      Focus Baseline                 5        1${RESET}

  ${BOLD}calibrations create/update/delete${RESET} — full calibration profile CRUD
  ${DIM}$${RESET} npx tsx cli.ts calibrations create "My Protocol" --actions "Eyes Open:20,Eyes Closed:20" --loops 3 --break 5
  ${DIM}$${RESET} npx tsx cli.ts calibrations create "Quick Baseline" --actions "Relax:30,Focus:30" --auto-start
  ${DIM}$${RESET} npx tsx cli.ts calibrations update "My Protocol" --loops 5 --break 10
  ${DIM}$${RESET} npx tsx cli.ts calibrations update "My Protocol" --name "Renamed Protocol"
  ${DIM}$${RESET} npx tsx cli.ts calibrations update "My Protocol" --actions "Eyes Open:30,Eyes Closed:30,Breathe:15"
  ${DIM}$${RESET} npx tsx cli.ts calibrations delete "My Protocol"

  ${BOLD}calibrate${RESET} — open calibration window and start immediately
  ${DIM}$${RESET} npx tsx cli.ts calibrate                              ${DIM}# uses active profile${RESET}
  ${DIM}$${RESET} npx tsx cli.ts calibrate --profile "Eyes Open/Closed" ${DIM}# by name${RESET}
  ${DIM}$${RESET} npx tsx cli.ts calibrate --profile default             ${DIM}# by id${RESET}
  ${DIM}$${RESET} npx tsx cli.ts calibrate --json | jq '.profile'
  ${DIM}# Output:${RESET}
  ${DIM}#   { "command": "run_calibration", "ok": true }${RESET}
  ${DIM}# Note: requires a Muse headband to be connected and streaming.${RESET}

  ${BOLD}timer${RESET} — open focus-timer window and auto-start the work phase
  ${DIM}$${RESET} npx tsx cli.ts timer
  ${DIM}# Output:${RESET}
  ${DIM}#   { "command": "timer", "ok": true }${RESET}
  ${DIM}# The timer opens using the last-saved preset (Pomodoro / Deep Work / Short Focus).${RESET}

  ${BOLD}search-labels${RESET} — semantic label search via fastembed HNSW
  ${DIM}$${RESET} npx tsx cli.ts search-labels "deep focus"
  ${DIM}$${RESET} npx tsx cli.ts search-labels "relaxed meditation" --k 10
  ${DIM}$${RESET} npx tsx cli.ts search-labels "anxiety" --mode context
  ${DIM}$${RESET} npx tsx cli.ts search-labels "flow state" --mode both --k 5
  ${DIM}$${RESET} npx tsx cli.ts search-labels "creative work" --json | jq '.results[].text'
  ${DIM}# Output:${RESET}
  ${DIM}#   ⚡ search-labels "deep focus"  (mode: text, k: 10)${RESET}
  ${DIM}#   { "command": "search_labels", "ok": true,${RESET}
  ${DIM}#     "query": "deep focus", "mode": "text", "model": "Xenova/bge-small-en-v1.5",${RESET}
  ${DIM}#     "k": 10, "count": 3,${RESET}
  ${DIM}#     "results": [${RESET}
  ${DIM}#       { "label_id": 7, "text": "focused reading session",${RESET}
  ${DIM}#         "context": "", "distance": 0.12, "similarity": 0.88,${RESET}
  ${DIM}#         "eeg_start": 1740412800, "eeg_end": 1740413100,${RESET}
  ${DIM}#         "created_at": 1740412810, "eeg_metrics": { "focus": 0.74, ... } },${RESET}
  ${DIM}#       ...${RESET}
  ${DIM}#     ] }${RESET}

  ${BOLD}interactive${RESET} — cross-modal graph search: query → text labels → EEG moments → nearby labels
  ${DIM}$${RESET} npx tsx cli.ts interactive "deep focus"
  ${DIM}$${RESET} npx tsx cli.ts interactive "meditation" --k-text 8 --k-eeg 8 --reach 15
  ${DIM}$${RESET} npx tsx cli.ts interactive "flow state" --json | jq '.nodes | length'
  ${DIM}$${RESET} npx tsx cli.ts interactive "anxiety" --dot | dot -Tsvg > graph.svg
  ${DIM}$${RESET} npx tsx cli.ts interactive "stress" --full
  ${DIM}# Output (default summary):${RESET}
  ${DIM}#   ⚡ interactive "deep focus"  (k-text:5, k-eeg:5, k-labels:3, reach:10m)${RESET}
  ${DIM}#   ${RESET}
  ${DIM}#   Graph  7 nodes · 9 edges${RESET}
  ${DIM}#   ──────────────────────────────────────${RESET}
  ${DIM}#   ● query       "deep focus"${RESET}
  ${DIM}#   ${RESET}
  ${DIM}#   ◆ Text Labels  (2 found)${RESET}
  ${DIM}#   #0  "focused reading session"         sim 88%  2/24/2026 8:00 AM${RESET}
  ${DIM}#   #1  "concentration phase"             sim 82%  2/23/2026 2:30 PM${RESET}
  ${DIM}#   ${RESET}
  ${DIM}#   ◈ EEG Moments  (3 found)${RESET}
  ${DIM}#   #0  2/24/2026, 8:12:45 AM             dist 0.023${RESET}
  ${DIM}#   ${RESET}
  ${DIM}#   ◉ Nearby Labels  (2 found)${RESET}
  ${DIM}#   #0  "eyes closed"                     2/24/2026, 8:13:00 AM  0.8m${RESET}
  ${DIM}# --dot format:${RESET}
  ${DIM}#   digraph interactive_search { … }  (pipe to graphviz)${RESET}
  ${DIM}# --json format:${RESET}
  ${DIM}#   { "query": "deep focus", "nodes": [...], "edges": [...], "dot": "…" }${RESET}

  ${BOLD}search${RESET} — find neurally similar moments (auto: last session)
  ${DIM}$${RESET} npx tsx cli.ts search                              ${DIM}# auto: last session, k=5${RESET}
  ${DIM}$${RESET} npx tsx cli.ts search --k 10                       ${DIM}# 10 nearest neighbors${RESET}
  ${DIM}$${RESET} npx tsx cli.ts search --start 1740412800 --end 1740415500 --k 20
  ${DIM}# Auto-range output:${RESET}
  ${DIM}#   ⚡ search${RESET}
  ${DIM}#     range: 1740412800–1740415500 (auto: 2/24/2026 8:00 AM → 8:45 AM, 45m 0s)${RESET}
  ${DIM}#     k: 5${RESET}
  ${DIM}#     rerun: npx tsx cli.ts search --start 1740412800 --end 1740415500 --k 5${RESET}
  ${DIM}# Result:${RESET}
  ${DIM}#   { "command": "search", "ok": true, "result": {${RESET}
  ${DIM}#     "query_count": 541, "searched_days": ["20260223","20260224"],${RESET}
  ${DIM}#     "results": [{ "timestamp_unix": 1740412800,${RESET}
  ${DIM}#       "neighbors": [{ "distance": 0.023, "timestamp_unix": 1740413100,${RESET}
  ${DIM}#         "date": "20260224", "labels": [{ "text": "focused" }], ... }] }] } }${RESET}

  ${BOLD}compare${RESET} — side-by-side A/B metrics (auto: last 2 sessions)
  ${DIM}$${RESET} npx tsx cli.ts compare                             ${DIM}# auto: last 2 sessions${RESET}
  ${DIM}$${RESET} npx tsx cli.ts compare --a-start 1740380100 --a-end 1740382665 --b-start 1740412800 --b-end 1740415510
  ${DIM}$${RESET} npx tsx cli.ts compare --json | jq '{a_focus: .a.focus, b_focus: .b.focus}'
  ${DIM}# Auto-range output:${RESET}
  ${DIM}#   ⚡ compare${RESET}
  ${DIM}#     A: 1740380100–1740382665 (auto: 2/23/2026 2:30 PM → 3:12 PM, 42m 45s)${RESET}
  ${DIM}#     B: 1740412800–1740415510 (auto: 2/24/2026 8:00 AM → 8:45 AM, 45m 10s)${RESET}
  ${DIM}#     rerun: npx tsx cli.ts compare --a-start 1740380100 --a-end 1740382665 --b-start 1740412800 --b-end 1740415510${RESET}
  ${DIM}# Result:${RESET}
  ${DIM}#   { "a": { "focus": 0.62, "relaxation": 0.45, "hr": 72.1, ... },${RESET}
  ${DIM}#     "b": { "focus": 0.71, "relaxation": 0.38, "hr": 68.4, ... },${RESET}
  ${DIM}#     "sleep_a": { ... }, "sleep_b": { ... },${RESET}
  ${DIM}#     "umap": { "queued": true, "job_id": 3, "estimated_secs": 12, "n_a": 513, "n_b": 541 } }${RESET}

  ${BOLD}sleep${RESET} — sleep staging (auto: last 24h of sessions)
  ${DIM}$${RESET} npx tsx cli.ts sleep                               ${DIM}# auto: last 24h of sessions${RESET}
  ${DIM}$${RESET} npx tsx cli.ts sleep --start 1740380100 --end 1740415510
  ${DIM}$${RESET} npx tsx cli.ts sleep --json | jq '.summary'
  ${DIM}# Output:${RESET}
  ${DIM}#   ⚡ sleep${RESET}
  ${DIM}#     range: 1740380100–1740415510 (auto: 2/23/2026 2:30 PM → 2/24/2026 8:45 AM, 9h 50m)${RESET}
  ${DIM}#     rerun: npx tsx cli.ts sleep --start 1740380100 --end 1740415510${RESET}
  ${DIM}#${RESET}
  ${DIM}#     Sleep Summary${RESET}
  ${DIM}#     total: 1054 epochs (88 min)${RESET}
  ${DIM}#     Wake  134  (12.7%)${RESET}
  ${DIM}#     N1     89  (8.4%)${RESET}
  ${DIM}#     N2    421  (39.9%)${RESET}
  ${DIM}#     N3    298  (28.3%)${RESET}
  ${DIM}#     REM   112  (10.6%)${RESET}

  ${BOLD}umap${RESET} — 3D UMAP projection with progress (auto: last 2 sessions)
  ${DIM}$${RESET} npx tsx cli.ts umap                                ${DIM}# auto: last 2 sessions${RESET}
  ${DIM}$${RESET} npx tsx cli.ts umap --a-start 1740380100 --a-end 1740382665 --b-start 1740412800 --b-end 1740415510
  ${DIM}$${RESET} npx tsx cli.ts umap --json | jq '.points | length'
  ${DIM}# Auto-range output:${RESET}
  ${DIM}#   ⚡ umap${RESET}
  ${DIM}#     A: 1740380100–1740382665 (auto: 2/23/2026 2:30 PM → 3:12 PM, 42m 45s)${RESET}
  ${DIM}#     B: 1740412800–1740415510 (auto: 2/24/2026 8:00 AM → 8:45 AM, 45m 10s)${RESET}
  ${DIM}#     rerun: npx tsx cli.ts umap --a-start 1740380100 --a-end 1740382665 --b-start 1740412800 --b-end 1740415510${RESET}
  ${DIM}#   enqueued job_id=5  n_a=513  n_b=541  est=14s${RESET}
  ${DIM}#   ████████████░░░░░░░░░░░░░░░░░░ 40%  epoch 80/200  42ms/ep  ~5s left${RESET}
  ${DIM}#   completed in 8432ms${RESET}
  ${DIM}# Result:${RESET}
  ${DIM}#   { "status": "complete", "result": {${RESET}
  ${DIM}#     "points": [{ "x": 1.23, "y": -0.45, "z": 2.01, "session": "A", "utc": 1740380105 }, ...],${RESET}
  ${DIM}#     "n_a": 513, "n_b": 541, "dim": 3, "elapsed_ms": 8432 } }${RESET}

  ${BOLD}dnd${RESET} — Do Not Disturb automation status and control
  ${DIM}$${RESET} npx tsx cli.ts dnd                                 ${DIM}# show config + live eligibility state${RESET}
  ${DIM}$${RESET} npx tsx cli.ts dnd on                              ${DIM}# force-enable DND (bypass EEG threshold)${RESET}
  ${DIM}$${RESET} npx tsx cli.ts dnd off                             ${DIM}# force-disable DND${RESET}
  ${DIM}$${RESET} npx tsx cli.ts dnd --json                          ${DIM}# raw JSON (pipe to jq)${RESET}
  ${DIM}# Output (dnd):${RESET}
  ${DIM}#   DND automation  enabled=false  threshold=60  duration=60s${RESET}
  ${DIM}#   focus timer     elapsed=0s / 60s required${RESET}
  ${DIM}#   app active      false${RESET}
  ${DIM}#   OS active       false  (macOS Assertions.json)${RESET}
  ${DIM}# Output (dnd on):${RESET}
  ${DIM}#   DND activated  ok=true${RESET}

  ${BOLD}listen${RESET} — stream live broadcast events
  ${DIM}$${RESET} npx tsx cli.ts listen                              ${DIM}# 5 seconds${RESET}
  ${DIM}$${RESET} npx tsx cli.ts listen --seconds 30
  ${DIM}$${RESET} npx tsx cli.ts listen --seconds 10 --json
  ${DIM}# Output:${RESET}
  ${DIM}#   ⚡ listen for 5s…${RESET}
  ${DIM}#     eeg ×47${RESET}
  ${DIM}#     ppg ×12${RESET}
  ${DIM}#     scores ×5${RESET}
  ${DIM}#   [{ "event": "eeg", "electrode": 0, "samples": [...], "timestamp": 1740412800.5 }, ...]${RESET}

  ${BOLD}llm${RESET} — LLM inference server management + chat
  ${DIM}$${RESET} npx tsx cli.ts llm status
  ${DIM}$${RESET} npx tsx cli.ts llm start           ${DIM}# load active model (may take seconds)${RESET}
  ${DIM}$${RESET} npx tsx cli.ts llm stop
  ${DIM}$${RESET} npx tsx cli.ts llm catalog
  ${DIM}$${RESET} npx tsx cli.ts llm add bartowski/Phi-4-mini-reasoning-GGUF Phi-4-mini-reasoning-Q4_K_M.gguf
  ${DIM}$${RESET} npx tsx cli.ts llm add bartowski/Phi-4-mini-reasoning-GGUF Phi-4-mini-reasoning-Q4_K_M.gguf --mmproj mmproj-Phi-4-mini-reasoning-BF16.gguf
  ${DIM}$${RESET} npx tsx cli.ts llm add https://huggingface.co/bartowski/Phi-4-mini-reasoning-GGUF/blob/main/Phi-4-mini-reasoning-Q4_K_M.gguf
  ${DIM}$${RESET} npx tsx cli.ts llm select "Qwen_Qwen3.5-4B-Q4_K_M.gguf"
  ${DIM}$${RESET} npx tsx cli.ts llm mmproj "mmproj-Qwen_Qwen3.5-4B-BF16.gguf"
  ${DIM}$${RESET} npx tsx cli.ts llm mmproj none     ${DIM}# disable vision projector${RESET}
  ${DIM}$${RESET} npx tsx cli.ts llm autoload-mmproj on
  ${DIM}$${RESET} npx tsx cli.ts llm download "Qwen_Qwen3.5-4B-Q4_K_M.gguf"
  ${DIM}$${RESET} npx tsx cli.ts llm pause "Qwen_Qwen3.5-4B-Q4_K_M.gguf"
  ${DIM}$${RESET} npx tsx cli.ts llm resume "Qwen_Qwen3.5-4B-Q4_K_M.gguf"
  ${DIM}$${RESET} npx tsx cli.ts llm downloads       ${DIM}# list all downloads with progress${RESET}
  ${DIM}$${RESET} npx tsx cli.ts llm refresh          ${DIM}# re-probe HF cache${RESET}
  ${DIM}$${RESET} npx tsx cli.ts llm fit              ${DIM}# hardware fit for all models${RESET}
  ${DIM}$${RESET} npx tsx cli.ts llm logs
  ${DIM}$${RESET} npx tsx cli.ts llm chat             ${DIM}# interactive REPL (multi-turn, type 'exit' to quit)${RESET}
  ${DIM}$${RESET} npx tsx cli.ts llm chat "What EEG band is linked to relaxation?"
  ${DIM}$${RESET} npx tsx cli.ts llm chat --system "You are a concise neuroscience assistant."
  ${DIM}$${RESET} npx tsx cli.ts llm chat "Explain delta waves" --temperature 0.3 --max-tokens 256
  ${DIM}$${RESET} npx tsx cli.ts llm chat "What's in this image?" --image eeg_plot.png
  ${DIM}$${RESET} npx tsx cli.ts llm chat "Compare these" --image a.jpg --image b.jpg
  ${DIM}$${RESET} npx tsx cli.ts llm chat "Describe" --image scan.png --http   ${DIM}# HTTP non-streaming${RESET}
  ${DIM}# Interactive REPL commands:${RESET}
  ${DIM}#   /image <path> — stage an image for the next message${RESET}
  ${DIM}#   /images       — show staged image count${RESET}
  ${DIM}#   /clear        — clear conversation history (keep system prompt)${RESET}
  ${DIM}#   /history      — show all messages in the current conversation${RESET}
  ${DIM}#   /help         — show REPL help${RESET}
  ${DIM}#   exit          — end the session${RESET}

  ${BOLD}raw${RESET} — send arbitrary JSON
  ${DIM}$${RESET} npx tsx cli.ts raw '{"command":"status"}'
  ${DIM}$${RESET} npx tsx cli.ts raw '{"command":"sessions"}' --json
  ${DIM}$${RESET} npx tsx cli.ts raw '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":3}'
  ${DIM}# Output: full JSON response from server, same as calling the command directly${RESET}
`);
  process.exit(0);
}

// ── Session-aware defaults ────────────────────────────────────────────────────

/**
 * A recording session as returned by the server's `sessions` command.
 * Each session is a contiguous range of EEG embedding epochs (gap ≤ 120 s).
 */
interface Session {
  /** Date string `YYYYMMDD` for the day this session belongs to. */
  day: string;
  /** Start of the session in unix seconds (UTC). */
  start_utc: number;
  /** End of the session in unix seconds (UTC). */
  end_utc: number;
  /** Number of 5-second embedding epochs in this session. */
  n_epochs: number;
}

/**
 * Fetch the full session list from the server (cached after first call).
 *
 * Used by auto-range helpers to pick smart defaults from actual recorded
 * data instead of arbitrary "last N hours" windows.  The cache avoids
 * redundant round-trips when multiple auto-range calls happen in sequence
 * (e.g. `compare` calls `autoRangeAB()` which internally needs sessions).
 *
 * @returns Array of sessions sorted oldest-first (server returns newest-first,
 *          but the cache preserves server order — our callers index from the end).
 */
let _sessionCache: Session[] | null = null;
async function getSessions(): Promise<Session[]> {
  if (_sessionCache) return _sessionCache;
  const r = await send({ command: "sessions" });
  _sessionCache = (r.ok ? r.sessions : []) as Session[];
  return _sessionCache;
}

/**
 * Format a UTC unix timestamp as a human-readable local date+time string.
 * Uses the system locale (e.g. `"2/24/2026, 8:00:00 AM"`).
 *
 * @param utc - Unix seconds (UTC).
 */
function fmtTs(utc: number): string {
  return new Date(utc * 1000).toLocaleString(undefined, { timeZoneName: "short" });
}

/**
 * Format a duration in seconds as a compact human-readable string.
 *
 * Examples: `45s`, `12m 30s`, `2h 15m`.
 *
 * @param secs - Duration in seconds.
 */
function fmtDur(secs: number): string {
  if (secs >= 3600) return `${Math.floor(secs/3600)}h ${Math.floor((secs%3600)/60)}m`;
  if (secs >= 60)   return `${Math.floor(secs/60)}m ${secs%60}s`;
  return `${secs}s`;
}

/**
 * Pick a smart `[start, end]` range from existing sessions.
 *
 * Strategy: use the most recent session's full time span.  If the server
 * has no sessions at all, falls back to `[now - fallbackSecs, now]`.
 *
 * Used by `search` and `sleep` when no `--start`/`--end` flags are given.
 *
 * @param fallbackSecs - Fallback window size in seconds (e.g. 600 for 10 min).
 * @returns Tuple of `[start_utc, end_utc]` in unix seconds.
 */
async function autoRange(fallbackSecs: number): Promise<[number, number]> {
  const sessions = await getSessions();
  if (sessions.length > 0) {
    // Server returns sessions newest-first; [0] is the most recent.
    const last = sessions[0];
    return [last.start_utc, last.end_utc];
  }
  const now = Math.floor(Date.now() / 1000);
  return [now - fallbackSecs, now];
}

/**
 * Pick two non-overlapping time ranges for A/B comparison.
 *
 * Strategy (in priority order):
 * 1. **≥2 sessions** → use the second-to-last as A, the last as B.
 * 2. **1 session**   → split it at the midpoint (first half = A, second = B).
 * 3. **0 sessions**  → fall back to `[now-2h, now-1h]` vs `[now-1h, now]`.
 *
 * Used by `compare` and `umap` when no `--a-start`/`--b-start` flags are given.
 *
 * @returns Object with `aStart`, `aEnd`, `bStart`, `bEnd` (unix seconds).
 */
async function autoRangeAB(): Promise<{ aStart: number; aEnd: number; bStart: number; bEnd: number }> {
  // Server returns sessions newest-first: [0] = most recent, [1] = second-most-recent.
  const sessions = await getSessions();
  if (sessions.length >= 2) {
    const a = sessions[1]; // second-most-recent → session A (earlier)
    const b = sessions[0]; // most recent → session B (later)
    return { aStart: a.start_utc, aEnd: a.end_utc, bStart: b.start_utc, bEnd: b.end_utc };
  }
  if (sessions.length === 1) {
    const s = sessions[0];
    const mid = Math.floor((s.start_utc + s.end_utc) / 2);
    return { aStart: s.start_utc, aEnd: mid, bStart: mid, bEnd: s.end_utc };
  }
  const now = Math.floor(Date.now() / 1000);
  return { aStart: now - 7200, aEnd: now - 3600, bStart: now - 3600, bEnd: now };
}

/**
 * Print a labeled time range to stdout.
 *
 * When `isDefault` is true (i.e. the range was auto-selected), appends a
 * dim annotation showing the human-readable date/time and duration so the
 * user understands what was chosen:
 * ```
 *   A: 1740380100–1740382665 (auto: 2/23/2026 2:30 PM → 3:12 PM, 42m 45s)
 * ```
 *
 * @param label     - Range label, e.g. `"A"`, `"B"`, `"range"`.
 * @param start     - Start time in unix seconds.
 * @param end       - End time in unix seconds.
 * @param isDefault - Whether this range was auto-selected (show annotation).
 */
function printRange(label: string, start: number, end: number, isDefault: boolean) {
  const tag = isDefault ? `${DIM}(auto: ${fmtTs(start)} → ${fmtTs(end)}, ${fmtDur(end - start)})${RESET}` : "";
  print(`  ${DIM}${label}:${RESET} ${CYAN}${start}${RESET}${DIM}–${RESET}${CYAN}${end}${RESET} ${tag}`);
}

/**
 * Print a copy-pasteable rerun command line with all resolved parameters.
 *
 * Only called when parameters were auto-selected, so the user can reproduce
 * the exact same query later without relying on auto-detection:
 * ```
 *   rerun: npx tsx cli.ts search --start 1740412800 --end 1740415500 --k 5
 * ```
 *
 * @param cmdLine - The command + flags portion (without the `npx tsx cli.ts` prefix).
 */
function printRerun(cmdLine: string) {
  print(`\n  ${DIM}rerun:${RESET} ${YELLOW}npx tsx cli.ts ${cmdLine}${RESET}\n`);
}

// ── Command handlers ──────────────────────────────────────────────────────────

/**
 * `status` — Fetch and display the full device/session/scores snapshot.
 *
 * Sends `{ command: "status" }` to the server.  Response includes:
 * - `device` — connection state, name, battery, firmware, sample counts
 * - `session` — current session start time and duration
 * - `embeddings` — today's count, total count, recording days
 * - `scores` — latest epoch metrics (focus, relaxation, HR, meditation, etc.)
 * - `signal_quality` — per-electrode quality (tp9, af7, af8, tp10)
 * - `sleep` — 48h sleep stage summary
 * - `labels` — total annotation count
 * - `calibration` — last calibration timestamp
 *
 * No parameters required.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Basic status snapshot:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"status"}'
 *
 * # Pipe to jq to extract a single metric:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"status"}' | jq '.scores.focus'
 * ```
 */
async function cmdStatus(args: Args): Promise<void> {
  /**
   * Fetch and render a single status snapshot.
   * Extracted so the poll loop can call it repeatedly on the same connection.
   */
  async function statusOnce(): Promise<void> {
  const r = await send({ command: "status" });
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);

  if (!jsonMode) {
    // ── Device ───────────────────────────────────────────────────────────
    if (r.device) {
      const d = r.device;
      const stateColor = d.state === "connected" ? GREEN : d.state === "connecting" ? YELLOW : RED;
      const batColor    = d.battery >= 50 ? GREEN : d.battery >= 20 ? YELLOW : RED;
      print("");
      print(`  ${BOLD}Device${RESET}`);
      print(`  ${DIM}state:${RESET}    ${stateColor}${d.state ?? "unknown"}${RESET}`
          + (d.name     ? `   ${DIM}name:${RESET} ${CYAN}${d.name}${RESET}`                            : "")
          + (d.battery  != null ? `   ${DIM}battery:${RESET} ${batColor}${d.battery}%${RESET}`         : "")
          + (d.firmware ? `   ${DIM}firmware:${RESET} ${DIM}${d.firmware}${RESET}`                     : ""));
      if (d.eeg_samples != null || d.ppg_samples != null || d.imu_samples != null) {
        print(`  ${DIM}samples:${RESET}  `
            + (d.eeg_samples != null ? `EEG ${CYAN}${d.eeg_samples}${RESET}  ` : "")
            + (d.ppg_samples != null ? `PPG ${CYAN}${d.ppg_samples}${RESET}  ` : "")
            + (d.imu_samples != null ? `IMU ${CYAN}${d.imu_samples}${RESET}`   : ""));
      }
    }

    // ── Session ──────────────────────────────────────────────────────────
    if (r.session) {
      const s = r.session;
      print("");
      print(`  ${BOLD}Session${RESET}`);
      print(`  ${DIM}started:${RESET}  ${CYAN}${fmtTs(s.start_utc)}${RESET}`
          + (s.duration_secs != null ? `   ${DIM}duration:${RESET} ${CYAN}${fmtDur(s.duration_secs)}${RESET}` : "")
          + (s.n_epochs      != null ? `   ${DIM}epochs:${RESET} ${CYAN}${s.n_epochs}${RESET}`                 : ""));
    }

    // ── Signal Quality ───────────────────────────────────────────────────
    if (r.signal_quality) {
      const q = r.signal_quality;
      const qColor = (v: number) => v >= 0.9 ? GREEN : v >= 0.7 ? YELLOW : RED;
      const qFmt   = (label: string, v: number | undefined) =>
        v != null ? `${DIM}${label}:${RESET} ${qColor(v)}${(v * 100).toFixed(0)}%${RESET}` : "";
      print("");
      print(`  ${BOLD}Signal Quality${RESET}`);
      // Dynamically render all channel quality keys (works for Muse 4ch, Hermes 8ch, MW75 12ch)
      const channelKeys = Object.keys(q).filter(k => typeof q[k] === "number");
      const cols = Math.min(channelKeys.length, 4);
      for (let i = 0; i < channelKeys.length; i += cols) {
        const row = channelKeys.slice(i, i + cols).map(k => qFmt(k, q[k])).filter(Boolean);
        if (row.length > 0) print(`  ${row.join("   ")}`);
      }
    }

    // ── Scores ───────────────────────────────────────────────────────────
    if (r.scores) {
      const s = r.scores;
      const scoreMetrics: [string, string][] = [
        ["focus",          "focus"],
        ["relaxation",     "relaxation"],
        ["engagement",     "engagement"],
        ["hr",             "hr"],
        ["meditation",     "meditation"],
        ["drowsiness",     "drowsiness"],
        ["mood",           "mood"],
        ["snr",            "snr"],
        ["stillness",      "stillness"],
        ["cognitive_load", "cog.load"],
      ];
      const present = scoreMetrics.filter(([k]) => s[k] != null);
      if (present.length > 0) {
        print("");
        print(`  ${BOLD}Scores${RESET}`);
        // Print in rows of 3
        const cols = 3;
        for (let i = 0; i < present.length; i += cols) {
          const row = present.slice(i, i + cols)
            .map(([k, label]) => `${DIM}${label.padEnd(12)}${RESET}${CYAN}${String(
              typeof s[k] === "number" && !Number.isInteger(s[k]) ? s[k].toFixed(2) : s[k]
            ).padStart(6)}${RESET}`)
            .join("   ");
          print(`  ${row}`);
        }
      }
    }

    // ── EEG Bands ────────────────────────────────────────────────────────
    if (r.scores?.bands) {
      const b = r.scores.bands;
      const bandMetrics: [string, string][] = [
        ["δ delta",  "rel_delta"],
        ["θ theta",  "rel_theta"],
        ["α alpha",  "rel_alpha"],
        ["β beta",   "rel_beta"],
        ["γ gamma",  "rel_gamma"],
      ];
      const present = bandMetrics.filter(([, k]) => b[k] != null);
      if (present.length > 0) {
        print("");
        print(`  ${BOLD}EEG Bands${RESET}`);
        const cols = 3;
        for (let i = 0; i < present.length; i += cols) {
          const row = present.slice(i, i + cols)
            .map(([label, k]) => `${DIM}${label.padEnd(12)}${RESET}${CYAN}${((b[k] as number) * 100).toFixed(1).padStart(6)}%${RESET}`)
            .join("   ");
          print(`  ${row}`);
        }
      }
    }

    // ── EEG Ratios & Indices ─────────────────────────────────────────────
    if (r.scores) {
      const s = r.scores;
      const ratioMetrics: [string, string, number][] = [
        ["FAA",              "faa",               3],
        ["theta/alpha",      "tar",               3],
        ["beta/alpha",       "bar",               3],
        ["delta/theta",      "dtr",               3],
        ["theta/beta",       "tbr",               3],
        ["PSE",              "pse",               3],
        ["alpha peak (Hz)",  "apf",               2],
        ["band-pwr slope",   "bps",               3],
        ["coherence",        "coherence",         3],
        ["mu suppression",   "mu_suppression",    3],
        ["SEF95 (Hz)",       "sef95",             2],
        ["spec. centroid",   "spectral_centroid", 2],
        ["laterality idx",   "laterality_index",  3],
      ];
      const present = ratioMetrics.filter(([, k]) => s[k] != null && s[k] !== 0);
      if (present.length > 0) {
        print("");
        print(`  ${BOLD}EEG Ratios & Indices${RESET}`);
        const cols = 3;
        for (let i = 0; i < present.length; i += cols) {
          const row = present.slice(i, i + cols)
            .map(([label, k, dec]) => `${DIM}${label.padEnd(16)}${RESET}${CYAN}${(s[k] as number).toFixed(dec).padStart(7)}${RESET}`)
            .join("   ");
          print(`  ${row}`);
        }
      }

      const complexityMetrics: [string, string][] = [
        ["Hjorth act.",     "hjorth_activity"],
        ["Hjorth mob.",     "hjorth_mobility"],
        ["Hjorth cplx.",    "hjorth_complexity"],
        ["perm. entropy",   "permutation_entropy"],
        ["Higuchi FD",      "higuchi_fd"],
        ["DFA exponent",    "dfa_exponent"],
        ["sample entropy",  "sample_entropy"],
        ["PAC θ-γ",        "pac_theta_gamma"],
      ];
      const cpresent = complexityMetrics.filter(([, k]) => s[k] != null && s[k] !== 0);
      if (cpresent.length > 0) {
        print("");
        print(`  ${BOLD}EEG Complexity${RESET}`);
        const cols = 3;
        for (let i = 0; i < cpresent.length; i += cols) {
          const row = cpresent.slice(i, i + cols)
            .map(([label, k]) => `${DIM}${label.padEnd(16)}${RESET}${CYAN}${(s[k] as number).toFixed(3).padStart(7)}${RESET}`)
            .join("   ");
          print(`  ${row}`);
        }
      }
    }

    // ── Embeddings ───────────────────────────────────────────────────────
    if (r.embeddings) {
      const e = r.embeddings;
      print("");
      print(`  ${BOLD}Embeddings${RESET}`);
      print(`  ${DIM}today:${RESET} ${CYAN}${e.today ?? 0}${RESET}   ${DIM}total:${RESET} ${CYAN}${e.total ?? 0}${RESET}   ${DIM}recording days:${RESET} ${CYAN}${e.recording_days ?? 0}${RESET}`);
    }

    // ── Labels ───────────────────────────────────────────────────────────
    if (r.labels != null) {
      const labelCount = typeof r.labels === "object" ? (r.labels.total ?? r.labels) : r.labels;
      const recent: Array<{ id: number; text: string; created_at: number }> =
        Array.isArray(r.labels?.recent) ? r.labels.recent : [];
      print("");
      print(`  ${BOLD}Labels${RESET}  ${DIM}(${labelCount} total)${RESET}`);
      if (recent.length === 0) {
        print(`  ${DIM}no labels yet${RESET}`);
      } else {
        for (const lbl of recent) {
          const when = fmtTs(lbl.created_at);
          print(`  ${DIM}#${lbl.id}${RESET}  ${GREEN}"${lbl.text}"${RESET}  ${DIM}${when}${RESET}`);
        }
      }
    }

    // ── Calibration ──────────────────────────────────────────────────────
    if (r.calibration != null) {
      const cal = r.calibration;
      print("");
      print(`  ${BOLD}Calibration${RESET}`);

      if (typeof cal === "number") {
        print(`  ${DIM}last:${RESET} ${CYAN}${fmtTs(cal)}${RESET}`);
      } else if (typeof cal === "object") {
        // ── timestamp
        const tsRaw = cal.last_calibration_utc ?? cal.last_utc ?? cal.completed_at ?? cal.timestamp ?? cal.last ?? cal.created_at;
        if (typeof tsRaw === "number") {
          print(`  ${DIM}last:${RESET} ${CYAN}${fmtTs(tsRaw)}${RESET}`);
        } else if (tsRaw != null) {
          print(`  ${DIM}last:${RESET} ${CYAN}${String(tsRaw)}${RESET}`);
        }

        // ── profile name (explicit label so it's always shown first and clearly)
        if (cal.name) print(`  ${DIM}profile:${RESET} ${CYAN}${cal.name}${RESET}`);

        // ── other scalar fields (loop_count, break_duration_secs, …)
        const skipKeys = new Set(["last_calibration_utc", "last_utc", "completed_at", "timestamp",
                                  "last", "created_at", "actions", "labels", "steps",
                                  "profile_id", "id", "name", "auto_start"]);
        for (const [k, v] of Object.entries(cal)) {
          if (skipKeys.has(k) || v == null || typeof v === "object") continue;
          print(`  ${DIM}${k}:${RESET} ${CYAN}${v}${RESET}`);
        }

        // ── all action labels shown as a sequence (no truncation)
        const itemsRaw: unknown[] =
          (Array.isArray(cal.actions) && cal.actions.length > 0) ? cal.actions :
          (Array.isArray(cal.labels)  && cal.labels.length  > 0) ? cal.labels  :
          (Array.isArray(cal.steps)   && cal.steps.length   > 0) ? cal.steps   : [];

        if (itemsRaw.length > 0) {
          const nameOf = (item: unknown): string => {
            if (typeof item === "string") return item;
            if (item && typeof item === "object") {
              const o = item as Record<string, unknown>;
              return String(o.name ?? o.label ?? o.title ?? o.action ?? JSON.stringify(item));
            }
            return String(item);
          };
          const allLabels = itemsRaw.map(nameOf);
          print(`  ${DIM}labels:${RESET} ${allLabels.map(n => `${CYAN}${n}${RESET}`).join(`${DIM} → ${RESET}`)}`);
        }
      }
    }

    // ── Hooks ─────────────────────────────────────────────────────────────
    if (r.hooks && typeof r.hooks === "object") {
      const h = r.hooks;
      print("");
      print(`  ${BOLD}Hooks${RESET}  ${DIM}(${h.total ?? 0} total, ${h.enabled ?? 0} enabled)${RESET}`);

      const lt = h.latest_trigger;
      if (lt && lt.triggered_at_utc) {
        const when = fmtTs(lt.triggered_at_utc);
        const ago  = Math.floor(Date.now() / 1000) - lt.triggered_at_utc;
        const agoStr = fmtDur(ago);
        const distStr = typeof lt.distance === "number" ? lt.distance.toFixed(4) : "?";
        const distColor = typeof lt.distance === "number" && lt.distance < 0.1 ? GREEN : CYAN;
        print(`  ${DIM}latest:${RESET} ${YELLOW}${lt.hook}${RESET}  ${DIM}${when}${RESET}  ${DIM}(${agoStr} ago)${RESET}`);
        print(`  ${DIM}  dist:${RESET} ${distColor}${distStr}${RESET}`
          + (lt.label_text ? `  ${DIM}label:${RESET} ${GREEN}"${lt.label_text}"${RESET}` : "")
          + (lt.label_id   != null ? `  ${DIM}id:${RESET} ${CYAN}${lt.label_id}${RESET}` : ""));
      } else {
        print(`  ${DIM}latest: never triggered${RESET}`);
      }
    }

    // ── Sleep (48 h summary) ─────────────────────────────────────────────
    if (r.sleep) {
      const s = r.sleep;
      const total = s.total_epochs ?? 0;
      const pct   = (n: number) => total > 0 ? ` ${DIM}(${((n / total) * 100).toFixed(0)}%)${RESET}` : "";
      print("");
      print(`  ${BOLD}Sleep (48 h)${RESET}`);
      print(`  ${DIM}total:${RESET} ${CYAN}${total}${RESET} epochs`
          + (s.epoch_secs ? `  ${DIM}(${((total * s.epoch_secs) / 60).toFixed(0)} min)${RESET}` : ""));
      if (s.wake_epochs != null) {
        print(`  ${GREEN}Wake${RESET}  ${CYAN}${s.wake_epochs}${RESET}${pct(s.wake_epochs)}`
            + `   ${YELLOW}N1${RESET}  ${CYAN}${s.n1_epochs ?? 0}${RESET}${pct(s.n1_epochs ?? 0)}`
            + `   ${BLUE}N2${RESET}  ${CYAN}${s.n2_epochs ?? 0}${RESET}${pct(s.n2_epochs ?? 0)}`
            + `   ${MAGENTA}N3${RESET}  ${CYAN}${s.n3_epochs ?? 0}${RESET}${pct(s.n3_epochs ?? 0)}`
            + `   ${RED}REM${RESET} ${CYAN}${s.rem_epochs ?? 0}${RESET}${pct(s.rem_epochs ?? 0)}`);
      }
    }

    // ── Recording History ────────────────────────────────────────────────
    if (r.history && r.history !== null) {
      const h = r.history;
      print("");
      print(`  ${BOLD}Recording History${RESET}`);
      print(`  ${DIM}sessions:${RESET} ${CYAN}${h.total_sessions}${RESET}  ${DIM}days:${RESET} ${CYAN}${h.recording_days}${RESET}  ${DIM}streak:${RESET} ${GREEN}${h.current_streak_days}d${RESET}  ${DIM}total:${RESET} ${CYAN}${h.total_recording_hours?.toFixed(1)}h${RESET}`);
      print(`  ${DIM}longest:${RESET} ${CYAN}${h.longest_session_min?.toFixed(0)}${RESET}min  ${DIM}avg:${RESET} ${CYAN}${h.avg_session_min?.toFixed(0)}${RESET}min`);

      if (h.today_vs_avg && Object.keys(h.today_vs_avg).length > 0) {
        print(`\n  ${BOLD}Today vs 7-Day Average${RESET}`);
        for (const [metric, v] of Object.entries(h.today_vs_avg) as [string, any][]) {
          const arrow = v.direction === "up" ? `${GREEN}↑${RESET}` : v.direction === "down" ? `${RED}↓${RESET}` : `${DIM}→${RESET}`;
          const pct = v.delta_pct > 0 ? `+${v.delta_pct.toFixed(1)}%` : `${v.delta_pct.toFixed(1)}%`;
          print(`    ${metric.padEnd(18)} ${CYAN}${v.today?.toFixed(2).padStart(7)}${RESET} ${DIM}vs${RESET} ${CYAN}${v.avg_7d?.toFixed(2).padStart(7)}${RESET}  ${arrow} ${pct}`);
        }
      }
    }

    print("");
  }

  printResult(r);
  } // end statusOnce

  if (args.poll) {
    print(`${BOLD}⚡ status${RESET} ${DIM}(polling every ${args.poll}s — press Ctrl+C to stop)${RESET}`);
    while (true) {
      print(`\n${DIM}── ${new Date().toLocaleTimeString()} ──────────────────────────────────────────────${RESET}`);
      await statusOnce();
      await new Promise(res => setTimeout(res, args.poll! * 1000));
    }
  } else {
    print(`${BOLD}⚡ status${RESET}`);
    await statusOnce();
  }
}

/**
 * `session [index]` — Full metric breakdown + first/second-half trends for one session.
 *
 * Resolves the target session by index (0 = most recent, 1 = previous, …).
 * Omitting the index defaults to 0 (the current/latest session).
 * Fetches the session list, picks the session at the given index, then calls
 * `session_metrics` which returns full-range averages plus first-half and
 * second-half sub-range metrics for every field in `SessionMetrics`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Fetch sessions first to get start/end timestamps:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"sessions"}' | jq '.sessions[0]'
 *
 * # Then call session_metrics with the resolved range:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"session_metrics","start_utc":1740412800,"end_utc":1740415510}'
 * ```
 *
 * @param args - Parsed CLI arguments; `sessionIndex` selects which session (default 0).
 */
async function cmdSession(args: Args): Promise<void> {
  const idx = args.sessionIndex ?? 0;   // default to latest when not specified
  print(`${BOLD}⚡ session${RESET} ${DIM}[${idx}]${RESET}`);

  const sr = await send({ command: "sessions" });
  if (!sr.ok) printError(`sessions failed: ${sr.error}`);

  const list: Array<{ day: string; start_utc: number; end_utc: number; n_epochs: number }> =
    sr.sessions ?? [];

  if (list.length === 0) printError("no sessions recorded yet");
  if (idx >= list.length) {
    printError(`session index ${idx} out of range — only ${list.length} session(s) available (0–${list.length - 1})`);
  }

  const s = list[idx];

  if (!jsonMode) {
    const startStr = fmtTs(s.start_utc);
    const endStr   = new Date(s.end_utc * 1000).toLocaleTimeString(undefined, { timeZoneName: "short" });
    print(`  ${CYAN}${s.day}${RESET}  ${startStr} → ${endStr}  ${YELLOW}${fmtDur(s.end_utc - s.start_utc)}${RESET}  ${DIM}${s.n_epochs} epochs${RESET}`);
    if (list.length > 1) {
      print(`  ${DIM}(index ${idx} of ${list.length - 1}; use \`session ${idx + 1}\` for previous)${RESET}`);
    }
  }

  const tm = await send({ command: "session_metrics", start_utc: s.start_utc, end_utc: s.end_utc });
  if (!tm.ok) printError(`session_metrics failed: ${tm.error}`);

  if (jsonMode) { printResult(tm); return; }

  const m = tm.metrics as Record<string, number>;
  const f = tm.first   as Record<string, number>;
  const sc = tm.second  as Record<string, number>;
  const t = tm.trends  as Record<string, string>;

  // ── Rendering helpers ────────────────────────────────────────────────────
  const arrowOf = (dir: string) =>
    dir === "up"   ? `${GREEN}↑${RESET}` :
    dir === "down" ? `${RED}↓${RESET}`   : `${DIM}→${RESET}`;

  // Format a metric row: label | avg value | arrow | (first → second)
  const row = (
    label: string,
    key: string,
    dec  = 2,
    pct  = false,   // multiply by 100 and show %
  ): string | null => {
    const avg = m[key];
    const fv  = f[key];
    const sv  = sc[key];
    if (avg == null || (avg === 0 && fv === 0 && sv === 0)) return null;
    const fmt  = (v: number) => pct ? `${(v * 100).toFixed(0)}%` : v.toFixed(dec);
    const dir  = t[key] ?? "flat";
    const delta = `${DIM}(${fmt(fv)} → ${fmt(sv)})${RESET}`;
    return (
      `  ${DIM}${label.padEnd(22)}${RESET}` +
      `${CYAN}${fmt(avg).padStart(7)}${RESET}` +
      `  ${arrowOf(dir)}  ${delta}`
    );
  };

  const section = (title: string, rows: (string | null)[]) => {
    const lines = rows.filter((r): r is string => r !== null);
    if (lines.length === 0) return;
    print(`\n  ${BOLD}${title}${RESET}`);
    lines.forEach(l => print(l));
  };

  // ── Metric groups ────────────────────────────────────────────────────────
  section("Core Scores", [
    row("focus",          "focus",          2),
    row("relaxation",     "relaxation",     2),
    row("engagement",     "engagement",     2),
    row("meditation",     "meditation",     2),
    row("mood",           "mood",           2),
    row("cognitive load", "cognitive_load", 2),
    row("drowsiness",     "drowsiness",     2),
  ]);

  section("PPG / Heart", [
    row("heart rate (bpm)", "hr",               1),
    row("rmssd (ms)",       "rmssd",            1),
    row("sdnn (ms)",        "sdnn",             1),
    row("pnn50 (%)",        "pnn50",            1),
    row("lf/hf ratio",      "lf_hf_ratio",      2),
    row("resp. rate",       "respiratory_rate", 1),
    row("SpO₂",            "spo2_estimate",    1),
    row("perfusion idx",    "perfusion_index",  3),
    row("stress index",     "stress_index",     2),
  ]);

  section("EEG Bands", [
    row("δ delta",      "rel_delta",      1, true),
    row("θ theta",      "rel_theta",      1, true),
    row("α alpha",      "rel_alpha",      1, true),
    row("β beta",       "rel_beta",       1, true),
    row("γ gamma",      "rel_gamma",      1, true),
    row("high γ",       "rel_high_gamma", 1, true),
  ]);

  section("EEG Ratios & Indices", [
    row("FAA",              "faa",              3),
    row("theta/alpha",      "tar",              3),
    row("beta/alpha",       "bar",              3),
    row("delta/theta",      "dtr",              3),
    row("theta/beta",       "tbr",              3),
    row("PSE",              "pse",              3),
    row("alpha peak (Hz)",  "apf",              2),
    row("band-pwr slope",   "bps",              3),
    row("SNR (dB)",         "snr",              1),
    row("coherence",        "coherence",        3),
    row("mu suppression",   "mu_suppression",   3),
    row("SEF95 (Hz)",       "sef95",            2),
    row("spectral centroid","spectral_centroid",2),
    row("laterality idx",   "laterality_index", 3),
  ]);

  section("Complexity Measures", [
    row("Hjorth activity",  "hjorth_activity",     3),
    row("Hjorth mobility",  "hjorth_mobility",     3),
    row("Hjorth complexity","hjorth_complexity",   3),
    row("perm. entropy",    "permutation_entropy", 3),
    row("Higuchi FD",       "higuchi_fd",          3),
    row("DFA exponent",     "dfa_exponent",        3),
    row("sample entropy",   "sample_entropy",      3),
    row("PAC θ-γ",         "pac_theta_gamma",     3),
  ]);

  section("Motion & Artifacts", [
    row("stillness",        "stillness",        2),
    row("head pitch",       "head_pitch",       1),
    row("head roll",        "head_roll",        1),
    row("nods",             "nod_count",        0),
    row("head shakes",      "shake_count",      0),
    row("blinks",           "blink_count",      0),
    row("blink rate /min",  "blink_rate",       1),
    row("jaw clenches",     "jaw_clench_count", 0),
    row("jaw clench rate",  "jaw_clench_rate",  1),
  ]);

  print("");
  printResult(tm);
}

/**
 * `sessions` — List all recording sessions across all days.
 *
 * Sends `{ command: "sessions" }`.  Response contains `sessions[]` array,
 * each with `{ day, start_utc, end_utc, n_epochs }`.  Sessions are
 * contiguous embedding ranges (gap threshold: 120 s).
 *
 * In normal mode, prints a human-readable table with date, time range,
 * duration, and epoch count before the full JSON.  In `--json` mode,
 * outputs only the raw JSON.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # List all sessions:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"sessions"}'
 *
 * # Extract the first (most recent) session via jq:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"sessions"}' | jq '.sessions[0]'
 * ```
 */
async function cmdSessions(args: Args): Promise<void> {
  print(`${BOLD}⚡ sessions${RESET}`);
  const r = await send({ command: "sessions" });
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);

  if (jsonMode) {
    printResult(r);
    return;
  }

  const list: Array<{ day: string; start_utc: number; end_utc: number; n_epochs: number }> =
    r.sessions || [];
  print(`${GREEN}${list.length}${RESET} session(s)\n`);

  for (const s of list) {
    const start = fmtTs(s.start_utc);
    const end   = new Date(s.end_utc * 1000).toLocaleTimeString(undefined, { timeZoneName: "short" });
    print(`  ${CYAN}${s.day}${RESET}  ${start} → ${end}  ${YELLOW}${fmtDur(s.end_utc - s.start_utc)}${RESET}  ${DIM}${s.n_epochs} epochs${RESET}`);

    if (args.trends) {
      if (s.n_epochs < 4) {
        print(`    ${DIM}(too short for trends)${RESET}`);
        continue;
      }
      printProgress(`  loading trends for ${s.day}…`);
      const tm = await send({ command: "session_metrics", start_utc: s.start_utc, end_utc: s.end_utc });
      clearProgress();
      if (!tm.ok || !tm.metrics || !tm.trends) continue;

      const m = tm.metrics;
      const t = tm.trends;

      // ── Arrow helper (direction + color) ──────────────────────────────
      const arrow = (dir: string) =>
        dir === "up"   ? `${GREEN}↑${RESET}` :
        dir === "down" ? `${RED}↓${RESET}`   : `${DIM}→${RESET}`;

      // ── Metric value formatter ─────────────────────────────────────────
      const v = (val: number | undefined, decimals = 2) =>
        val != null && val !== 0 ? `${CYAN}${val.toFixed(decimals)}${RESET}` : null;

      // ── Build compact metric tokens: "label val↑" ──────────────────────
      const token = (label: string, val: number | undefined, dir: string, dec = 2) => {
        const fv = v(val, dec);
        return fv ? `${DIM}${label}${RESET} ${fv}${arrow(dir)}` : null;
      };

      const METRICS_DISPLAY: [string, string, string, number][] = [
        // [display-label, metrics-key, trend-key-in-t, decimal-places]
        ["focus",    "focus",       "focus",      2],
        ["relax",    "relaxation",  "relaxation", 2],
        ["hr",       "hr",          "hr",         1],
        ["medit",    "mood",        "meditation", 2],
        ["eng",      "engagement",  "engagement", 2],
        ["snr",      "snr",         "snr",        1],
      ];

      const tokens = METRICS_DISPLAY
        .map(([label, mKey, tKey, dec]) => token(label, m[mKey], t[tKey], dec))
        .filter(Boolean);

      if (tokens.length > 0) print(`    ${tokens.join("  ")}`);

      // ── Second line: band powers ───────────────────────────────────────
      const bands: [string, string][] = [
        ["δ", "rel_delta"], ["θ", "rel_theta"], ["α", "rel_alpha"],
        ["β", "rel_beta"],  ["γ", "rel_gamma"],
      ];
      const bandTokens = bands
        .map(([label, key]) => {
          const val = m[key];
          return val != null && val !== 0
            ? `${DIM}${label}${RESET} ${CYAN}${(val * 100).toFixed(0)}%${RESET}`
            : null;
        })
        .filter(Boolean);
      if (bandTokens.length > 0) print(`    ${DIM}bands:${RESET} ${bandTokens.join("  ")}`);
    }
  }

  print("");
  printResult(r);
}

/**
 * `notify` — Send a native OS notification through the Skill app.
 *
 * Sends `{ command: "notify", title, body? }`.  The server fires a
 * platform notification via `tauri-plugin-notification`.  Useful for
 * triggering alerts from scripts or automation pipelines.
 *
 * Response: `{ ok: true }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Title only:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"notify","title":"Session done"}'
 *
 * # Title + body:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"notify","title":"Focus done","body":"Take a 5-minute break"}'
 * ```
 *
 * @param title - Notification title (required).
 * @param body  - Notification body text (optional).
 */
/**
 * `say` — Speak text aloud via the on-device KittenTTS engine (fire-and-forget).
 *
 * Sends `{ command: "say", text: "..." }`.  The server enqueues the utterance
 * on the dedicated TTS thread and returns immediately — the response arrives
 * before audio playback begins, so this command never blocks on audio duration.
 *
 * The TTS engine (kittentts-rs, ONNX + espeak-ng) must be initialised; on
 * first use it downloads a ~30 MB model from HuggingFace Hub.  Subsequent
 * calls use the cached model.
 *
 * Response: `{ command: "say", ok: true, spoken: "<text>" }`.
 *
 * **HTTP equivalent:**
 * ```sh
 * curl -s -X POST http://127.0.0.1:8375/say \
 *   -H "Content-Type: application/json" \
 *   -d '{"text":"Eyes open. Starting calibration."}'
 *
 * # Or via the universal tunnel:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"say","text":"Calibration complete."}'
 * ```
 *
 * **WebSocket equivalent:**
 * ```json
 * { "command": "say", "text": "Break time. Next: Eyes Closed." }
 * ```
 *
 * @param text - The English phrase to speak.
 */
async function cmdSay(text: string, voice?: string): Promise<void> {
  const voiceLabel = voice ? `  ${DIM}voice: ${CYAN}${voice}${RESET}` : "";
  print(`${BOLD}⚡ say${RESET} ${GREEN}"${text}"${RESET}${voiceLabel}`);

  const payload: { command: string; text: string; voice?: string } = { command: "say", text };
  if (voice) payload.voice = voice;

  const r = await send(payload);
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);
  if (!jsonMode) {
    const voiceUsed = r.voice ? `  ${DIM}(voice: ${CYAN}${r.voice}${RESET}${DIM})${RESET}` : "";
    print(`  ${DIM}spoken:${RESET} ${GREEN}"${r.spoken ?? text}"${RESET}${voiceUsed}`);
  }
  printResult(r);
}

async function cmdNotify(title: string, body?: string): Promise<void> {
  print(`${BOLD}⚡ notify${RESET} ${GREEN}"${title}"${RESET}${body ? ` ${DIM}"${body}"${RESET}` : ""}`);
  const cmd: { command: string; title: string; body?: string } = { command: "notify", title };
  if (body) cmd.body = body;
  const r = await send(cmd);
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);
  printResult(r);
}

/**
 * `calibrations` — Full CRUD for calibration profiles.
 *
 * Subcommands:
 * - `calibrations` / `calibrations list` — list all profiles.
 * - `calibrations get <id>` — inspect a single profile.
 * - `calibrations create "name" --actions "L1:20,L2:20"` — create a new profile.
 * - `calibrations update <id-or-name> [--name/--actions/--loops/--break/--auto-start]` — update.
 * - `calibrations delete <id-or-name>` — remove a profile.
 *
 * @param args - Parsed CLI arguments.
 */
/**
 * Parse a compact actions string into an array of `{ label, duration_secs }`.
 *
 * Format: `"Eyes Open:20,Eyes Closed:20"` → `[{ label: "Eyes Open", duration_secs: 20 }, ...]`
 *
 * If no colon is present in an entry, defaults to 20 seconds.
 */
function parseCalActions(raw: string): Array<{ label: string; duration_secs: number }> {
  return raw.split(",")
    .map(s => s.trim())
    .filter(Boolean)
    .map(s => {
      const colonIdx = s.lastIndexOf(":");
      if (colonIdx > 0) {
        const label = s.slice(0, colonIdx).trim();
        const dur   = Number(s.slice(colonIdx + 1).trim());
        return { label, duration_secs: isNaN(dur) || dur <= 0 ? 20 : dur };
      }
      return { label: s, duration_secs: 20 };
    });
}

/**
 * Resolve a profile identifier (UUID or name substring) to a UUID.
 *
 * Fetches the profile list from the server and matches:
 * 1. Exact UUID match.
 * 2. Case-insensitive name substring match.
 *
 * Exits with an error if no match is found.
 */
async function resolveProfileId(idOrName: string): Promise<string> {
  const lr = await send({ command: "list_calibrations" });
  if (!lr.ok) printError(`could not list profiles: ${lr.error}`);
  const profiles = (lr.profiles ?? []) as Array<{ id: string; name: string }>;
  if (profiles.length === 0) printError("no calibration profiles found");

  const exact  = profiles.find(p => p.id === idOrName);
  const byName = profiles.find(p => p.name.toLowerCase().includes(idOrName.toLowerCase()));
  const match  = exact ?? byName;

  if (!match) {
    const names = profiles.map(p => `"${p.name}" (${p.id})`).join("\n    ");
    printError(`profile "${idOrName}" not found.\n  Available profiles:\n    ${names}`);
  }
  return match!.id;
}

async function cmdCalibrations(args: Args): Promise<void> {
  const sub = args.subAction ?? "list";

  if (sub === "get") {
    if (args.id == null) printError("usage: cli.ts calibrations get <id>");
    print(`${BOLD}⚡ calibrations get${RESET} ${CYAN}${args.id}${RESET}`);
    const r = await send({ command: "get_calibration", id: args.id });
    if (!r.ok) printError(`server returned ok=false: ${r.error}`);
    if (!jsonMode && r.profile) {
      const p = r.profile;
      print("");
      print(`  ${BOLD}${p.name ?? "(unnamed)"}${RESET}  ${DIM}id ${p.id}${RESET}`);
      if (p.loop_count        != null) print(`  ${DIM}loop count:${RESET}     ${CYAN}${p.loop_count}${RESET}`);
      if (p.break_duration_secs != null) print(`  ${DIM}break duration:${RESET} ${CYAN}${p.break_duration_secs}s${RESET}`);
      if (p.auto_start        != null) print(`  ${DIM}auto start:${RESET}     ${CYAN}${p.auto_start}${RESET}`);
      if (Array.isArray(p.actions) && p.actions.length > 0) {
        print(`  ${DIM}actions:${RESET}`);
        (p.actions as any[]).forEach((a: any, i: number) => {
          const name = typeof a === "string" ? a : (a.name ?? a.label ?? JSON.stringify(a));
          print(`    ${DIM}${i + 1}.${RESET} ${CYAN}${name}${RESET}`);
        });
      }
      print("");
    }
    printResult(r);
    return;
  }

  // ── create ─────────────────────────────────────────────────────────────
  if (sub === "create") {
    if (!args.calName) printError('usage: calibrations create "Profile Name" --actions "Eyes Open:20,Eyes Closed:20" [--loops 3] [--break 5] [--auto-start]');
    if (!args.calActions) printError("--actions is required for create. Format: \"Label1:secs,Label2:secs\" (e.g. \"Eyes Open:20,Eyes Closed:20\")");

    const actions = parseCalActions(args.calActions!);
    if (actions.length === 0) printError("--actions must contain at least one label:duration pair");

    print(`${BOLD}⚡ calibrations create${RESET} ${GREEN}"${args.calName}"${RESET}`);

    const cmd: Record<string, unknown> = {
      command: "create_calibration",
      name:    args.calName,
      actions,
    };
    if (args.calLoops != null)     cmd.loop_count          = args.calLoops;
    if (args.calBreak != null)     cmd.break_duration_secs = args.calBreak;
    if (args.calAutoStart != null) cmd.auto_start          = args.calAutoStart;

    const r = await send(cmd);
    if (!r.ok) printError(`create failed: ${r.error}`);

    if (!jsonMode && r.profile) {
      const p = r.profile;
      print(`  ${GREEN}✓${RESET} created ${CYAN}${p.name}${RESET}  ${DIM}id: ${p.id}${RESET}`);
      print(`  ${DIM}actions:${RESET} ${actions.map((a: any) => `${CYAN}${a.label}${RESET} ${DIM}(${a.duration_secs}s)${RESET}`).join(`${DIM} → ${RESET}`)}`);
      print(`  ${DIM}loops:${RESET} ${CYAN}${p.loop_count}${RESET}  ${DIM}break:${RESET} ${CYAN}${p.break_duration_secs}s${RESET}  ${DIM}auto-start:${RESET} ${CYAN}${p.auto_start}${RESET}`);
    }
    printResult(r);
    return;
  }

  // ── update ─────────────────────────────────────────────────────────────
  if (sub === "update") {
    if (!args.profileId) printError("usage: calibrations update <id-or-name> [--name ...] [--actions ...] [--loops N] [--break N] [--auto-start]");

    const profileUuid = await resolveProfileId(args.profileId);

    print(`${BOLD}⚡ calibrations update${RESET} ${CYAN}${args.profileId}${RESET}`);

    const cmd: Record<string, unknown> = {
      command: "update_calibration",
      id:      profileUuid,
    };
    if (args.calName    != null) cmd.name                = args.calName;
    if (args.calActions != null) cmd.actions             = parseCalActions(args.calActions);
    if (args.calLoops   != null) cmd.loop_count          = args.calLoops;
    if (args.calBreak   != null) cmd.break_duration_secs = args.calBreak;
    if (args.calAutoStart != null) cmd.auto_start        = args.calAutoStart;

    const r = await send(cmd);
    if (!r.ok) printError(`update failed: ${r.error}`);

    if (!jsonMode && r.profile) {
      const p = r.profile;
      print(`  ${GREEN}✓${RESET} updated ${CYAN}${p.name}${RESET}  ${DIM}id: ${p.id}${RESET}`);
    }
    printResult(r);
    return;
  }

  // ── delete ─────────────────────────────────────────────────────────────
  if (sub === "delete") {
    if (!args.profileId) printError("usage: calibrations delete <id-or-name>");

    const profileUuid = await resolveProfileId(args.profileId);

    print(`${BOLD}⚡ calibrations delete${RESET} ${CYAN}${args.profileId}${RESET}`);

    const r = await send({ command: "delete_calibration", id: profileUuid });
    if (!r.ok) printError(`delete failed: ${r.error}`);

    print(`  ${GREEN}✓${RESET} deleted`);
    printResult(r);
    return;
  }

  // Default: list
  print(`${BOLD}⚡ calibrations${RESET}`);
  const r = await send({ command: "list_calibrations" });
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);
  if (!jsonMode && r.profiles) {
    const profiles = r.profiles as Array<{ id: unknown; name: string; loop_count?: number; actions?: unknown[] }>;
    if (profiles.length === 0) {
      print(`  ${DIM}no calibration profiles found${RESET}`);
    } else {
      print(`\n  ${DIM}${"id".padEnd(6)} ${"name".padEnd(30)} ${"actions".padEnd(6)} loop${RESET}`);
      print(`  ${DIM}${"─".repeat(58)}${RESET}`);
      for (const p of profiles) {
        const nActions = Array.isArray(p.actions) ? p.actions.length : "?";
        const loops    = p.loop_count != null ? String(p.loop_count) : "–";
        print(`  ${CYAN}${String(p.id).padEnd(6)}${RESET} ${p.name.padEnd(30)} ${String(nActions).padEnd(6)} ${loops}`);
      }
      print("");
    }
  }
  printResult(r);
}

/**
 * `calibrate` — Open the calibration window and start a profile immediately.
 *
 * Resolves the target profile in this order:
 * 1. `--profile <name-or-id>` — match against profile `name` (case-insensitive
 *    substring) or exact `id` UUID from `list_calibrations`.
 * 2. No flag — uses the server's currently active profile.
 *
 * Sends `{ command: "run_calibration", id? }`.  Requires a Muse headband to
 * be connected and streaming; returns ok=false otherwise.
 *
 * Response: `{ ok: true }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # List available calibration profiles first:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"list_calibrations"}'
 *
 * # Run calibration with the server's active profile:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"run_calibration"}'
 *
 * # Run a specific profile by UUID (from list_calibrations response):
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"run_calibration","id":"a1b2c3d4-e5f6-7890-abcd-ef1234567890"}'
 * ```
 *
 * @param profileArg - Optional name or UUID from `--profile` flag.
 */
async function cmdCalibrate(profileArg?: string): Promise<void> {
  print(`${BOLD}⚡ calibrate${RESET}`);

  let profileId: string | undefined;

  if (profileArg) {
    // Resolve name → id
    const lr = await send({ command: "list_calibrations" });
    if (!lr.ok) printError(`could not list profiles: ${lr.error}`);

    const profiles = (lr.profiles ?? []) as Array<{ id: string; name: string; actions: unknown[] }>;
    if (profiles.length === 0) printError("no calibration profiles found");

    // Prefer exact id match, then case-insensitive name substring
    const exact  = profiles.find(p => p.id === profileArg);
    const byName = profiles.find(p => p.name.toLowerCase().includes(profileArg.toLowerCase()));
    const match  = exact ?? byName;

    if (!match) {
      const names = profiles.map(p => `"${p.name}" (${p.id})`).join("\n    ");
      printError(`profile "${profileArg}" not found.\n  Available profiles:\n    ${names}`);
    }

    profileId = match.id;
    print(`  ${DIM}profile:${RESET} ${CYAN}${match.name}${RESET}  ${DIM}(${match.id})${RESET}`);
  } else {
    print(`  ${DIM}profile:${RESET} ${DIM}active (server default)${RESET}`);
  }

  const cmd: { command: string; id?: string } = { command: "run_calibration" };
  if (profileId) cmd.id = profileId;

  const r = await send(cmd, 15000);
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);
  printResult(r);
}

/**
 * `timer` — Open the focus-timer window and auto-start the work phase.
 *
 * Sends `{ command: "timer" }`.  If the window is already open it is brought
 * to the front and started via a Tauri event.  The timer uses the preset
 * last saved in the focus-timer window (Pomodoro / Deep Work / Short Focus).
 *
 * Response: `{ ok: true }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Open focus-timer window and auto-start:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"timer"}'
 * ```
 */
async function cmdTimer(): Promise<void> {
  print(`${BOLD}⚡ timer${RESET}`);
  const r = await send({ command: "timer" });
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);
  printResult(r);
}

/**
 * `label` — Create a timestamped text annotation on the current EEG moment.
 *
 * Sends `{ command: "label", text, context?, label_start_utc? }`.  The server
 * stores the label in the label database and broadcasts a `label-created`
 * event to all connected clients (including the desktop app's dashboard).
 *
 * Optional flags:
 * - `--context "..."` — long-form body stored alongside the short text; used
 *   by `search-labels --mode context` and `--mode both`.
 * - `--at <utc>`       — backdate the label to a specific unix second instead
 *   of using the current time (useful for retrospective annotation).
 *
 * Response: `{ ok: true, label_id: <number> }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Create a label at the current moment:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"label","text":"meditation start"}'
 *
 * # With long-form context:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"label","text":"eyes closed","context":"4-7-8 breathing exercise"}'
 *
 * # Backdated to a specific timestamp:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"label","text":"retrospective note","label_start_utc":1740412800}'
 *
 * # Extract the assigned label_id:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"label","text":"eyes closed relaxation"}' | jq '.label_id'
 * ```
 *
 * @param args - Parsed CLI arguments (`text`, optional `context`, optional `at`).
 */
async function cmdLabel(args: Args): Promise<void> {
  const text = args.text!;
  print(`${BOLD}⚡ label${RESET} ${GREEN}"${text}"${RESET}`
    + (args.context ? `  ${DIM}(+context)${RESET}` : "")
    + (args.at      ? `  ${DIM}at ${new Date(args.at * 1000).toLocaleString()}${RESET}` : ""));

  const payload: { command: string; text: string; context?: string; label_start_utc?: number } =
    { command: "label", text };
  if (args.context) payload.context         = args.context;
  if (args.at)      payload.label_start_utc = args.at;

  const r = await send(payload);
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);
  printResult(r);
}

/**
 * `search-labels` — Find labels semantically similar to a free-text query.
 *
 * Sends `{ command: "search_labels", query, k, ef, mode }` to the server.
 * The server embeds `query` using the configured fastembed model and searches
 * the in-memory HNSW label index.  Three modes are available:
 *
 * - `"text"` (default) — searches the label short-text HNSW
 * - `"context"` — searches the long-context HNSW (empty if no context fields set)
 * - `"both"` — runs both searches and deduplicates by best cosine distance
 *
 * Results include cosine distance, similarity (1 − distance), the full label
 * text+context, EEG window timestamps, and per-window EEG band metrics.
 *
 * Response: `{ query, mode, model, k, count, results[]: { label_id, text,
 *   context, distance, similarity, eeg_start, eeg_end, created_at,
 *   embedding_model, eeg_metrics } }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Default text mode, k=10:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"search_labels","query":"deep focus","k":10,"mode":"text"}'
 *
 * # Context mode with custom ef, pipe result texts to jq:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"search_labels","query":"flow state","k":5,"mode":"context","ef":80}' \
 *   | jq '[.results[].text]'
 * ```
 *
 * @param args - Parsed CLI arguments (`text` = query string, `k`, `ef`, `mode`).
 */
async function cmdSearchLabels(args: Args): Promise<void> {
  if (!args.text) printError('usage: cli.ts search-labels "your query text"');
  const query = args.text!;
  const k     = args.k    ?? 10;
  const mode  = args.mode ?? "text";

  const validModes = ["text", "context", "both"];
  if (!validModes.includes(mode)) {
    printError(`invalid --mode "${mode}": must be one of ${validModes.join(", ")}`);
  }

  print(`${BOLD}⚡ search-labels${RESET} ${GREEN}"${query}"${RESET}  ${DIM}(mode: ${mode}, k: ${k})${RESET}`);

  const cmd: { command: string; query: string; k: number; mode: string; ef?: number } = {
    command: "search_labels",
    query,
    k,
    mode,
  };
  if (args.ef != null) cmd.ef = args.ef;

  const r = await send(cmd, 30000);
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);

  if (!jsonMode) {
    const results = (r.results ?? []) as Array<{
      label_id: number; text: string; context: string;
      distance: number; similarity: number;
      eeg_start: number; eeg_end: number; created_at: number;
      embedding_model: string; eeg_metrics: Record<string, number> | null;
    }>;

    print(`\n  ${DIM}model:${RESET}  ${CYAN}${r.model ?? "unknown"}${RESET}`);
    print(`  ${DIM}k:${RESET}      ${CYAN}${r.k}${RESET}   ${DIM}results:${RESET} ${GREEN}${r.count ?? results.length}${RESET}\n`);

    if (results.length === 0) {
      print(`  ${DIM}no results — labels may not be embedded yet (check Settings → Embeddings)${RESET}`);
    } else {
      for (const hit of results) {
        const simPct    = ((hit.similarity ?? 0) * 100).toFixed(0);
        const distStr   = typeof hit.distance === "number" ? hit.distance.toFixed(4) : "?";
        const created   = fmtTs(hit.created_at);
        const duration  = hit.eeg_end - hit.eeg_start;

        print(`  ${BOLD}#${hit.label_id}${RESET}  ${GREEN}"${hit.text}"${RESET}`);
        print(`     ${DIM}similarity:${RESET} ${CYAN}${simPct}%${RESET}  ${DIM}distance:${RESET} ${CYAN}${distStr}${RESET}  ${DIM}model:${RESET} ${hit.embedding_model ?? "?"}`);
        print(`     ${DIM}recorded:${RESET}  ${created}  ${DIM}(${duration}s window)${RESET}`);

        if (hit.context) {
          const preview = hit.context.length > 80
            ? hit.context.slice(0, 80) + "…"
            : hit.context;
          print(`     ${DIM}context:${RESET}   ${preview}`);
        }

        if (hit.eeg_metrics && Object.keys(hit.eeg_metrics).length > 0) {
          const m = hit.eeg_metrics;
          const metricStr = (["focus","relaxation","engagement","hr","mood"] as const)
            .filter(k => typeof m[k] === "number")
            .map(k => `${k}=${CYAN}${(m[k] as number).toFixed(2)}${RESET}`)
            .join("  ");
          if (metricStr) print(`     ${DIM}eeg:${RESET}       ${metricStr}`);
        }
        print("");
      }
    }
  }

  printResult(r);
}

/**
 * `interactive <keyword>` — Cross-modal 4-layer graph search.
 *
 * Invokes the same pipeline as the interactive search in the desktop UI:
 * 1. Embed the keyword as a text vector via fastembed.
 * 2. Search the label text-HNSW for semantically similar labels   (layer 1 — `text_label`).
 * 3. For each label, compute the mean EEG embedding of its time window.
 * 4. Search all daily EEG HNSW indices → nearest EEG moments       (layer 2 — `eeg_point`).
 * 5. For each EEG moment, find labels within `reach_minutes`        (layer 3 — `found_label`).
 *
 * Output formats (mutually exclusive, checked in priority order):
 * - `--dot`  → Graphviz DOT source only (pipe to `dot -Tsvg` or `dot -Tpng`).
 * - `--json` → raw JSON: `{ query, k_text, k_eeg, k_labels, reach_minutes, nodes, edges, dot }`.
 * - `--full` → human-readable summary **plus** colorized JSON.
 * - (default) → human-readable summary only.
 *
 * **HTTP / WS equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"interactive_search","query":"deep focus","k_text":5,"k_eeg":5,"k_labels":3,"reach_minutes":10}'
 * ```
 *
 * @param args - Parsed CLI arguments. `keyword` is the query string (required).
 */
async function cmdInteractive(args: Args): Promise<void> {
  if (!args.keyword) printError('usage: cli.ts interactive "your keyword"');

  const query   = args.keyword!;
  const kText   = args.kText   ?? 5;
  const kEeg    = args.kEeg    ?? 5;
  const kLabels = args.kLabels ?? 3;
  const reach   = args.reach   ?? 10;

  // ── Send to the server ────────────────────────────────────────────────────
  if (!args.dot) {
    print(`${BOLD}⚡ interactive${RESET} ${GREEN}"${query}"${RESET}  ${DIM}(k-text:${kText}, k-eeg:${kEeg}, k-labels:${kLabels}, reach:${reach}m)${RESET}`);
  }

  const r = await send({
    command:       "interactive_search",
    query,
    k_text:        kText,
    k_eeg:         kEeg,
    k_labels:      kLabels,
    reach_minutes: reach,
  }, 60_000);

  if (!r.ok && r.error) printError(`server returned error: ${r.error}`);

  // ── Format: --dot ─────────────────────────────────────────────────────────
  if (args.dot) {
    if (!r.dot) {
      printError("server did not return a DOT graph (requires Skill ≥ latest with interactive_search WS support)");
    }
    process.stdout.write(r.dot + "\n");
    return;
  }

  // ── Format: --json → handled by printResult at end ────────────────────────

  // ── Format: default summary (also shown before --full JSON) ───────────────
  if (!jsonMode) {
    const nodes: Array<{
      id: string; kind: string; text?: string;
      timestamp_unix?: number; distance: number;
      eeg_metrics?: Record<string, number | null> | null;
      parent_id?: string;
    }> = r.nodes ?? [];

    const edges: Array<{
      from_id: string; to_id: string; distance: number; kind: string;
    }> = r.edges ?? [];

    // ── Node / edge counts by kind ─────────────────────────────────────────
    const textLabels  = nodes.filter(n => n.kind === "text_label");
    const eegPoints   = nodes.filter(n => n.kind === "eeg_point");
    const foundLabels = nodes.filter(n => n.kind === "found_label");

    const edgesByKind: Record<string, number> = {};
    for (const e of edges) edgesByKind[e.kind] = (edgesByKind[e.kind] ?? 0) + 1;

    print("");
    print(`  ${BOLD}Graph${RESET}  ${CYAN}${nodes.length}${RESET} node${nodes.length !== 1 ? "s" : ""} · ${CYAN}${edges.length}${RESET} edge${edges.length !== 1 ? "s" : ""}`);
    if (Object.keys(edgesByKind).length > 0) {
      const edgeSummary = Object.entries(edgesByKind)
        .map(([k, c]) => `${DIM}${k}${RESET} ×${CYAN}${c}${RESET}`)
        .join("  ");
      print(`  ${DIM}edges:${RESET}  ${edgeSummary}`);
    }
    print(`  ${DIM}${"─".repeat(54)}${RESET}`);

    // ── Query node ────────────────────────────────────────────────────────
    print(`\n  ${MAGENTA}●${RESET} ${BOLD}query${RESET}  ${GREEN}"${query}"${RESET}`);

    // ── Text label nodes ──────────────────────────────────────────────────
    if (textLabels.length > 0) {
      print(`\n  ${BLUE}◆${RESET} ${BOLD}Text Labels${RESET}  ${DIM}(${textLabels.length} semantically similar ${textLabels.length === 1 ? "label" : "labels"})${RESET}`);
      for (let i = 0; i < textLabels.length; i++) {
        const n = textLabels[i];
        const sim    = ((1 - n.distance) * 100).toFixed(0);
        const simCol = n.distance < 0.1 ? GREEN : n.distance < 0.25 ? CYAN : YELLOW;
        const when   = n.timestamp_unix ? `  ${DIM}${fmtTs(n.timestamp_unix)}${RESET}` : "";
        print(`  ${DIM}#${i}${RESET}  ${GREEN}"${n.text ?? "?"}"${RESET}${when}`);
        print(`      ${DIM}similarity:${RESET} ${simCol}${sim}%${RESET}  ${DIM}dist:${RESET} ${simCol}${n.distance.toFixed(4)}${RESET}`);
        // Optional EEG metrics inline
        if (n.eeg_metrics) {
          const m = n.eeg_metrics;
          const parts = (["focus","relaxation","engagement","hr","meditation"] as const)
            .filter(k => typeof m[k] === "number")
            .map(k => `${DIM}${k}${RESET} ${CYAN}${(m[k] as number).toFixed(2)}${RESET}`);
          if (parts.length) print(`      ${parts.join("  ")}`);
        }
      }
    } else {
      print(`\n  ${DIM}No text labels found — annotate some moments with \`cli.ts label "..."\` first.${RESET}`);
    }

    // ── EEG point nodes ───────────────────────────────────────────────────
    if (eegPoints.length > 0) {
      print(`\n  ${YELLOW}◈${RESET} ${BOLD}EEG Moments${RESET}  ${DIM}(${eegPoints.length} neural ${eegPoints.length === 1 ? "moment" : "moments"} found)${RESET}`);
      for (let i = 0; i < eegPoints.length; i++) {
        const n = eegPoints[i];
        const when    = n.timestamp_unix ? fmtTs(n.timestamp_unix) : "?";
        const distCol = n.distance < 0.1 ? GREEN : n.distance < 0.2 ? CYAN : YELLOW;
        print(`  ${DIM}#${i}${RESET}  ${CYAN}${when}${RESET}   ${DIM}dist:${RESET} ${distCol}${n.distance.toFixed(4)}${RESET}  ${DIM}← ${n.parent_id ?? "?"}${RESET}`);
      }
    } else {
      print(`\n  ${DIM}No EEG moments found — ensure EEG data is recorded and embedded.${RESET}`);
    }

    // ── Found label nodes ─────────────────────────────────────────────────
    if (foundLabels.length > 0) {
      print(`\n  ${GREEN}◉${RESET} ${BOLD}Nearby Labels${RESET}  ${DIM}(${foundLabels.length} ${foundLabels.length === 1 ? "label" : "labels"} found near EEG moments)${RESET}`);
      for (let i = 0; i < foundLabels.length; i++) {
        const n     = foundLabels[i];
        const when  = n.timestamp_unix ? fmtTs(n.timestamp_unix) : "?";
        const mins  = (n.distance * reach).toFixed(1);
        print(`  ${DIM}#${i}${RESET}  ${GREEN}"${n.text ?? "?"}"${RESET}   ${DIM}${when}  ${mins}m from EEG point${RESET}`);
      }
    }

    // ── DOT hint ──────────────────────────────────────────────────────────
    if (r.dot) {
      print(`\n  ${DIM}tip: rerun with --dot | dot -Tsvg > graph.svg  to visualize${RESET}`);
    }

    print("");
  }

  printResult(r);
}

/**
 * `search` — Find neurally similar moments via ANN (approximate nearest neighbor).
 *
 * Sends `{ command: "search", start_utc, end_utc, k }`.  The server loads
 * all embedding epochs in the query range, then searches the HNSW index
 * across all recording days for the `k` nearest neighbors of each query
 * embedding (cosine distance).
 *
 * Auto-range: if no `--start`/`--end` given, uses the most recent session.
 * Default `k`: 5.
 *
 * Response: `{ result: { query_count, searched_days[], results[]: {
 *   timestamp_unix, neighbors[]: { distance, timestamp_unix, date,
 *   device_name, labels[], metrics? } } } }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Search with explicit range and k=5:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":5}'
 *
 * # Increase k and count returned results via jq:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":20}' \
 *   | jq '.result.results | length'
 * ```
 *
 * @param args - Parsed CLI arguments (may contain `start`, `end`, `k`).
 */
async function cmdSearch(args: Args): Promise<void> {
  const hasExplicitRange = args.start != null || args.end != null;
  let start: number, end: number;
  if (hasExplicitRange) {
    const now = Math.floor(Date.now() / 1000);
    start = args.start ?? now - 600;
    end = args.end ?? now;
  } else {
    [start, end] = await autoRange(600);
  }
  const k = args.k ?? 5;

  print(`${BOLD}⚡ search${RESET}`);
  printRange("range", start, end, !hasExplicitRange);
  print(`  ${DIM}k:${RESET} ${CYAN}${k}${RESET}`);
  if (!hasExplicitRange) printRerun(`search --start ${start} --end ${end} --k ${k}`);

  const r = await send({ command: "search", start_utc: start, end_utc: end, k });
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);

  if (!jsonMode && r.result) {
    const res = r.result;
    const a   = res.analysis ?? {};

    // Collect every neighbor entry across all query points
    const allNeighbors: any[] = (res.results ?? []).flatMap((q: any) => q.neighbors ?? []);

    // ── Overview ──────────────────────────────────────────────────────────
    print("");
    print(`  ${BOLD}Search Results${RESET}`);
    print(
      `  ${DIM}query epochs:${RESET} ${CYAN}${res.query_count}${RESET}` +
      `   ${DIM}searched days:${RESET} ${CYAN}${res.searched_days?.length ?? 0}${RESET}` +
      `   ${DIM}total matches:${RESET} ${CYAN}${allNeighbors.length}${RESET}` +
      (a.time_span_hours != null ? `   ${DIM}span:${RESET} ${CYAN}${a.time_span_hours.toFixed(1)}h${RESET}` : "")
    );

    // ── Match quality (distance stats + visual bar) ───────────────────────
    if (a.distance_stats) {
      const d = a.distance_stats;
      // similarity = 1 − distance; show as a quality bar
      const simPct = Math.round(Math.max(0, Math.min(100, (1 - d.mean) * 100)));
      print(`\n  ${BOLD}Match Quality${RESET}  ${DIM}(cosine distance — lower = more similar)${RESET}`);
      print(`  ${progressBar(simPct, 24)}  ${DIM}similarity${RESET} ${CYAN}${simPct}%${RESET}`);
      print(
        `  ${DIM}min${RESET} ${CYAN}${d.min}${RESET}` +
        `   ${DIM}mean${RESET} ${CYAN}${d.mean}${RESET}` +
        `   ${DIM}max${RESET} ${CYAN}${d.max}${RESET}` +
        `   ${DIM}σ${RESET} ${CYAN}${d.stddev}${RESET}`
      );
    }

    // ── Neighbor metrics: avg + min/max computed from raw neighbor records ─
    const NEIGHBOR_METRIC_DEFS: [string, string, number][] = [
      ["focus",          "focus",          2],
      ["relaxation",     "relaxation",     2],
      ["engagement",     "engagement",     2],
      ["meditation",     "meditation",     2],
      ["mood",           "mood",           2],
      ["cognitive load", "cognitive_load", 2],
      ["drowsiness",     "drowsiness",     2],
      ["hr (bpm)",       "hr",             1],
      ["snr (dB)",       "snr",            1],
      ["α alpha",        "rel_alpha",      2],
      ["β beta",         "rel_beta",       2],
      ["θ theta",        "rel_theta",      2],
      ["FAA",            "faa",            3],
      ["θ/α ratio",      "tar",            3],
    ];

    // Build min/max ranges from individual neighbor metrics
    const ranges: Record<string, { min: number; max: number }> = {};
    for (const n of allNeighbors) {
      if (!n.metrics) continue;
      for (const [, key] of NEIGHBOR_METRIC_DEFS) {
        const v: number | undefined = n.metrics[key];
        if (v == null) continue;
        if (!ranges[key]) ranges[key] = { min: v, max: v };
        else { ranges[key].min = Math.min(ranges[key].min, v); ranges[key].max = Math.max(ranges[key].max, v); }
      }
    }

    const presentMetrics = NEIGHBOR_METRIC_DEFS.filter(([, key]) => a.neighbor_metrics?.[key] != null);
    if (presentMetrics.length > 0) {
      print(`\n  ${BOLD}Neighbor Metrics${RESET}  ${DIM}(avg · min–max across ${allNeighbors.length} matches)${RESET}`);
      for (const [label, key, dec] of presentMetrics) {
        const avg = (a.neighbor_metrics[key] as number).toFixed(dec);
        const rng = ranges[key]
          ? `  ${DIM}${ranges[key].min.toFixed(dec)} – ${ranges[key].max.toFixed(dec)}${RESET}`
          : "";
        print(`  ${DIM}${label.padEnd(18)}${RESET} ${CYAN}${avg.padStart(7)}${RESET}${rng}`);
      }
    }

    // ── Top 5 closest individual matches ─────────────────────────────────
    const topMatches = [...allNeighbors]
      .sort((a, b) => (a.distance as number) - (b.distance as number))
      .slice(0, 5);

    if (topMatches.length > 0) {
      print(`\n  ${BOLD}Top Matches${RESET}  ${DIM}(closest by cosine distance)${RESET}`);
      for (let i = 0; i < topMatches.length; i++) {
        const n       = topMatches[i];
        const when    = fmtTs(n.timestamp_unix);
        const distCol = (n.distance as number) < 0.1 ? GREEN : (n.distance as number) < 0.2 ? CYAN : YELLOW;
        const labelStr = (n.labels as any[] ?? [])
          .map((l: any) => `"${l.text ?? l}"`)
          .join(", ");

        let line = `  ${DIM}#${i + 1}${RESET}  ${CYAN}${when}${RESET}  ${DIM}dist${RESET} ${distCol}${(n.distance as number).toFixed(4)}${RESET}`;

        if (n.metrics) {
          const mParts = [
            n.metrics.focus      != null ? `focus ${CYAN}${(n.metrics.focus      as number).toFixed(2)}${RESET}` : null,
            n.metrics.relaxation != null ? `relax ${CYAN}${(n.metrics.relaxation as number).toFixed(2)}${RESET}` : null,
            n.metrics.hr         != null ? `hr ${CYAN}${(n.metrics.hr           as number).toFixed(1)}${RESET}` : null,
            n.metrics.meditation != null ? `medit ${CYAN}${(n.metrics.meditation as number).toFixed(2)}${RESET}` : null,
          ].filter(Boolean);
          if (mParts.length) line += `   ${mParts.join("  ")}`;
        }
        if (labelStr) line += `   ${DIM}${labelStr}${RESET}`;
        print(line);
      }
    }

    // ── Temporal distribution — mini bar chart by hour of day ─────────────
    const hourDist = a.temporal_distribution as Record<string, number> | undefined;
    if (hourDist && Object.keys(hourDist).length > 0) {
      const hourEntries = Object.entries(hourDist).sort(([a], [b]) => Number(a) - Number(b));
      const maxCount    = Math.max(...hourEntries.map(([, c]) => c as number), 1);
      print(`\n  ${BOLD}Temporal Distribution${RESET}  ${DIM}(matches by hour of day, UTC)${RESET}`);
      // Print in two columns of 12 hours each
      for (let h = 0; h < 12; h++) {
        const left  = hourEntries.find(([k]) => Number(k) === h);
        const right = hourEntries.find(([k]) => Number(k) === h + 12);
        const fmtBar = (entry: [string, number] | undefined) => {
          if (!entry) return " ".repeat(32);
          const [hour, cnt] = entry;
          const len = Math.round((cnt as number / maxCount) * 12);
          const bar = `${BLUE}${"█".repeat(len)}${DIM}${"░".repeat(12 - len)}${RESET}`;
          return `${DIM}${String(hour).padStart(2)}:00${RESET} ${bar} ${CYAN}${String(cnt).padStart(4)}${RESET}`;
        };
        print(`  ${fmtBar(left)}    ${fmtBar(right)}`);
      }
    }

    // ── Top matched days ──────────────────────────────────────────────────
    if (a.top_days?.length > 0) {
      const days = (a.top_days as [string, number][])
        .map(([d, c]) => `${CYAN}${d}${RESET} ${DIM}(${c})${RESET}`)
        .join("   ");
      print(`\n  ${DIM}top days:${RESET}  ${days}`);
    }

    print("");
  }

  printResult(r);
}

/**
 * `compare` — Side-by-side A/B session metrics comparison.
 *
 * Sends `{ command: "compare", a_start_utc, a_end_utc, b_start_utc, b_end_utc }`.
 * The server computes aggregated band-power metrics, derived scores (focus,
 * relaxation, HR, etc.), and sleep staging for both ranges.  It also
 * enqueues a UMAP 3D projection job (poll with `umap_poll` for results).
 *
 * Auto-range: if no `--a-start` etc. given, uses the last two sessions
 * (or splits a single session in half).
 *
 * Response: `{ a: SessionMetrics, b: SessionMetrics, sleep_a, sleep_b,
 *   umap: { queued, job_id, estimated_secs, n_a, n_b } }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Compare two sessions side-by-side:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"compare","a_start_utc":1740380100,"a_end_utc":1740382665,"b_start_utc":1740412800,"b_end_utc":1740415510}'
 *
 * # Extract just the focus scores for A and B:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"compare","a_start_utc":1740380100,"a_end_utc":1740382665,"b_start_utc":1740412800,"b_end_utc":1740415510}' \
 *   | jq '{a_focus: .a.focus, b_focus: .b.focus}'
 * ```
 *
 * @param args - Parsed CLI arguments (may contain `aStart`, `aEnd`, `bStart`, `bEnd`).
 */
async function cmdCompare(args: Args): Promise<void> {
  const hasExplicitRange = args.aStart != null || args.aEnd != null || args.bStart != null || args.bEnd != null;
  let aStart: number, aEnd: number, bStart: number, bEnd: number;
  if (hasExplicitRange) {
    const now = Math.floor(Date.now() / 1000);
    aStart = args.aStart ?? now - 7200;
    aEnd   = args.aEnd   ?? now - 3600;
    bStart = args.bStart ?? now - 3600;
    bEnd   = args.bEnd   ?? now;
  } else {
    ({ aStart, aEnd, bStart, bEnd } = await autoRangeAB());
  }

  print(`${BOLD}⚡ compare${RESET}`);
  printRange("A", aStart, aEnd, !hasExplicitRange);
  printRange("B", bStart, bEnd, !hasExplicitRange);
  if (!hasExplicitRange) printRerun(`compare --a-start ${aStart} --a-end ${aEnd} --b-start ${bStart} --b-end ${bEnd}`);

  const r = await send({
    command: "compare",
    a_start_utc: aStart, a_end_utc: aEnd,
    b_start_utc: bStart, b_end_utc: bEnd,
  });
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);

  if (!jsonMode && r.insights) {
    const ins = r.insights;
    print("");
    print(`  ${BOLD}Compare Insights${RESET}  ${DIM}(${ins.n_epochs_a} vs ${ins.n_epochs_b} epochs)${RESET}`);

    // Deltas table
    if (ins.deltas) {
      print(`\n  ${"metric".padEnd(18)} ${"A".padStart(8)} ${"B".padStart(8)} ${"Δ".padStart(8)} ${"Δ%".padStart(7)}  dir`);
      print(`  ${DIM}${"─".repeat(60)}${RESET}`);
      const keyMetrics = ["focus","relaxation","engagement","meditation","hr","drowsiness","mood","snr","stillness","cognitive_load"];
      for (const m of keyMetrics) {
        const d = ins.deltas[m];
        if (!d) continue;
        const arrow = d.direction === "up" ? `${GREEN}↑${RESET}` : d.direction === "down" ? `${RED}↓${RESET}` : `${DIM}→${RESET}`;
        const pctStr = d.pct > 0 ? `+${d.pct.toFixed(1)}%` : `${d.pct.toFixed(1)}%`;
        print(`  ${m.padEnd(18)} ${CYAN}${d.a?.toFixed(2).padStart(8)}${RESET} ${CYAN}${d.b?.toFixed(2).padStart(8)}${RESET} ${d.abs >= 0 ? "+" : ""}${d.abs?.toFixed(2).padStart(7)} ${pctStr.padStart(7)}  ${arrow}`);
      }
    }

    if (ins.improved?.length > 0) {
      print(`\n  ${GREEN}▲ improved:${RESET} ${ins.improved.join(", ")}`);
    }
    if (ins.declined?.length > 0) {
      print(`  ${RED}▼ declined:${RESET} ${ins.declined.join(", ")}`);
    }
    print("");
  }

  printResult(r);
}

/**
 * `sleep` — Classify EEG epochs into sleep stages (Wake/N1/N2/N3/REM).
 *
 * Sends `{ command: "sleep", start_utc, end_utc }`.  The server uses
 * relative band-power ratios (delta/theta/alpha/beta) with simplified
 * AASM heuristics to classify each 5-second embedding epoch.
 *
 * Auto-range: if no `--start`/`--end` given, spans all sessions from the
 * last 24 hours.  Falls back to the most recent session, or `now - 8h`.
 *
 * In normal mode, prints a colored sleep summary table (stage counts +
 * percentages) before the full JSON with per-epoch hypnogram data.
 *
 * Response: `{ epochs[]: { utc, stage, rel_delta, rel_theta, rel_alpha,
 *   rel_beta }, summary: { total_epochs, wake_epochs, n1/n2/n3/rem_epochs,
 *   epoch_secs } }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`:
 * ```sh
 * # Sleep staging over an explicit time range:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"sleep","start_utc":1740380100,"end_utc":1740415510}'
 *
 * # Extract just the summary block:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"sleep","start_utc":1740380100,"end_utc":1740415510}' | jq '.summary'
 * ```
 *
 * @param args - Parsed CLI arguments (may contain `start`, `end`).
 */
async function cmdSleep(args: Args): Promise<void> {
  const hasExplicitRange = args.start != null || args.end != null;
  const hasIndex         = args.sessionIndex != null;
  let start: number, end: number;
  let rangeIsDefault = false;

  if (hasExplicitRange) {
    // --start / --end always wins
    const now = Math.floor(Date.now() / 1000);
    start = args.start ?? now - 28800;
    end   = args.end   ?? now;
  } else if (hasIndex) {
    // Resolve session by index (0 = latest, 1 = previous, …)
    const sessions = await getSessions();
    const idx = args.sessionIndex!;
    if (sessions.length === 0) printError("no sessions recorded yet");
    if (idx >= sessions.length) {
      printError(`session index ${idx} out of range — only ${sessions.length} session(s) available (0–${sessions.length - 1})`);
    }
    const s = sessions[idx];
    start = s.start_utc;
    end   = s.end_utc;
  } else {
    // Default: span all sessions from the last 24 h
    rangeIsDefault = true;
    const sessions = await getSessions();
    const now = Math.floor(Date.now() / 1000);
    const cutoff = now - 86400;
    const recent = sessions.filter(s => s.end_utc > cutoff);
    if (recent.length > 0) {
      start = recent[recent.length - 1].start_utc;
      end   = recent[0].end_utc;
    } else if (sessions.length > 0) {
      start = sessions[0].start_utc;
      end   = sessions[0].end_utc;
    } else {
      start = now - 28800;
      end   = now;
    }
  }

  print(`${BOLD}⚡ sleep${RESET}`
    + (hasIndex ? `  ${DIM}[session ${args.sessionIndex}]${RESET}` : ""));
  printRange("range", start, end, rangeIsDefault);
  if (rangeIsDefault) printRerun(`sleep --start ${start} --end ${end}`);

  const r = await send({ command: "sleep", start_utc: start, end_utc: end });
  if (!r.ok) printError(`server returned ok=false: ${r.error}`);

  if (!jsonMode && r.summary) {
    const s = r.summary;
    print("");
    print(`  ${BOLD}Sleep Summary${RESET}`);
    print(`  ${DIM}total:${RESET} ${CYAN}${s.total_epochs}${RESET} epochs (${CYAN}${((s.total_epochs * 5) / 60).toFixed(0)}${RESET} min)`);
    print(`  ${GREEN}Wake${RESET}  ${s.wake_epochs}  ${DIM}(${((s.wake_epochs / Math.max(s.total_epochs, 1)) * 100).toFixed(1)}%)${RESET}`);
    print(`  ${YELLOW}N1${RESET}    ${s.n1_epochs}  ${DIM}(${((s.n1_epochs / Math.max(s.total_epochs, 1)) * 100).toFixed(1)}%)${RESET}`);
    print(`  ${BLUE}N2${RESET}    ${s.n2_epochs}  ${DIM}(${((s.n2_epochs / Math.max(s.total_epochs, 1)) * 100).toFixed(1)}%)${RESET}`);
    print(`  ${MAGENTA}N3${RESET}    ${s.n3_epochs}  ${DIM}(${((s.n3_epochs / Math.max(s.total_epochs, 1)) * 100).toFixed(1)}%)${RESET}`);
    print(`  ${RED}REM${RESET}   ${s.rem_epochs}  ${DIM}(${((s.rem_epochs / Math.max(s.total_epochs, 1)) * 100).toFixed(1)}%)${RESET}`);
  }

  if (!jsonMode && r.analysis && r.analysis !== null) {
    const a = r.analysis;
    print("");
    print(`  ${BOLD}Sleep Analysis${RESET}`);
    print(`  ${DIM}efficiency:${RESET}    ${CYAN}${a.efficiency_pct?.toFixed(1)}%${RESET}`);
    print(`  ${DIM}onset latency:${RESET} ${CYAN}${a.onset_latency_min?.toFixed(1)}${RESET} min`);
    if (a.rem_latency_min != null) {
      print(`  ${DIM}REM latency:${RESET}   ${CYAN}${a.rem_latency_min?.toFixed(1)}${RESET} min`);
    }
    print(`  ${DIM}transitions:${RESET}   ${CYAN}${a.transitions}${RESET}  ${DIM}awakenings:${RESET} ${CYAN}${a.awakenings}${RESET}`);

    if (a.stage_minutes) {
      const sm = a.stage_minutes;
      print(`\n  ${DIM}Stage durations:${RESET} Wake ${CYAN}${sm.wake}${RESET}m  N1 ${CYAN}${sm.n1}${RESET}m  N2 ${CYAN}${sm.n2}${RESET}m  N3 ${CYAN}${sm.n3}${RESET}m  REM ${CYAN}${sm.rem}${RESET}m`);
    }

    if (a.bouts && Object.keys(a.bouts).length > 0) {
      print(`\n  ${DIM}Bout analysis:${RESET}`);
      for (const [stage, b] of Object.entries(a.bouts) as [string, any][]) {
        print(`    ${stage.padEnd(6)} ${CYAN}${b.count}${RESET} bouts  avg ${CYAN}${b.mean_min?.toFixed(1)}${RESET}m  max ${CYAN}${b.max_min?.toFixed(1)}${RESET}m`);
      }
    }
  }

  if (!jsonMode) print("");
  printResult(r);
}

/**
 * `umap` — Compute a 3D UMAP projection of EEG embeddings from two sessions.
 *
 * This is a two-phase operation:
 * 1. **Enqueue** — sends `{ command: "umap", a_start_utc, a_end_utc,
 *    b_start_utc, b_end_utc }`.  Server returns immediately with
 *    `{ job_id, estimated_secs, n_a, n_b }`.
 * 2. **Poll** — repeatedly sends `{ command: "umap_poll", job_id }` every
 *    2 s.  Displays a live progress bar with epoch count, ms/epoch, and
 *    ETA.  Timeout: 5 minutes.
 *
 * The server runs GPU-accelerated UMAP via `fast-umap` (wgpu/CubeCL backend).
 * Results are cached in `~/.skill/umap_cache/` keyed by time ranges.
 *
 * Auto-range: same as `compare` — last two sessions or split-half.
 *
 * Final response: `{ status: "complete", result: { points[]: { x, y, z,
 *   session, utc, label? }, n_a, n_b, dim, elapsed_ms } }`.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`
 * (two separate requests — enqueue, then poll in a loop):
 * ```sh
 * # Step 1 — enqueue the UMAP job:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"umap","a_start_utc":1740380100,"a_end_utc":1740382665,"b_start_utc":1740412800,"b_end_utc":1740415510}'
 * # → {"ok":true,"job_id":3,"estimated_secs":12,"n_a":513,"n_b":541}
 *
 * # Step 2 — poll until status === "complete" (repeat with the returned job_id):
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"umap_poll","job_id":3}' | jq '{status: .status, points: (.result.points | length)}'
 * ```
 *
 * @param args - Parsed CLI arguments (may contain `aStart`, `aEnd`, `bStart`, `bEnd`).
 */
async function cmdUmap(args: Args): Promise<void> {
  const hasExplicitRange = args.aStart != null || args.aEnd != null || args.bStart != null || args.bEnd != null;
  let aStart: number, aEnd: number, bStart: number, bEnd: number;
  if (hasExplicitRange) {
    const now = Math.floor(Date.now() / 1000);
    const SIX_H = 6 * 3600;
    aStart = args.aStart ?? now - SIX_H;
    aEnd   = args.aEnd   ?? now - SIX_H / 2;
    bStart = args.bStart ?? now - SIX_H / 2;
    bEnd   = args.bEnd   ?? now;
  } else {
    ({ aStart, aEnd, bStart, bEnd } = await autoRangeAB());
  }

  print(`${BOLD}⚡ umap${RESET}`);
  printRange("A", aStart, aEnd, !hasExplicitRange);
  printRange("B", bStart, bEnd, !hasExplicitRange);
  if (!hasExplicitRange) printRerun(`umap --a-start ${aStart} --a-end ${aEnd} --b-start ${bStart} --b-end ${bEnd}`);

  // Enqueue
  const enq = await send({
    command: "umap",
    a_start_utc: aStart, a_end_utc: aEnd,
    b_start_utc: bStart, b_end_utc: bEnd,
  }, 30000);
  if (!enq.ok) printError(`enqueue failed: ${enq.error}`);

  printInfo(`enqueued job_id=${enq.job_id}  n_a=${enq.n_a}  n_b=${enq.n_b}  est=${enq.estimated_secs}s`);

  if (!enq.job_id && enq.job_id !== 0) printError("no job_id returned");

  // Poll with progress
  const pollStart = Date.now();
  const POLL_TIMEOUT = 300_000;
  let result: any = null;
  let pollCount = 0;

  while (Date.now() - pollStart < POLL_TIMEOUT) {
    await new Promise(r => setTimeout(r, 2000));
    pollCount++;
    let poll: any;
    try {
      poll = await send({ command: "umap_poll", job_id: enq.job_id }, 60000);
    } catch {
      const elapsed = ((Date.now() - pollStart) / 1000).toFixed(0);
      printProgress(`⏳ waiting for GPU… ${elapsed}s elapsed`);
      continue;
    }

    if (poll.status === "complete") {
      clearProgress();
      result = poll;
      break;
    }
    if (poll.status === "error") {
      clearProgress();
      printError(`UMAP job error: ${poll.error}`);
    }
    if (poll.status === "not_found") {
      clearProgress();
      printError(`job ${enq.job_id} not found`);
    }

    // Progress display
    const p = poll.progress;
    const elapsed = ((Date.now() - pollStart) / 1000).toFixed(0);
    if (p && p.total_epochs > 0) {
      const pct = Math.round(p.epoch / p.total_epochs * 100);
      const bar = progressBar(pct, 30);
      const eta = p.epoch_ms > 0 ? ((p.total_epochs - p.epoch) * p.epoch_ms / 1000).toFixed(0) : "?";
      printProgress(`${bar} ${pct}%  epoch ${p.epoch}/${p.total_epochs}  ${p.epoch_ms.toFixed(0)}ms/ep  ~${eta}s left`);
    } else {
      printProgress(`⏳ initializing… ${elapsed}s`);
    }
  }

  if (!result) {
    clearProgress();
    printError(`UMAP timed out after ${POLL_TIMEOUT / 1000}s`);
  }

  printInfo(`completed in ${result.elapsed_ms}ms`);

  // Show UMAP cluster analysis
  const umapResult = result.result || result;
  if (!jsonMode && umapResult.analysis && umapResult.analysis !== null) {
    const a = umapResult.analysis;
    print("");
    print(`  ${BOLD}UMAP Cluster Analysis${RESET}`);
    print(`  ${DIM}separation score:${RESET} ${CYAN}${a.separation_score?.toFixed(2)}${RESET}  ${DIM}(higher = better A/B separation)${RESET}`);
    print(`  ${DIM}inter-cluster:${RESET}    ${CYAN}${a.inter_cluster_distance?.toFixed(2)}${RESET}`);
    print(`  ${DIM}intra-spread A:${RESET}   ${CYAN}${a.intra_spread_a?.toFixed(2)}${RESET}  ${DIM}B:${RESET} ${CYAN}${a.intra_spread_b?.toFixed(2)}${RESET}`);

    if (a.centroid_a && a.centroid_b) {
      const fmtPt = (p: number[]) => `(${p.map((v: number) => v.toFixed(2)).join(", ")})`;
      print(`  ${DIM}centroid A:${RESET} ${fmtPt(a.centroid_a)}  ${DIM}B:${RESET} ${fmtPt(a.centroid_b)}`);
    }

    const nOut = (a.n_outliers_a || 0) + (a.n_outliers_b || 0);
    if (nOut > 0) {
      print(`  ${YELLOW}outliers:${RESET} ${a.n_outliers_a} in A, ${a.n_outliers_b} in B  ${DIM}(>2σ from centroid)${RESET}`);
    }
    print("");
  }

  printResult(result);
}

/**
 * Render a fixed-width progress bar using Unicode block characters.
 *
 * Example at 40%: `████████████░░░░░░░░░░░░░░░░░░`
 *
 * @param pct   - Completion percentage (0–100).
 * @param width - Total bar width in characters.
 * @returns ANSI-colored string (blue filled, gray empty).
 */
function progressBar(pct: number, width: number): string {
  const filled = Math.round(width * pct / 100);
  const empty = width - filled;
  return `${BLUE}${"█".repeat(filled)}${GRAY}${"░".repeat(empty)}${RESET}`;
}

/**
 * `dnd [on|off]` — Show DND automation status, or force-override it.
 *
 * With no subcommand, sends `{ command: "dnd" }` and prints the full status
 * snapshot: config (enabled, threshold, duration, mode), live timer state
 * (elapsed_secs toward duration_secs), and the real OS-level Focus state.
 *
 * With `on` or `off`, sends `{ command: "dnd_set", enabled: true/false }` to
 * activate or deactivate DND immediately, bypassing the EEG threshold.
 *
 * **HTTP equivalents**
 * ```sh
 * # Status
 * curl http://localhost:PORT/dnd
 *
 * # Force on
 * curl -X POST http://localhost:PORT/dnd \
 *      -H 'Content-Type: application/json' \
 *      -d '{"enabled":true}'
 *
 * # Force off
 * curl -X POST http://localhost:PORT/dnd \
 *      -H 'Content-Type: application/json' \
 *      -d '{"enabled":false}'
 * ```
 *
 * @param args - Parsed CLI args; `subAction` is `"on"`, `"off"`, or undefined.
 */
async function cmdDnd(args: Args): Promise<void> {
  const sub = args.subAction; // "on" | "off" | undefined

  if (sub === "on" || sub === "off") {
    // ── Force-override ──────────────────────────────────────────────────────
    const enabled = sub === "on";
    print(`${BOLD}⚡ dnd ${enabled ? "on" : "off"}${RESET}`);
    print(`  ${DIM}force-${enabled ? "enabling" : "disabling"} DND (bypasses EEG threshold)${RESET}`);

    const r = await send({ command: "dnd_set", enabled });
    if (!r.ok) printError(`dnd_set failed: ${r.error ?? "OS call returned false"}`);

    if (jsonMode) {
      printResult(r);
    } else {
      const status = r.ok
        ? `${GREEN}activated${RESET}`
        : `${RED}failed (OS call rejected — check macOS Focus permissions)${RESET}`;
      print(`  DND ${enabled ? "enabled" : "disabled"}  ${status}`);
      printResult(r);
    }
    return;
  }

  // ── Status snapshot ─────────────────────────────────────────────────────
  print(`${BOLD}⚡ dnd${RESET}  ${DIM}automation status${RESET}`);

  const r = await send({ command: "dnd" });
  if (!r.ok) printError(`dnd failed: ${r.error}`);

  if (jsonMode) {
    printResult(r);
    return;
  }

  // Human-readable summary
  const yn = (v: boolean | null | undefined) =>
    v === true  ? `${GREEN}yes${RESET}` :
    v === false ? `${RED}no${RESET}`   :
    `${DIM}n/a (non-macOS)${RESET}`;

  const avg = typeof r.avg_score === "number" ? r.avg_score : 0;

  // Score bar — filled portion represents avg_score / 100
  const scoreBar = (avg: number, threshold: number) => {
    const pct  = Math.min(avg / 100, 1.0);
    const fill = Math.round(pct * 24);
    const empty = 24 - fill;
    const color = avg >= threshold ? GREEN : YELLOW;
    return `${color}${"█".repeat(fill)}${RESET}${DIM}${"░".repeat(empty)}${RESET}  ${avg.toFixed(1)} / ${threshold}`;
  };

  // Window fill bar — how many samples collected vs target
  const windowBar = (count: number, total: number) => {
    const pct  = total > 0 ? Math.min(count / total, 1.0) : 0;
    const fill = Math.round(pct * 24);
    const empty = 24 - fill;
    return `${CYAN}${"█".repeat(fill)}${RESET}${DIM}${"░".repeat(empty)}${RESET}  ${count} / ${total} samples`;
  };

  print("");
  print(`  ${CYAN}DND automation${RESET}`);
  print(`    enabled        ${yn(r.enabled)}${r.enabled ? "" : `  ${DIM}(turn on in Settings → Do Not Disturb)${RESET}`}`);
  print(`    threshold      ${CYAN}${r.threshold}${RESET}  ${DIM}avg focus score (0–100) required to activate${RESET}`);
  print(`    window         ${CYAN}${r.duration_secs}s${RESET}  ${DIM}(≈ ${r.window_size} samples at ~4 Hz)${RESET}`);
  print(`    mode           ${DIM}${r.mode_identifier}${RESET}`);
  print("");
  print(`  ${CYAN}Rolling average${RESET}  ${DIM}(avg of last ${r.window_size} focus scores)${RESET}`);
  print(`    ${scoreBar(avg, r.threshold)}`);
  if (avg >= r.threshold) {
    print(`    ${GREEN}▶ above threshold — DND ${r.dnd_active ? "is active" : "activates once window fills"}${RESET}`);
  } else {
    print(`    ${DIM}▷ below threshold  (need ${(r.threshold - avg).toFixed(1)} more)${RESET}`);
  }
  print("");
  print(`  ${CYAN}Sample window${RESET}`);
  print(`    ${windowBar(r.sample_count, r.window_size)}`);
  if (r.sample_count < r.window_size) {
    print(`    ${DIM}▷ filling… (${r.window_size - r.sample_count} samples until window is full)${RESET}`);
  }
  print("");
  print(`  ${CYAN}State${RESET}`);
  print(`    app activated  ${yn(r.dnd_active)}  ${DIM}(this app set DND)${RESET}`);
  print(`    OS active      ${yn(r.os_active)}  ${DIM}(macOS Assertions.json / defaults read)${RESET}`);

  if (r.dnd_active !== r.os_active && r.os_active !== null) {
    print(`    ${YELLOW}⚠ app and OS states differ — OS may have been changed manually${RESET}`);
  }
  if (!r.enabled && r.os_active) {
    print(`    ${YELLOW}⚠ DND is on but automation is disabled — disable manually or turn automation on${RESET}`);
  }

  print("");
  print(`  ${DIM}Tips:${RESET}`);
  print(`    ${DIM}• 'dnd on'  — force-enable immediately${RESET}`);
  print(`    ${DIM}• 'dnd off' — force-disable immediately${RESET}`);
  print(`    ${DIM}• 'listen'  — streams dnd-eligibility events live (avg_score, sample_count, dnd_active)${RESET}`);

  printResult(r);
}

async function cmdHooks(args: Args): Promise<void> {
  const sub = (args.subAction ?? "status").toLowerCase();

  if (sub === "status") {
    print(`${BOLD}🪝 proactive hooks${RESET}`);
    const r = await send({ command: "hooks_status" });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_status failed"); }

    const hooks = Array.isArray(r.hooks) ? r.hooks : [];
    if (hooks.length === 0) {
      print(`  ${DIM}(no hooks configured)${RESET}`);
      printResult(r);
      return;
    }

    for (const row of hooks) {
      const hook = row.hook ?? {};
      const trig = row.last_trigger ?? null;
      const enabled = hook.enabled ? `${GREEN}on${RESET}` : `${GRAY}off${RESET}`;
      const scenario = String(hook.scenario ?? "any");
      print(`  ${CYAN}${hook.name ?? "(unnamed)"}${RESET}  [${enabled}]  ${DIM}scenario=${scenario}${RESET}`);
      if (trig?.triggered_at_utc) {
        const label = trig.label_text ? ` label=${JSON.stringify(trig.label_text)}` : "";
        const dist = typeof trig.distance === "number" ? ` d=${trig.distance.toFixed(3)}` : "";
        print(`    last=${trig.triggered_at_utc}${dist}${label}`);
      } else {
        print(`    ${DIM}last=never${RESET}`);
      }
    }

    printResult(r);
    return;
  }

  if (sub === "suggest") {
    if (!args.text) printError('usage: cli.ts hooks suggest "kw1,kw2"');
    const keywords = (args.text ?? "")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    print(`${BOLD}🪝 hooks suggest${RESET} ${DIM}${keywords.join(", ")}${RESET}`);
    const r = await send({ command: "hooks_suggest", keywords });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_suggest failed"); }
    const s = r.suggestion ?? {};
    print(`  suggested   ${CYAN}${Number(s.suggested ?? 0.1).toFixed(2)}${RESET}`);
    print(`  distances   min=${Number(s.eeg_min ?? 0).toFixed(3)} p25=${Number(s.eeg_p25 ?? 0).toFixed(3)} p50=${Number(s.eeg_p50 ?? 0).toFixed(3)} p75=${Number(s.eeg_p75 ?? 0).toFixed(3)} max=${Number(s.eeg_max ?? 0).toFixed(3)}`);
    print(`  samples     labels=${s.label_n ?? 0} refs=${s.ref_n ?? 0} eeg=${s.sample_n ?? 0}`);
    if (s.note) print(`  ${DIM}${s.note}${RESET}`);
    printResult(r);
    return;
  }

  if (sub === "log") {
    const limit = args.limit ?? 20;
    const offset = args.offset ?? 0;
    print(`${BOLD}🪝 hooks log${RESET} ${DIM}(limit=${limit}, offset=${offset})${RESET}`);
    const r = await send({ command: "hooks_log", limit, offset });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_log failed"); }
    const rows = Array.isArray(r.rows) ? r.rows : [];
    const total = Number(r.total ?? 0);

    if (rows.length === 0) {
      print(`  ${DIM}(no hook events)${RESET}`);
      printResult(r);
      return;
    }

    for (const row of rows) {
      let hookName = "(unknown)";
      let scenario = "any";
      let label = "";
      let dist = "";
      try {
        const h = JSON.parse(row.hook_json ?? "{}");
        const t = JSON.parse(row.trigger_json ?? "{}");
        hookName = h.name ?? hookName;
        scenario = h.scenario ?? scenario;
        if (t.label_text) label = ` label=${JSON.stringify(t.label_text)}`;
        if (typeof t.distance === "number") dist = ` d=${t.distance.toFixed(3)}`;
      } catch {}
      print(`  ${CYAN}${hookName}${RESET}  ${DIM}[${scenario}]${RESET}  ts=${row.triggered_at_utc}${dist}${label}`);
    }
    print(`  ${DIM}showing ${rows.length} row(s), total=${total}${RESET}`);
    printResult(r);
    return;
  }

  if (sub === "list") {
    print(`${BOLD}🪝 hooks list${RESET}`);
    const r = await send({ command: "hooks_get" });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_get failed"); }

    const hooks = Array.isArray(r.hooks) ? r.hooks : [];
    if (hooks.length === 0) {
      print(`  ${DIM}(no hooks configured)${RESET}`);
      printResult(r);
      return;
    }

    for (const h of hooks) {
      const enabled = h.enabled ? `${GREEN}on${RESET}` : `${GRAY}off${RESET}`;
      const kws = Array.isArray(h.keywords) ? h.keywords.join(", ") : "";
      print(`  ${CYAN}${h.name ?? "(unnamed)"}${RESET}  [${enabled}]  scenario=${h.scenario ?? "any"}  threshold=${Number(h.distance_threshold ?? 0.1).toFixed(2)}  recent=${h.recent_limit ?? 12}`);
      if (kws) print(`    keywords: ${kws}`);
      if (h.command) print(`    command: ${h.command}`);
      if (h.text)    print(`    text: ${h.text}`);
    }
    printResult(r);
    return;
  }

  if (sub === "add") {
    if (!args.hookName) printError('usage: hooks add <name> [--keywords "k1,k2"] [--scenario any] [--command cmd] [--hook-text txt] [--threshold 0.14] [--recent 12]');

    // Fetch current hooks
    const r0 = await send({ command: "hooks_get" });
    if (!r0.ok) { printResult(r0); printError(r0.error ?? "hooks_get failed"); }
    const current: any[] = Array.isArray(r0.hooks) ? r0.hooks : [];

    if (current.some((h: any) => h.name === args.hookName)) {
      printError(`hook "${args.hookName}" already exists — use 'hooks update' to modify it`);
    }

    const newHook: any = {
      name: args.hookName!,
      enabled: true,
      keywords: args.hookKeywords ? args.hookKeywords.split(",").map((s: string) => s.trim()).filter(Boolean) : [],
      scenario: args.hookScenario ?? "any",
      command: args.hookCommand ?? "",
      text: args.hookText ?? "",
      distance_threshold: args.hookThreshold ?? 0.1,
      recent_limit: args.hookRecent ?? 12,
    };

    current.push(newHook);
    const r = await send({ command: "hooks_set", hooks: current });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_set failed"); }
    print(`${GREEN}✓${RESET} hook ${CYAN}${args.hookName}${RESET} added`);
    printResult(r);
    return;
  }

  if (sub === "remove") {
    if (!args.hookName) printError("usage: hooks remove <name>");

    const r0 = await send({ command: "hooks_get" });
    if (!r0.ok) { printResult(r0); printError(r0.error ?? "hooks_get failed"); }
    const current: any[] = Array.isArray(r0.hooks) ? r0.hooks : [];
    const before = current.length;
    const filtered = current.filter((h: any) => h.name !== args.hookName);

    if (filtered.length === before) {
      printError(`hook "${args.hookName}" not found`);
    }

    const r = await send({ command: "hooks_set", hooks: filtered });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_set failed"); }
    print(`${GREEN}✓${RESET} hook ${CYAN}${args.hookName}${RESET} removed`);
    printResult(r);
    return;
  }

  if (sub === "enable" || sub === "disable") {
    if (!args.hookName) printError(`usage: hooks ${sub} <name>`);

    const r0 = await send({ command: "hooks_get" });
    if (!r0.ok) { printResult(r0); printError(r0.error ?? "hooks_get failed"); }
    const current: any[] = Array.isArray(r0.hooks) ? r0.hooks : [];
    const hook = current.find((h: any) => h.name === args.hookName);
    if (!hook) printError(`hook "${args.hookName}" not found`);

    hook.enabled = sub === "enable";
    const r = await send({ command: "hooks_set", hooks: current });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_set failed"); }
    print(`${GREEN}✓${RESET} hook ${CYAN}${args.hookName}${RESET} ${sub === "enable" ? "enabled" : "disabled"}`);
    printResult(r);
    return;
  }

  if (sub === "update") {
    if (!args.hookName) printError('usage: hooks update <name> [--keywords "k1,k2"] [--scenario any] [--command cmd] [--hook-text txt] [--threshold 0.14] [--recent 12]');

    const r0 = await send({ command: "hooks_get" });
    if (!r0.ok) { printResult(r0); printError(r0.error ?? "hooks_get failed"); }
    const current: any[] = Array.isArray(r0.hooks) ? r0.hooks : [];
    const hook = current.find((h: any) => h.name === args.hookName);
    if (!hook) printError(`hook "${args.hookName}" not found`);

    if (args.hookKeywords !== undefined) hook.keywords = args.hookKeywords.split(",").map((s: string) => s.trim()).filter(Boolean);
    if (args.hookScenario !== undefined) hook.scenario = args.hookScenario;
    if (args.hookCommand  !== undefined) hook.command  = args.hookCommand;
    if (args.hookText     !== undefined) hook.text     = args.hookText;
    if (args.hookThreshold !== undefined) hook.distance_threshold = args.hookThreshold;
    if (args.hookRecent    !== undefined) hook.recent_limit = args.hookRecent;

    const r = await send({ command: "hooks_set", hooks: current });
    if (!r.ok) { printResult(r); printError(r.error ?? "hooks_set failed"); }
    print(`${GREEN}✓${RESET} hook ${CYAN}${args.hookName}${RESET} updated`);
    printResult(r);
    return;
  }

  printError(`unknown hooks subcommand: "${sub}". Valid: status list add remove enable disable update suggest log`);
}

/**
 * `listen` — Passively listen for broadcast events from the Skill server.
 *
 * Opens a WebSocket and collects all broadcast events (EEG packets, PPG,
 * IMU, scores, label-created, etc.) for the specified duration.
 *
 * In normal mode, prints a grouped summary (event type × count) followed
 * by the full event array.  In `--json` mode, outputs only the array.
 *
 * Useful for verifying that the headset is streaming data, debugging
 * event formats, or piping live data to external tools.
 *
 * **HTTP equivalent:** Not available over HTTP.
 * The server pushes broadcast events only over the WebSocket connection;
 * `POST /` is a request/response tunnel with no streaming support.
 * Use `--ws` (or the default auto-transport, which prefers WebSocket) to
 * receive live events.
 *
 * @param seconds - How long to listen (default 5 s via `--seconds` flag).
 */
async function cmdListen(seconds: number): Promise<void> {
  print(`${BOLD}⚡ listen${RESET} ${DIM}for ${seconds}s…${RESET}\n`);

  // Show a live countdown while waiting so the terminal doesn't appear hung.
  let remaining = seconds;
  const ticker = setInterval(() => {
    remaining--;
    if (remaining > 0) printProgress(`⏳ ${remaining}s remaining…`);
  }, 1000);

  const events = await collectEvents(seconds * 1000);

  clearInterval(ticker);
  clearProgress();

  if (jsonMode) {
    printResult(events);
    return;
  }

  if (events.length === 0) {
    print(`  ${DIM}no events received (is a Muse connected?)${RESET}`);
    return;
  }

  // Group by type
  const byType: Record<string, any[]> = {};
  for (const e of events) {
    (byType[e.event] ??= []).push(e);
  }

  for (const [type, evts] of Object.entries(byType)) {
    print(`  ${CYAN}${type}${RESET} ${DIM}×${evts.length}${RESET}`);
  }

  // ── Highlight hook triggers ─────────────────────────────────────────────
  const hookEvents = byType["hook"] ?? [];
  if (hookEvents.length > 0) {
    print("");
    print(`  ${BOLD}🪝 Hook Triggers${RESET}`);
    for (const e of hookEvents) {
      const p = e.payload ?? e;
      const distStr = typeof p.distance === "number" ? p.distance.toFixed(4) : "?";
      const distColor = typeof p.distance === "number" && p.distance < 0.1 ? GREEN : CYAN;
      print(`  ${YELLOW}${p.hook ?? "(unnamed)"}${RESET}  ${DIM}[${p.scenario ?? "any"}]${RESET}  ${DIM}dist:${RESET} ${distColor}${distStr}${RESET}`);
      if (p.label_text) {
        print(`    ${DIM}matched label:${RESET} ${GREEN}"${p.label_text}"${RESET}  ${DIM}id:${RESET} ${CYAN}${p.label_id ?? "?"}${RESET}`);
      }
      if (p.command) print(`    ${DIM}command:${RESET} ${p.command}`);
      if (p.text)    print(`    ${DIM}text:${RESET} ${p.text}`);
    }
  }

  print("");
  printResult(events);
}

/**
 * `raw` — Send an arbitrary JSON command and print the server's response.
 *
 * Useful for testing new/undocumented commands, or for constructing precise
 * queries that the named commands don't expose (e.g. custom `ef` parameter
 * for search, or `label_start_utc` for backdated labels).
 *
 * The JSON must contain a `"command"` field.  The response is printed as-is.
 *
 * **HTTP equivalent** — `POST /` with `Content-Type: application/json`
 * (the body is forwarded verbatim, exactly as in `raw`):
 * ```sh
 * # Send any arbitrary command:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":3}'
 *
 * # Another example — back-dated label with a custom timestamp field:
 * curl -s -X POST http://127.0.0.1:8375/ \
 *   -H "Content-Type: application/json" \
 *   -d '{"command":"label","text":"retrospective note","label_start_utc":1740412800}'
 * ```
 *
 * @param rawJson - The raw JSON string to send, e.g. `'{"command":"status"}'`.
 */
// ── LLM image helpers ─────────────────────────────────────────────────────────

import { readFileSync } from "fs";
import { extname } from "path";

/**
 * Infer the MIME type from a file extension.
 */
function imageMime(filePath: string): string {
  const ext = extname(filePath).toLowerCase();
  switch (ext) {
    case ".png":  return "image/png";
    case ".gif":  return "image/gif";
    case ".webp": return "image/webp";
    case ".bmp":  return "image/bmp";
    default:      return "image/jpeg";
  }
}

/**
 * Read one image file from disk and return an OpenAI-format `image_url` content part:
 * ```json
 * { "type": "image_url", "image_url": { "url": "data:image/jpeg;base64,..." } }
 * ```
 * Throws if the file cannot be read.
 */
function loadImagePart(filePath: string): { type: string; image_url: { url: string } } {
  let data: Buffer;
  try { data = readFileSync(filePath); }
  catch (e: any) { printError(`cannot read image file "${filePath}": ${e.message}`); }
  const mime = imageMime(filePath);
  const url  = `data:${mime};base64,${data!.toString("base64")}`;
  return { type: "image_url", image_url: { url } };
}

/**
 * Load multiple image files and return them as `image_url` content parts.
 * Exits with an error if any file cannot be read.
 */
function loadImageParts(
  filePaths: string[],
): Array<{ type: string; image_url: { url: string } }> {
  return filePaths.map(loadImagePart);
}

/**
 * Build an OpenAI user message that may contain images + text.
 *
 * If `imageParts` is non-empty the content is a parts array
 * `[...imageParts, {type:"text", text}]`; otherwise it is a plain string.
 */
function buildUserMessage(
  text: string,
  imageParts: Array<{ type: string; image_url: { url: string } }>,
): { role: string; content: unknown } {
  if (imageParts.length === 0) {
    return { role: "user", content: text };
  }
  const parts: unknown[] = [...imageParts];
  if (text) parts.push({ type: "text", text });
  return { role: "user", content: parts };
}

// ── LLM command ───────────────────────────────────────────────────────────────

/**
 * `llm` — control the built-in LLM inference server.
 *
 * Subcommands:
 *   status                  Print server state (stopped/loading/running), model, context size
 *   start                   Load the active model and start the inference server
 *   stop                    Stop the server and free GPU/CPU memory
 *   catalog                 List all models with download states and active selections
 *   download <filename>     Download a GGUF model by filename (fire-and-forget; poll catalog for progress)
 *   cancel <filename>       Cancel an in-progress download
 *   delete <filename>       Delete a locally-cached model file
 *   logs                    Print last 500 LLM server log lines
 *   chat                    Interactive multi-turn REPL (type /help inside; exit to quit)
 *   chat "message"          Single-shot: send one message, stream the reply, and exit
 *   chat --image <path>     Attach images to a message (vision models only)
 */
async function cmdLlm(args: Args): Promise<void> {
  const sub = args.subAction ?? "status";

  switch (sub) {
    // ── status ──────────────────────────────────────────────────────────────
    case "status": {
      print(`${BOLD}🤖 llm status${RESET}`);
      const r = await send({ command: "llm_status" });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_status failed"); }
      const statusColor = r.status === "running" ? GREEN : r.status === "loading" ? YELLOW : GRAY;
      print(`  status         ${statusColor}${r.status}${RESET}`);
      if (r.model_name) print(`  model          ${CYAN}${r.model_name}${RESET}`);
      if (r.n_ctx)       print(`  context window ${CYAN}${r.n_ctx}${RESET} tokens`);
      print(`  vision         ${r.supports_vision ? `${GREEN}yes${RESET}` : `${GRAY}no${RESET}`}`);
      printResult(r);
      break;
    }

    // ── start ────────────────────────────────────────────────────────────────
    case "start": {
      print(`${BOLD}🤖 llm start${RESET} ${DIM}(loading model — this may take several seconds)${RESET}`);
      const r = await send({ command: "llm_start" }, 120_000);
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_start failed"); }
      print(`  ${GREEN}✓${RESET} ${r.result}`);
      printResult(r);
      break;
    }

    // ── stop ─────────────────────────────────────────────────────────────────
    case "stop": {
      print(`${BOLD}🤖 llm stop${RESET}`);
      const r = await send({ command: "llm_stop" });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_stop failed"); }
      print(`  ${GREEN}✓${RESET} ${r.result}`);
      printResult(r);
      break;
    }

    // ── catalog ───────────────────────────────────────────────────────────────
    case "catalog": {
      print(`${BOLD}🤖 llm catalog${RESET}`);
      const r = await send({ command: "llm_catalog" });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_catalog failed"); }
      const entries: any[] = r.entries ?? [];
      print(`  ${entries.length} model(s) in catalog`);
      print(`  active model   ${CYAN}${r.active_model || "(none)"}${RESET}`);
      print(`  active mmproj  ${CYAN}${r.active_mmproj || "(none)"}${RESET}`);
      print("");
      for (const e of entries) {
        const stateColor = e.state === "downloaded" ? GREEN
          : e.state === "downloading" ? YELLOW
          : e.state === "failed"      ? RED
          : GRAY;
        const pct = e.state === "downloading" ? ` ${Math.round((e.progress ?? 0) * 100)}%` : "";
        const active = e.filename === r.active_model ? ` ${GREEN}← active${RESET}` : "";
        print(`  ${stateColor}${e.state}${RESET}${pct}  ${CYAN}${e.filename}${RESET}${active}`);
        if (e.status_msg) print(`            ${DIM}${e.status_msg}${RESET}`);
      }
      printResult(r);
      break;
    }

    // ── download ─────────────────────────────────────────────────────────────
    case "download": {
      const filename = args.text;
      if (!filename) printError("usage: cli.ts llm download <filename>");
      print(`${BOLD}🤖 llm download${RESET} ${CYAN}${filename}${RESET} ${DIM}(fire-and-forget — poll 'llm catalog' for progress)${RESET}`);
      const r = await send({ command: "llm_download", filename });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_download failed"); }
      print(`  ${GREEN}✓${RESET} queued: ${CYAN}${r.filename}${RESET}`);
      printResult(r);
      break;
    }

    // ── cancel ───────────────────────────────────────────────────────────────
    case "cancel": {
      const filename = args.text;
      if (!filename) printError("usage: cli.ts llm cancel <filename>");
      print(`${BOLD}🤖 llm cancel${RESET} ${CYAN}${filename}${RESET}`);
      const r = await send({ command: "llm_cancel_download", filename });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_cancel_download failed"); }
      print(`  ${GREEN}✓${RESET} cancel signalled for ${CYAN}${r.filename}${RESET}`);
      printResult(r);
      break;
    }

    // ── delete ───────────────────────────────────────────────────────────────
    case "delete": {
      const filename = args.text;
      if (!filename) printError("usage: cli.ts llm delete <filename>");
      print(`${BOLD}🤖 llm delete${RESET} ${CYAN}${filename}${RESET}`);
      const r = await send({ command: "llm_delete", filename });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_delete failed"); }
      print(`  ${GREEN}✓${RESET} deleted ${CYAN}${r.filename}${RESET}`);
      printResult(r);
      break;
    }

    // ── logs ─────────────────────────────────────────────────────────────────
    case "logs": {
      print(`${BOLD}🤖 llm logs${RESET}`);
      const r = await send({ command: "llm_logs" });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_logs failed"); }
      const logs: any[] = r.logs ?? [];
      if (logs.length === 0) {
        print(`  ${GRAY}(no log entries)${RESET}`);
      } else {
        for (const entry of logs) {
          const levelColor = entry.level === "error" ? RED : entry.level === "warn" ? YELLOW : GRAY;
          const ts = new Date(entry.ts).toISOString().replace("T", " ").replace("Z", "");
          print(`  ${GRAY}${ts}${RESET} ${levelColor}[${entry.level}]${RESET} ${entry.message}`);
        }
      }
      print(`  ${DIM}${logs.length} log line(s)${RESET}`);
      printResult(r);
      break;
    }

    // ── chat ─────────────────────────────────────────────────────────────────
    case "chat": {
      if (transport !== "ws") {
        printError(
          "llm chat requires WebSocket (HTTP has no streaming support).\n" +
          "  Use --ws to force WebSocket, or omit --http for auto-transport."
        );
      }

      // ── Shared GenParams (applied to every turn) ──────────────────────────
      const genParams: Record<string, unknown> = {};
      if (args.temperature !== undefined) genParams.temperature = args.temperature;
      if (args.maxTokens   !== undefined) genParams.max_tokens  = args.maxTokens;

      // ── Stream one assistant turn, returns accumulated text ───────────────
      /**
       * Send a `llm_chat` WebSocket command with the current conversation
       * history and stream delta tokens directly to stdout.
       *
       * Resolves with the full assistant reply text so the caller can append
       * it to the history for the next turn.  Rejects on timeout or error.
       */
      function streamTurn(
        messages: Array<{ role: string; content: string }>,
        timeoutMs = 120_000,
      ): Promise<{ text: string; promptTokens: number; completionTokens: number; nCtx: number }> {
        return new Promise((resolve, reject) => {
          let text = "";
          const timer = setTimeout(() => {
            ws.off("message", handler);
            reject(new Error("llm_chat timeout"));
          }, timeoutMs);

          const handler = (raw: any) => {
            let data: any;
            try { data = JSON.parse(raw.toString()); } catch { return; }
            if (data.command !== "llm_chat") return;

            switch (data.type) {
              case "delta":
                process.stdout.write(data.text ?? "");
                text += data.text ?? "";
                break;
              case "done":
                process.stdout.write("\n");
                clearTimeout(timer);
                ws.off("message", handler);
                resolve({
                  text,
                  promptTokens:     data.prompt_tokens     ?? 0,
                  completionTokens: data.completion_tokens ?? 0,
                  nCtx:             data.n_ctx             ?? 0,
                });
                break;
              case "error":
                clearTimeout(timer);
                ws.off("message", handler);
                reject(new Error(data.error ?? "llm_chat error"));
                break;
              default:
                if (data.ok === false) {
                  clearTimeout(timer);
                  ws.off("message", handler);
                  reject(new Error(data.error ?? "llm_chat failed"));
                }
            }
          };

          ws.on("message", handler);
          ws.send(JSON.stringify({ command: "llm_chat", messages, ...genParams }));
        });
      }

      // ── Conversation history (grows across turns) ─────────────────────────
      const history: Array<{ role: string; content: unknown }> = [];
      if (args.system) {
        history.push({ role: "system", content: args.system });
      }

      // ── Single-shot mode: message provided on command line ────────────────
      if (args.text) {
        // Load any --image flags from disk
        const imgParts = loadImageParts(args.images ?? []);
        if (imgParts.length > 0 && transport !== "ws") {
          // HTTP fallback: use POST /llm/chat which accepts base64 JSON
          const imageUrls = imgParts.map(p => p.image_url.url);
          const payload: Record<string, unknown> = {
            message:     args.text,
            images:      imageUrls,
            ...(args.system      && { system:      args.system      }),
            ...(args.temperature !== undefined && { temperature: args.temperature }),
            ...(args.maxTokens   !== undefined && { max_tokens:  args.maxTokens   }),
          };
          print(`${BOLD}🤖 llm chat${RESET} ${DIM}${args.text}${RESET} ${DIM}[${imgParts.length} image(s) via HTTP]${RESET}\n`);
          let res: Response;
          try {
            res = await fetch(`${httpBase}/llm/chat`, {
              method:  "POST",
              headers: { "Content-Type": "application/json" },
              body:    JSON.stringify(payload),
            });
          } catch (e: any) { printError(`POST /llm/chat failed: ${e.message}`); }
          const r = await res!.json();
          if (!r.ok) { printResult(r); printError(r.error ?? "llm chat failed"); }
          print(r.text);
          if (!jsonMode) {
            print(`\n${DIM}  finish: ${r.prompt_tokens}+${r.completion_tokens} tokens, n_ctx=${r.n_ctx}${RESET}`);
          } else {
            console.log(JSON.stringify({ text: r.text, prompt_tokens: r.prompt_tokens, completion_tokens: r.completion_tokens }));
          }
          break;
        }

        history.push(buildUserMessage(args.text, imgParts));
        const imgNote = imgParts.length > 0 ? ` ${DIM}[${imgParts.length} image(s)]${RESET}` : "";
        print(`${BOLD}🤖 llm chat${RESET} ${DIM}${args.text}${RESET}${imgNote}\n`);
        const result = await streamTurn(history as Array<{ role: string; content: string }>);
        if (!jsonMode) {
          print(`\n${DIM}  finish: ${result.promptTokens}+${result.completionTokens} tokens, n_ctx=${result.nCtx}${RESET}`);
        } else {
          console.log(JSON.stringify({
            text:               result.text,
            prompt_tokens:      result.promptTokens,
            completion_tokens:  result.completionTokens,
          }));
        }
        break;
      }

      // ── Interactive REPL mode: no message arg provided ────────────────────
      const readline = await import("readline");
      const rl = readline.createInterface({
        input:  process.stdin,
        output: process.stdout,
        terminal: true,
      });

      // Track token usage across the session
      let totalPrompt = 0;
      let totalCompletion = 0;
      let turnCount = 0;
      // Images staged for the next message (set via /image command)
      let pendingImages: Array<{ type: string; image_url: { url: string } }> =
        loadImageParts(args.images ?? []);

      const systemNote = args.system
        ? `${DIM}system: ${args.system.slice(0, 60)}${args.system.length > 60 ? "…" : ""}${RESET}\n`
        : "";

      // Print header
      process.stdout.write(
        `\n${BOLD}🤖 NeuroSkill™ LLM Chat${RESET}  ${DIM}(type ${CYAN}exit${DIM} or press ${CYAN}Ctrl+C${DIM} to quit)${RESET}\n` +
        systemNote +
        `${DIM}─────────────────────────────────────────────────────────────${RESET}\n\n`,
      );

      // Wrap the readline question in a promise so we can await it
      const question = (prompt: string): Promise<string | null> =>
        new Promise((resolve) => {
          rl.question(prompt, resolve);
          rl.once("close", () => resolve(null)); // Ctrl+D / pipe closed
        });

      // Graceful Ctrl+C
      rl.on("SIGINT", () => {
        process.stdout.write("\n");
        rl.close();
      });

      // Chat loop
      while (true) {
        // Show pending image indicator in the prompt
        const imgIndicator = pendingImages.length > 0
          ? `${YELLOW}[${pendingImages.length} img]${RESET} `
          : "";
        const input = await question(`${imgIndicator}${BOLD}${CYAN}You:${RESET} `);

        // null = Ctrl+D / pipe closed
        if (input === null) break;

        const trimmed = input.trim();
        if (!trimmed) continue;
        if (trimmed.toLowerCase() === "exit" || trimmed.toLowerCase() === "quit") break;

        // ── REPL commands ────────────────────────────────────────────────────
        if (trimmed === "/clear") {
          history.length = 0;
          if (args.system) history.push({ role: "system", content: args.system });
          pendingImages = [];
          turnCount = 0;
          process.stdout.write(`${DIM}  conversation cleared${RESET}\n\n`);
          continue;
        }

        if (trimmed === "/history") {
          for (const m of history) {
            const roleColor = m.role === "user" ? CYAN : m.role === "system" ? YELLOW : GREEN;
            const contentStr = typeof m.content === "string"
              ? m.content
              : JSON.stringify(m.content);
            process.stdout.write(
              `${roleColor}[${m.role}]${RESET} ${contentStr.slice(0, 120)}${contentStr.length > 120 ? "…" : ""}\n`,
            );
          }
          process.stdout.write("\n");
          continue;
        }

        if (trimmed.startsWith("/image ")) {
          // /image <path> — load image file for the next message
          const imgPath = trimmed.slice(7).trim();
          try {
            const part = loadImagePart(imgPath);
            pendingImages.push(part);
            process.stdout.write(
              `${GREEN}  ✓ image staged${RESET}: ${CYAN}${imgPath}${RESET} ` +
              `${DIM}(${pendingImages.length} pending — send your message to include it)${RESET}\n\n`,
            );
          } catch {
            process.stdout.write(`${RED}  error: cannot read image file "${imgPath}"${RESET}\n\n`);
          }
          continue;
        }

        if (trimmed === "/images") {
          if (pendingImages.length === 0) {
            process.stdout.write(`${DIM}  no images staged${RESET}\n\n`);
          } else {
            process.stdout.write(`${DIM}  ${pendingImages.length} image(s) staged for next message${RESET}\n\n`);
          }
          continue;
        }

        if (trimmed === "/help") {
          process.stdout.write(
            `${DIM}  /clear           — clear conversation history (keep system prompt)\n` +
            `  /history         — print all messages in the conversation\n` +
            `  /image <path>    — stage an image file for the next message\n` +
            `  /images          — show count of staged images\n` +
            `  /help            — show this help\n` +
            `  exit             — end the session\n\n${RESET}`,
          );
          continue;
        }

        // ── Send message (with any staged images) ─────────────────────────
        const userMsg = buildUserMessage(trimmed, pendingImages);
        const imgNote = pendingImages.length > 0 ? ` ${DIM}[${pendingImages.length} img]${RESET}` : "";
        pendingImages = []; // consume staged images

        history.push(userMsg);
        process.stdout.write(`\n${BOLD}${GREEN}Assistant:${RESET}${imgNote} `);

        let result: { text: string; promptTokens: number; completionTokens: number; nCtx: number };
        try {
          result = await streamTurn(history as Array<{ role: string; content: string }>);
        } catch (e: any) {
          process.stdout.write(`\n${RED}Error: ${e.message}${RESET}\n\n`);
          history.pop(); // remove the user message so history stays consistent
          continue;
        }

        history.push({ role: "assistant", content: result.text });
        turnCount++;
        totalPrompt     += result.promptTokens;
        totalCompletion += result.completionTokens;

        process.stdout.write(
          `${DIM}  [${result.promptTokens}+${result.completionTokens} tokens, n_ctx=${result.nCtx}]${RESET}\n\n`,
        );
      }

      // Session summary
      rl.close();
      if (turnCount > 0) {
        process.stdout.write(
          `${DIM}─────────────────────────────────────────────────────────────\n` +
          `  session ended — ${turnCount} turn(s), ` +
          `${totalPrompt} prompt + ${totalCompletion} completion tokens\n${RESET}\n`,
        );
      } else {
        process.stdout.write(`${DIM}  (no messages sent)${RESET}\n`);
      }
      break;
    }

    // ── add ───────────────────────────────────────────────────────────────
    case "add": {
      // Supports two forms:
      //   llm add <repo> <filename>          — explicit repo + filename
      //   llm add <repo>/<filename>          — combined (split on last /)
      //   llm add <url>                      — full HF URL
      let repo: string;
      let filename: string;
      const raw = args.text;
      const addUsage =
        "usage: cli.ts llm add <repo> <filename> [--mmproj <file>]\n" +
        "       cli.ts llm add <hf-url> [--mmproj <file>]\n\n" +
        "  Examples:\n" +
        "    llm add bartowski/Phi-4-mini-reasoning-GGUF Phi-4-mini-reasoning-Q4_K_M.gguf\n" +
        "    llm add bartowski/Phi-4-mini-reasoning-GGUF Phi-4-mini-reasoning-Q4_K_M.gguf --mmproj mmproj-Phi-4-mini-reasoning-BF16.gguf\n" +
        "    llm add https://huggingface.co/bartowski/Phi-4-mini-reasoning-GGUF/blob/main/Phi-4-mini-reasoning-Q4_K_M.gguf";
      if (!raw) printError(addUsage);

      if (args.body) {
        // Two positional args: repo + filename
        repo = raw!;
        filename = args.body;
      } else if (raw!.startsWith("http://") || raw!.startsWith("https://")) {
        // Full HF URL: https://huggingface.co/<org>/<repo>/blob/main/<filename>
        // or          https://huggingface.co/<org>/<repo>/resolve/main/<filename>
        const urlMatch = raw!.match(/huggingface\.co\/([^/]+\/[^/]+)\/(?:blob|resolve)\/[^/]+\/(.+?)(?:\?.*)?$/);
        if (!urlMatch) printError(
          "could not parse HuggingFace URL. Expected format:\n" +
          "  https://huggingface.co/<org>/<repo>/blob/main/<filename>"
        );
        repo = urlMatch![1];
        filename = decodeURIComponent(urlMatch![2]);
      } else {
        printError(addUsage);
      }

      const mmproj = args.mmproj || undefined;
      const mmNote = mmproj ? ` + mmproj ${CYAN}${mmproj}${RESET}` : "";
      print(`${BOLD}🤖 llm add${RESET} ${CYAN}${repo!}${RESET} / ${CYAN}${filename!}${RESET}${mmNote}`);
      const payload: Record<string, unknown> = { command: "llm_add_model", repo: repo!, filename: filename!, download: true };
      if (mmproj) payload.mmproj = mmproj;
      const r = await send(payload);
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_add_model failed"); }
      print(`  ${GREEN}✓${RESET} added ${CYAN}${r.filename}${RESET} from ${DIM}${r.repo}${RESET}`);
      if (r.mmproj) print(`  ${GREEN}✓${RESET} mmproj ${CYAN}${r.mmproj}${RESET}`);
      print(`  ${DIM}download started — poll with: llm downloads${RESET}`);
      printResult(r);
      break;
    }

    // ── select ─────────────────────────────────────────────────────────────
    case "select": {
      const filename = args.text;
      if (!filename) printError("usage: cli.ts llm select <filename>");
      print(`${BOLD}🤖 llm select${RESET} ${CYAN}${filename}${RESET}`);
      const r = await send({ command: "llm_select_model", filename });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_select_model failed"); }
      print(`  ${GREEN}✓${RESET} active model:  ${CYAN}${r.active_model}${RESET}`);
      if (r.active_mmproj) print(`  active mmproj: ${CYAN}${r.active_mmproj}${RESET}`);
      printResult(r);
      break;
    }

    // ── mmproj ───────────────────────────────────────────────────────────────
    case "mmproj": {
      const filename = args.text;
      if (!filename) printError("usage: cli.ts llm mmproj <filename|none>");
      const actual = filename!.toLowerCase() === "none" ? "" : filename!;
      print(`${BOLD}🤖 llm mmproj${RESET} ${CYAN}${filename}${RESET}`);
      const r = await send({ command: "llm_select_mmproj", filename: actual });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_select_mmproj failed"); }
      print(`  ${GREEN}✓${RESET} active mmproj: ${CYAN}${r.active_mmproj || "(none)"}${RESET}`);
      print(`  active model:  ${CYAN}${r.active_model || "(none)"}${RESET}`);
      printResult(r);
      break;
    }

    // ── autoload-mmproj ──────────────────────────────────────────────────────
    case "autoload-mmproj": {
      const val = args.text?.toLowerCase();
      if (!val || !["on", "off", "true", "false", "1", "0"].includes(val)) {
        printError("usage: cli.ts llm autoload-mmproj <on|off>");
      }
      const enabled = ["on", "true", "1"].includes(val!);
      print(`${BOLD}🤖 llm autoload-mmproj${RESET} ${enabled ? GREEN + "on" : GRAY + "off"}${RESET}`);
      const r = await send({ command: "llm_set_autoload_mmproj", enabled });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_set_autoload_mmproj failed"); }
      print(`  ${GREEN}✓${RESET} autoload_mmproj = ${enabled ? "on" : "off"}`);
      printResult(r);
      break;
    }

    // ── pause ────────────────────────────────────────────────────────────────
    case "pause": {
      const filename = args.text;
      if (!filename) printError("usage: cli.ts llm pause <filename>");
      print(`${BOLD}🤖 llm pause${RESET} ${CYAN}${filename}${RESET}`);
      const r = await send({ command: "llm_pause_download", filename });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_pause_download failed"); }
      print(`  ${GREEN}✓${RESET} pause signalled for ${CYAN}${r.filename}${RESET}`);
      printResult(r);
      break;
    }

    // ── resume ───────────────────────────────────────────────────────────────
    case "resume": {
      const filename = args.text;
      if (!filename) printError("usage: cli.ts llm resume <filename>");
      print(`${BOLD}🤖 llm resume${RESET} ${CYAN}${filename}${RESET}`);
      const r = await send({ command: "llm_resume_download", filename });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_resume_download failed"); }
      print(`  ${GREEN}✓${RESET} resume signalled for ${CYAN}${r.filename}${RESET}`);
      printResult(r);
      break;
    }

    // ── downloads ────────────────────────────────────────────────────────────
    case "downloads": {
      print(`${BOLD}🤖 llm downloads${RESET}`);
      const r = await send({ command: "llm_downloads" });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_downloads failed"); }
      const items: any[] = r.downloads ?? [];
      if (items.length === 0) {
        print(`  ${GRAY}(no downloads)${RESET}`);
      } else {
        for (const d of items) {
          const stateColor = d.state === "downloaded" ? GREEN
            : d.state === "downloading" ? YELLOW
            : d.state === "paused"      ? BLUE
            : d.state === "failed"      ? RED
            : d.state === "cancelled"   ? GRAY
            : GRAY;
          const pct = d.state === "downloading" ? ` ${Math.round((d.progress ?? 0) * 100)}%` : "";
          const sizeNote = d.size_gb ? ` ${DIM}(${d.size_gb.toFixed(1)} GB)${RESET}` : "";
          print(`  ${stateColor}${(d.state ?? "unknown").padEnd(13)}${RESET}${pct}  ${CYAN}${d.filename}${RESET}${sizeNote}`);
          if (d.status_msg) print(`                 ${DIM}${d.status_msg}${RESET}`);
        }
      }
      print(`  ${DIM}${items.length} download(s)${RESET}`);
      printResult(r);
      break;
    }

    // ── refresh ──────────────────────────────────────────────────────────────
    case "refresh": {
      print(`${BOLD}🤖 llm refresh${RESET}`);
      const r = await send({ command: "llm_refresh_catalog" });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_refresh_catalog failed"); }
      print(`  ${GREEN}✓${RESET} catalog refreshed`);
      printResult(r);
      break;
    }

    // ── fit ──────────────────────────────────────────────────────────────────
    case "fit": {
      print(`${BOLD}🤖 llm hardware fit${RESET}`);
      const r = await send({ command: "llm_hardware_fit" });
      if (!r.ok) { printResult(r); printError(r.error ?? "llm_hardware_fit failed"); }
      const fits: any[] = r.fits ?? [];
      if (fits.length === 0) {
        print(`  ${GRAY}(no models in catalog)${RESET}`);
      } else {
        // Optional filter by filename
        const filtered = args.text
          ? fits.filter((f: any) => f.filename.toLowerCase().includes(args.text!.toLowerCase()))
          : fits;
        const fitColor = (level: string) =>
          level === "perfect" ? GREEN
          : level === "good" ? GREEN
          : level === "marginal" ? YELLOW
          : RED;
        for (const f of filtered) {
          print(`  ${fitColor(f.fit_level)}${f.fit_level.padEnd(10)}${RESET} ${CYAN}${f.run_mode.padEnd(8)}${RESET} ${f.memory_required_gb?.toFixed(1) ?? "?"}GB  ${CYAN}${f.filename}${RESET}`);
        }
        print(`  ${DIM}${filtered.length} model(s)${RESET}`);
      }
      printResult(r);
      break;
    }

    default:
      printError(`unknown llm subcommand: "${sub}". Valid: status start stop catalog add select mmproj autoload-mmproj download pause resume cancel delete downloads refresh fit logs chat`);
  }
}

async function cmdRaw(rawJson: string): Promise<void> {
  let cmd: any;
  try {
    cmd = JSON.parse(rawJson);
  } catch {
    printError(`invalid JSON: ${rawJson}`);
  }
  if (!cmd.command) printError("JSON must have a \"command\" field");

  print(`${BOLD}⚡ raw${RESET} ${DIM}${cmd.command}${RESET}`);
  const r = await send(cmd, 60000);
  printResult(r);
}

// ── Main ──────────────────────────────────────────────────────────────────────

/**
 * CLI entry point.
 *
 * 1. Parse command-line arguments.
 * 2. Discover the NeuroSkill™ WebSocket server (mDNS → lsof → explicit port).
 * 3. Connect via WebSocket (up to 3 retries).
 * 4. Dispatch to the appropriate command handler.
 * 5. Close the socket and exit.
 *
 * A 10-minute global timeout kills the process if anything hangs.
 */
async function main(): Promise<void> {
  const args = parseArgs();
  jsonMode = args.json;
  fullMode = args.full;

  // Apply NO_COLOR / --no-color / non-TTY after all flags are parsed.
  // jsonMode also implies no-color (JSON output has no ANSI codes anyway).
  if (noColorMode || jsonMode) applyNoColor();

  if (args.command === "version") {
    console.log(`cli.ts ${CLI_VERSION}`);
    process.exit(0);
  }
  if (args.command === "help") showHelp();
  if (!args.command) {
    if (jsonMode) printError("no command specified");
    showHelp();
  }

  const port = await discover(args.port);
  httpBase = `http://127.0.0.1:${port}`;

  if (args.http) {
    // ── Forced HTTP ───────────────────────────────────────────────────────
    transport = "http";
    send = sendHttp;
    printInfo(`transport: HTTP ${httpBase}`);
  } else if (args.ws) {
    // ── Forced WebSocket (retry on failure, fatal if unreachable) ─────────
    transport = "ws";
    await connect(port);
    printInfo(`transport: WebSocket ws://127.0.0.1:${port}`);
  } else {
    // ── Auto: try WebSocket first, silently fall back to HTTP ─────────────
    printInfo("auto-transport: probing WebSocket…");
    const wsOk = await tryConnectOnce(port);
    if (wsOk) {
      transport = "ws";
      printInfo(`transport: WebSocket ws://127.0.0.1:${port}`);
    } else {
      transport = "http";
      send = sendHttp;
      printInfo(`WebSocket unavailable — transport: HTTP ${httpBase}`);
    }
  }

  try {
    switch (args.command) {
      case "status":
        await cmdStatus(args);
        break;
      case "session":
        await cmdSession(args);
        break;
      case "sessions":
        await cmdSessions(args);
        break;
      case "say":
        if (!args.text) printError("usage: cli.ts say \"text to speak\" [--voice <name>]");
        await cmdSay(args.text!, args.voice);
        break;
      case "notify":
        if (!args.text) printError("usage: cli.ts notify \"title\" [\"body\"]");
        await cmdNotify(args.text!, args.body);
        break;
      case "calibrations":
        await cmdCalibrations(args);
        break;
      case "calibrate":
        await cmdCalibrate(args.profile);
        break;
      case "timer":
        await cmdTimer();
        break;
      case "dnd":
        await cmdDnd(args);
        break;
      case "label":
        if (!args.text) printError("usage: cli.ts label \"your annotation text\"");
        await cmdLabel(args);
        break;
      case "search-labels":
        await cmdSearchLabels(args);
        break;
      case "interactive":
        await cmdInteractive(args);
        break;
      case "search":
        await cmdSearch(args);
        break;
      case "compare":
        await cmdCompare(args);
        break;
      case "sleep":
        await cmdSleep(args);
        break;
      case "umap":
        await cmdUmap(args);
        break;
      case "listen":
        if (transport === "http") {
          printError(
            "listen requires WebSocket (HTTP has no push events).\n" +
            "  Use --ws to force WebSocket, or omit --http for auto-transport."
          );
        }
        await cmdListen(args.seconds ?? 5);
        break;
      case "hooks":
        await cmdHooks(args);
        break;
      case "llm":
        await cmdLlm(args);
        break;
      case "raw":
        if (!args.rawJson) printError("usage: cli.ts raw '{\"command\":\"status\"}'");
        await cmdRaw(args.rawJson!);
        break;
      default:
        printError(`unknown command: "${args.command}". Run with --help to see available commands.`);
    }
  } finally {
    shutdown();
  }
}

/**
 * Tear down all resources so Node exits immediately after output is flushed.
 *
 * Without this, the process hangs because:
 * - The WebSocket keeps the event loop alive even after `.close()`.
 * - The global safety timeout (`setTimeout(…, 600_000)`) prevents exit.
 * - Any lingering `send()` timeout timers hold references.
 *
 * We terminate the socket, clear the timer, and call `process.exit(0)`.
 */
function shutdown(code = 0): void {
  if (transport === "ws") { try { ws?.terminate(); } catch {} }
  clearTimeout(globalTimer);
  process.exit(code);
}

globalTimer = setTimeout(() => printError("global timeout (10 min)"), 600_000);
main().catch((e: any) => printError(e.message));
