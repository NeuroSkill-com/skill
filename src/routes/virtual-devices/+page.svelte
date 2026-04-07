<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3 only. -->
<!-- Virtual Devices window — start pre-configured virtual EEG devices via daemon. -->
<script lang="ts">
import { onDestroy, onMount } from "svelte";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import {
  lslConnect,
  lslStartVirtualSourceConfigured,
  lslStopVirtualSource,
  lslVirtualSourceRunning,
} from "$lib/daemon/lsl";
import { t } from "$lib/i18n/index.svelte";
import { addToast } from "$lib/stores/toast.svelte";
import { useWindowTitle } from "$lib/stores/window-title.svelte";
import type { LineNoise, SignalQuality, SignalTemplate, VirtualEegConfig } from "$lib/virtual-eeg/generator";
// Preview only — offline JS signal shape, not live data
import { generateSamples, getChannelLabels } from "$lib/virtual-eeg/generator";

useWindowTitle("window.title.virtualDevices");

// ── Preset definitions ────────────────────────────────────────────────────

interface DevicePreset {
  id: string;
  name: string;
  desc: string;
  icon: string;
  tags: string[];
  config: VirtualEegConfig;
}

const BASE: Omit<VirtualEegConfig, "channels" | "sampleRate"> = {
  template: "good_quality",
  quality: "good",
  amplitudeUv: 50,
  noiseUv: 5,
  lineNoise: "none",
  dropoutProb: 0,
  fileData: null,
  fileName: null,
};

const PRESETS: DevicePreset[] = [
  {
    id: "muse_s",
    name: "Muse S",
    desc: t("vdev.presetMuseDesc"),
    icon: "🧠",
    tags: ["4ch", "256 Hz"],
    config: { ...BASE, channels: 4, sampleRate: 256, amplitudeUv: 45 },
  },
  {
    id: "openbci_cyton",
    name: "OpenBCI Cyton",
    desc: t("vdev.presetCytonDesc"),
    icon: "🔬",
    tags: ["8ch", "256 Hz"],
    config: { ...BASE, channels: 8, sampleRate: 256, quality: "excellent", amplitudeUv: 60, noiseUv: 3 },
  },
  {
    id: "eeg_cap_32",
    name: t("vdev.presetCap32"),
    desc: t("vdev.presetCap32Desc"),
    icon: "🎓",
    tags: ["32ch", "256 Hz"],
    config: { ...BASE, channels: 32, sampleRate: 256 },
  },
  {
    id: "clean_alpha",
    name: t("vdev.presetAlpha"),
    desc: t("vdev.presetAlphaDesc"),
    icon: "🌊",
    tags: ["4ch", "256 Hz", "α"],
    config: { ...BASE, channels: 4, sampleRate: 256, quality: "excellent", amplitudeUv: 80, noiseUv: 2 },
  },
  {
    id: "artifact_test",
    name: t("vdev.presetArtifact"),
    desc: t("vdev.presetArtifactDesc"),
    icon: "⚡",
    tags: ["8ch", "256 Hz", "noisy"],
    config: {
      ...BASE,
      channels: 8,
      sampleRate: 256,
      template: "bad_quality",
      quality: "poor",
      noiseUv: 30,
      lineNoise: "50hz",
    },
  },
  {
    id: "dropout_test",
    name: t("vdev.presetDropout"),
    desc: t("vdev.presetDropoutDesc"),
    icon: "📶",
    tags: ["4ch", "256 Hz", "lossy"],
    config: {
      ...BASE,
      channels: 4,
      sampleRate: 256,
      template: "interruptions",
      quality: "fair",
      noiseUv: 8,
      dropoutProb: 0.3,
    },
  },
  {
    id: "minimal",
    name: t("vdev.presetMinimal"),
    desc: t("vdev.presetMinimalDesc"),
    icon: "〜",
    tags: ["1ch", "128 Hz"],
    config: { ...BASE, channels: 1, sampleRate: 128, template: "sine", noiseUv: 2 },
  },
];

const CHANNEL_OPTIONS = [1, 2, 4, 8, 16, 32];
const RATE_OPTIONS = [128, 256, 512, 1000];
const QUALITY_OPTIONS: { key: SignalQuality; label: string }[] = [
  { key: "poor", label: "veeg.qualityPoor" },
  { key: "fair", label: "veeg.qualityFair" },
  { key: "good", label: "veeg.qualityGood" },
  { key: "excellent", label: "veeg.qualityExcellent" },
];
const TEMPLATES: { key: SignalTemplate; label: string; desc: string }[] = [
  { key: "sine", label: "vdev.cfgTemplateSine", desc: "vdev.cfgTemplateSineDesc" },
  { key: "good_quality", label: "vdev.cfgTemplateGood", desc: "vdev.cfgTemplateGoodDesc" },
  { key: "bad_quality", label: "vdev.cfgTemplateBad", desc: "vdev.cfgTemplateBadDesc" },
  { key: "interruptions", label: "vdev.cfgTemplateInterruptions", desc: "vdev.cfgTemplateInterruptionsDesc" },
];
const LINE_NOISE_OPTIONS: { key: LineNoise; label: string }[] = [
  { key: "none", label: "vdev.cfgLineNoiseNone" },
  { key: "50hz", label: "vdev.cfgLineNoise50" },
  { key: "60hz", label: "vdev.cfgLineNoise60" },
];

// ── State ─────────────────────────────────────────────────────────────────

let selectedPresetId = $state<string>("muse_s");
let showCustom = $derived(selectedPresetId === "custom");
let isCustomSelected = $derived(selectedPresetId === "custom");

let config = $state<VirtualEegConfig>({ ...PRESETS[0].config });
let showAdvanced = $state(false);

// ── Daemon state ──────────────────────────────────────────────────────────

/** Virtual LSL source is running inside the daemon. */
let lslRunning = $state(false);
/** Dashboard session state ("disconnected" | "scanning" | "connected" …). */
let sessionState = $state<string>("disconnected");
/** An async action (start / stop) is in flight. */
let busy = $state(false);

// ── Poll timer ────────────────────────────────────────────────────────────

let pollTimer: ReturnType<typeof setInterval> | null = null;

// ── Preview canvas (offline — no live data) ───────────────────────────────

let previewCanvas = $state<HTMLCanvasElement | null>(null);
let previewTimer: ReturnType<typeof setInterval> | null = null;
let previewIdx = $state(0);
let previewBuffers = $state<number[][]>([]);
let channelLabels = $derived(getChannelLabels(config.channels));

// ── Preset selection ──────────────────────────────────────────────────────

function selectPreset(preset: DevicePreset) {
  if (lslRunning) return;
  selectedPresetId = preset.id;
  config = { ...preset.config };
}

function selectCustom() {
  if (lslRunning) return;
  selectedPresetId = "custom";
}

// ── Daemon actions ────────────────────────────────────────────────────────
//
// Architecture: Virtual Devices window → daemon (HTTP) → virtual LSL source
//               → dashboard connects via normal LSL session start
//               → daemon streams EEG over WebSocket → dashboard charts

async function start() {
  if (busy || lslRunning) return;
  busy = true;
  try {
    // 1. Tell the daemon to start a virtual LSL source with the preset config.
    //    The daemon creates "SkillVirtualEEG" on the local network with the
    //    requested channels, rate, template, quality, noise, etc.
    await lslStartVirtualSourceConfigured(toSourceConfig(config));
    lslRunning = true;

    // 2. Give the LSL source a moment to announce itself.
    await new Promise<void>((r) => setTimeout(r, 600));

    // 3. Connect the dashboard to it through the normal LSL session path.
    //    The daemon discovers the source, opens a session, and starts
    //    streaming EEG events over the daemon WebSocket → dashboard.
    await lslConnect("SkillVirtualEEG");
  } catch (e: unknown) {
    if (lslRunning) {
      await lslStopVirtualSource().catch(() => {});
      lslRunning = false;
    }
    addToast("error", t("vdev.lslSourceTitle"), String(e));
  } finally {
    busy = false;
  }
}

async function stop() {
  if (busy) return;
  busy = true;
  try {
    await daemonInvoke("cancel_session", {}).catch(() => {});
    await lslStopVirtualSource();
    lslRunning = false;
    sessionState = "disconnected";
  } catch (e: unknown) {
    addToast("error", t("vdev.lslSourceTitle"), String(e));
  } finally {
    busy = false;
  }
}

function toSourceConfig(c: VirtualEegConfig) {
  return {
    channels: c.channels,
    sampleRate: c.sampleRate,
    template: c.template,
    quality: c.quality,
    amplitudeUv: c.amplitudeUv,
    noiseUv: c.noiseUv,
    lineNoise: c.lineNoise,
    dropoutProb: c.dropoutProb,
  };
}

// ── Status polling ────────────────────────────────────────────────────────

async function pollStatus() {
  try {
    lslRunning = await lslVirtualSourceRunning();
    if (lslRunning) {
      const s = await daemonInvoke<{ state: string }>("get_status");
      sessionState = s.state ?? "disconnected";
    } else {
      sessionState = "disconnected";
    }
  } catch {
    /* ignore — daemon may be starting */
  }
}

// ── Signal preview (offline, JS-only) ────────────────────────────────────

function startPreview() {
  stopPreview();
  const ch = Math.min(config.channels, 4);
  previewBuffers = Array.from({ length: ch }, () => []);
  previewIdx = 0;
  previewTimer = setInterval(
    () => {
      for (let i = 0; i < 8; i++) {
        const smp = generateSamples(config, previewIdx++);
        for (let c = 0; c < ch; c++) {
          previewBuffers[c].push(smp[c]);
          if (previewBuffers[c].length > 512) previewBuffers[c].shift();
        }
      }
      drawPreview();
    },
    (1000 * 8) / config.sampleRate,
  );
}

function stopPreview() {
  if (previewTimer) {
    clearInterval(previewTimer);
    previewTimer = null;
  }
}

function drawPreview() {
  const canvas = previewCanvas;
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const w = canvas.width;
  const h = canvas.height;
  const ch = previewBuffers.length;
  const chH = h / Math.max(ch, 1);
  ctx.fillStyle = "#0c0c14";
  ctx.fillRect(0, 0, w, h);
  const colors = ["#22c55e", "#3b82f6", "#f59e0b", "#ef4444"];
  for (let i = 0; i < ch; i++) {
    const buf = previewBuffers[i];
    if (buf.length < 2) continue;
    const yC = chH * i + chH / 2;
    const scale = chH / (config.amplitudeUv * 4 || 200);
    ctx.strokeStyle = colors[i % colors.length];
    ctx.lineWidth = 1;
    ctx.beginPath();
    buf.forEach((v, x) => {
      const px = (x / 512) * w;
      const py = yC - v * scale;
      x === 0 ? ctx.moveTo(px, py) : ctx.lineTo(px, py);
    });
    ctx.stroke();
    ctx.fillStyle = colors[i % colors.length];
    ctx.font = "10px monospace";
    ctx.fillText(channelLabels[i] ?? `Ch${i + 1}`, 4, chH * i + 12);
  }
}

// Restart preview whenever config changes (reactive)
$effect(() => {
  void config;
  startPreview();
  return () => stopPreview();
});

// ── Lifecycle ─────────────────────────────────────────────────────────────

onMount(async () => {
  await pollStatus();
  pollTimer = setInterval(pollStatus, 2000);
});

onDestroy(() => {
  if (pollTimer) clearInterval(pollTimer);
  stopPreview();
});
</script>

<!-- ── Layout: full-height, scrollable ───────────────────────────────────── -->
<main class="h-full min-h-0 overflow-y-auto px-5 pt-5 pb-8 flex flex-col gap-5">

  <!-- Page header -->
  <div class="flex flex-col gap-1 shrink-0">
    <h1 class="text-[0.82rem] font-bold tracking-tight text-foreground">{t("vdev.title")}</h1>
    <p class="text-[0.6rem] text-muted-foreground leading-relaxed max-w-[520px]">{t("vdev.desc")}</p>
  </div>

  <!-- ── Status bar ──────────────────────────────────────────────────────── -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden shrink-0
              {lslRunning ? 'ring-1 ring-green-500/30' : ''}">
    <CardContent class="flex items-center gap-3 py-3 px-4">

      <!-- Source indicator -->
      <span class="relative flex h-2.5 w-2.5 shrink-0">
        {#if lslRunning}
          <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-60"></span>
          <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-green-500"></span>
        {:else}
          <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-muted-foreground/25"></span>
        {/if}
      </span>

      <!-- Status text -->
      <div class="flex flex-col gap-0.5 flex-1 min-w-0">
        <div class="flex items-center gap-2 flex-wrap">
          <!-- Source pill -->
          <span class="inline-flex items-center gap-1 text-[0.52rem] font-bold tracking-widest uppercase
                       px-1.5 py-0.5 rounded-full
                       {lslRunning
                         ? 'bg-green-500/15 text-green-600 dark:text-green-400'
                         : 'bg-muted text-muted-foreground/50'}">
            {t("vdev.statusSource")}: {lslRunning ? "ON" : "OFF"}
          </span>

          <!-- Session pill — reflects actual daemon session state -->
          {#if lslRunning}
            <span class="inline-flex items-center gap-1 text-[0.52rem] font-bold tracking-widest uppercase
                         px-1.5 py-0.5 rounded-full
                         {sessionState === 'connected'
                           ? 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400'
                           : sessionState === 'scanning' || sessionState === 'connecting'
                             ? 'bg-amber-500/15 text-amber-600 dark:text-amber-300'
                             : 'bg-muted text-muted-foreground/50'}">
              {t("vdev.statusSession")}:
              {sessionState === 'connected' ? t("vdev.sessionConnected")
              : sessionState === 'scanning' || sessionState === 'connecting' ? t("vdev.sessionConnecting")
              : t("vdev.sessionDisconnected")}
            </span>
          {/if}
        </div>

        {#if lslRunning}
          <span class="text-[0.5rem] text-muted-foreground/50 tabular-nums font-mono">
            {config.channels}ch · {config.sampleRate} Hz · {t(`veeg.quality${config.quality.charAt(0).toUpperCase()}${config.quality.slice(1)}`)}
          </span>
        {:else if selectedPresetId && selectedPresetId !== "custom"}
          {@const p = PRESETS.find((x) => x.id === selectedPresetId)}
          {#if p}
            <span class="text-[0.5rem] text-muted-foreground/40">
              {t("vdev.selected")}: {p.name} · {p.tags.join(" · ")}
            </span>
          {/if}
        {/if}
      </div>

      <!-- Start / Stop -->
      <Button
        variant={lslRunning ? "destructive" : "default"}
        size="sm"
        class="h-7 text-[0.62rem] px-4 shrink-0"
        disabled={busy}
        onclick={lslRunning ? stop : start}
      >
        {#if busy}
          <span class="flex items-center gap-1.5">
            <span class="h-3 w-3 border-2 border-current border-t-transparent rounded-full animate-spin"></span>
          </span>
        {:else if lslRunning}
          {t("vdev.stopBtn")}
        {:else}
          {t("vdev.startBtn")}
        {/if}
      </Button>
    </CardContent>
  </Card>

  <!-- ── Preset grid ─────────────────────────────────────────────────────── -->
  <section class="flex flex-col gap-2 shrink-0">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("vdev.presets")}
    </span>

    <div class="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4">
      {#each PRESETS as preset}
        {@const isSelected = selectedPresetId === preset.id}
        <button
          class="flex flex-col gap-1.5 rounded-xl border px-3 py-3 text-left transition-all cursor-pointer
                 {isSelected
                   ? 'border-primary/40 bg-primary/8 ring-1 ring-primary/20'
                   : 'border-border dark:border-white/[0.07] bg-white dark:bg-[#14141e] hover:border-primary/20 hover:bg-muted/30'}
                 {lslRunning ? 'opacity-40 pointer-events-none' : ''}"
          onclick={() => selectPreset(preset)}
          disabled={lslRunning}
          aria-pressed={isSelected}
        >
          <span class="text-[1.1rem] leading-none">{preset.icon}</span>
          <span class="text-[0.66rem] font-semibold text-foreground leading-tight">{preset.name}</span>
          <span class="text-[0.52rem] text-muted-foreground leading-relaxed line-clamp-2">{preset.desc}</span>
          <div class="flex flex-wrap gap-1 mt-auto pt-1">
            {#each preset.tags as tag}
              <span class="inline-block rounded-full px-1.5 py-0.5 text-[0.44rem] font-semibold tracking-wide
                           {isSelected ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground'}">
                {tag}
              </span>
            {/each}
          </div>
        </button>
      {/each}

      <!-- Custom card -->
      <button
        class="flex flex-col gap-1.5 rounded-xl border px-3 py-3 text-left transition-all cursor-pointer
               {isCustomSelected
                 ? 'border-primary/40 bg-primary/8 ring-1 ring-primary/20'
                 : 'border-dashed border-border dark:border-white/[0.07] bg-white dark:bg-[#14141e] hover:border-primary/20 hover:bg-muted/30'}
               {lslRunning ? 'opacity-40 pointer-events-none' : ''}"
        onclick={selectCustom}
        disabled={lslRunning}
        aria-pressed={isCustomSelected}
      >
        <span class="text-[1.1rem] leading-none">⚙️</span>
        <span class="text-[0.66rem] font-semibold text-foreground leading-tight">{t("vdev.presetCustom")}</span>
        <span class="text-[0.52rem] text-muted-foreground leading-relaxed">{t("vdev.presetCustomDesc")}</span>
        <div class="flex flex-wrap gap-1 mt-auto pt-1">
          <span class="inline-block rounded-full px-1.5 py-0.5 text-[0.44rem] font-semibold tracking-wide
                       {isCustomSelected ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground'}">
            {t("vdev.configure")}
          </span>
        </div>
      </button>
    </div>
  </section>

  <!-- ── Custom configurator ─────────────────────────────────────────────── -->
  {#if showCustom}
    <section class="flex flex-col gap-4 shrink-0">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
        {t("vdev.customConfig")}
      </span>

      <!-- Template -->
      <div class="flex flex-col gap-1.5">
        <span class="text-[0.54rem] font-medium text-muted-foreground px-0.5">{t("vdev.cfgTemplate")}</span>
        <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
          <CardContent class="flex flex-col gap-0 py-0 px-0">
            {#each TEMPLATES as tmpl, i}
              <button
                class="flex items-start gap-3 px-4 py-3 text-left transition-colors cursor-pointer
                       {config.template === tmpl.key ? 'bg-primary/8' : 'hover:bg-muted/30'}
                       {i < TEMPLATES.length - 1 ? 'border-b border-border dark:border-white/[0.05]' : ''}"
                onclick={() => { config.template = tmpl.key; }}
                disabled={lslRunning}
              >
                <span class="mt-0.5 flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border
                             {config.template === tmpl.key ? 'border-primary bg-primary' : 'border-muted-foreground/30'}">
                  {#if config.template === tmpl.key}
                    <span class="h-1.5 w-1.5 rounded-full bg-white"></span>
                  {/if}
                </span>
                <div class="flex flex-col gap-0.5 min-w-0">
                  <span class="text-[0.64rem] font-medium text-foreground">{t(tmpl.label)}</span>
                  <span class="text-[0.54rem] text-muted-foreground leading-relaxed">{t(tmpl.desc)}</span>
                </div>
              </button>
            {/each}
          </CardContent>
        </Card>
      </div>

      <!-- Channels + Sample rate -->
      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="flex flex-col gap-4 py-4 px-4">
          <!-- Channels -->
          <div class="flex flex-col gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.64rem] font-semibold text-foreground">{t("vdev.cfgChannels")}</span>
              <span class="text-[0.54rem] text-muted-foreground">{t("vdev.cfgChannelsDesc")}</span>
            </div>
            <div class="flex gap-1.5 flex-wrap">
              {#each CHANNEL_OPTIONS as n}
                <button
                  class="h-7 px-2.5 rounded-md text-[0.58rem] font-medium transition-colors cursor-pointer
                         {config.channels === n ? 'bg-primary text-primary-foreground' : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                  onclick={() => { config.channels = n; }}
                  disabled={lslRunning}
                >{n}ch</button>
              {/each}
            </div>
            {#if channelLabels.length > 0}
              <span class="text-[0.48rem] text-muted-foreground/40 font-mono leading-relaxed">
                {channelLabels.join(", ")}
              </span>
            {/if}
          </div>

          <Separator class="bg-border dark:bg-white/[0.06]" />

          <!-- Sample rate -->
          <div class="flex flex-col gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.64rem] font-semibold text-foreground">{t("vdev.cfgRate")}</span>
              <span class="text-[0.54rem] text-muted-foreground">{t("vdev.cfgRateDesc")}</span>
            </div>
            <div class="flex gap-1.5 flex-wrap">
              {#each RATE_OPTIONS as hz}
                <button
                  class="h-7 px-2.5 rounded-md text-[0.58rem] font-medium transition-colors cursor-pointer
                         {config.sampleRate === hz ? 'bg-primary text-primary-foreground' : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                  onclick={() => { config.sampleRate = hz; }}
                  disabled={lslRunning}
                >{hz} Hz</button>
              {/each}
            </div>
          </div>
        </CardContent>
      </Card>

      <!-- Signal quality -->
      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="flex flex-col gap-3 py-4 px-4">
          <div class="flex flex-col gap-0.5">
            <span class="text-[0.64rem] font-semibold text-foreground">{t("vdev.cfgQuality")}</span>
            <span class="text-[0.54rem] text-muted-foreground">{t("vdev.cfgQualityDesc")}</span>
          </div>
          <div class="flex gap-1.5">
            {#each QUALITY_OPTIONS as q}
              <button
                class="flex-1 h-7 rounded-md text-[0.58rem] font-medium transition-colors cursor-pointer
                       {config.quality === q.key ? 'bg-primary text-primary-foreground' : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                onclick={() => { config.quality = q.key; }}
                disabled={lslRunning}
              >{t(q.label)}</button>
            {/each}
          </div>
        </CardContent>
      </Card>

      <!-- Advanced (collapsible) -->
      <div class="flex flex-col gap-2">
        <button
          class="flex items-center gap-1.5 text-[0.56rem] font-semibold tracking-widest uppercase
                 text-muted-foreground px-0.5 cursor-pointer w-fit"
          onclick={() => { showAdvanced = !showAdvanced; }}
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
               stroke-linecap="round" stroke-linejoin="round"
               class="w-2.5 h-2.5 transition-transform {showAdvanced ? 'rotate-90' : ''}">
            <path d="M9 18l6-6-6-6"/>
          </svg>
          {t("vdev.cfgAdvanced")}
        </button>

        {#if showAdvanced}
          <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
            <CardContent class="flex flex-col gap-0 py-0 px-0">

              <!-- Amplitude -->
              <div class="flex items-center justify-between gap-4 px-4 py-3
                          border-b border-border dark:border-white/[0.05]">
                <div class="flex flex-col gap-0.5 min-w-0">
                  <span class="text-[0.64rem] font-semibold text-foreground">{t("vdev.cfgAmplitude")}</span>
                  <span class="text-[0.52rem] text-muted-foreground">{t("vdev.cfgAmplitudeDesc")}</span>
                </div>
                <input type="number" min="1" max="500" step="5"
                  bind:value={config.amplitudeUv}
                  disabled={lslRunning}
                  class="w-20 h-7 rounded-md border border-border bg-background px-2 text-[0.62rem]
                         text-right font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring
                         disabled:opacity-40" />
              </div>

              <!-- Noise -->
              <div class="flex items-center justify-between gap-4 px-4 py-3
                          border-b border-border dark:border-white/[0.05]">
                <div class="flex flex-col gap-0.5 min-w-0">
                  <span class="text-[0.64rem] font-semibold text-foreground">{t("vdev.cfgNoise")}</span>
                  <span class="text-[0.52rem] text-muted-foreground">{t("vdev.cfgNoiseDesc")}</span>
                </div>
                <input type="number" min="0" max="100" step="1"
                  bind:value={config.noiseUv}
                  disabled={lslRunning}
                  class="w-20 h-7 rounded-md border border-border bg-background px-2 text-[0.62rem]
                         text-right font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring
                         disabled:opacity-40" />
              </div>

              <!-- Line noise -->
              <div class="flex items-center justify-between gap-4 px-4 py-3
                          border-b border-border dark:border-white/[0.05]">
                <div class="flex flex-col gap-0.5 min-w-0">
                  <span class="text-[0.64rem] font-semibold text-foreground">{t("vdev.cfgLineNoise")}</span>
                  <span class="text-[0.52rem] text-muted-foreground">{t("vdev.cfgLineNoiseDesc")}</span>
                </div>
                <div class="flex gap-1 shrink-0">
                  {#each LINE_NOISE_OPTIONS as opt}
                    <button
                      class="h-7 px-2.5 rounded-md text-[0.56rem] font-medium cursor-pointer transition-colors
                             {config.lineNoise === opt.key
                               ? 'bg-primary text-primary-foreground'
                               : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                      onclick={() => { config.lineNoise = opt.key; }}
                      disabled={lslRunning}
                    >{t(opt.label)}</button>
                  {/each}
                </div>
              </div>

              <!-- Dropout -->
              <div class="flex items-center justify-between gap-4 px-4 py-3">
                <div class="flex flex-col gap-0.5 min-w-0">
                  <span class="text-[0.64rem] font-semibold text-foreground">{t("vdev.cfgDropout")}</span>
                  <span class="text-[0.52rem] text-muted-foreground">{t("vdev.cfgDropoutDesc")}</span>
                </div>
                <input type="number" min="0" max="1" step="0.05"
                  bind:value={config.dropoutProb}
                  disabled={lslRunning}
                  class="w-20 h-7 rounded-md border border-border bg-background px-2 text-[0.62rem]
                         text-right font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring
                         disabled:opacity-40" />
              </div>

            </CardContent>
          </Card>
        {/if}
      </div>
    </section>
  {/if}


  <!-- ── Signal preview (offline) ───────────────────────────────────────── -->
  <section class="flex flex-col gap-2 shrink-0">
    <div class="flex items-center gap-2 px-0.5">
      <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground">
        {t("vdev.previewOffline")}
      </span>
      <span class="text-[0.46rem] text-muted-foreground/35 ml-auto font-mono">
        {config.channels}ch · {config.sampleRate} Hz
      </span>
    </div>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="py-2 px-3">
        <canvas
          bind:this={previewCanvas}
          width="660"
          height="140"
          class="w-full h-[140px] rounded-md bg-[#0c0c14]"
        ></canvas>
        <p class="text-[0.46rem] text-muted-foreground/35 mt-1.5 leading-relaxed">
          {t("vdev.previewOfflineDesc")}
        </p>
      </CardContent>
    </Card>
  </section>

</main>
