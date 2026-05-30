#!/usr/bin/env node
// Cross-platform clean script — removes gitignored build artifacts and
// node_modules from all sub-packages. Reports disk space reclaimed.
// Rust target/ is handled separately by clean:rust.

import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

const DIRS = [
  // npm / JS build artifacts
  "node_modules",
  "neuroloop/node_modules",
  "neuroskill/node_modules",
  "extensions/browser/node_modules",
  "extensions/vscode/node_modules",
  // VS Code test runner binary (~640 MB)
  "extensions/vscode/.vscode-test",
  // Xcode build artifacts
  "extensions/widgets/.build",
  // Svelte / Vite build cache
  ".svelte-kit",
  "build",
  // Sidecar binaries (re-built or downloaded by tauri build / setup scripts)
  "src-tauri/binaries",
];

function dirSize(dir) {
  let total = 0;
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return 0;
  }
  for (const e of entries) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) total += dirSize(p);
    else {
      try {
        total += fs.statSync(p).size;
      } catch {}
    }
  }
  return total;
}

function fmt(bytes) {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function removeDir(abs) {
  try {
    fs.rmSync(abs, { recursive: true, force: true, maxRetries: 3, retryDelay: 500 });
  } catch {
    execSync(`rm -rf ${JSON.stringify(abs)}`, { stdio: "inherit" });
  }
}

let totalBytes = 0;
for (const rel of DIRS) {
  const abs = path.join(root, rel);
  if (!fs.existsSync(abs)) continue;
  const bytes = dirSize(abs);
  totalBytes += bytes;
  process.stdout.write(`  removing ${rel} (${fmt(bytes)})…`);
  removeDir(abs);
  console.log(" done");
}

if (totalBytes === 0) {
  console.log("nothing to clean");
} else {
  console.log(`\ncleaned ${fmt(totalBytes)}`);
}
