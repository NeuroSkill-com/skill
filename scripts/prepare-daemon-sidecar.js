#!/usr/bin/env node
/**
 * Build the daemon binary and copy it to src-tauri/binaries/ for Tauri sidecar usage.
 * Thin wrapper around the shared compile-product recipe (--skip-app --stage-sidecar).
 */

import {
  compileProduct,
  parseCompileProductArgs,
} from "./lib/compile-product.mjs";

const extra = process.argv.slice(2);
if (process.env.SKILL_DAEMON_TARGET && !extra.includes("--target")) {
  extra.unshift("--target", process.env.SKILL_DAEMON_TARGET);
}

const opts = parseCompileProductArgs([
  "--release",
  "--skip-app",
  "--stage-sidecar",
  ...extra,
]);

try {
  compileProduct(opts);
} catch (err) {
  console.error(`[prepare-daemon-sidecar] ${err?.message || err}`);
  process.exit(typeof err?.status === "number" ? err.status : 1);
}
