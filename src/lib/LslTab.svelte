<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- LSL tab — discover local LSL streams, pair for auto-connect, and manage rlsl-iroh remote sink. -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { onDestroy, onMount } from "svelte";

import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";
import type { DeviceStatus } from "$lib/types";

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

interface LslPairedStream {
  source_id: string;
  name: string;
  stream_type: string;
  channels: number;
  sample_rate: number;
}

interface LslConfig {
  auto_connect: boolean;
  paired_streams: LslPairedStream[];
}

interface SecondarySession {
  id: string;
  device_name: string;
  device_kind: string;
  channels: number;
  sample_rate: number;
  sample_count: number;
  csv_path: string;
  started_at: number;
  battery: number;
}

// ── State ──────────────────────────────────────────────────────────────────
let streams = $state<LslStream[]>([]);
let scanning = $state(false);
let connecting = $state<string | null>(null);
let scanError = $state("");
let lastScanTime = $state<number | null>(null);

let autoConnect = $state(false);
let pairedStreams = $state<LslPairedStream[]>([]);

// Idle timeout
let idleTimeoutSecs = $state<number | null>(15); // null = disabled
let idleTimeoutSaving = $state(false);

// Live session status
let sessionState = $state<string>("disconnected");
let sessionDeviceKind = $state<string>("");
let sessionDeviceName = $state<string | null>(null);
let sessionSampleCount = $state(0);

let secondarySessions = $state<SecondarySession[]>([]);

let irohStatus = $state<LslIrohStatus>({ running: false, endpoint_id: null });
let irohStarting = $state(false);
let irohError = $state("");
let irohCopied = $state(false);
let irohExpanded = $state(false);

let scanTimer: ReturnType<typeof setInterval> | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;
let unlisteners: UnlistenFn[] = [];

// ── Derived ────────────────────────────────────────────────────────────────
let isSessionActive = $derived(sessionState === "connected" || sessionState === "scanning");

let isLslSessionActive = $derived(isSessionActive && (sessionDeviceKind === "lsl" || sessionDeviceKind === "lsl-iroh"));

let isOtherSessionActive = $derived(isSessionActive && !isLslSessionActive);

let sortedStreams = $derived(
  [...streams].sort((a, b) => {
    // Paired first, then alphabetical
    if (a.paired !== b.paired) return a.paired ? -1 : 1;
    return a.name.localeCompare(b.name);
  }),
);

let lastScanLabel = $derived.by(() => {
  if (!lastScanTime) return "";
  const secs = Math.floor((Date.now() - lastScanTime) / 1000);
  if (secs < 5) return t("lsl.lastScanJustNow");
  if (secs < 60) return `${secs}s ago`;
  return `${Math.floor(secs / 60)}m ago`;
});

// ── Actions ────────────────────────────────────────────────────────────────
async function scanStreams() {
  scanning = true;
  scanError = "";
  try {
    streams = await invoke<LslStream[]>("lsl_discover");
    lastScanTime = Date.now();
  } catch (e: unknown) {
    scanError = String(e);
  } finally {
    scanning = false;
  }
}

async function connectStream(stream: LslStream) {
  connecting = stream.name;
  try {
    await invoke("lsl_connect", { name: stream.name });
  } catch (e: unknown) {
    scanError = String(e);
  } finally {
    connecting = null;
  }
}

async function switchToStream(stream: LslStream) {
  connecting = stream.name;
  try {
    await invoke("lsl_switch_session", { name: stream.name });
  } catch (e: unknown) {
    scanError = String(e);
  } finally {
    connecting = null;
  }
}

async function startSecondary(stream: LslStream) {
  connecting = stream.name;
  try {
    await invoke("lsl_start_secondary", { name: stream.name });
  } catch (e: unknown) {
    scanError = String(e);
  } finally {
    connecting = null;
  }
}

async function cancelSecondary(sessionId: string) {
  await invoke("lsl_cancel_secondary", { sessionId });
}

async function connectOrSwitch(stream: LslStream) {
  if (!stream.paired) {
    // Pair first
    await invoke("lsl_pair_stream", {
      sourceId: stream.source_id,
      name: stream.name,
      streamType: stream.type,
      channels: stream.channels,
      sampleRate: stream.sample_rate,
    });
    pairedStreams = [
      ...pairedStreams,
      {
        source_id: stream.source_id,
        name: stream.name,
        stream_type: stream.type,
        channels: stream.channels,
        sample_rate: stream.sample_rate,
      },
    ];
    streams = streams.map((s) => (s.source_id === stream.source_id ? { ...s, paired: true } : s));
  }
  // If another session is active, switch; otherwise connect
  if (isSessionActive) {
    await switchToStream(stream);
  } else {
    await connectStream(stream);
  }
}

async function pairAndConnect(stream: LslStream) {
  // Pair if not already
  if (!stream.paired) {
    await invoke("lsl_pair_stream", {
      sourceId: stream.source_id,
      name: stream.name,
      streamType: stream.type,
      channels: stream.channels,
      sampleRate: stream.sample_rate,
    });
    pairedStreams = [
      ...pairedStreams,
      {
        source_id: stream.source_id,
        name: stream.name,
        stream_type: stream.type,
        channels: stream.channels,
        sample_rate: stream.sample_rate,
      },
    ];
    streams = streams.map((s) => (s.source_id === stream.source_id ? { ...s, paired: true } : s));
  }
  // Connect
  await connectStream(stream);
}

async function togglePair(stream: LslStream) {
  if (stream.paired) {
    await invoke("lsl_unpair_stream", { sourceId: stream.source_id });
    pairedStreams = pairedStreams.filter((p) => p.source_id !== stream.source_id);
    streams = streams.map((s) => (s.source_id === stream.source_id ? { ...s, paired: false } : s));
  } else {
    await invoke("lsl_pair_stream", {
      sourceId: stream.source_id,
      name: stream.name,
      streamType: stream.type,
      channels: stream.channels,
      sampleRate: stream.sample_rate,
    });
    pairedStreams = [
      ...pairedStreams,
      {
        source_id: stream.source_id,
        name: stream.name,
        stream_type: stream.type,
        channels: stream.channels,
        sample_rate: stream.sample_rate,
      },
    ];
    streams = streams.map((s) => (s.source_id === stream.source_id ? { ...s, paired: true } : s));
  }
}

async function unpairById(sourceId: string) {
  await invoke("lsl_unpair_stream", { sourceId });
  pairedStreams = pairedStreams.filter((p) => p.source_id !== sourceId);
  streams = streams.map((s) => (s.source_id === sourceId ? { ...s, paired: false } : s));
}

async function setIdleTimeout(secs: number | null) {
  if (idleTimeoutSaving) return;
  idleTimeoutSaving = true;
  idleTimeoutSecs = secs;
  try {
    await invoke("lsl_set_idle_timeout", { secs: secs ?? null });
  } finally {
    idleTimeoutSaving = false;
  }
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

function fmtRate(hz: number): string {
  return hz % 1 === 0 ? `${hz}` : hz.toFixed(1);
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
onMount(async () => {
  try {
    const cfg = await invoke<LslConfig>("lsl_get_config");
    autoConnect = cfg.auto_connect;
    pairedStreams = cfg.paired_streams;
  } catch {
    /* ignore */
  }

  try {
    const t = await invoke<number | null>("lsl_get_idle_timeout");
    idleTimeoutSecs = t;
  } catch {
    /* ignore */
  }

  // Get initial session status
  try {
    const s = await invoke<DeviceStatus>("get_status");
    sessionState = s.state;
    sessionDeviceKind = s.device_kind;
    sessionDeviceName = s.device_name;
    sessionSampleCount = s.sample_count;
  } catch {
    /* ignore */
  }

  await refreshIrohStatus();
  await scanStreams();

  // Load initial secondary sessions
  try {
    secondarySessions = await invoke<SecondarySession[]>("list_secondary_sessions");
  } catch {
    /* ignore */
  }

  pollTimer = setInterval(refreshIrohStatus, 5000);
  manageAutoScanTimer();

  unlisteners.push(
    await listen("lsl-auto-connect", () => {
      scanStreams();
    }),
    await listen<DeviceStatus>("status", (ev) => {
      sessionState = ev.payload.state;
      sessionDeviceKind = ev.payload.device_kind;
      sessionDeviceName = ev.payload.device_name;
      sessionSampleCount = ev.payload.sample_count;
    }),
    await listen<SecondarySession[]>("secondary-sessions", (ev) => {
      secondarySessions = ev.payload;
    }),
  );
});

onDestroy(() => {
  if (pollTimer) clearInterval(pollTimer);
  if (scanTimer) clearInterval(scanTimer);
  for (const u of unlisteners) u();
});
</script>

<!-- ── Other-device session banner ─────────────────────────────────────── -->
{#if isOtherSessionActive}
  <Card class="border-amber-500/30 bg-amber-500/5 dark:bg-amber-500/5 gap-0 py-0 overflow-hidden">
    <CardContent class="flex items-center gap-3 p-4">
      <span class="relative flex h-2.5 w-2.5 shrink-0">
        <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-amber-500"></span>
      </span>
      <div class="flex flex-col gap-0.5 flex-1 min-w-0">
        <span class="text-[0.72rem] font-semibold text-amber-700 dark:text-amber-300 leading-tight">
          {t("lsl.otherSessionActive")}
        </span>
        <span class="text-[0.58rem] text-amber-600/70 dark:text-amber-400/70 truncate">
          {sessionDeviceName ?? sessionDeviceKind}
          {#if sessionSampleCount > 0}
            · {sessionSampleCount.toLocaleString()} samples
          {/if}
        </span>
      </div>
      <span
        class="text-[0.5rem] font-bold tracking-widest uppercase text-amber-600/60 dark:text-amber-400/60"
      >
        {t("lsl.switchHint")}
      </span>
    </CardContent>
  </Card>
{/if}

<!-- ── Live LSL Session Banner ────────────────────────────────────────────── -->
{#if isLslSessionActive}
  <Card class="border-emerald-500/30 bg-emerald-500/5 dark:bg-emerald-500/5 gap-0 py-0 overflow-hidden">
    <CardContent class="flex items-center gap-3 p-4">
      <span class="relative flex h-2.5 w-2.5 shrink-0">
        <span
          class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"
        ></span>
        <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-emerald-500"></span>
      </span>
      <div class="flex flex-col gap-0.5 flex-1 min-w-0">
        <span class="text-[0.72rem] font-semibold text-emerald-700 dark:text-emerald-300 leading-tight">
          {t("lsl.sessionActive")}
        </span>
        <span class="text-[0.58rem] text-emerald-600/70 dark:text-emerald-400/70 truncate">
          {sessionDeviceName ?? "LSL Stream"}
          {#if sessionSampleCount > 0}
            · {sessionSampleCount.toLocaleString()} samples
          {/if}
        </span>
      </div>
      <span
        class="text-[0.52rem] font-bold tracking-widest uppercase text-emerald-600 dark:text-emerald-400"
      >
        {sessionState === "scanning" ? t("lsl.connecting") : t("lsl.streaming")}
      </span>
    </CardContent>
  </Card>
{/if}

<!-- ── Secondary Sessions Strip ────────────────────────────────────────── -->
{#if secondarySessions.length > 0}
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("lsl.backgroundSessions")} ({secondarySessions.length})
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="py-0 px-0">
        {#each secondarySessions as sess (sess.id)}
          <div
            class="flex items-center gap-3 px-4 py-2.5 border-b last:border-b-0
                   border-border dark:border-white/[0.05]"
          >
            <span class="relative flex h-2 w-2 shrink-0">
              <span
                class="animate-ping absolute inline-flex h-full w-full rounded-full bg-violet-400 opacity-75"
              ></span>
              <span class="relative inline-flex rounded-full h-2 w-2 bg-violet-500"></span>
            </span>
            <div class="flex flex-col gap-0.5 flex-1 min-w-0">
              <span class="text-[0.68rem] font-semibold text-foreground truncate">
                {sess.device_name}
              </span>
              <span class="text-[0.54rem] text-muted-foreground">
                {sess.channels}ch · {fmtRate(sess.sample_rate)} Hz · {sess.sample_count.toLocaleString()} samples
              </span>
            </div>
            <span
              class="text-[0.46rem] font-bold tracking-widest uppercase px-1.5 py-0.5 rounded
                     bg-violet-500/10 text-violet-600 dark:text-violet-400"
            >
              {t("lsl.recording")}
            </span>
            <button
              class="text-muted-foreground/40 hover:text-red-500 cursor-pointer text-[0.7rem] shrink-0"
              onclick={() => cancelSecondary(sess.id)}
              title={t("lsl.stopRecording")}
            >
              ✕
            </button>
          </div>
        {/each}
      </CardContent>
    </Card>
  </section>
{/if}

<!-- ── Auto-Connect Toggle ────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("lsl.autoConnect")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">
      <button
        role="switch" aria-checked={autoConnect}
        onclick={toggleAutoConnect}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]"
      >
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
          <div class="flex flex-col gap-1.5">
            {#each pairedStreams as paired}
              <div class="flex items-center gap-2 text-[0.58rem]">
                <span class="font-semibold text-foreground truncate">{paired.name || paired.source_id}</span>
                {#if paired.name}
                  <span class="text-muted-foreground/40 font-mono text-[0.5rem] truncate"
                    >{paired.source_id}</span
                  >
                {/if}
                <span class="text-muted-foreground/50 shrink-0">
                  {paired.channels}ch · {fmtRate(paired.sample_rate)} Hz
                </span>
                <button
                  class="ml-auto text-muted-foreground/40 hover:text-red-500 cursor-pointer shrink-0 text-[0.6rem]"
                  onclick={() => unpairById(paired.source_id)}
                  title={t("lsl.unpair")}
                >
                  ✕
                </button>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </CardContent>
  </Card>
</section>

<!-- ── Idle Timeout ──────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("lsl.idleTimeout")}
    </span>
    {#if idleTimeoutSaving}
      <span class="text-[0.52rem] text-muted-foreground/60">saving…</span>
    {/if}
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
      <div class="flex flex-col gap-3 px-4 py-3.5">
        <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
          {t("lsl.idleTimeoutDesc")}
        </p>

        <div class="flex items-center gap-1.5 flex-wrap">
          <button
            onclick={() => setIdleTimeout(null)}
            class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold transition-all cursor-pointer select-none
                   {idleTimeoutSecs === null
                     ? 'border-slate-400/50 bg-slate-500/10 text-slate-600 dark:text-slate-300'
                     : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
            {t("lsl.idleTimeoutDisabled")}
          </button>
          {#each ([[10, "10 s"], [15, "15 s"], [30, "30 s"], [60, "60 s"], [120, "2 min"]] as [number, string][]) as [secs, label]}
            <button
              onclick={() => setIdleTimeout(secs)}
              class="rounded-lg border px-2.5 py-1.5 text-[0.66rem] font-semibold transition-all cursor-pointer select-none
                     {idleTimeoutSecs === secs
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] text-muted-foreground hover:text-foreground hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
              {label}
            </button>
          {/each}
        </div>

        <div class="flex items-center gap-1.5 text-[0.58rem]">
          {#if idleTimeoutSecs === null}
            <span class="text-muted-foreground/60">
              {t("lsl.idleTimeoutDisabled")} — stream will never auto-stop due to silence.
            </span>
          {:else}
            <span class="text-[0.52rem] font-bold tracking-widest uppercase text-violet-500">
              {t("lsl.idleTimeoutEnabled")}
            </span>
            <span class="text-muted-foreground/60">
              — stops after {idleTimeoutSecs}s of silence
            </span>
          {/if}
        </div>
      </div>
    </CardContent>
  </Card>
</section>

<!-- ── Local LSL Streams ──────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("lsl.localStreams")}
    </span>
    {#if lastScanLabel}
      <span class="ml-auto text-[0.5rem] text-muted-foreground/40">{lastScanLabel}</span>
    {/if}
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 p-4">
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
            {streams.length === 1 ? "stream" : "streams"}
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

      {#if sortedStreams.length > 0}
        <div class="flex flex-col gap-2 mt-1">
          {#each sortedStreams as stream (stream.source_id || stream.name)}
            <div
              class="flex items-center gap-3 rounded-lg border px-4 py-3 transition-colors
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
                    class="text-[0.5rem] font-bold tracking-widest uppercase px-1.5 py-0.5 rounded
                               bg-primary/10 text-primary">{stream.type}</span
                  >
                  {#if stream.paired}
                    <span
                      class="text-[0.46rem] font-bold tracking-widest uppercase px-1.5 py-0.5 rounded
                                 bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
                    >
                      {t("lsl.paired")}
                    </span>
                  {/if}
                </div>
                <div class="flex items-center gap-2 text-[0.58rem] text-muted-foreground">
                  <span>{stream.channels}ch</span>
                  <span class="text-muted-foreground/30">·</span>
                  <span>{fmtRate(stream.sample_rate)} Hz</span>
                  {#if stream.hostname}
                    <span class="text-muted-foreground/30">·</span>
                    <span class="truncate">{stream.hostname}</span>
                  {/if}
                </div>
              </div>

              <!-- Actions -->
              <div class="flex items-center gap-1.5 shrink-0">
                {#if stream.paired}
                  <Button
                    variant="ghost"
                    size="sm"
                    class="h-7 text-[0.54rem] px-2 text-muted-foreground/60 hover:text-red-500"
                    onclick={() => togglePair(stream)}
                  >
                    {t("lsl.unpair")}
                  </Button>
                {:else}
                  <Button
                    variant="ghost"
                    size="sm"
                    class="h-7 text-[0.54rem] px-2 text-muted-foreground"
                    onclick={() => togglePair(stream)}
                  >
                    {t("lsl.pair")}
                  </Button>
                {/if}
                {#if isSessionActive}
                  <!-- When a session is active, offer both Switch and Background -->
                  <Button
                    variant="outline"
                    size="sm"
                    class="h-7 text-[0.54rem] px-2 text-muted-foreground"
                    disabled={connecting !== null}
                    onclick={() => startSecondary(stream)}
                    title={t("lsl.backgroundHint")}
                  >
                    {connecting === stream.name ? "…" : t("lsl.background")}
                  </Button>
                  <Button
                    variant="default"
                    size="sm"
                    class="h-7 text-[0.58rem] px-3"
                    disabled={connecting !== null}
                    onclick={() => connectOrSwitch(stream)}
                  >
                    {connecting === stream.name ? t("lsl.connecting") : t("lsl.switchTo")}
                  </Button>
                {:else}
                  <Button
                    variant={stream.paired ? "default" : "outline"}
                    size="sm"
                    class="h-7 text-[0.58rem] px-3"
                    disabled={connecting !== null}
                    onclick={() => connectOrSwitch(stream)}
                  >
                    {#if connecting === stream.name}
                      {t("lsl.connecting")}
                    {:else if stream.paired}
                      {t("lsl.connect")}
                    {:else}
                      {t("lsl.pairAndConnect")}
                    {/if}
                  </Button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else if scanning}
        <div class="flex items-center justify-center py-6 gap-2">
          <span
            class="h-4 w-4 border-2 border-primary/40 border-t-primary rounded-full animate-spin"
          ></span>
          <span class="text-[0.62rem] text-muted-foreground">{t("lsl.scanningNetwork")}</span>
        </div>
      {:else}
        <!-- Empty state -->
        <div
          class="flex flex-col items-center gap-3 py-6 text-center"
        >
          <div class="text-[1.5rem] opacity-20">📡</div>
          <div class="flex flex-col gap-1">
            <span class="text-[0.68rem] font-medium text-muted-foreground/70">{t("lsl.noStreams")}</span>
            <span class="text-[0.56rem] text-muted-foreground/40 max-w-[28rem] leading-relaxed">
              {t("lsl.noStreamsHint")}
            </span>
          </div>
        </div>
      {/if}
    </CardContent>
  </Card>
</section>

<Separator class="bg-border dark:bg-white/[0.05]" />

<!-- ── Remote LSL via iroh — collapsible ──────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <button
    class="flex items-center gap-1.5 px-0.5 text-left cursor-pointer group"
    onclick={() => (irohExpanded = !irohExpanded)}
  >
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="w-3 h-3 text-muted-foreground/40 transition-transform {irohExpanded
        ? 'rotate-90'
        : ''}"
    >
      <path d="m9 18 6-6-6-6" />
    </svg>
    <span
      class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground group-hover:text-foreground/60 transition-colors"
    >
      {t("lsl.irohRemote")}
    </span>
    {#if irohStatus.running}
      <span class="relative flex h-1.5 w-1.5 ml-1">
        <span
          class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"
        ></span>
        <span class="relative inline-flex rounded-full h-1.5 w-1.5 bg-emerald-500"></span>
      </span>
    {/if}
  </button>

  {#if irohExpanded}
    <Card
      class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden"
    >
      <CardContent class="flex flex-col gap-3 p-4">
        <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
          {t("lsl.irohDesc")}
        </p>

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
              <span class="relative inline-flex rounded-full h-2 w-2 bg-muted-foreground/30"
              ></span>
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

        {#if irohStatus.endpoint_id}
          <div
            class="flex flex-col gap-2 rounded-lg bg-cyan-500/5 border border-cyan-500/20 px-4 py-3"
          >
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
            <code
              class="text-[0.62rem] font-mono text-foreground break-all select-all leading-relaxed"
            >
              {irohStatus.endpoint_id}
            </code>
            <p class="text-[0.54rem] text-muted-foreground/60 leading-relaxed">
              {t("lsl.irohEndpointIdHint")}
            </p>
          </div>
        {/if}
      </CardContent>
    </Card>
  {/if}
</section>
