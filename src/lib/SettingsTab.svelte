<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Settings tab — Storage format, GPU/memory, activity tracking, logging, data dir, WS server -->
<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { relaunch } from "@tauri-apps/plugin-process";
import { onDestroy, onMount } from "svelte";

import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";

// ── Types ──────────────────────────────────────────────────────────────────
interface LogConfig {
  embedder: boolean;
  bluetooth: boolean;
  websocket: boolean;
  csv: boolean;
  filter: boolean;
  bands: boolean;
  tts: boolean;
  llm: boolean;
  chat_store: boolean;
  history: boolean;
  hooks: boolean;
  tools: boolean;
}

interface GpuStats {
  render: number;
  tiler: number;
  overall: number;
  isUnifiedMemory: boolean;
  totalMemoryBytes: number | null;
  freeMemoryBytes: number | null;
}
let gpuStats = $state<GpuStats | null>(null);

// ── Geo Provider ───────────────────────────────────────────────────────────
type GeoProvider = "off" | "local" | "remote";
let geoProvider = $state<GeoProvider>("remote");

// Persist geoProvider in localStorage (or use invoke for backend persistence if needed)
onMount(() => {
  const stored = localStorage.getItem("geoProvider");
  if (stored === "off" || stored === "local" || stored === "remote") geoProvider = stored;
});
$effect(() => {
  localStorage.setItem("geoProvider", geoProvider);
});

let storageFormat = $state<"csv" | "parquet" | "both">("csv");
let logConfig = $state<LogConfig>({
  embedder: true,
  bluetooth: true,
  websocket: false,
  csv: false,
  filter: false,
  bands: false,
  tts: false,
  llm: false,
  chat_store: false,
  history: false,
  hooks: true,
  tools: false,
});
let dataDirCurrent = $state("");
let dataDirDefault = $state("");
let dataDirInput = $state("");
let dataDirSaving = $state(false);
let dataDirChanged = $state(false);
let now = $state(Math.floor(Date.now() / 1000));

// ── Activity tracking ────────────────────────────────────────────────────────
interface ActiveWindowInfo {
  app_name: string;
  app_path: string;
  window_title: string;
  activated_at: number;
}
let trackActiveWindow = $state(true);
let currentActiveWindow = $state<ActiveWindowInfo | null>(null);
let trackInputActivity = $state(true);
let mainWindowAutoFit = $state(true);
// [kbd_ts, mouse_ts] in unix seconds; 0 = never
let lastInputActivity = $state<[number, number]>([0, 0]);

// ── WS server config ────────────────────────────────────────────────────────
let wsHost = $state("127.0.0.1");
let wsPort = $state(8375);
let wsPortInput = $state("8375");
let wsHostChanged = $state(false);
let wsPortChanged = $state(false);
let wsPortError = $state("");
let wsSaving = $state(false);
let wsChanged = $derived(wsHostChanged || wsPortChanged);

// ── API token ───────────────────────────────────────────────────────────────
let apiToken = $state("");
let apiTokenInput = $state("");
let apiTokenDirty = $derived(apiTokenInput !== apiToken);

const OVERLAP_PRESETS: [string, number][] = [
  ["0 s — none", 0],
  ["1.25 s — 25%", 1.25],
  ["2.5 s — 50%", 2.5],
  ["3.75 s — 75%", 3.75],
  ["4.5 s — 90%", 4.5],
];

// ── Helpers ────────────────────────────────────────────────────────────────
function fmtLastSeen(ts: number) {
  if (ts === 0) return "never";
  const d = now - ts;
  if (d < 5) return "just now";
  if (d < 60) return `${d}s ago`;
  if (d < 3600) return `${Math.floor(d / 60)}m ago`;
  return `${Math.floor(d / 3600)}h ago`;
}

// ── Log config ────────────────────────────────────────────────────────────
async function toggleLog(key: keyof LogConfig) {
  const next = { ...logConfig, [key]: !logConfig[key] };
  logConfig = next;
  await invoke("set_log_config", { config: next });
}

// ── Lifecycle ──────────────────────────────────────────────────────────────
let unlisteners: UnlistenFn[] = [];
let nowTimer: ReturnType<typeof setInterval>;

onMount(async () => {
  gpuStats = await invoke<GpuStats | null>("get_gpu_stats").catch(() => null);
  storageFormat = (await invoke<string>("get_storage_format").catch(() => "csv")) as "csv" | "parquet" | "both";
  logConfig = await invoke<LogConfig>("get_log_config");
  {
    const [cur, def] = await invoke<[string, string]>("get_data_dir");
    dataDirCurrent = cur;
    dataDirDefault = def;
    dataDirInput = cur;
  }
  {
    const [h, p] = await invoke<[string, number]>("get_ws_config");
    wsHost = h;
    wsPort = p;
    wsPortInput = String(p);
  }
  apiToken = await invoke<string>("get_api_token");
  apiTokenInput = apiToken;
  trackActiveWindow = await invoke<boolean>("get_active_window_tracking");
  currentActiveWindow = await invoke<ActiveWindowInfo | null>("get_active_window");
  trackInputActivity = await invoke<boolean>("get_input_activity_tracking");
  mainWindowAutoFit = await invoke<boolean>("get_main_window_auto_fit").catch(() => true);
  lastInputActivity = await invoke<[number, number]>("get_last_input_activity");
  nowTimer = setInterval(() => (now = Math.floor(Date.now() / 1000)), 1000);

  unlisteners.push(
    await listen<ActiveWindowInfo | null>("active-window-changed", (ev) => {
      currentActiveWindow = ev.payload;
    }),
    await listen<[number, number]>("input-activity", (ev) => {
      lastInputActivity = ev.payload;
    }),
  );
});
onDestroy(() => {
  // biome-ignore lint/suspicious/useIterableCallbackReturn: unlisten fns return void-Promise, not a value
  unlisteners.forEach((u) => u());
  clearInterval(nowTimer);
});
</script>

<!-- ── Geo Provider Toggle ────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("settings.geoProvider")}
  </span>
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 p-4">
      <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
        {t("settings.geoProviderDesc")}
      </p>
      <div class="flex gap-2">
        <label class="flex items-center gap-2 cursor-pointer">
          <input type="radio" name="geoProvider" value="off" bind:group={geoProvider} />
          <span>{t("settings.geoProviderOff")}</span>
        </label>
        <label class="flex items-center gap-2 cursor-pointer">
          <input type="radio" name="geoProvider" value="local" bind:group={geoProvider} />
          <span>{t("settings.geoProviderLocal")}</span>
        </label>
        <label class="flex items-center gap-2 cursor-pointer">
          <input type="radio" name="geoProvider" value="remote" bind:group={geoProvider} />
          <span>{t("settings.geoProviderRemote")}</span>
        </label>
      </div>
    </CardContent>
  </Card>
</section>

<!-- ── Storage Format ───────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("settings.storageFormat")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 p-4">
      <p class="text-[0.64rem] text-muted-foreground leading-relaxed">
        {t("settings.storageFormatDesc")}
      </p>
      <div class="flex gap-2">
        {#each (["csv", "parquet", "both"] as const) as fmt}
          <button onclick={async () => { storageFormat = fmt; await invoke("set_storage_format", { format: fmt }); }}
            class="flex flex-col items-center gap-1 rounded-xl border px-4 py-3 flex-1
                   transition-all cursor-pointer select-none
                   {storageFormat === fmt
                     ? 'border-primary/50 bg-primary/10'
                     : 'border-border dark:border-white/[0.08] bg-muted dark:bg-[#1a1a28] hover:bg-slate-100 dark:hover:bg-white/[0.04]'}">
            <span class="text-[0.78rem] font-bold leading-tight
                         {storageFormat === fmt ? 'text-primary' : 'text-foreground'}">
              {fmt === "csv" ? "CSV" : fmt === "parquet" ? "Parquet" : t("settings.storageFormatBoth")}
            </span>
            <span class="text-[0.56rem] text-muted-foreground text-center leading-tight">
              {fmt === "csv" ? t("settings.storageFormatCsvDesc") :
               fmt === "parquet" ? t("settings.storageFormatParquetDesc") :
               t("settings.storageFormatBothDesc")}
            </span>
            {#if storageFormat === fmt}
              <span class="text-[0.52rem] font-bold tracking-widest uppercase text-primary mt-0.5">{t("common.active")}</span>
            {/if}
          </button>
        {/each}
      </div>
    </CardContent>
  </Card>
</section>

<!-- ── GPU / Memory ─────────────────────────────────────────────────────────── -->
{#if gpuStats}
  {@const fmtBytes = (b: number | null) => {
    if (b === null || b <= 0) return null;
    const gb = b / (1024 ** 3);
    return gb >= 1 ? `${gb.toFixed(1)} GB` : `${(b / (1024 ** 2)).toFixed(0)} MB`;
  }}
  {@const usedBytes  = (gpuStats.totalMemoryBytes !== null && gpuStats.freeMemoryBytes !== null)
    ? gpuStats.totalMemoryBytes - gpuStats.freeMemoryBytes : null}
  {@const usedPct    = (usedBytes !== null && gpuStats.totalMemoryBytes)
    ? Math.round(usedBytes / gpuStats.totalMemoryBytes * 100) : null}
  {@const memLabel   = gpuStats.isUnifiedMemory ? "Unified Memory (RAM)" : "VRAM"}

  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      GPU · {memLabel}
    </span>

    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">

        <!-- Memory bar -->
        {#if gpuStats.totalMemoryBytes}
          <div class="flex flex-col gap-2 px-4 py-3.5">
            <div class="flex items-baseline justify-between">
              <span class="text-[0.72rem] font-semibold text-foreground">{memLabel}</span>
              {#if fmtBytes(gpuStats.totalMemoryBytes)}
                <span class="text-[0.68rem] text-muted-foreground tabular-nums">
                  {fmtBytes(gpuStats.totalMemoryBytes)}
                  {#if gpuStats.isUnifiedMemory}<span class="text-[0.56rem] ml-0.5 text-muted-foreground/60">total</span>{/if}
                </span>
              {/if}
            </div>

            {#if usedPct !== null && gpuStats.freeMemoryBytes !== null}
              <!-- Progress bar -->
              <div class="h-2 w-full rounded-full bg-muted dark:bg-white/[0.07] overflow-hidden">
                <div
                  class="h-full rounded-full transition-all duration-500
                         {usedPct > 85 ? 'bg-red-500' : usedPct > 65 ? 'bg-amber-500' : 'bg-violet-500'}"
                  style="width: {usedPct}%">
                </div>
              </div>
              <div class="flex items-center justify-between text-[0.6rem] text-muted-foreground tabular-nums">
                <span>
                  {fmtBytes(usedBytes)} used
                  <span class="text-muted-foreground/50">·</span>
                  {fmtBytes(gpuStats.freeMemoryBytes)} free
                </span>
                <span class="{usedPct > 85 ? 'text-red-500' : usedPct > 65 ? 'text-amber-500' : ''}">
                  {usedPct}%
                </span>
              </div>
            {:else if gpuStats.freeMemoryBytes}
              <p class="text-[0.64rem] text-muted-foreground">
                {fmtBytes(gpuStats.freeMemoryBytes)} free
              </p>
            {/if}

            {#if gpuStats.isUnifiedMemory}
              <p class="text-[0.58rem] text-muted-foreground/60 leading-relaxed -mt-0.5">
                Apple Silicon uses a single unified memory pool shared by CPU and GPU.
                "Free" includes inactive pages that can be reclaimed immediately.
              </p>
            {/if}
          </div>
        {/if}

        <!-- GPU utilisation -->
        {#if gpuStats.overall > 0 || gpuStats.render > 0 || gpuStats.tiler > 0}
          <div class="flex items-center gap-4 px-4 py-3 bg-slate-50 dark:bg-[#111118]">
            <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground shrink-0">
              GPU Usage
            </span>
            {#each ([
              ["Render",  gpuStats.render],
              ["Tiler",   gpuStats.tiler],
              ["Overall", gpuStats.overall],
            ] as [string, number][]).filter(([, v]) => v > 0) as [label, val]}
              <div class="flex items-center gap-1.5">
                <div class="h-1.5 w-16 rounded-full bg-muted dark:bg-white/[0.07] overflow-hidden">
                  <div class="h-full rounded-full bg-violet-500/70 transition-all"
                       style="width:{Math.round(val * 100)}%"></div>
                </div>
                <span class="text-[0.58rem] text-muted-foreground tabular-nums">
                  {label} {Math.round(val * 100)}%
                </span>
              </div>
            {/each}
          </div>
        {/if}

      </CardContent>
    </Card>
  </section>
{/if}

<!-- ── Main Window ─────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    Main Window
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">
      <button
        onclick={async () => {
          mainWindowAutoFit = !mainWindowAutoFit;
          await invoke("set_main_window_auto_fit", { enabled: mainWindowAutoFit });
        }}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {mainWindowAutoFit ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
          <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {mainWindowAutoFit ? 'translate-x-4' : 'translate-x-0.5'}"></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            Auto-fit dashboard height
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            Expands or contracts the main window to match dashboard content, clamped to screen height.
          </span>
        </div>
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {mainWindowAutoFit ? 'text-emerald-500' : 'text-muted-foreground/50'}">
          {mainWindowAutoFit ? t("common.on") : t("common.off")}
        </span>
      </button>
    </CardContent>
  </Card>
</section>

<!-- ── Activity Tracking ────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("settings.activityTracking")}
    </span>
    <span class="ml-auto text-[0.52rem] text-muted-foreground/50">{t("settings.activityDb")}</span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">

      <!-- ── Active-window toggle ─────────────────────────────────────────── -->
      <button
        onclick={async () => {
          trackActiveWindow = !trackActiveWindow;
          await invoke("set_active_window_tracking", { enabled: trackActiveWindow });
          if (!trackActiveWindow) currentActiveWindow = null;
        }}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {trackActiveWindow ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
          <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {trackActiveWindow ? 'translate-x-4' : 'translate-x-0.5'}"></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("settings.activeWindowToggle")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("settings.activeWindowToggleDesc")}
          </span>
        </div>
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {trackActiveWindow ? 'text-emerald-500' : 'text-muted-foreground/50'}">
          {trackActiveWindow ? t("common.on") : t("common.off")}
        </span>
      </button>

      <!-- Current window preview -->
      {#if trackActiveWindow}
        <div class="border-t border-border dark:border-white/[0.05] px-4 py-3 flex flex-col gap-2 bg-muted/20 dark:bg-white/[0.01]">
          <span class="text-[0.54rem] font-semibold tracking-widest uppercase text-muted-foreground/70">
            {t("settings.activeWindowCurrent")}
          </span>
          {#if currentActiveWindow}
            <div class="flex flex-col gap-1.5">
              {#each ([
                [t("settings.activeWindowApp"),   currentActiveWindow.app_name,     "font-semibold text-foreground"],
                [t("settings.activeWindowTitle"),  currentActiveWindow.window_title, "text-foreground/80"],
                [t("settings.activeWindowPath"),   currentActiveWindow.app_path,     "font-mono text-muted-foreground"],
                [t("settings.activeWindowSince"),  fmtLastSeen(currentActiveWindow.activated_at), "text-muted-foreground"],
              ] as [string, string, string][]).filter(([, v]) => v) as [label, value, cls]}
                <div class="flex items-baseline gap-2">
                  <span class="text-[0.56rem] text-muted-foreground/55 shrink-0 w-[4.5rem] text-right">{label}</span>
                  <span class="text-[0.68rem] {cls} truncate">{value}</span>
                </div>
              {/each}
            </div>
          {:else}
            <p class="text-[0.62rem] text-muted-foreground/50 italic">{t("settings.activeWindowNone")}</p>
          {/if}
        </div>
      {/if}

      <Separator class="bg-border dark:bg-white/[0.05]" />

      <!-- ── Input-activity toggle ────────────────────────────────────────── -->
      <button
        onclick={async () => {
          trackInputActivity = !trackInputActivity;
          await invoke("set_input_activity_tracking", { enabled: trackInputActivity });
          if (!trackInputActivity) lastInputActivity = [0, 0];
        }}
        class="flex items-center gap-3 px-4 py-3.5 text-left transition-colors w-full
               hover:bg-slate-50 dark:hover:bg-white/[0.02]">
        <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                    {trackInputActivity ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
          <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                      {trackInputActivity ? 'translate-x-4' : 'translate-x-0.5'}"></div>
        </div>
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-[0.72rem] font-semibold text-foreground leading-tight">
            {t("settings.inputActivityToggle")}
          </span>
          <span class="text-[0.58rem] text-muted-foreground leading-tight">
            {t("settings.inputActivityToggleDesc")}
          </span>
        </div>
        <span class="ml-auto text-[0.52rem] font-bold tracking-widest uppercase shrink-0
                     {trackInputActivity ? 'text-emerald-500' : 'text-muted-foreground/50'}">
          {trackInputActivity ? t("common.on") : t("common.off")}
        </span>
      </button>

      <!-- Last keyboard / mouse timestamps + live status -->
      {#if trackInputActivity}
        {@const hasData = lastInputActivity[0] > 0 || lastInputActivity[1] > 0}
        <div class="border-t border-border dark:border-white/[0.05] px-4 py-3 flex flex-col gap-2.5 bg-muted/20 dark:bg-white/[0.01]">

          <!-- Live status badge -->
          <div class="flex items-center gap-2">
            <span class="relative flex h-2 w-2 shrink-0">
              {#if hasData}
                <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
                <span class="relative inline-flex rounded-full h-2 w-2 bg-emerald-500"></span>
              {:else}
                <span class="relative inline-flex rounded-full h-2 w-2 bg-muted-foreground/30"></span>
              {/if}
            </span>
            <span class="text-[0.62rem] font-semibold
                         {hasData ? 'text-emerald-600 dark:text-emerald-400' : 'text-muted-foreground/60'}">
              {hasData ? t("settings.inputActivityActive") : t("settings.inputActivityNoData")}
            </span>
          </div>

          <!-- Keyboard / mouse last-seen rows -->
          <div class="flex flex-col gap-1.5">
            {#each ([
              [t("settings.inputActivityKeyboard"), lastInputActivity[0]],
              [t("settings.inputActivityMouse"),    lastInputActivity[1]],
            ] as [string, number][]) as [label, ts]}
              <div class="flex items-baseline gap-2">
                <span class="text-[0.56rem] text-muted-foreground/55 shrink-0 w-[4.5rem] text-right">{label}</span>
                <span class="text-[0.68rem] {ts > 0 ? 'text-foreground/80' : 'text-muted-foreground/40 italic'}">
                  {ts > 0 ? fmtLastSeen(ts) : t("settings.inputActivityNever")}
                </span>
              </div>
            {/each}
          </div>

          <!-- No-permission note (static info, always shown) -->
          <p class="text-[0.54rem] text-muted-foreground/50 leading-relaxed">
            {t("settings.inputActivityPermNote")}
          </p>
        </div>
      {/if}

    </CardContent>
  </Card>
</section>

<!-- ── Logging ───────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
      {t("settings.logging")}
    </span>
    <span class="ml-auto text-[0.56rem] text-muted-foreground/60">{dataDirCurrent}/log_config.json</span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="py-0 px-0">
      <div class="grid grid-cols-2 divide-x divide-y divide-border dark:divide-white/[0.05]">
        {#each ([
          ["embedder",  t("settings.logEmbedder"),   t("settings.logEmbedderDesc")],
          ["bluetooth", t("settings.logBluetooth"),   t("settings.logBluetoothDesc")],
          ["websocket", t("settings.logWebsocket"),   t("settings.logWebsocketDesc")],
          ["csv",       t("settings.logCsv"),         t("settings.logCsvDesc")],
          ["filter",    t("settings.logFilter"),       t("settings.logFilterDesc")],
          ["bands",     t("settings.logBands"),        t("settings.logBandsDesc")],
          ["tts",        t("settings.logTts"),          t("settings.logTtsDesc")],
          ["llm",        t("settings.logLlm"),          t("settings.logLlmDesc")],
          ["chat_store", t("settings.logChatStore"),     t("settings.logChatStoreDesc")],
          ["history",    t("settings.logHistory"),       t("settings.logHistoryDesc")],
          ["hooks",     t("settings.logHooks"),        t("settings.logHooksDesc")],
          ["tools",     t("settings.logTools"),        t("settings.logToolsDesc")],
        ] as [keyof LogConfig, string, string][]) as [key, label, desc]}
          <button
            onclick={() => toggleLog(key)}
            class="flex items-center gap-3 px-4 py-3 text-left transition-colors
                   hover:bg-slate-50 dark:hover:bg-white/[0.02]">
            <!-- Toggle pill -->
            <div class="relative shrink-0 w-8 h-4 rounded-full transition-colors
                        {logConfig[key] ? 'bg-emerald-500' : 'bg-muted dark:bg-white/[0.08]'}">
              <div class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
                          {logConfig[key] ? 'translate-x-4' : 'translate-x-0.5'}"></div>
            </div>
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.72rem] font-semibold text-foreground leading-tight">{label}</span>
              <span class="text-[0.58rem] text-muted-foreground leading-tight truncate">{desc}</span>
            </div>
          </button>
        {/each}
      </div>
    </CardContent>
  </Card>
</section>

<!-- ── Data Directory ──────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("settings.dataDir")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 py-3">
      <p class="text-[0.62rem] text-muted-foreground leading-relaxed">
        {t("settings.dataDirDesc")}
      </p>

      <div class="flex flex-col gap-1">
        <span class="text-[0.54rem] text-muted-foreground/60">
          {t("settings.dataDirDefault", { path: dataDirDefault })}
        </span>
      </div>

      <div class="flex items-center gap-2">
        <input type="text"
               bind:value={dataDirInput}
               oninput={() => { dataDirChanged = dataDirInput !== dataDirCurrent; }}
               placeholder={dataDirDefault}
               class="flex-1 h-7 rounded-md border border-border bg-background px-2 text-[0.68rem]
                      font-mono text-foreground placeholder:text-muted-foreground/40
                      focus:outline-none focus:ring-1 focus:ring-ring" />
        <Button variant="outline" size="sm"
                class="h-7 text-[0.58rem] px-2.5 border-border dark:border-white/10"
                onclick={async () => { await invoke("open_skill_dir"); }}>
          {t("settings.dataDirOpen")}
        </Button>
        {#if dataDirInput !== dataDirDefault}
          <Button variant="ghost" size="sm"
                  class="h-7 text-[0.58rem] px-2 text-muted-foreground hover:text-foreground"
                  onclick={() => { dataDirInput = dataDirDefault; dataDirChanged = dataDirInput !== dataDirCurrent; }}>
            {t("settings.dataDirReset")}
          </Button>
        {/if}
      </div>

      {#if dataDirChanged}
        <div class="flex items-center gap-2 rounded-lg bg-amber-500/10 border border-amber-500/20 px-3 py-2">
          <span class="text-[0.58rem] text-amber-600 dark:text-amber-400 flex-1">
            {t("settings.dataDirRestart")}
          </span>
          <Button variant="outline" size="sm"
                  class="h-7 text-[0.58rem] px-3"
                  disabled={dataDirSaving}
                  onclick={async () => {
                    dataDirSaving = true;
                    try {
                      const val = dataDirInput === dataDirDefault ? "" : dataDirInput;
                      await invoke("set_data_dir", { path: val });
                      dataDirCurrent = dataDirInput;
                      dataDirChanged = false;
                      // Offer restart
                      try { await relaunch(); } catch { /* user can restart manually */ }
                    } catch (e: unknown) {
                      console.error("set_data_dir error:", e);
                    } finally {
                      dataDirSaving = false;
                    }
                  }}>
            {dataDirSaving ? "…" : t("settings.dataDirRestartNow")}
          </Button>
        </div>
      {/if}
    </CardContent>
  </Card>
</section>

<!-- ── WebSocket Server ──────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("settings.wsConfig")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 py-3">

      <!-- Host selector -->
      <div class="flex flex-col gap-1">
        <span class="text-[0.62rem] font-medium text-foreground">{t("settings.wsHost")}</span>
        <div class="flex flex-col gap-1.5 mt-0.5">
          {#each [["127.0.0.1", t("settings.wsHostLoopback")], ["0.0.0.0", t("settings.wsHostLan")]] as [val, lbl]}
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="radio" name="wsHost" value={val}
                     checked={wsHost === val}
                     onchange={() => { wsHost = val; wsHostChanged = true; }}
                class="accent-violet-500" />
              <span class="text-[0.68rem] text-foreground">{lbl}</span>
            </label>
          {/each}
        </div>
        {#if wsHost === "0.0.0.0"}
          <p class="text-[0.58rem] text-amber-600 dark:text-amber-400 leading-relaxed mt-0.5">
            {t("settings.wsHostDesc")}
          </p>
        {/if}
      </div>

      <Separator />

      <!-- Port input -->
      <div class="flex flex-col gap-1">
        <span class="text-[0.62rem] font-medium text-foreground">{t("settings.wsPort")}</span>
        <p class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("settings.wsPortDesc")}</p>
        <div class="flex items-center gap-2">
          <input type="number" min="1024" max="65535"
                 bind:value={wsPortInput}
                 oninput={() => {
                   const n = parseInt(wsPortInput, 10);
                   if (isNaN(n) || n < 1024 || n > 65535) {
                     wsPortError = t("settings.wsPortInvalid");
                     wsPortChanged = false;
                   } else {
                     wsPortError = "";
                     wsPortChanged = n !== wsPort;
                   }
                 }}
                 class="w-28 h-7 rounded-md border border-border bg-background px-2 text-[0.68rem]
                        font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          {#if wsPortError}
            <span class="text-[0.58rem] text-red-500">{wsPortError}</span>
          {/if}
        </div>
      </div>

      <!-- Save banner -->
      {#if wsChanged && !wsPortError}
        <div class="flex items-center gap-2 rounded-lg bg-cyan-500/10 border border-cyan-500/20 px-3 py-2">
          <span class="text-[0.58rem] text-cyan-600 dark:text-cyan-400 flex-1">
            Apply changes to the server.
          </span>
          <Button variant="outline" size="sm"
                  class="h-7 text-[0.58rem] px-3"
                  disabled={wsSaving}
                  onclick={async () => {
                    const port = parseInt(wsPortInput, 10);
                    if (isNaN(port) || port < 1024 || port > 65535) return;
                    wsSaving = true;
                    try {
                      const newPort = await invoke<number>("set_ws_config", { host: wsHost, port });
                      wsPort = newPort;
                      wsPortInput = String(newPort);
                      wsHostChanged = false;
                      wsPortChanged = false;
                    } catch (e: unknown) {
                      console.error("set_ws_config error:", e);
                    } finally {
                      wsSaving = false;
                    }
                  }}>
            {wsSaving ? "Applying…" : "Apply"}
          </Button>
        </div>
      {/if}

    </CardContent>
  </Card>
</section>

<!-- ── API Token ────────────────────────────────────────────────────────────── -->
<section class="flex flex-col gap-2">
  <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
    {t("settings.apiToken")}
  </span>

  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col gap-3 py-3">
      <div class="flex flex-col gap-1">
        <span class="text-[0.62rem] font-medium text-foreground">{t("settings.apiTokenLabel")}</span>
        <p class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("settings.apiTokenDesc")}</p>
        <div class="flex items-center gap-2 mt-1">
          <input type="password"
                 bind:value={apiTokenInput}
                 placeholder={t("settings.apiTokenPlaceholder")}
                 class="flex-1 h-7 rounded-md border border-border bg-background px-2 text-[0.68rem]
                        font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          {#if apiTokenDirty}
            <Button variant="outline" size="sm"
                    class="h-7 text-[0.58rem] px-3"
                    onclick={async () => {
                      await invoke("set_api_token", { token: apiTokenInput });
                      apiToken = apiTokenInput;
                    }}>
              {t("common.save")}
            </Button>
          {/if}
        </div>
        {#if apiToken}
          <p class="text-[0.52rem] text-emerald-600 dark:text-emerald-400 mt-0.5">
            {t("settings.apiTokenActive")}
          </p>
        {:else}
          <p class="text-[0.52rem] text-muted-foreground/60 mt-0.5">
            {t("settings.apiTokenNone")}
          </p>
        {/if}
      </div>
    </CardContent>
  </Card>
</section>

