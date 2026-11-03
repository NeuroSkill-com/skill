#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Stop every running skill-daemon (`npm run stop`).
//
// Matches the exact process name. `pkill -f skill-daemon` matched whole command
// lines, so on Linux it also killed the `sh -c` npm ran it in, and anywhere it
// hit commands such as `tail -f skill-daemon.log`.
import { spawnSync } from "node:child_process";

const isWindows = process.platform === "win32";
const result = isWindows
  ? spawnSync("taskkill", ["/F", "/IM", "skill-daemon.exe"], { stdio: "ignore" })
  : spawnSync("pkill", ["-x", "skill-daemon"], { stdio: "ignore" });

if (result.error) {
  console.error(`could not run ${isWindows ? "taskkill" : "pkill"}: ${result.error.message}`);
  process.exit(1);
}
console.log(result.status === 0 ? "skill-daemon stopped" : "no skill-daemon running");
