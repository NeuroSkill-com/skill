<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- LSL tab — discover local LSL streams and manage rlsl-iroh remote sink. -->
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
}

interface LslIrohStatus {
  running: boolean;
  endpoint_id: string | null;
}

// ── State ──────────────────────────────────────────────────────────────────
let streams = $state<LslStream[]>([]);
let scanning = $state(false);
let connecting = $state<string | null>(null);
let scanError = $state("");

let irohStatus = $state<LslIrohStatus>({ running: false, endpoint_id: null });
let irohStarting = $state(false);
let irohError = $state("");
let irohCopied = $state(false);

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
  await refreshIrohStatus();
  // Poll iroh status every 5 s so UI reflects when remote connects
  pollTimer = setInterval(refreshIrohStatus, 5000);
});

onDestroy(() => {
  if (pollTimer) clearInterval(pollTimer);
});
</script>

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
        <Button variant="outline" size="sm"
                class="h-7 text-[0.62rem] px-3"
                disabled={scanning}
                onclick={scanStreams}>
          {#if scanning}
            <span class="flex items-center gap-1.5">
              <span class="h-3 w-3 border-2 border-current border-t-transparent rounded-full animate-spin"></span>
              {t("lsl.scanning")}
            </span>
          {:else}
            {t("lsl.scanButton")}
          {/if}
        </Button>
        {#if streams.length > 0}
          <span class="text-[0.58rem] text-muted-foreground">
            {streams.length} {streams.length === 1 ? "stream" : "streams"} found
          </span>
        {/if}
      </div>

      {#if scanError}
        <p class="text-[0.58rem] text-red-500 leading-relaxed">{scanError}</p>
      {/if}

      {#if streams.length > 0}
        <div class="flex flex-col gap-2 mt-1">
          {#each streams as stream (stream.source_id || stream.name)}
            <div class="flex items-center gap-3 rounded-lg border border-border dark:border-white/[0.08]
                        bg-muted/30 dark:bg-white/[0.02] px-4 py-3">
              <!-- Stream info -->
              <div class="flex flex-col gap-1 flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-[0.72rem] font-semibold text-foreground truncate">{stream.name}</span>
                  <span class="text-[0.52rem] font-bold tracking-widest uppercase px-1.5 py-0.5 rounded
                               bg-primary/10 text-primary">{stream.type}</span>
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

              <!-- Connect button -->
              <Button variant="default" size="sm"
                      class="h-7 text-[0.58rem] px-3 shrink-0"
                      disabled={connecting !== null}
                      onclick={() => connectStream(stream.name)}>
                {connecting === stream.name ? t("lsl.connecting") : t("lsl.connect")}
              </Button>
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
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
            <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
          </span>
          <span class="text-[0.62rem] font-semibold text-emerald-600 dark:text-emerald-400">
            {t("lsl.irohRunning")}
          </span>
          <Button variant="outline" size="sm"
                  class="h-7 text-[0.58rem] px-3 ml-auto border-red-500/30 text-red-500 hover:bg-red-500/10"
                  onclick={stopIroh}>
            {t("lsl.irohStop")}
          </Button>
        {:else}
          <span class="relative flex h-2 w-2 shrink-0">
            <span class="relative inline-flex rounded-full h-2 w-2 bg-muted-foreground/30"></span>
          </span>
          <span class="text-[0.62rem] text-muted-foreground/60">
            {t("lsl.irohStopped")}
          </span>
          <Button variant="outline" size="sm"
                  class="h-7 text-[0.58rem] px-3 ml-auto"
                  disabled={irohStarting}
                  onclick={startIroh}>
            {#if irohStarting}
              <span class="flex items-center gap-1.5">
                <span class="h-3 w-3 border-2 border-current border-t-transparent rounded-full animate-spin"></span>
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
            <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-cyan-600 dark:text-cyan-400">
              {t("lsl.irohEndpointId")}
            </span>
            <button
              class="ml-auto text-[0.52rem] font-semibold px-2 py-0.5 rounded
                     border border-cyan-500/30 text-cyan-600 dark:text-cyan-400
                     hover:bg-cyan-500/10 transition-colors cursor-pointer"
              onclick={copyEndpointId}>
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
