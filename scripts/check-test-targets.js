#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Fail if an integration-test file would never run.
//
// Crates that merge their fast integration tests into one `it` binary set
// `autotests = false`, so cargo no longer discovers `tests/*.rs` on its own. A new
// `tests/foo.rs` that is not added to `tests/it.rs` (or declared as a `[[test]]`)
// then compiles nowhere and is silently skipped, which looks exactly like a
// passing suite. This check lists such files.
//
//   npm run check:test-targets

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/// Cargo.toml files of every workspace member that can hold tests.
function manifests(root) {
  const found = [];
  const crates = join(root, "crates");
  if (existsSync(crates)) {
    for (const name of readdirSync(crates)) {
      const manifest = join(crates, name, "Cargo.toml");
      if (existsSync(manifest)) found.push(manifest);
    }
  }
  const app = join(root, "src-tauri", "Cargo.toml");
  if (existsSync(app)) found.push(app);
  return found;
}

/// Test files registered by a manifest: each `[[test]] path = "tests/x.rs"`, plus
/// the modules those roots pull in with `#[path = "x.rs"] mod x;` or `mod x;`.
function registeredFiles(manifestText, testsDir) {
  const roots = [...manifestText.matchAll(/^\s*path\s*=\s*"tests\/([^"/]+\.rs)"/gm)].map((m) => m[1]);
  const registered = new Set(roots);
  for (const rootFile of roots) {
    const file = join(testsDir, rootFile);
    if (!existsSync(file)) continue;
    const source = readFileSync(file, "utf8");
    for (const m of source.matchAll(/#\[path\s*=\s*"([^"/]+\.rs)"\]/g)) registered.add(m[1]);
    for (const m of source.matchAll(/^\s*(?:pub\s+)?mod\s+(\w+)\s*;/gm)) registered.add(`${m[1]}.rs`);
  }
  return registered;
}

/// `{ crate, file }` for each top-level `tests/*.rs` of an `autotests = false` crate
/// that nothing registers.
export function findUnregisteredTests(root = ROOT) {
  const problems = [];
  for (const manifest of manifests(root)) {
    const text = readFileSync(manifest, "utf8");
    if (!/^\s*autotests\s*=\s*false\b/m.test(text)) continue;
    const crateDir = dirname(manifest);
    const testsDir = join(crateDir, "tests");
    if (!existsSync(testsDir)) continue;
    const registered = registeredFiles(text, testsDir);
    for (const file of readdirSync(testsDir)
      .filter((f) => f.endsWith(".rs"))
      .sort()) {
      if (!registered.has(file)) problems.push({ crate: relative(root, crateDir), file });
    }
  }
  return problems;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const problems = findUnregisteredTests();
  if (problems.length === 0) {
    console.log("[check:test-targets] OK: every integration-test file is registered.");
  } else {
    console.error("[check:test-targets] These test files are never compiled or run:\n");
    for (const { crate, file } of problems) {
      console.error(`  ${crate}/tests/${file}`);
    }
    console.error(
      "\nThose crates set `autotests = false`. Add each file as a `mod` line in the crate's\n" +
        '`tests/it.rs` (`#[path = "<file>"] mod <name>;`), or declare it as a `[[test]]` in Cargo.toml.',
    );
    process.exit(1);
  }
}
