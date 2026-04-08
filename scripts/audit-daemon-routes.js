#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";

const src = readFileSync("src/lib/daemon/invoke-proxy.ts", "utf8");
const routeBlock = src.split("const ROUTES")[1]?.split("};")[0] ?? "";

const rows = [...routeBlock.matchAll(/^\s+([a-zA-Z0-9_]+):\s*\[\s*([GP]),\s*"([^"]+)"\s*\],/gm)].map((m) => ({
  cmd: m[1],
  method: m[2] === "G" ? "GET" : "POST",
  path: m[3],
}));

rows.sort((a, b) => a.cmd.localeCompare(b.cmd));

const byMethod = {
  GET: rows.filter((r) => r.method === "GET").length,
  POST: rows.filter((r) => r.method === "POST").length,
};

const md = [
  "# Tauri → Daemon Route Audit",
  "",
  `Generated: ${new Date().toISOString()}`,
  "",
  `- Total commands in invoke-proxy ROUTES: **${rows.length}**`,
  `- GET: **${byMethod.GET}**`,
  `- POST: **${byMethod.POST}**`,
  "",
  "| Command | Method | Path |",
  "|---|---|---|",
  ...rows.map((r) => `| \`${r.cmd}\` | ${r.method} | \`${r.path}\` |`),
  "",
].join("\n");

const outPath = "docs/testing/tauri-daemon-route-audit.md";
writeFileSync(outPath, md);
console.log(`wrote ${outPath} (${rows.length} rows)`);
