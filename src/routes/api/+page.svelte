<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- API Status window — connected WebSocket clients + request log -->
<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke }             from "@tauri-apps/api/core";
  import { Badge }              from "$lib/components/ui/badge";
  import { Card, CardContent }  from "$lib/components/ui/card";
  import { t }                  from "$lib/i18n/index.svelte";
  import { useWindowTitle } from "$lib/window-title.svelte";
  import DisclaimerFooter from "$lib/DisclaimerFooter.svelte";
  import { fmtTime } from "$lib/format";

  // ── Types ──────────────────────────────────────────────────────────────────
  interface WsClient {
    peer:         string;
    connected_at: number;
  }
  interface WsRequestLog {
    timestamp: number;
    peer:      string;
    command:   string;
    ok:        boolean;
  }

  // ── State ──────────────────────────────────────────────────────────────────
  let port     = $state(0);
  let clients  = $state<WsClient[]>([]);
  let requests = $state<WsRequestLog[]>([]);
  let now      = $state(Math.floor(Date.now() / 1000));
  let copied   = $state("");

  // ── Helpers ────────────────────────────────────────────────────────────────
  function fmtAgo(utc: number): string {
    const d = now - utc;
    if (d < 5)    return t("common.justNow");
    if (d < 60)   return t("common.secondsAgo", { n: d });
    if (d < 3600) return t("common.minutesAgo", { n: Math.floor(d / 60) });
    return t("common.hoursAgo", { n: Math.floor(d / 3600) });
  }

  function shortPeer(peer: string): string {
    const idx = peer.lastIndexOf(":");
    return idx > 0 ? peer.slice(0, idx) : peer;
  }

  async function copyText(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = label;
      setTimeout(() => { if (copied === label) copied = ""; }, 1500);
    } catch { /* clipboard not available */ }
  }

  // ── Data fetching ──────────────────────────────────────────────────────────
  async function refresh() {
    [port, clients, requests] = await Promise.all([
      invoke<number>("get_ws_port"),
      invoke<WsClient[]>("get_ws_clients"),
      invoke<WsRequestLog[]>("get_ws_request_log"),
    ]);
    requests = [...requests].reverse();
  }

  let pollTimer: ReturnType<typeof setInterval>;
  let nowTimer:  ReturnType<typeof setInterval>;

  onMount(async () => {
    await refresh();
    // Mark "try the API" onboarding step as done.
    try {
      const ob = JSON.parse(localStorage.getItem("onboardDone") ?? "{}");
      if (!ob.apiVisited) { ob.apiVisited = true; localStorage.setItem("onboardDone", JSON.stringify(ob)); }
    } catch (e) { console.warn("[api] onboarding localStorage update failed:", e); }
    pollTimer = setInterval(refresh, 2000);
    nowTimer  = setInterval(() => (now = Math.floor(Date.now() / 1000)), 1000);
    window.addEventListener("skill:api-refresh", refresh);
  });
  onDestroy(() => {
    clearInterval(pollTimer);
    clearInterval(nowTimer);
    window.removeEventListener("skill:api-refresh", refresh);
  });

  let wsUrl = $derived(`ws://localhost:${port}`);

  // ── CLI docs ───────────────────────────────────────────────────────────────
  let activeTab = $state<"overview" | "cli_tool" | "websocket" | "python" | "node">("overview");

  const CLI_EXAMPLES = {
    overview: `# NeuroSkill™ EEG — two ways to interact
#
# 1. neuroskill — high-level typed CLI (included in the repo)
#    npx neuroskill <command> [options]
#
# 2. Raw WebSocket — connect any client to ws://localhost:{port}
#    Messages are JSON; send a command → receive a response.
#
# ── neuroskill commands ──────────────────────────────────────────
#   status                   device / session / scores snapshot
#   sessions                 list all recorded sessions
#   label "text"             create timestamped EEG annotation
#   search                   ANN similarity search (auto: last session)
#   compare                  A/B metrics (auto: last 2 sessions)
#   sleep                    sleep staging (auto: last 24 h)
#   umap                     3D UMAP projection with progress bar
#   listen [--seconds N]     stream broadcast events for N seconds
#   raw '{"command":"..."}'  send arbitrary JSON, print response
#
# ── Global options ────────────────────────────────────────────
#   --port <n>   skip mDNS, connect to explicit port
#   --json       raw JSON output (pipe to jq)
#   --help       full help with all examples`,

    cli_tool: `# ── Install prerequisites (one-time) ─────────────────────────
# Requires: Node ≥ 18, bonjour-service + ws (already in package.json)
npm install neuroskill -g          # from the skill project root

# ── Quick start ───────────────────────────────────────────────
npx neuroskill status              # device state, scores, sleep
npx neuroskill status --json       # raw JSON (pipeable)
npx neuroskill status --json | jq '.scores'

npx neuroskill sessions            # list all recordings
npx neuroskill sessions --json | jq '.sessions[0]'

npx neuroskill label "deep focus"  # annotate current moment
npx neuroskill listen --seconds 10 # stream live events

# ── Time-range commands (auto-select when flags omitted) ──────
npx neuroskill search                         # last session, k=5
npx neuroskill search --start 1740412800 --end 1740415500 --k 10

npx neuroskill compare                        # last 2 sessions
npx neuroskill compare \\
  --a-start 1740380100 --a-end 1740382665 \\
  --b-start 1740412800 --b-end 1740415510

npx neuroskill sleep                          # last 24 h
npx neuroskill sleep --start 1740380100 --end 1740415510

npx neuroskill umap                           # 3D projection
npx neuroskill umap --json | jq '.points | length'

# ── Raw WebSocket passthrough ─────────────────────────────────
npx neuroskill raw '{"command":"status"}'
npx neuroskill raw '{"command":"search","start_utc":1740412800,"end_utc":1740415500,"k":3}'

# ── Explicit port (skip mDNS) ─────────────────────────────────
npx neuroskill --port {port} status`,

    websocket: `# ── wscat (Node.js) ──────────────────────────────────────────
npm install -g wscat
wscat -c ws://localhost:{port}
> {"command":"status"}

# ── websocat (Rust binary, no Node runtime needed) ────────────
# Install: https://github.com/vi/websocat/releases
echo '{"command":"status"}' | websocat ws://localhost:{port}

# Stream EEG live:
echo '{"command":"stream_eeg"}' \\
  | websocat --no-close ws://localhost:{port}

# ── mDNS discovery (find port automatically) ──────────────────
# macOS:
dns-sd -B _skill._tcp
# Linux:
avahi-browse -r _skill._tcp`,

    python: `# ── Python (websockets library) ──────────────────────────────
# pip install websockets
import asyncio, json, websockets

async def main():
    async with websockets.connect("ws://localhost:{port}") as ws:
        # Get device status
        await ws.send(json.dumps({"command": "status"}))
        status = json.loads(await ws.recv())
        print(status)

        # Search embeddings
        await ws.send(json.dumps({
            "command":   "search",
            "start_utc": 1740412800,
            "end_utc":   1740415500,
            "k":         5,
        }))
        result = json.loads(await ws.recv())
        print(f"{result.get('query_count')} queries, "
              f"{len(result.get('results', []))} result groups")

asyncio.run(main())`,

    node: `// ── Node.js (ws library) ─────────────────────────────────────
// npm install ws
const WebSocket = require("ws");
const ws = new WebSocket("ws://localhost:{port}");

ws.on("open", () => {
  // Get status
  ws.send(JSON.stringify({ command: "status" }));
});

ws.on("message", (data) => {
  const msg = JSON.parse(data.toString());

  if (msg.command === "status") {
    console.log("device:", msg.device_name, "state:", msg.state);

    // Then search the last session's embeddings
    ws.send(JSON.stringify({
      command:   "search",
      start_utc: msg.session_start_utc,
      end_utc:   msg.session_end_utc,
      k: 5,
    }));
  } else if (msg.command === "search") {
    console.log("found", msg.results?.length, "result groups");
    ws.close();
  }
});`,
  } as const;

  function cliCode(key: keyof typeof CLI_EXAMPLES): string {
    return CLI_EXAMPLES[key].replace(/\{port\}/g, String(port || "…"));
  }

  async function copyCode(key: keyof typeof CLI_EXAMPLES) {
    await copyText(cliCode(key), `cli-${key}`);
  }

  useWindowTitle("window.title.api");
</script>

<main class="h-full min-h-0 overflow-y-auto px-4 py-4 flex flex-col gap-3"
      aria-label={t("apiStatus.title")}>

  <!-- ── Server info (compact 2-row grid) ──────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden shrink-0">
    <CardContent class="px-4 py-2.5 flex flex-col gap-1.5">
      <div class="flex items-center gap-2 flex-wrap">
        <div class="w-2 h-2 rounded-full bg-green-500 shrink-0"></div>
        <span class="text-[0.72rem] font-semibold text-foreground">{t("apiStatus.serverRunning")}</span>
        <Badge variant="outline" class="text-[0.55rem] py-0 px-1.5 bg-primary/10 text-primary border-primary/20">
          WebSocket
        </Badge>
        <Badge variant="outline" class="text-[0.55rem] py-0 px-1.5 bg-violet-500/10 text-violet-600 dark:text-violet-400 border-violet-500/20">
          mDNS
        </Badge>
        <span class="ml-auto text-[0.6rem] text-muted-foreground">{t("apiStatus.port")}</span>
        <kbd class="font-mono text-[0.65rem] font-bold text-foreground bg-muted dark:bg-white/[0.06]
                    border border-border dark:border-white/[0.1] rounded px-1.5 py-0.5">
          {port}
        </kbd>
      </div>
      <!-- Quick connect row -->
      <div class="flex items-center gap-2 flex-wrap">
        <button
          onclick={() => copyText(wsUrl, "ws")}
          class="group flex items-center gap-1.5 rounded-md border border-border dark:border-white/[0.08]
                 bg-muted dark:bg-white/[0.04] hover:bg-slate-100 dark:hover:bg-white/[0.06]
                 px-2 py-1 transition-colors cursor-pointer"
          title={t("apiStatus.clickToCopy")}
        >
          <code class="font-mono text-[0.62rem] text-foreground select-all">{wsUrl}</code>
          <span class="text-[0.55rem] text-muted-foreground/50 group-hover:text-muted-foreground transition-colors">
            {copied === "ws" ? "✓" : "⎘"}
          </span>
        </button>
        <button
          onclick={() => copyText(`dns-sd -B _skill._tcp`, "mdns")}
          class="group flex items-center gap-1.5 rounded-md border border-border dark:border-white/[0.08]
                 bg-muted dark:bg-white/[0.04] hover:bg-slate-100 dark:hover:bg-white/[0.06]
                 px-2 py-1 transition-colors cursor-pointer"
          title={t("apiStatus.clickToCopy")}
        >
          <code class="font-mono text-[0.62rem] text-foreground select-all">dns-sd -B _skill._tcp</code>
          <span class="text-[0.55rem] text-muted-foreground/50 group-hover:text-muted-foreground transition-colors">
            {copied === "mdns" ? "✓" : "⎘"}
          </span>
        </button>
      </div>
    </CardContent>
  </Card>

  <!-- ── CLI Documentation ────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-1.5 shrink-0">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.52rem] font-semibold tracking-widest uppercase text-muted-foreground">
        CLI &amp; Code Examples
      </span>
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <!-- Tab row -->
      <div class="flex border-b border-border dark:border-white/[0.06] overflow-x-auto">
        {#each ([["overview","Overview"],["cli_tool","neuroskill"],["websocket","WebSocket"],["python","Python"],["node","Node.js"]] as const) as [key, label]}
          <button
            onclick={() => activeTab = key}
            class="flex-1 py-1.5 text-[0.6rem] font-semibold transition-colors
                   {activeTab === key
                     ? 'bg-primary/10 text-primary border-b-2 border-primary'
                     : 'text-muted-foreground hover:text-foreground hover:bg-muted/30'}"
          >
            {label}
          </button>
        {/each}
      </div>

      <!-- Code block -->
      <div class="relative">
        <pre class="px-3 py-3 text-[0.6rem] font-mono text-foreground/80
                    leading-relaxed overflow-x-auto max-h-52 select-text
                    scrollbar-thin">{cliCode(activeTab)}</pre>
        <button
          onclick={() => copyCode(activeTab)}
          class="absolute top-2 right-2 text-[0.52rem] px-1.5 py-0.5
                 rounded border border-border dark:border-white/[0.08]
                 bg-background/80 text-muted-foreground
                 hover:text-foreground hover:bg-accent transition-colors"
          title="Copy to clipboard"
        >
          {copied.startsWith("cli-") && copied === `cli-${activeTab}` ? "✓ copied" : "⎘ copy"}
        </button>
      </div>
    </Card>
  </section>

  <!-- ── Connected Clients ─────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-1.5 shrink-0">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.52rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("apiStatus.connectedClients")}
      </span>
      <Badge variant="outline" class="text-[0.5rem] py-0 px-1 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20">
        {clients.length}
      </Badge>
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      {#if clients.length === 0}
        <CardContent class="flex items-center gap-3 py-4 px-4">
          <span class="text-xl">🔌</span>
          <div class="flex flex-col gap-0.5">
            <p class="text-[0.72rem] text-muted-foreground">{t("apiStatus.noClients")}</p>
            <p class="text-[0.6rem] text-muted-foreground/50 leading-relaxed">
              {t("apiStatus.noClientsHint", { port })}
            </p>
          </div>
        </CardContent>
      {:else}
        <CardContent class="py-0 px-0">
          <div class="divide-y divide-border dark:divide-white/[0.05]">
            {#each clients as client}
              <div class="flex items-center gap-2 px-4 py-2">
                <div class="w-1.5 h-1.5 rounded-full bg-green-500 shrink-0"></div>
                <span class="font-mono text-[0.68rem] font-semibold text-foreground truncate">{shortPeer(client.peer)}</span>
                <span class="font-mono text-[0.55rem] text-muted-foreground/40 truncate hidden sm:inline">{client.peer}</span>
                <span class="ml-auto text-[0.55rem] text-muted-foreground whitespace-nowrap shrink-0">
                  {fmtTime(client.connected_at)}
                  <span class="text-muted-foreground/40">({fmtAgo(client.connected_at)})</span>
                </span>
              </div>
            {/each}
          </div>
        </CardContent>
      {/if}
    </Card>
  </section>

  <!-- ── Request Log ───────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-1.5 min-h-0 flex-1">
    <div class="flex items-center gap-2 px-0.5 shrink-0">
      <span class="text-[0.52rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("apiStatus.requestLog")}
      </span>
      <Badge variant="outline" class="text-[0.5rem] py-0 px-1">
        {requests.length}
      </Badge>
    </div>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden flex-1 min-h-0 flex flex-col">
      {#if requests.length === 0}
        <CardContent class="flex items-center gap-3 py-4 px-4">
          <span class="text-xl">📋</span>
          <p class="text-[0.72rem] text-muted-foreground">{t("apiStatus.noRequests")}</p>
        </CardContent>
      {:else}
        <!-- Header row -->
        <div class="grid grid-cols-[64px_1fr_80px_40px] gap-1 px-3 py-1.5 shrink-0
                    bg-slate-50 dark:bg-[#111118] text-[0.5rem] font-semibold
                    tracking-widest uppercase text-muted-foreground/50 border-b border-border dark:border-white/[0.04]">
          <span>{t("apiStatus.time")}</span>
          <span>{t("apiStatus.client")}</span>
          <span>{t("apiStatus.command")}</span>
          <span class="text-right">{t("apiStatus.status")}</span>
        </div>
        <!-- Scrollable rows -->
        <div class="overflow-y-auto flex-1 min-h-0">
          {#each requests as req}
            <div class="grid grid-cols-[64px_1fr_80px_40px] gap-1 px-3 py-1 items-center
                        hover:bg-slate-50 dark:hover:bg-white/[0.02] transition-colors
                        border-b border-border/30 dark:border-white/[0.02] last:border-0">
              <span class="font-mono text-[0.58rem] text-muted-foreground tabular-nums truncate">
                {fmtTime(req.timestamp)}
              </span>
              <span class="font-mono text-[0.58rem] text-foreground/60 truncate">
                {shortPeer(req.peer)}
              </span>
              <span class="font-mono text-[0.6rem] font-semibold text-foreground truncate">
                {req.command}
              </span>
              <span class="text-right">
                {#if req.ok}
                  <span class="text-[0.5rem] font-bold text-green-600 dark:text-green-400">OK</span>
                {:else}
                  <span class="text-[0.5rem] font-bold text-red-500 dark:text-red-400">ERR</span>
                {/if}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </Card>
  </section>

  <DisclaimerFooter />
</main>

<style>
  :global(body) { overflow: hidden; }
</style>
