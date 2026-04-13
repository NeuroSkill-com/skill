#!/usr/bin/env npx tsx
// SPDX-License-Identifier: GPL-3.0-only
// scripts/daemon.ts — Start the skill-daemon with nice DX.
//
// Usage:
//   npm run daemon                  # build + start on default port (18444)
//   npm run daemon -- --port 9000   # custom port
//   npm run daemon -- --no-build    # skip cargo build
//   npm run daemon -- --force       # kill ALL daemon instances first
//   npm run daemon -- --release     # build in release mode (default)
//   npm run daemon -- --debug       # build in debug mode
//   npm run daemon -- --clean       # fresh data dir (temp, wiped on exit)
//   npm run daemon -- --help

import { execSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { homedir, platform, tmpdir } from "node:os";
import { join } from "node:path";

// ── Colours ──────────────────────────────────────────────────────────────────

const no = process.env.NO_COLOR || !process.stdout.isTTY;
const B = no ? "" : "\x1b[1m";
const D = no ? "" : "\x1b[2m";
const R = no ? "" : "\x1b[0m";
const G = no ? "" : "\x1b[32m";
const Y = no ? "" : "\x1b[33m";
const C = no ? "" : "\x1b[36m";
const RED = no ? "" : "\x1b[31m";

function log(msg: string) {
  console.log(`  ${msg}`);
}
function ok(msg: string) {
  log(`${G}✓${R} ${msg}`);
}
function warn(msg: string) {
  log(`${Y}!${R} ${msg}`);
}
function err(msg: string) {
  log(`${RED}✗${R} ${msg}`);
}
function info(msg: string) {
  log(`${D}${msg}${R}`);
}

// ── Args ─────────────────────────────────────────────────────────────────────

interface Opts {
  host: string;
  port: number;
  build: boolean;
  release: boolean;
  force: boolean;
  clean: boolean;
  sign: boolean;
  help: boolean;
  embed: boolean;
  virtual: boolean;
  cpu: boolean;
}

function parseArgs(): Opts {
  const opts: Opts = {
    host: "127.0.0.1",
    port: 18444,
    build: true,
    release: true,
    force: false,
    clean: false,
    sign: true,
    help: false,
    embed: false,
    virtual: false,
    cpu: false,
  };
  const args = process.argv.slice(2);
  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--port":
      case "-p":
        opts.port = parseInt(args[++i], 10);
        if (!opts.port || opts.port < 1 || opts.port > 65535) {
          err(`invalid port: ${args[i]}`);
          process.exit(1);
        }
        break;
      case "--host":
        opts.host = args[++i];
        break;
      case "--no-build":
        opts.build = false;
        break;
      case "--debug":
        opts.release = false;
        break;
      case "--release":
        opts.release = true;
        break;
      case "--force":
      case "-f":
        opts.force = true;
        break;
      case "--clean":
        opts.clean = true;
        break;
      case "--no-sign":
        opts.sign = false;
        break;
      case "--embed":
        opts.embed = true;
        break;
      case "--virtual":
        opts.virtual = true;
        opts.embed = true;
        break;
      case "--cpu":
        opts.cpu = true;
        break;
      case "--help":
      case "-h":
        opts.help = true;
        break;
      default:
        err(`unknown option: ${args[i]}`);
        process.exit(1);
    }
  }
  return opts;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const ROOT = join(import.meta.dirname, "..");

function daemonBin(release: boolean): string {
  const profile = release ? "release" : "debug";
  return join(ROOT, "src-tauri", "target", profile, "skill-daemon");
}

function findDaemonPids(): number[] {
  try {
    if (platform() === "win32") {
      const out = execSync('tasklist /FI "IMAGENAME eq skill-daemon.exe" /FO CSV /NH', { encoding: "utf8" });
      return [...out.matchAll(/"skill-daemon\.exe","(\d+)"/gi)].map((m) => parseInt(m[1], 10));
    }
    const out = execSync("pgrep -f skill-daemon", { encoding: "utf8" }).trim();
    return out
      ? out
          .split("\n")
          .map(Number)
          .filter((p) => p > 0 && p !== process.pid)
      : [];
  } catch {
    return [];
  }
}

function findPortPids(port: number): number[] {
  try {
    if (platform() === "win32") return [];
    const out = execSync(`lsof -ti :${port}`, { encoding: "utf8" }).trim();
    return out
      ? out
          .split("\n")
          .map(Number)
          .filter((p) => p > 0)
      : [];
  } catch {
    return [];
  }
}

function killPids(pids: number[], label: string): void {
  for (const pid of pids) {
    try {
      process.kill(pid, "SIGTERM");
      info(`killed ${label} (PID ${pid})`);
    } catch {
      /* already dead */
    }
  }
}

function tokenPath(): string {
  if (platform() === "darwin")
    return join(homedir(), "Library", "Application Support", "skill", "daemon", "auth.token");
  if (platform() === "win32")
    return join(process.env.APPDATA || join(homedir(), "AppData", "Roaming"), "skill", "daemon", "auth.token");
  return join(process.env.XDG_CONFIG_HOME || join(homedir(), ".config"), "skill", "daemon", "auth.token");
}

function waitForHealthz(port: number, timeoutMs = 15000): Promise<boolean> {
  const start = Date.now();
  return new Promise((resolve) => {
    const check = () => {
      fetch(`http://127.0.0.1:${port}/healthz`, { signal: AbortSignal.timeout(1000) })
        .then((r) => {
          if (r.ok) resolve(true);
          else retry();
        })
        .catch(retry);
    };
    const retry = () => {
      if (Date.now() - start > timeoutMs) {
        resolve(false);
        return;
      }
      setTimeout(check, 300);
    };
    check();
  });
}

// ── Main ─────────────────────────────────────────────────────────────────────

async function main() {
  const opts = parseArgs();

  if (opts.help) {
    console.log(`
${B}skill-daemon${R} — start the NeuroSkill daemon

${B}Usage:${R}
  npm run daemon                     build + start (${C}127.0.0.1:18444${R})
  npm run daemon -- --port 9000      custom port
  npm run daemon -- --host 0.0.0.0   listen on all interfaces (LAN)
  npm run daemon -- --no-build       skip cargo build
  npm run daemon -- --force          kill ALL running daemon instances
  npm run daemon -- --debug          build debug profile
  npm run daemon -- --clean          fresh temp data dir (wiped on exit)
  npm run daemon -- --virtual        start virtual EEG device (LSL)
  npm run daemon -- --embed          enable EXG embeddings for virtual EEG
  npm run daemon -- --cpu            force LLM inference on CPU (default: GPU)
  npm run daemon -- --no-sign        skip macOS codesigning

${B}Examples:${R}
  npm run daemon                     ${D}# dev workflow — build + run${R}
  npm run daemon -- --no-build -f    ${D}# restart quickly${R}
  npm run daemon -- --virtual        ${D}# with virtual EEG for testing${R}
  npm run daemon -- --host 0.0.0.0   ${D}# expose on LAN${R}
  npm run daemon -- --clean --embed  ${D}# clean e2e-style run${R}
  npm run daemon -- -p 9000 --debug  ${D}# debug build on alt port${R}
`);
    process.exit(0);
  }

  console.log(`\n${B}skill-daemon${R} ${D}${opts.host}:${C}${opts.port}${R}\n`);

  // ── Kill existing ────────────────────────────────────────────────────────

  if (opts.force) {
    const pids = findDaemonPids();
    if (pids.length) {
      killPids(pids, "daemon");
      await new Promise((r) => setTimeout(r, 500));
      ok(`killed ${pids.length} daemon instance(s)`);
    } else {
      info("no running daemon instances found");
    }
  } else {
    // Just check the port
    const portPids = findPortPids(opts.port);
    if (portPids.length) {
      warn(`port ${opts.port} in use (PID ${portPids.join(", ")})`);
      log(`  use ${C}--force${R} to kill, or ${C}--port <n>${R} for a different port`);
      process.exit(1);
    }
  }

  // ── Build ────────────────────────────────────────────────────────────────

  const bin = daemonBin(opts.release);

  if (opts.build) {
    const profile = opts.release ? "release" : "dev";
    const flags = opts.release ? "--release" : "";
    log(`building ${D}(${profile})${R}...`);
    try {
      execSync(`cargo build ${flags} -p skill-daemon`.trim(), {
        cwd: ROOT,
        stdio: ["ignore", "pipe", "inherit"],
        encoding: "utf8",
      });
      ok("build complete");
    } catch {
      err("build failed");
      process.exit(1);
    }

    // Codesign on macOS
    if (opts.sign && platform() === "darwin") {
      try {
        const ids = execSync("security find-identity -v -p codesigning", { encoding: "utf8" });
        if (ids.includes("NeuroSkill Dev")) {
          execSync(`codesign -s "NeuroSkill Dev" -f "${bin}"`, { stdio: "ignore" });
          ok("codesigned");
        }
      } catch {
        /* non-fatal */
      }
    }
  }

  if (!existsSync(bin)) {
    err(`binary not found: ${bin}`);
    log(`  run without ${C}--no-build${R} to compile first`);
    process.exit(1);
  }

  // ── Data dir ─────────────────────────────────────────────────────────────

  let dataDir: string | undefined;
  if (opts.clean) {
    dataDir = join(tmpdir(), `skill-daemon-${Date.now()}`);
    mkdirSync(dataDir, { recursive: true });
    info(`clean data dir: ${dataDir}`);
  }

  // ── Env ──────────────────────────────────────────────────────────────────

  const env: Record<string, string> = { ...process.env } as Record<string, string>;
  if (dataDir) env.SKILL_DATA_DIR = dataDir;
  if (opts.embed) env.SKILL_VIRTUAL_EMBED = "1";
  env.SKILL_DAEMON_ADDR = `${opts.host}:${opts.port}`;

  // ── Start ────────────────────────────────────────────────────────────────

  log("starting...");

  const child = spawn(bin, [], {
    env,
    stdio: ["ignore", "pipe", "pipe"],
    detached: false,
  });

  let exited = false;
  child.on("exit", (code, signal) => {
    exited = true;
    if (signal === "SIGTERM" || signal === "SIGINT") {
      info("daemon stopped");
    } else if (code !== 0) {
      err(`daemon exited with code ${code}`);
    }
    // Clean up temp dir
    if (dataDir) {
      try {
        rmSync(dataDir, { recursive: true, force: true });
      } catch {}
      info(`cleaned up ${dataDir}`);
    }
  });

  // Forward daemon stderr (logs) with prefix
  child.stderr?.on("data", (chunk: Buffer) => {
    for (const line of chunk.toString().split("\n")) {
      if (line.trim()) console.log(`  ${D}│${R} ${line}`);
    }
  });

  // Suppress stdout (healthz probes etc.)
  child.stdout?.resume();

  // Wait for healthz
  const ready = await waitForHealthz(opts.port);
  if (!ready) {
    if (exited) {
      err("daemon exited before becoming ready");
    } else {
      err(`daemon did not respond on port ${opts.port} within 15s`);
      child.kill("SIGTERM");
    }
    process.exit(1);
  }

  // ── Ready ────────────────────────────────────────────────────────────────

  // Re-read token fresh after daemon start (it may have been regenerated)
  const tp = tokenPath();
  const token = existsSync(tp) ? readFileSync(tp, "utf8").trim() : "";
  const apiBase = `http://127.0.0.1:${opts.port}`;

  function api(path: string, method = "GET", body?: string) {
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (token) headers.Authorization = `Bearer ${token}`;
    return fetch(`${apiBase}${path}`, {
      method,
      headers,
      body,
      signal: AbortSignal.timeout(10000),
    });
  }

  // ── Virtual EEG ──────────────────────────────────────────────────────────

  if (opts.virtual) {
    try {
      // Wait for auto-connect to finish its BLE scan (~6s after boot),
      // then cancel and take over
      info("waiting for daemon init to settle...");
      await new Promise((r) => setTimeout(r, 6000));
      await api("/v1/control/cancel-session", "POST", "{}").catch(() => {});
      await api("/v1/control/disable-reconnect", "POST", "{}").catch(() => {});
      await new Promise((r) => setTimeout(r, 500));

      // Start virtual LSL source
      const vRes = await api("/v1/lsl/virtual-source/start", "POST", "{}");
      if (!vRes.ok) {
        warn(`virtual source start failed: HTTP ${vRes.status} ${await vRes.text()}`);
      } else {
        const vJson = (await vRes.json()) as { running?: boolean };
        if (vJson.running) {
          ok("virtual EEG source started (LSL: SkillVirtualEEG, 32ch @ 256Hz)");
        } else {
          warn(`virtual source response: ${JSON.stringify(vJson)}`);
        }
      }

      // Wait for LSL stream to be discoverable
      await new Promise((r) => setTimeout(r, 1500));

      // Discover
      const dRes = await api("/v1/lsl/discover");
      if (!dRes.ok) {
        warn(`LSL discover failed: HTTP ${dRes.status}`);
      } else {
        const streams = (await dRes.json()) as { name?: string; source_id?: string }[];
        const vStream = (Array.isArray(streams) ? streams : []).find((s: { name?: string }) =>
          String(s.name || "").includes("Virtual"),
        );
        if (vStream) {
          // Pair
          const pRes = await api(
            "/v1/lsl/pair",
            "POST",
            JSON.stringify({
              sourceId: vStream.source_id || "skill-virtual-eeg-001",
              name: "SkillVirtualEEG",
              streamType: "EEG",
              channels: 32,
              sampleRate: 256,
            }),
          );
          if (pRes.ok) {
            ok("virtual stream paired");
          } else {
            warn(`pair failed: HTTP ${pRes.status}`);
          }

          // Start recording session
          const sRes = await api(
            "/v1/control/start-session",
            "POST",
            JSON.stringify({ target: "lsl:SkillVirtualEEG" }),
          );
          if (sRes.ok) {
            ok("recording session started (lsl:SkillVirtualEEG)");
          } else {
            warn(`session start failed: HTTP ${sRes.status}`);
          }
        } else {
          warn(`virtual stream not found in LSL discover (got ${streams?.length ?? 0} streams)`);
        }
      }
    } catch (e) {
      warn(`virtual EEG setup failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  // ── Inference device (CPU override) ────────────────────────────────────────

  if (opts.cpu) {
    try {
      const devRes = await api("/v1/settings/inference-device", "POST", JSON.stringify({ device: "cpu" }));
      if (devRes.ok) {
        ok("inference device set to CPU");
      } else {
        warn(`failed to set inference device: HTTP ${devRes.status}`);
      }
    } catch (e) {
      warn(`inference device override failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  // ── LLM auto-select (clean env) ───────────────────────────────────────────

  if (opts.clean) {
    try {
      info("refreshing LLM catalog from HF cache...");
      await api("/v1/llm/catalog/refresh", "POST", "{}");

      const catRes = await api("/v1/llm/catalog");
      if (catRes.ok) {
        interface CatalogEntry {
          filename?: string;
          params_b?: number;
          state?: string;
        }
        const catalog = (await catRes.json()) as { entries?: CatalogEntry[] };
        const entries: CatalogEntry[] = catalog.entries || [];
        const ready = entries.filter((e: CatalogEntry) => e.state === "ready");

        if (ready.length > 0) {
          // Prefer Q4_K_M quant, then smallest param count
          ready.sort((a: CatalogEntry, b: CatalogEntry) => {
            const aQ4 = (a.filename || "").includes("Q4_K_M") ? 0 : 1;
            const bQ4 = (b.filename || "").includes("Q4_K_M") ? 0 : 1;
            if (aQ4 !== bQ4) return aQ4 - bQ4;
            return (a.params_b || 999) - (b.params_b || 999);
          });

          const best = ready[0];
          info(`selecting model: ${best.filename} (${best.params_b ?? "?"}B params)`);

          const selRes = await api(
            "/v1/llm/selection/active-model",
            "POST",
            JSON.stringify({ filename: best.filename }),
          );
          if (selRes.ok) {
            ok(`model selected: ${best.filename}`);

            const startRes = await api("/v1/llm/server/start", "POST", "{}");
            if (startRes.ok) {
              ok("LLM server starting");
            } else {
              warn(`LLM server start failed: HTTP ${startRes.status}`);
            }
          } else {
            warn(`model select failed: HTTP ${selRes.status}`);
          }
        } else {
          info("no ready GGUF models found in HF cache");
        }
      }
    } catch (e) {
      warn(`LLM auto-select failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  console.log();
  ok(`daemon ready on ${C}${apiBase}${R}`);
  if (opts.host === "0.0.0.0") {
    info(`listening on all interfaces (LAN-accessible)`);
  }
  if (token) {
    ok(`auth token: ${D}${tp}${R}`);
  }
  if (dataDir) {
    info(`data: ${dataDir}`);
  }
  if (opts.embed) {
    info("virtual EEG embeddings enabled");
  }
  console.log();
  info(`${D}press ${B}ctrl+c${R}${D} to stop${R}`);
  console.log();

  // ── Graceful shutdown ────────────────────────────────────────────────────

  const shutdown = () => {
    if (!exited) {
      log("\nshutting down...");
      child.kill("SIGTERM");
      // Give it 3s to stop gracefully
      setTimeout(() => {
        if (!exited) {
          child.kill("SIGKILL");
        }
      }, 3000);
    }
  };

  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

main().catch((e) => {
  err(e.message);
  process.exit(1);
});
