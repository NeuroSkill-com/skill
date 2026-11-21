#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// check.mjs — the one list of project checks. The git hooks, `npm test`,
// `npm run bump -- --check` and CI's frontend job all run steps from here, so a
// check added once is enforced everywhere.
//
//   npm test                          the `fast` tier (default)
//   npm test -- push                  what pre-push runs: scoped to files changed vs upstream
//   npm test -- full                  everything, unscoped (bump --check runs this)
//   npm test -- vitest clippy         individual steps, in the order given
//   npm test -- full --continue       keep going after a failure, report all at the end
//   npm test -- --list                list tiers and steps
//
// Tiers:
//   commit    pre-commit hook: staged files only, seconds (formats + re-stages Rust)
//   fast      the inner loop: format, lint, types, unit tests, clippy, tier-1 Rust tests
//   push      pre-push hook: like `full`, but only for what the push changes
//   full      every check CI runs on Linux, plus deny
//   frontend  CI's frontend job (no Rust toolchain needed)
//
// Steps whose tool is not installed are skipped with a note (CI runs them).

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

const WIN = process.platform === "win32";

// ── Tool helpers ─────────────────────────────────────────────────────────────

function have(tool) {
  return spawnSync(WIN ? "where" : "which", [tool], { stdio: "ignore" }).status === 0;
}

function git(...args) {
  try {
    return execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return "";
  }
}

const lines = (s) => s.split(/\r?\n/).filter(Boolean);
const matching = (files, re) => files.filter((f) => re.test(f));

// A compile cache speeds every cargo step. kache is preferred (an exported
// RUSTC_WRAPPER overrides ~/.cargo/config.toml); a caller-set one is kept.
if (!process.env.RUSTC_WRAPPER) {
  const cache = ["kache", "sccache"].find(have);
  if (cache) {
    process.env.RUSTC_WRAPPER = cache;
    process.env.CMAKE_C_COMPILER_LAUNCHER ??= cache;
    process.env.CMAKE_CXX_COMPILER_LAUNCHER ??= cache;
  }
}

// ── Changed-file scopes (commit / push tiers) ────────────────────────────────

let stagedCache;
function staged() {
  stagedCache ??= lines(git("diff", "--cached", "--name-only", "--diff-filter=ACMR"));
  return stagedCache;
}

let pushBaseCache;
function pushBase() {
  if (pushBaseCache !== undefined) return pushBaseCache;
  if (git("rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}")) {
    pushBaseCache = git("merge-base", "HEAD", "@{upstream}");
  } else {
    pushBaseCache = git("rev-parse", "--verify", "--quiet", "HEAD~1") ? "HEAD~1" : "HEAD";
  }
  return pushBaseCache;
}

let pushedCache;
function pushed() {
  pushedCache ??= lines(git("diff", "--name-only", "--diff-filter=d", `${pushBase()}...HEAD`)).filter(existsSync);
  return pushedCache;
}

/// `-p` flags for the workspace crates a push touches, plus whether the app crate
/// (src-tauri, or the root manifest/lock) is affected. A crate with its own
/// [workspace] (skill-viz3d) is not a member, so it is fmt-checked on its own.
function pushedCrates() {
  const crates = new Set();
  const standalone = new Set();
  let app = false;
  for (const f of pushed()) {
    const m = f.match(/^crates\/(skill-[^/]+)\//);
    if (m) {
      const manifest = `crates/${m[1]}/Cargo.toml`;
      if (/^\[workspace\]/m.test(readFileSync(manifest, "utf8"))) standalone.add(manifest);
      else crates.add(m[1]);
    } else if (/^src-tauri\//.test(f) || f === "Cargo.toml" || f === "Cargo.lock") {
      app = true;
    }
  }
  return { flags: [...crates].sort().flatMap((c) => ["-p", c]), standalone: [...standalone], app };
}

const RUST_FILES = /\.rs$|(^|\/)Cargo\.(toml|lock)$/;
const FRONTEND_FILES = /^(src\/|scripts\/|package(-lock)?\.json$|svelte\.config\.js$|vite\.config\.js$|tsconfig\.json$|biome\.json$)/;
const SHELL_FILES = /\.sh$|^\.githooks\/|^scripts\/rlx$|^scripts\/pkg-scripts\//;

// ── Steps ────────────────────────────────────────────────────────────────────
//
// `run` returns a list of argv commands (or [] to skip, or a string reason to
// skip with a note). `tool` names a binary that must be installed.

const DENY = ["cargo", "deny", "check", "-A", "no-license-field", "-A", "parse-error", "-A", "license-not-encountered"];
const VITEST_EXCLUDES = ["--exclude", "src/tests/*e2e*", "--exclude", "extensions/**"];
const TIER1_CRATES = ["skill-constants", "skill-eeg", "skill-tools", "skill-jobs", "skill-exg", "skill-gpu"];

const STEPS = {
  // ── commit tier ──
  "rlx-lock": {
    desc: "Cargo.toml / Cargo.lock pin rlx to GitHub, never a local path",
    run: () => [["node", "scripts/check.mjs", "--rlx-lock-guard"]],
  },
  "catalog-json": {
    desc: "src-tauri/llm_catalog.json parses",
    run: (scope) =>
      scope === "commit" && !staged().some((f) => f.endsWith("llm_catalog.json"))
        ? []
        : [["node", "-e", "JSON.parse(require('fs').readFileSync('src-tauri/llm_catalog.json','utf8'))"]],
  },
  "i18n-locales": {
    desc: "every locale has the critical i18n keys",
    run: (scope) =>
      scope === "commit" && !matching(staged(), /^src\/lib\/i18n\/.*\.ts$/).length
        ? []
        : [["node", "scripts/check-critical-i18n-locales.js"]],
  },
  "rustfmt-staged": {
    desc: "cargo fmt, then re-stage the formatted .rs files",
    tool: "cargo",
    run: () => {
      const rs = matching(staged(), /\.rs$/).filter(existsSync);
      return rs.length ? [["cargo", "fmt"], ["git", "add", "--", ...rs]] : [];
    },
  },

  // ── formatting / lint ──
  fmt: {
    desc: "cargo fmt --check + biome format",
    run: (scope) => {
      if (scope === "push") {
        if (!matching(pushed(), RUST_FILES).length) return [];
        const { standalone } = pushedCrates();
        return [["cargo", "fmt", "--check"], ...standalone.map((m) => ["cargo", "fmt", "--check", "--manifest-path", m])];
      }
      return [
        ["cargo", "fmt", "--check"],
        ["npx", "biome", "format", "src/", "scripts/"],
      ];
    },
  },
  biome: {
    desc: "biome lint (push: changed files only)",
    run: (scope) => {
      if (scope !== "push") return [["npx", "biome", "lint", "src/", "scripts/"]];
      const files = matching(pushed(), FRONTEND_FILES).filter((f) => /\.(ts|js|svelte|json)$/.test(f));
      return files.length ? [["npx", "biome", "check", "--no-errors-on-unmatched", ...files]] : [];
    },
  },
  shellcheck: {
    desc: "shell scripts (push: changed files only)",
    tool: "shellcheck",
    run: (scope) => {
      if (scope !== "push") return [["bash", "scripts/lint-shell.sh"]];
      const files = matching(pushed(), SHELL_FILES).filter((f) => !f.startsWith("patches/"));
      return files.length ? [["shellcheck", "--severity=warning", ...files]] : [];
    },
  },
  ruff: { desc: "Python scripts", tool: "ruff", run: () => [["ruff", "check", "."]] },
  pwsh: {
    desc: "PowerShell scripts (PSScriptAnalyzer)",
    tool: "pwsh",
    run: () => [["pwsh", "-NoProfile", "-File", "scripts/lint-powershell.ps1"]],
  },
  "tiny-text": { desc: "no sub-11px text classes", run: () => [["node", "scripts/lint-tiny-text.mjs"]] },

  // ── frontend ──
  i18n: {
    desc: "i18n sync + audit",
    run: () => [
      ["npx", "tsx", "scripts/sync-i18n.ts", "--check"],
      ["npx", "tsx", "scripts/audit-i18n.ts", "--check"],
    ],
  },
  types: {
    desc: "svelte-check + the daemon/markdown/font guards (npm run check)",
    run: () => [["npm", "run", "-s", "check"]],
  },
  vitest: {
    desc: "frontend unit tests (push: tests related to changed files)",
    run: (scope) => {
      if (scope !== "push") return [["npx", "vitest", "run", ...VITEST_EXCLUDES]];
      const changed = matching(pushed(), FRONTEND_FILES);
      const related = matching(changed, /^src\/.*\.(ts|js|svelte)$/);
      const cmds = [];
      if (related.length) cmds.push(["npx", "vitest", "related", "--run", ...VITEST_EXCLUDES, ...related]);
      // Guard tests read sources via fs, which `vitest related` cannot see.
      if (matching(changed, /^(src\/lib\/daemon\/|scripts\/)/).length) {
        cmds.push(["npx", "vitest", "run", "src/tests/daemon-client.test.ts"]);
      }
      return cmds;
    },
  },
  a11y: { desc: "accessibility audit", run: () => [["node", "scripts/audit-a11y.js", "--check"]] },
  catalogs: {
    desc: "LLM catalog, TTS catalog parity, README supported list, HF downloads cache",
    run: () => [
      ["node", "scripts/sync-llm-catalog.js", "--check"],
      ["node", "scripts/check-tts-catalog-parity.js"],
      ["node", "scripts/sync-readme-supported.js", "--check"],
      ["node", "scripts/update-hf-downloads-cache.mjs", "--check"],
    ],
  },

  // ── repo meta ──
  changelog: {
    desc: "changelog fragments are well-formed",
    run: (scope) =>
      scope === "push"
        ? [["node", "scripts/check-changelog-fragments.js", "--base", pushBase(), "--head", "HEAD"]]
        : [["node", "scripts/check-changelog-fragments.js"]],
  },
  ci: {
    desc: "ci.mjs self-test + VERSION sync + test-target wiring",
    run: (scope) => {
      if (scope === "push" && !matching(pushed(), /^scripts\/ci\.mjs$|^\.github\/|^VERSION$|Cargo\.toml$/).length) return [];
      return [
        ["node", "scripts/ci.mjs", "self-test"],
        ["node", "scripts/check-test-targets.js"],
      ];
    },
  },

  // ── Rust ──
  deny: {
    desc: "cargo deny (licenses, advisories, bans)",
    tool: "cargo-deny",
    run: (scope) => (scope === "push" && !matching(pushed(), RUST_FILES).length ? [] : [DENY]),
  },
  clippy: {
    desc: "cargo clippy -D warnings (push: changed crates only)",
    tool: "cargo",
    run: (scope) => {
      if (scope === "push") {
        if (!matching(pushed(), RUST_FILES).length) return [];
        const { flags, app } = pushedCrates();
        const cmds = [];
        if (flags.length) cmds.push(["cargo", "clippy", "--locked", ...flags, "--", "-D", "warnings"]);
        if (app) cmds.push(["cargo", "clippy", "--locked", "-p", "skill", "--", "-D", "warnings"]);
        return cmds;
      }
      return [
        ["node", "scripts/check-rust-version.mjs"],
        ["cargo", "clippy", "--locked", "--workspace", "--exclude", "skill", "--", "-D", "warnings"],
        ["cargo", "clippy", "--locked", "-p", "skill", "--", "-D", "warnings"],
      ];
    },
  },
  rust: {
    desc: "cargo test (fast: tier-1 crates; push: changed crates; full: workspace)",
    tool: "cargo",
    run: (scope) => {
      if (scope === "fast") return [["cargo", "test", "--locked", ...TIER1_CRATES.flatMap((c) => ["-p", c])]];
      if (scope === "push") {
        if (!matching(pushed(), RUST_FILES).length) return [];
        const { flags, app } = pushedCrates();
        const cmds = [];
        if (flags.length) cmds.push(["cargo", "test", "--locked", ...flags]);
        if (app) cmds.push(["cargo", "test", "--locked", "-p", "skill"]);
        return cmds;
      }
      return [["cargo", "test", "--locked", "--workspace", "--exclude", "skill"]];
    },
  },

  // ── opt-in (by name only; need builds, models or hardware) ──
  smoke: { desc: "build verification smoke test", optIn: true, run: () => [["bash", "scripts/smoke-test.sh"]] },
  "daemon-packaging": {
    desc: "daemon sidecar install/upgrade (needs a packaged build)",
    optIn: true,
    run: () => [["bash", "scripts/test-daemon-packaging.sh"]],
  },
  "daemon-e2e": {
    desc: "daemon service installer E2E (needs a daemon build)",
    optIn: true,
    run: () => [["bash", "scripts/test-daemon-e2e.sh"]],
  },
  "llm-e2e": {
    desc: "LLM end-to-end (downloads a model)",
    optIn: true,
    run: () => [["cargo", "test", "-p", "skill-llm", "--features", "llm", "--test", "llm_e2e", "--", "--nocapture"]],
  },
  "mlx-e2e": {
    desc: "UMAP GPU + FFT Metal E2E (macOS)",
    optIn: true,
    run: () =>
      process.platform !== "darwin"
        ? "requires macOS"
        : [
            ["cargo", "test", "-p", "skill-router", "--features", "gpu", "--", "umap_e2e", "--nocapture", "--test-threads=1", "--include-ignored"],
            ["cargo", "test", "-p", "skill-eeg", "--features", "rlx-fft-metal", "--", "--nocapture"],
          ],
  },
  widgets: {
    desc: "WidgetKit extension build + tests (macOS + Xcode)",
    optIn: true,
    tool: "xcodebuild",
    run: () => [["bash", "extensions/widgets/build-widgets.sh", "--test"]],
  },
};

const TIERS = {
  commit: ["rlx-lock", "catalog-json", "i18n-locales", "rustfmt-staged"],
  fast: ["fmt", "biome", "tiny-text", "types", "vitest", "ci", "clippy", "rust"],
  push: ["rlx-lock", "changelog", "fmt", "biome", "shellcheck", "vitest", "ci", "deny", "clippy", "rust"],
  full: [
    "rlx-lock", "changelog", "fmt", "biome", "shellcheck", "ruff", "pwsh", "tiny-text", "i18n", "i18n-locales",
    "types", "a11y", "catalogs", "ci", "vitest", "deny", "clippy", "rust",
  ],
  frontend: ["i18n", "i18n-locales", "fmt-frontend", "biome", "types", "a11y", "catalogs", "vitest"],
};

// CI's frontend job has no Rust toolchain: biome format alone.
STEPS["fmt-frontend"] = { desc: "biome format", run: () => [["npx", "biome", "format", "src/", "scripts/"]] };

// ── rlx lock guard (run as its own process by the `rlx-lock` step) ───────────
//
// Local development path-patches rlx onto sibling checkouts (scripts/rlx local),
// which must never reach a commit: CI and releases build the GitHub pins.

function rlxLockGuard() {
  const fail = (msg) => {
    console.error(`❌ ${msg}`);
    process.exit(1);
  };
  const indexed = (path) => git("show", `:${path}`);
  const toml = indexed("Cargo.toml") || readFileSync("Cargo.toml", "utf8");
  if (/^[^#\n]*path\s*=\s*"\.\.\/rlx/m.test(toml)) {
    fail("Cargo.toml has a ../rlx path dependency. Local rlx belongs in `scripts/rlx local`, not in Cargo.toml.");
  }
  const lock = indexed("Cargo.lock") || readFileSync("Cargo.lock", "utf8");
  const rlx = lock.match(/\[\[package\]\]\nname = "rlx"\nversion = "[^"]+"\n(source = "[^"]+")?/);
  const source = rlx?.[1] ?? "";
  if (!/source = "git\+https:\/\/github\.com\/MIT-RLX\/rlx(\.git)?/.test(source)) {
    fail(
      `Cargo.lock does not pin rlx to git+https://github.com/MIT-RLX/rlx (${source || "path source"}).\n` +
        "   Fix: scripts/rlx deployed   (restores the GitHub-pinned lock; `scripts/rlx local` afterwards)",
    );
  }
}

// ── Runner ───────────────────────────────────────────────────────────────────

function list() {
  console.log("Tiers:");
  for (const [tier, steps] of Object.entries(TIERS)) console.log(`  ${tier.padEnd(9)} ${steps.join(" ")}`);
  console.log("\nSteps:");
  for (const [name, step] of Object.entries(STEPS)) {
    console.log(`  ${name.padEnd(17)} ${step.desc}${step.optIn ? "  (opt-in)" : ""}`);
  }
}

function runCommand(argv) {
  console.log(`\x1b[2m$ ${argv.join(" ")}\x1b[0m`);
  const r = spawnSync(argv[0], argv.slice(1), { stdio: "inherit", shell: WIN });
  return r.status === 0;
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes("--rlx-lock-guard")) return rlxLockGuard();
  if (args.includes("--list") || args.includes("-h") || args.includes("--help")) return list();

  const keepGoing = args.includes("--continue");
  const names = args.filter((a) => !a.startsWith("-"));
  const tier = names.length === 1 && TIERS[names[0]] ? names[0] : names.length ? null : "fast";
  const plan = tier ? TIERS[tier] : names;
  // Steps named one by one run unscoped, like `full`.
  const scope = tier ?? "full";

  for (const name of plan) {
    if (!STEPS[name]) {
      console.error(`Unknown tier or step: ${name}\n`);
      list();
      process.exit(2);
    }
  }

  const t0 = Date.now();
  const results = [];
  for (const name of plan) {
    const step = STEPS[name];
    const label = `\x1b[1;36m▸ ${name}\x1b[0m \x1b[2m${step.desc}\x1b[0m`;
    if (step.tool && !have(step.tool)) {
      if (scope !== "commit") console.log(`${label}\n  \x1b[33m⊘ skipped: ${step.tool} not installed (CI runs it)\x1b[0m`);
      results.push([name, "skip"]);
      continue;
    }
    const cmds = step.run(scope);
    if (typeof cmds === "string") {
      console.log(`${label}\n  \x1b[33m⊘ skipped: ${cmds}\x1b[0m`);
      results.push([name, "skip"]);
      continue;
    }
    if (!cmds.length) {
      results.push([name, "n/a"]);
      continue;
    }
    console.log(label);
    const s0 = Date.now();
    const ok = cmds.every(runCommand);
    const secs = ((Date.now() - s0) / 1000).toFixed(0);
    console.log(ok ? `  \x1b[32m✓ ${name} (${secs}s)\x1b[0m` : `  \x1b[31m✗ ${name} (${secs}s)\x1b[0m`);
    results.push([name, ok ? "pass" : "fail"]);
    if (!ok && !keepGoing) break;
  }

  const count = (s) => results.filter(([, r]) => r === s).length;
  const failed = results.filter(([, r]) => r === "fail").map(([n]) => n);
  const secs = ((Date.now() - t0) / 1000).toFixed(0);
  const summary = `${count("pass")} passed, ${failed.length} failed, ${count("skip")} skipped (${secs}s)`;
  if (failed.length) {
    console.log(`\n\x1b[1;31m${tier ?? "check"}: ${summary} — failed: ${failed.join(", ")}\x1b[0m`);
    process.exit(1);
  }
  console.log(`\n\x1b[1;32m${tier ?? "check"}: ${summary}\x1b[0m`);
}

main();
