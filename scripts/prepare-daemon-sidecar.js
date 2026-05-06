#!/usr/bin/env node
/**
 * Build the daemon binary and copy it to src-tauri/binaries/ for Tauri sidecar usage.
 * Cross-platform replacement for prepare-daemon-sidecar.sh (avoids `bash` on Windows).
 */

import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { arch, platform } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");
const targetDir = resolve(root, "src-tauri", "target");
const binDir = resolve(root, "src-tauri", "binaries");

function detectTargetTriple() {
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

function runOrThrow(cmd, args) {
  const result = spawnSync(cmd, args, {
    cwd: root,
    stdio: "inherit",
    env: process.env,
  });
  if (result.error) throw result.error;
  if (typeof result.status === "number" && result.status !== 0) {
    process.exit(result.status);
  }
}

const triple = process.env.SKILL_DAEMON_TARGET || detectTargetTriple();
const ext = triple.includes("windows") ? ".exe" : "";
const tripleLabel = triple || "native";
const isWindows = triple.includes("windows") || platform() === "win32";

// skill-tty is unix-only — Windows shell hooks don't invoke it. Build it on
// macOS/Linux only so the Windows pipeline doesn't waste CI minutes on it.
const cratesToBuild = ["skill-daemon"];
if (!isWindows) {
  cratesToBuild.push("skill-tty");
}

console.log(`🔧 Building ${cratesToBuild.join(", ")} for ${triple || "native"} (release)…`);

const cargoArgs = ["build", "--release"];
for (const c of cratesToBuild) {
  cargoArgs.push("-p", c);
}
if (triple) {
  cargoArgs.push("--target", triple);
}

runOrThrow("cargo", cargoArgs);

mkdirSync(binDir, { recursive: true });

function stageBinary(name) {
  const candidates = [
    triple ? resolve(targetDir, triple, "release", `${name}${ext}`) : null,
    resolve(targetDir, "release", `${name}${ext}`),
  ].filter(Boolean);

  const src = candidates.find((p) => existsSync(p));
  if (!src) {
    console.error(`❌ ${name} binary not found after build`);
    process.exit(1);
  }

  const dst = resolve(binDir, `${name}-${tripleLabel}${ext}`);
  copyFileSync(src, dst);
  try {
    chmodSync(dst, 0o755);
  } catch {
    // Windows may ignore chmod; safe to continue.
  }

  const releaseDir = triple ? resolve(targetDir, triple, "release") : resolve(targetDir, "release");
  const releaseDst = resolve(releaseDir, `${name}${ext}`);
  if (existsSync(releaseDir) && src !== releaseDst) {
    copyFileSync(src, releaseDst);
    console.log(`Copied to ${releaseDst}`);
  } else if (src === releaseDst) {
    console.log(`${name} already at ${releaseDst}`);
  }

  const size = statSync(dst).size;
  const mb = (size / (1024 * 1024)).toFixed(1);
  console.log(`✅ ${name} sidecar ready: ${dst} (${mb} MiB)`);
}

for (const c of cratesToBuild) {
  stageBinary(c);
}
