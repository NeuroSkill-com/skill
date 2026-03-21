#!/usr/bin/env node
// Cross-platform clean script for src-tauri/target.
// Reports disk space reclaimed.

import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

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

function fmt(bytes) {
  if (bytes >= 1024 ** 3) return (bytes / 1024 ** 3).toFixed(2) + " GB";
  if (bytes >= 1024 ** 2) return (bytes / 1024 ** 2).toFixed(1) + " MB";
  if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
  return bytes + " B";
}

if (!fs.existsSync(target)) {
  console.log("\n  \x1b[2m>\x1b[0m src-tauri/target does not exist — nothing to clean.\n");
  process.exit(0);
}

console.log("\n  \x1b[36m...\x1b[0m Calculating size of src-tauri/target ...");

const bytes = dirSize(target);

console.log(`  \x1b[33m>\x1b[0m   Found \x1b[1m${fmt(bytes)}\x1b[0m in build artifacts`);
console.log("  \x1b[31m>\x1b[0m   Removing src-tauri/target ...");

fs.rmSync(target, { recursive: true, force: true });

console.log(`  \x1b[32m✓\x1b[0m   Freed \x1b[1;32m${fmt(bytes)}\x1b[0m of disk space.\n`);
