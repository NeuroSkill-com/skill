#!/usr/bin/env node
// Cross-platform clean script for src-tauri/target.
// Reports disk space reclaimed.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const target = path.join(__dirname, "..", "src-tauri", "target");

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
    if (e.isDirectory()) {
      total += dirSize(p);
    } else {
      try {
        total += fs.statSync(p).size;
      } catch {}
    }
  }
  return total;
}

function _fmt(bytes) {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

if (!fs.existsSync(target)) {
  process.exit(0);
}

const _bytes = dirSize(target);

try {
  fs.rmSync(target, { recursive: true, force: true, maxRetries: 3, retryDelay: 500 });
} catch {
  // Fallback: use system rm for very large trees where Node's rmSync hits ENOTEMPTY races
  const { execSync } = await import("node:child_process");
  execSync(`rm -rf ${JSON.stringify(target)}`, { stdio: "inherit" });
}
