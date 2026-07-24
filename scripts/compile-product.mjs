#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
//
// Shared product compile entrypoint. See scripts/lib/compile-product.mjs.

import {
  compileProduct,
  parseCompileProductArgs,
  printCompileProductHelp,
} from "./lib/compile-product.mjs";

try {
  const opts = parseCompileProductArgs(process.argv.slice(2));
  if (opts.help) {
    printCompileProductHelp();
    process.exit(0);
  }
  compileProduct(opts);
} catch (err) {
  console.error(`[compile-product] ${err?.message || err}`);
  process.exit(typeof err?.status === "number" ? err.status : 1);
}
