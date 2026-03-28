<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- LSL tab — discover local LSL streams, pair for auto-connect, and manage rlsl-iroh remote sink. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onDestroy, onMount } from "svelte";

import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
interface LslStream {
  name: string;
  type: string;
  channels: number;
  sample_rate: number;
  source_id: string;
  hostname: string;
  paired: boolean;
}

interface LslIrohStatus {
  running: boolean;
  endpoint_id: string | null;
}

interface LslConfig {
  auto_connect: boolean;
  paired_streams: string[];
}

// ── State ──────────────────────────────────────────────────────────────────
let streams = $state<LslStream[]>([]);
let scanning = $state(false);
let connecting = $state<string | null>(null);
let scanError = $state("");

let autoConnect = $state(false);
let pairedStreams = $state<string[]>([]);

let irohStatus = $state<LslIrohStatus>({ running: false, endpoint_id: null });
let irohStarting = $state(false);
let irohError = $state("");
let irohCopied = $state(false);

let scanTimer: ReturnType<typeof setInterval> | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;

// ── Actions ────────────────────────────────────────────────────────────────
async function scanStreams() {
  scanning = true;
  scanError = "";
  try {
    streams = await invoke<LslStream[]>("lsl_discover");
  } catch (e: unknown) {
    scanError = String(e);
  } finally {
    scanning = false;
  }
}

async function connectStream(name: string) {
  connecting = name;
  try {
    await invoke("lsl_connect", { name });
  } catch (e: unknown) {
    scanError = String(e);
  } finally {
    connecting = null;
  }
}

async function togglePair(sourceId: string, isPaired: boolean) {
  if (isPaired) {
    await invoke("lsl_unpair_stream", { sourceId });
    pairedStreams = pairedStreams.filter((id) => id !== sourceId);
  } else {
    await invoke("lsl_pair_stream", { sourceId });
    pairedStreams = [...pairedStreams, sourceId];
  }
  // Update paired flag in streams list
  streams = streams.map((s) => (s.source_id === sourceId ? { ...s, paired: !isPaired } : s));
}

async function toggleAutoConnect() {
  autoConnect = !autoConnect;
  await invoke("lsl_set_auto_connect", { enabled: autoConnect });
  manageAutoScanTimer();
}

function manageAutoScanTimer() {
  if (scanTimer) {
    clearInterval(scanTimer);
    scanTimer = null;
  }
  if (autoConnect) {
    // Auto-scan every 10s when auto-connect is on
    scanTimer = setInterval(scanStreams, 10_000);
  }
}

async function startIroh() {
  irohStarting = true;
  irohError = "";
  try {
    irohStatus = await invoke<LslIrohStatus>("lsl_iroh_start");
  } catch (e: unknown) {
    irohError = String(e);
  } finally {
    irohStarting = false;
  }
}

async function stopIroh() {
  await invoke("lsl_iroh_stop");
  irohStatus = { running: false, endpoint_id: null };
}

async function refreshIrohStatus() {
  try {
    irohStatus = await invoke<LslIrohStatus>("lsl_iroh_status");
  } catch {
    /* ignore */
  }
}

async function copyEndpointId() {
  if (!irohStatus.endpoint_id) return;
  try {
    await navigator.clipboard.writeText(irohStatus.endpoint_id);
    irohCopied = true;
    setTimeout(() => (irohCopied = false), 2000);
  } catch {
    /* ignore */
  }
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
onMount(async () => {
  // Load config
  try {
    const cfg = await invoke<LslConfig>("lsl_get_config");
    autoConnect = cfg.auto_connect;
    pairedStreams = cfg.paired_streams;
  } catch {
    /* ignore */
  }

  await refreshIrohStatus();
  // Initial scan
  await scanStreams();

  // Poll iroh status every 5s
  pollTimer = setInterval(refreshIrohStatus, 5000);
  // Start auto-scan timer if auto-connect is on
  manageAutoScanTimer();
});

onDestroy(() => {
  if (pollTimer) clearInterval(pollTimer);
  if (scanTimer) clearInterval(scanTimer);
});
</script>

<!-- ── Auto-Connect Toggle ────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("lsl.autoConnect")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">
      <button
        onclick={toggleAutoConnect}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div
          class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {autoConnect ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}"
        >
          <div
            class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {autoConnect ? 'translate-x-4' : 'translate-x-0.5'}"
          ></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("lsl.autoConnectToggle")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("lsl.autoConnectDesc")}
          </span>
        </div>
        <span
          class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {autoConnect ? 'text-emerald-500' : 'text-muted-foreground/50'}"
        >
          {autoConnect ? t("common.on") : t("common.off")}
        </span>
      </button>

      <!-- Paired streams list -->
      {#if pairedStreams.length > 0}
        <div
          class="border-t border-border dark:border-white/[0.05] px-4 py-3 flex flex-col gap-2 bg-muted/20 dark:bg-white/[0.01]"
        >
          <span class="text-[0.54rem] font-semibold tracking-widest uppercase text-muted-foreground/70">
            {t("lsl.pairedStreams")} ({pairedStreams.length})
          </span>
          <div class="flex flex-wrap gap-1.5">
            {#each pairedStreams as sourceId}
              <span
                class="inline-flex items-center gap-1 text-[0.58rem] font-mono px-2 py-1
                       rounded-md bg-primary/10 text-primary border border-primary/20"
              >
                {sourceId}
                <button
                  class="ml-0.5 text-primary/50 hover:text-primary cursor-pointer"
                  onclick={() => togglePair(sourceId, true)}
                  title={t("lsl.unpair")}
                >
                  ✕
                </button>
              </span>
            {/each}
          </div>
        </div>
      {/if}
    </CardContent>
  </Card>
</section>

<!-- ── Local LSL Streams ──────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("lsl.localStreams")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 p-4">
      <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
        {t("lsl.localStreamsDesc")}
      </p>

      <div class="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          class="h-7 text-[0.62rem] px-3"
          disabled={scanning}
          onclick={scanStreams}
        >
          {#if scanning}
            <span class="flex items-center gap-1.5">
              <span
                class="h-3 w-3 border-2 border-current border-t-transparent rounded-full animate-spin"
              ></span>
              {t("lsl.scanning")}
            </span>
          {:else}
            {t("lsl.scanButton")}
          {/if}
        </Button>
        {#if streams.length > 0}
          <span class="text-[0.58rem] text-muted-foreground">
            {streams.length}
            {streams.length === 1 ? "stream" : "streams"} found
          </span>
        {/if}
        {#if autoConnect}
          <span
            class="ml-auto flex items-center gap-1.5 text-[0.52rem] font-semibold text-emerald-600 dark:text-emerald-400"
          >
            <span class="relative flex h-1.5 w-1.5">
              <span
                class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"
              ></span>
              <span class="relative inline-flex rounded-full h-1.5 w-1.5 bg-emerald-500"></span>
            </span>
            {t("lsl.autoScanning")}
          </span>
        {/if}
      </div>

      {#if scanError}
        <p class="text-[0.58rem] text-red-500 leading-relaxed">{scanError}</p>
      {/if}

      {#if streams.length > 0}
        <div class="flex flex-col gap-2 mt-1">
          {#each streams as stream (stream.source_id || stream.name)}
            <div
              class="flex items-center gap-3 rounded-lg border px-4 py-3
                        {stream.paired
                ? 'border-primary/30 bg-primary/5 dark:bg-primary/5'
                : 'border-border dark:border-white/[0.08] bg-muted/30 dark:bg-white/[0.02]'}"
            >
              <!-- Stream info -->
              <div class="flex flex-col gap-1 flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-[0.72rem] font-semibold text-foreground truncate"
                    >{stream.name}</span
                  >
                  <span
                    class="text-[0.52rem] font-bold tracking-widest uppercase px-1.5 py-0.5 rounded
                               bg-primary/10 text-primary">{stream.type}</span
                  >
                  {#if stream.paired}
                    <span
                      class="text-[0.48rem] font-bold tracking-widest uppercase px-1.5 py-0.5 rounded
                                 bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
                    >
                      {t("lsl.paired")}
                    </span>
                  {/if}
                </div>
                <div class="flex items-center gap-3 text-[0.58rem] text-muted-foreground">
                  <span>{stream.channels}ch</span>
                  <span>·</span>
                  <span>{stream.sample_rate} Hz</span>
                  {#if stream.hostname}
                    <span>·</span>
                    <span class="truncate">{stream.hostname}</span>
                  {/if}
                </div>
                {#if stream.source_id}
                  <span class="text-[0.52rem] font-mono text-muted-foreground/50 truncate">
                    {stream.source_id}
                  </span>
                {/if}
              </div>

              <!-- Actions -->
              <div class="flex items-center gap-1.5 shrink-0">
                <!-- Pair/Unpair button -->
                <Button
                  variant="ghost"
                  size="sm"
                  class="h-7 text-[0.54rem] px-2 {stream.paired
                    ? 'text-emerald-600 dark:text-emerald-400'
                    : 'text-muted-foreground'}"
                  onclick={() => togglePair(stream.source_id, stream.paired)}
                >
                  {stream.paired ? t("lsl.unpair") : t("lsl.pair")}
                </Button>
                <!-- Connect button -->
                <Button
                  variant="default"
                  size="sm"
                  class="h-7 text-[0.58rem] px-3"
                  disabled={connecting !== null}
                  onclick={() => connectStream(stream.name)}
                >
                  {connecting === stream.name ? t("lsl.connecting") : t("lsl.connect")}
                </Button>
              </div>
            </div>
          {/each}
        </div>
      {:else if !scanning}
        <p class="text-[0.58rem] text-muted-foreground/50 italic">
          {t("lsl.noStreams")}
        </p>
      {/if}
    </CardContent>
  </Card>
</section>

<Separator class="bg-border dark:bg-white/[0.05]" />

<!-- ── Remote LSL via iroh (rlsl-iroh) ────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("lsl.irohRemote")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 p-4">
      <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
        {t("lsl.irohDesc")}
      </p>

      <!-- Status + controls -->
      <div class="flex items-center gap-2">
        {#if irohStatus.running}
          <span class="relative flex h-2 w-2 shrink-0">
            <span
              class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"
            ></span>
            <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
          <span class="text-[0.62rem] font-semibold text-emerald-600 dark:text-emerald-400">
            {t("lsl.irohRunning")}
          </span>
          <Button
            variant="outline"
            size="sm"
            class="h-7 text-[0.58rem] px-3 ml-auto border-red-500/30 text-red-500 hover:bg-red-500/10"
            onclick={stopIroh}
          >
            {t("lsl.irohStop")}
          </Button>
        {:else}
          <span class="relative flex h-2 w-2 shrink-0">
            <span class="relative inline-flex rounded-full h-2 w-2 bg-muted-foreground/30"></span>
          </span>
          <span class="text-[0.62rem] text-muted-foreground/60">
            {t("lsl.irohStopped")}
          </span>
          <Button
            variant="outline"
            size="sm"
            class="h-7 text-[0.58rem] px-3 ml-auto"
            disabled={irohStarting}
            onclick={startIroh}
          >
            {#if irohStarting}
              <span class="flex items-center gap-1.5">
                <span
                  class="h-3 w-3 border-2 border-current border-t-transparent rounded-full animate-spin"
                ></span>
                {t("lsl.irohStarting")}
              </span>
            {:else}
              {t("lsl.irohStart")}
            {/if}
          </Button>
        {/if}
      </div>

      {#if irohError}
        <p class="text-[0.58rem] text-red-500 leading-relaxed">{irohError}</p>
      {/if}

      <!-- Endpoint ID -->
      {#if irohStatus.endpoint_id}
        <div class="flex flex-col gap-2 rounded-lg bg-cyan-500/5 border border-cyan-500/20 px-4 py-3">
          <div class="flex items-center gap-2">
            <span
              class="text-[0.56rem] font-semibold tracking-widest uppercase text-cyan-600 dark:text-cyan-400"
            >
              {t("lsl.irohEndpointId")}
            </span>
            <button
              class="ml-auto text-[0.52rem] font-semibold px-2 py-0.5 rounded
                     border border-cyan-500/30 text-cyan-600 dark:text-cyan-400
                     hover:bg-cyan-500/10 transition-colors cursor-pointer"
              onclick={copyEndpointId}
            >
              {irohCopied ? t("lsl.irohCopied") : t("lsl.irohCopy")}
            </button>
          </div>
          <code class="text-[0.62rem] font-mono text-foreground break-all select-all leading-relaxed">
            {irohStatus.endpoint_id}
          </code>
          <p class="text-[0.54rem] text-muted-foreground/60 leading-relaxed">
            {t("lsl.irohEndpointIdHint")}
          </p>
        </div>
      {/if}
    </CardContent>
  </Card>
</section>
