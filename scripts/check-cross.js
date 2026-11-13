#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Type-check the builds a macOS `cargo check` cannot see.
//
//   npm run check:cross [-- --only daemon|app]
//
// Code behind `cfg(not(target_os = "macos"))` or `cfg(windows)` is invisible to a
// local check, so a dependency bump can break it and nothing complains until the
// Windows release job fails, after the tag is already pushed. `sysinfo` 0.40 did
// exactly that to skill-gpu. This runs the Windows daemon and app checks through
// cargo-xwin instead (about 4.5 minutes warm), with --locked like the release job.
//
// Linux is not covered: its daemon needs the GTK / WebKit system libraries, which
// a Mac does not have. CI is the check for that one.

import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const TARGET = "x86_64-pc-windows-msvc";

const CHECKS = {
  daemon: { label: "Windows daemon", args: ["-p", "skill-daemon", "--features", "windows"] },
  app: { label: "Windows app", args: ["-p", "skill", "--features", "custom-protocol"] },
};

const args = process.argv.slice(2);
let only = null;
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--only" && CHECKS[args[i + 1]]) only = args[++i];
  else {
    console.error("Usage: npm run check:cross [-- --only daemon|app]");
    process.exit(1);
  }
}

const onWindows = process.platform === "win32";
if (!onWindows) {
  const probe = spawnSync("cargo", ["xwin", "--version"], { stdio: "ignore" });
  if (probe.status !== 0) {
    console.error(
      "[check:cross] cargo-xwin is not installed. Install it once:\n" +
        "  cargo install cargo-xwin --locked\n" +
        `  rustup target add ${TARGET}`,
    );
    process.exit(2);
  }
}

let failed = false;
for (const [name, { label, args: cargoArgs }] of Object.entries(CHECKS)) {
  if (only && only !== name) continue;
  const command = onWindows ? ["check"] : ["xwin", "check"];
  const full = [...command, "--locked", ...cargoArgs, "--target", TARGET];
  console.log(`\n[check:cross] ${label}: cargo ${full.join(" ")}`);
  const result = spawnSync("cargo", full, { cwd: ROOT, stdio: "inherit" });
  if (result.status !== 0) {
    console.error(`[check:cross] ${label} FAILED`);
    failed = true;
  } else {
    console.log(`[check:cross] ${label} ok`);
  }
}
process.exit(failed ? 1 : 0);
