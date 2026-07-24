#!/usr/bin/env node
// Fail if UI still uses daemonInvoke("cmd") for commands that already have
// typed clients in src/lib/daemon/*.ts. Keeps the transitional invoke-proxy
// from growing back over migrated surfaces.

import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

/** command name → typed module (documentation for failures) */
const TYPED = {
  list_auth_tokens: "tokens.ts",
  create_auth_token: "tokens.ts",
  revoke_auth_token: "tokens.ts",
  delete_auth_token: "tokens.ts",
  refresh_default_token: "tokens.ts",
  cancel_tool_call: "chat.ts",
  rename_chat_session: "chat.ts",
  delete_chat_session: "chat.ts",
  list_chat_sessions: "chat.ts",
  forget_device: "devices.ts",
  set_preferred_device: "devices.ts",
  retry_connect: "devices.ts",
  cancel_retry: "devices.ts",
  get_status: "devices.ts (getDeviceStatus)",
  get_cortex_ws_state: "devices.ts (getCortexWsState)",
  get_devices: "devices.ts",
  pair_device: "devices.ts",
};

const ROOTS = ["src/lib", "src/routes"];
const SKIP = new Set([
  path.resolve("src/lib/daemon/invoke-proxy.ts"),
  path.resolve("src/tests"),
]);

async function* walk(dir) {
  for (const ent of await readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      if (p.includes(`${path.sep}tests${path.sep}`) || p.endsWith(`${path.sep}tests`)) continue;
      yield* walk(p);
    } else if (/\.(ts|svelte)$/.test(ent.name)) {
      yield p;
    }
  }
}

const invokeRe = /daemonInvoke\s*(?:<[^>]*>)?\s*\(\s*["']([a-zA-Z0-9_]+)["']/g;
const violations = [];

for (const root of ROOTS) {
  for await (const file of walk(path.resolve(root))) {
    if ([...SKIP].some((s) => file === s || file.startsWith(s + path.sep))) continue;
    const src = await readFile(file, "utf8");
    for (const m of src.matchAll(invokeRe)) {
      const cmd = m[1];
      if (TYPED[cmd]) {
        violations.push({ file: path.relative(process.cwd(), file), cmd, via: TYPED[cmd] });
      }
    }
  }
}

if (violations.length) {
  console.error("daemonInvoke used for commands that have typed clients:\n");
  for (const v of violations) {
    console.error(`  ${v.file}: daemonInvoke("${v.cmd}") → use $lib/daemon/${v.via}`);
  }
  console.error("\nSee docs/architecture.md (Frontend API rule).");
  process.exit(1);
}

console.log(`ok: no daemonInvoke for ${Object.keys(TYPED).length} typed-client commands`);
