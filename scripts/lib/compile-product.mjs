// SPDX-License-Identifier: GPL-3.0-only
//
// Shared product compile recipe: skill-daemon (OS umbrella) → optional
// skill-tty → skill (custom-protocol). Used by release CI, dry-run,
// prepare-daemon-sidecar, and tauri-build so feature flags cannot drift.
//
// OS umbrellas (exactly one):
//   apple   — Metal + MLX
//   linux   — CUDA + wgpu (runtime: CUDA → wgpu → CPU)
//   windows — CUDA + wgpu (runtime: CUDA → wgpu → CPU)
//
// rlx-cuda uses cudarc dynamic-loading — no local CUDA toolkit is required
// to compile. Missing NVIDIA drivers are a runtime concern, not a build flavor.

import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { detectHostTriple, resolveTargetTriple } from "./target-triples.mjs";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** @typedef {"apple" | "linux" | "windows"} DaemonOsFeature */

/**
 * Map a Rust target triple to the skill-daemon OS umbrella feature.
 * @param {string} triple
 * @returns {DaemonOsFeature}
 */
export function daemonOsFeature(triple) {
  const t = String(triple || "");

  if (t.includes("apple-darwin")) return "apple";
  if (t.includes("linux")) return "linux";
  if (t.includes("windows")) return "windows";

  throw new Error(
    `Cannot map target triple "${t || "(empty)"}" to a skill-daemon OS feature ` +
      `(expected *apple-darwin* / *linux* / *windows*). Pass an explicit product target.`,
  );
}

/**
 * Resolve Cargo's target directory (honors CARGO_TARGET_DIR; falls back to
 * workspace config via `cargo metadata`, then src-tauri/target).
 * @returns {string}
 */
export function resolveCargoTargetDir() {
  if (process.env.CARGO_TARGET_DIR) {
    return resolve(process.env.CARGO_TARGET_DIR);
  }
  const meta = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps"],
    {
      cwd: REPO_ROOT,
      encoding: "utf8",
      env: process.env,
      shell: false,
    },
  );
  if (meta.status === 0 && meta.stdout) {
    try {
      const json = JSON.parse(meta.stdout);
      if (json.target_directory) return resolve(json.target_directory);
    } catch {
      // fall through
    }
  }
  return resolve(REPO_ROOT, "src-tauri", "target");
}

/**
 * @param {string} cmd
 * @param {string[]} args
 * @param {{ cwd?: string, env?: NodeJS.ProcessEnv, capture?: boolean }} [opts]
 */
function runCargo(cmd, args, opts = {}) {
  const result = spawnSync(cmd, args, {
    cwd: opts.cwd ?? REPO_ROOT,
    stdio: opts.capture ? "pipe" : "inherit",
    encoding: opts.capture ? "utf8" : undefined,
    env: { ...process.env, ...opts.env },
    shell: false,
  });
  if (result.error) throw result.error;
  if (typeof result.status === "number" && result.status !== 0) {
    const err = new Error(`Command failed (exit ${result.status}): ${cmd} ${args.join(" ")}`);
    err.status = result.status;
    err.stdout = result.stdout;
    err.stderr = result.stderr;
    throw err;
  }
  return result;
}

/**
 * Post-build proof that the selected OS umbrella pulled the expected LLM
 * backends into `skill-llm` (package names are stable in `cargo tree`;
 * intermediate Cargo feature names like `llm-rlx-metal` are not).
 * @param {DaemonOsFeature} osFeature
 * @param {string} triple
 */
export function verifyDaemonFeatures(osFeature, triple) {
  /** @type {Record<DaemonOsFeature, string[]>} */
  const expected = {
    apple: ["rlx-metal ", "rlx-mlx "],
    linux: ["rlx-cuda ", "rlx-wgpu "],
    windows: ["rlx-cuda ", "rlx-wgpu "],
  };
  const want = expected[osFeature];
  if (!want) {
    throw new Error(`No feature-proof spec for OS umbrella "${osFeature}"`);
  }
  // Probe skill-llm alone so TTS / other crates that may pull extra backends
  // don't mask the LLM umbrella.
  const args = [
    "tree",
    "-p",
    "skill-llm",
    "--features",
    osFeature,
    "--prefix",
    "none",
    "--target",
    triple,
  ];
  const out = runCargo("cargo", args, { capture: true });
  // stdout only — stderr can mention sibling-overlay duplicate package names.
  const text = out.stdout || "";
  const missing = want.filter((p) => !text.includes(p));
  if (missing.length) {
    throw new Error(
      `Feature proof failed for skill-llm --features ${osFeature}: ` +
        `missing [${missing.map((s) => s.trim()).join(", ")}]`,
    );
  }
  console.log(
    `[compile-product] feature proof ok (${osFeature}: ${want.map((s) => s.trim()).join(", ")})`,
  );
}

/**
 * Compile NeuroSkill product crates with a single, consistent feature recipe.
 *
 * @param {{
 *   target?: string | null,
 *   release?: boolean,
 *   locked?: boolean,
 *   timings?: boolean,
 *   daemon?: boolean,
 *   tty?: boolean | "auto",
 *   app?: boolean,
 *   stageSidecar?: boolean,
 *   verifyFeatures?: boolean,
 *   env?: NodeJS.ProcessEnv,
 *   help?: boolean,
 * }} [opts]
 */
export function compileProduct(opts = {}) {
  const release = opts.release !== false;
  const locked = Boolean(opts.locked);
  const timings = Boolean(opts.timings);
  const buildDaemon = opts.daemon !== false;
  const buildApp = opts.app !== false;
  const stageSidecar = Boolean(opts.stageSidecar);
  const verifyFeatures = opts.verifyFeatures ?? (locked || release);

  const rawTarget =
    opts.target ?? process.env.CARGO_BUILD_TARGET ?? process.env.TAURI_TARGET ?? null;
  const resolved = resolveTargetTriple(rawTarget || detectHostTriple() || undefined);
  const triple = resolved.triple;
  if (resolved.profile !== "default") {
    process.env.SKILL_MAC_PROFILE = resolved.profile;
  }

  const osFeature = daemonOsFeature(triple);
  const ttyOpt = opts.tty ?? "auto";
  const buildTty = ttyOpt === "auto" ? !triple.includes("windows") : Boolean(ttyOpt);

  /**
   * @param {string[]} extra
   * @param {{ timings?: boolean }} [flags]
   */
  function baseArgs(extra, flags = {}) {
    const args = ["build", ...extra];
    if (release) args.push("--release");
    if (locked) args.push("--locked");
    if (flags.timings) args.push("--timings");
    if (triple) args.push("--target", triple);
    return args;
  }

  console.log(
    `[compile-product] target=${triple} os=${osFeature} release=${release} ` +
      `locked=${locked} timings=${timings}(daemon-only) ` +
      `daemon=${buildDaemon} tty=${buildTty} app=${buildApp}`,
  );

  if (buildDaemon) {
    // Never pass --no-default-features: OS umbrellas include `product`, but
    // callers must not strip defaults accidentally in ad-hoc cargo commands.
    runCargo(
      "cargo",
      baseArgs(["-p", "skill-daemon", "--features", osFeature], { timings }),
      { env: opts.env },
    );
    if (verifyFeatures) {
      verifyDaemonFeatures(osFeature, triple);
    }
  }

  if (buildTty) {
    runCargo("cargo", baseArgs(["-p", "skill-tty"]), { env: opts.env });
  }

  if (buildApp) {
    runCargo("cargo", baseArgs(["-p", "skill", "--features", "custom-protocol"]), {
      env: opts.env,
    });
  }

  if (stageSidecar && buildDaemon) {
    stageDaemonSidecars({ triple, release, includeTty: buildTty });
  }

  return {
    triple,
    osFeature,
    release,
    built: { daemon: buildDaemon, tty: buildTty, app: buildApp },
  };
}

/**
 * Copy built daemon (and tty) into src-tauri/binaries/ for Tauri sidecar wiring.
 * @param {{ triple: string, release: boolean, includeTty: boolean, targetDir?: string }} opts
 */
export function stageDaemonSidecars({ triple, release, includeTty, targetDir }) {
  const cargoTarget = targetDir ?? resolveCargoTargetDir();
  const binDir = resolve(REPO_ROOT, "src-tauri", "binaries");
  const profileDir = release ? "release" : "debug";
  const ext = triple.includes("windows") ? ".exe" : "";
  const crates = ["skill-daemon"];
  if (includeTty) crates.push("skill-tty");

  mkdirSync(binDir, { recursive: true });

  for (const name of crates) {
    const candidates = [
      triple ? resolve(cargoTarget, triple, profileDir, `${name}${ext}`) : null,
      resolve(cargoTarget, profileDir, `${name}${ext}`),
    ].filter(Boolean);

    const src = candidates.find((p) => existsSync(p));
    if (!src) {
      throw new Error(
        `${name} binary not found after compile-product (looked in ${candidates.join(", ")})`,
      );
    }

    const dst = resolve(binDir, `${name}-${triple}${ext}`);
    copyFileSync(src, dst);
    try {
      chmodSync(dst, 0o755);
    } catch {
      // Windows may ignore chmod.
    }

    const outDir = triple
      ? resolve(cargoTarget, triple, profileDir)
      : resolve(cargoTarget, profileDir);
    const size = statSync(src).size;
    console.log(
      `[compile-product] staged ${name} → ${dst} (${(size / (1024 * 1024)).toFixed(1)} MiB; also in ${outDir})`,
    );
  }
}

/**
 * @param {string[]} argv
 */
export function parseCompileProductArgs(argv) {
  /** @type {Parameters<typeof compileProduct>[0]} */
  const opts = {
    release: true,
    locked: false,
    timings: false,
    daemon: true,
    tty: "auto",
    app: true,
    stageSidecar: false,
  };

  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--target") opts.target = argv[++i];
    else if (a === "--release") opts.release = true;
    else if (a === "--debug" || a === "--dev") opts.release = false;
    else if (a === "--locked") opts.locked = true;
    else if (a === "--timings") opts.timings = true;
    else if (a === "--skip-app") opts.app = false;
    else if (a === "--skip-daemon") opts.daemon = false;
    else if (a === "--skip-tty") opts.tty = false;
    else if (a === "--with-tty") opts.tty = true;
    else if (a === "--stage-sidecar") opts.stageSidecar = true;
    else if (a === "--no-verify-features") opts.verifyFeatures = false;
    else if (a === "--verify-features") opts.verifyFeatures = true;
    else if (a === "--help" || a === "-h") opts.help = true;
    else throw new Error(`Unknown compile-product flag: ${a}`);
  }
  return opts;
}

export function printCompileProductHelp() {
  console.log(`
Usage: node scripts/compile-product.mjs [options]
       node scripts/ci.mjs compile-product [options]

Build skill-daemon (OS umbrella) → skill-tty (non-Windows targets) → skill
(custom-protocol). Linux/Windows compile CUDA + wgpu; missing NVIDIA drivers
fall back at runtime (CUDA → wgpu → CPU).

Options:
  --target <triple>     Rust target (default: host / CARGO_BUILD_TARGET)
  --release             Release profile (default)
  --debug / --dev       Debug profile
  --locked              Pass --locked to cargo
  --timings             Pass --timings only on the daemon build
  --skip-app            Do not build the Tauri app crate
  --skip-daemon         Do not build skill-daemon
  --skip-tty            Do not build skill-tty
  --with-tty            Force skill-tty even on Windows targets
  --stage-sidecar       Copy daemon/tty into src-tauri/binaries/
  --verify-features     cargo-tree proof of OS backends (default on release)
  --no-verify-features  Skip feature proof
`);
}
