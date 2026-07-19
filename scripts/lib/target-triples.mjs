/**
 * Central registry of Rust target triples and project-specific aliases.
 *
 * MacBook Neo (A18 Pro) uses the same Rust triple as M-series Macs
 * (`aarch64-apple-darwin`). Aliases like `mac-neo` map to that triple and
 * set SKILL_MAC_PROFILE=neo for runtime/build tuning on 8 GB A-series Macs.
 *
 * CLI:
 *   node scripts/lib/target-triples.mjs resolve <input>   → canonical triple
 *   node scripts/lib/target-triples.mjs profile <input>   → profile id or "default"
 *   node scripts/lib/target-triples.mjs detect             → host triple
 *   node scripts/lib/target-triples.mjs list               → JSON catalog
 */

import { arch, platform } from "node:os";
import { pathToFileURL } from "node:url";

/** @typedef {{ triple: string, profile: string, label: string, notes?: string }} TargetSpec */

/** @type {Record<string, TargetSpec>} */
export const TARGETS = {
  "aarch64-apple-darwin": {
    triple: "aarch64-apple-darwin",
    profile: "default",
    label: "macOS Apple Silicon (arm64)",
    notes: "M-series, MacBook Neo (A-series), and all arm64 Macs",
  },
  "x86_64-apple-darwin": {
    triple: "x86_64-apple-darwin",
    profile: "default",
    label: "macOS Intel (x86_64)",
  },
  "universal-apple-darwin": {
    triple: "universal-apple-darwin",
    profile: "default",
    label: "macOS universal (arm64 + x86_64 fat binary)",
  },
  "aarch64-unknown-linux-gnu": {
    triple: "aarch64-unknown-linux-gnu",
    profile: "default",
    label: "Linux arm64",
  },
  "x86_64-unknown-linux-gnu": {
    triple: "x86_64-unknown-linux-gnu",
    profile: "default",
    label: "Linux x86_64",
  },
  "x86_64-pc-windows-msvc": {
    triple: "x86_64-pc-windows-msvc",
    profile: "default",
    label: "Windows x86_64 (MSVC)",
  },
  "aarch64-pc-windows-msvc": {
    triple: "aarch64-pc-windows-msvc",
    profile: "default",
    label: "Windows arm64 (MSVC)",
  },
  "x86_64-pc-windows-gnu": {
    triple: "x86_64-pc-windows-gnu",
    profile: "default",
    label: "Windows x86_64 (MinGW)",
  },
};

/**
 * Aliases resolve to a canonical triple plus an optional macOS hardware profile.
 * @type {Record<string, TargetSpec>}
 */
export const ALIASES = {
  // MacBook Neo — A18 Pro, 8 GB; same Rust triple as M-series.
  "mac-neo": {
    triple: "aarch64-apple-darwin",
    profile: "neo",
    label: "MacBook Neo (A-series)",
    notes: "Alias for aarch64-apple-darwin with SKILL_MAC_PROFILE=neo",
  },
  "aarch64-apple-darwin-neo": {
    triple: "aarch64-apple-darwin",
    profile: "neo",
    label: "MacBook Neo (A-series)",
  },
  "apple-a18": {
    triple: "aarch64-apple-darwin",
    profile: "neo",
    label: "Apple A18-class Mac (entry)",
  },
  // Shorthand for the default macOS arm64 release target.
  "mac-arm64": {
    triple: "aarch64-apple-darwin",
    profile: "default",
    label: "macOS Apple Silicon (arm64)",
  },
  "macos-arm64": {
    triple: "aarch64-apple-darwin",
    profile: "default",
    label: "macOS Apple Silicon (arm64)",
  },
};

/**
 * @param {string | null | undefined} input
 * @returns {TargetSpec | null}
 */
export function lookupTarget(input) {
  const key = (input ?? "").trim().toLowerCase();
  if (!key) return null;
  return ALIASES[key] ?? TARGETS[key] ?? null;
}

/**
 * Resolve a triple or alias to canonical build metadata.
 * Unknown inputs pass through unchanged (allows future/custom triples).
 *
 * @param {string | null | undefined} input
 * @returns {TargetSpec}
 */
export function resolveTargetTriple(input) {
  const key = (input ?? "").trim();
  if (!key) {
    return {
      triple: detectHostTriple() || "aarch64-apple-darwin",
      profile: "default",
      label: "host",
    };
  }

  const known = lookupTarget(key);
  if (known) return { ...known };

  return {
    triple: key,
    profile: "default",
    label: key,
  };
}

/**
 * @returns {string}
 */
export function detectHostTriple() {
  const p = platform();
  const a = arch();
  if (p === "darwin" && a === "arm64") return "aarch64-apple-darwin";
  if (p === "darwin" && a === "x64") return "x86_64-apple-darwin";
  if (p === "linux" && a === "x64") return "x86_64-unknown-linux-gnu";
  if (p === "linux" && a === "arm64") return "aarch64-unknown-linux-gnu";
  if (p === "win32" && a === "x64") return "x86_64-pc-windows-msvc";
  if (p === "win32" && a === "arm64") return "aarch64-pc-windows-msvc";
  return "";
}

/**
 * Default `--target` for macOS release builds.
 * @returns {string}
 */
export function defaultMacBuildTarget() {
  return "aarch64-apple-darwin";
}

/**
 * Set SKILL_MAC_PROFILE when a non-default profile is selected.
 *
 * @param {string} profile
 * @param {NodeJS.ProcessEnv} [env]
 */
export function applyMacProfileEnv(profile, env = process.env) {
  if (profile && profile !== "default") {
    env.SKILL_MAC_PROFILE = profile;
  }
}

/**
 * Replace alias values in `--target` CLI args with the canonical triple.
 *
 * @param {string[]} args
 * @param {string} [canonicalTriple]
 * @returns {string[]}
 */
export function rewriteTargetArgs(args, canonicalTriple) {
  const out = [...args];
  for (let i = 0; i < out.length; i++) {
    if (out[i] === "--target" && i + 1 < out.length) {
      const resolved = resolveTargetTriple(out[i + 1]);
      out[i + 1] = canonicalTriple ?? resolved.triple;
      applyMacProfileEnv(resolved.profile);
      break;
    }
    if (out[i].startsWith("--target=")) {
      const raw = out[i].slice("--target=".length);
      const resolved = resolveTargetTriple(raw);
      out[i] = `--target=${canonicalTriple ?? resolved.triple}`;
      applyMacProfileEnv(resolved.profile);
      break;
    }
  }
  return out;
}

/**
 * Resolve env-based target overrides (TAURI_TARGET, CARGO_BUILD_TARGET, SKILL_DAEMON_TARGET).
 *
 * @param {NodeJS.ProcessEnv} [env]
 */
export function resolveEnvTargets(env = process.env) {
  for (const key of ["TAURI_TARGET", "CARGO_BUILD_TARGET", "SKILL_DAEMON_TARGET"]) {
    const raw = env[key];
    if (!raw) continue;
    const resolved = resolveTargetTriple(raw);
    env[key] = resolved.triple;
    applyMacProfileEnv(resolved.profile, env);
  }
}

function main() {
  const [cmd, arg] = process.argv.slice(2);
  switch (cmd) {
    case "resolve":
      console.log(resolveTargetTriple(arg).triple);
      break;
    case "profile":
      console.log(resolveTargetTriple(arg).profile);
      break;
    case "detect":
      console.log(detectHostTriple());
      break;
    case "list":
      console.log(JSON.stringify({ targets: TARGETS, aliases: ALIASES }, null, 2));
      break;
    default:
      console.error(
        "Usage: node scripts/lib/target-triples.mjs <resolve|profile|detect|list> [input]",
      );
      process.exit(1);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
