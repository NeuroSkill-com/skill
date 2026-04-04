<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Virtual EEG Device — configurable synthetic EEG data generator. -->
<script lang="ts">
import { onDestroy } from "svelte";
import { Button } from "$lib/components/ui/button";
import { Card, CardContent } from "$lib/components/ui/card";
import { Separator } from "$lib/components/ui/separator";
import { t } from "$lib/i18n/index.svelte";
import {
  createRuntime,
  DEFAULT_CONFIG,
  generateSamples,
  getChannelLabels,
  type LineNoise,
  type SignalQuality,
  type SignalTemplate,
  startRuntime,
  stopRuntime,
  type VirtualEegConfig,
  type VirtualEegRuntime,
} from "$lib/virtual-eeg/generator";

// ── State ──────────────────────────────────────────────────────────────────

let config = $state<VirtualEegConfig>({ ...DEFAULT_CONFIG });
let runtime = $state<VirtualEegRuntime | null>(null);
let running = $state(false);
let showAdvanced = $state(false);

// Preview canvas
let previewCanvas = $state<HTMLCanvasElement | null>(null);
let previewTimer: ReturnType<typeof setInterval> | null = null;
let previewIdx = 0;
let previewBuffers: number[][] = [];

// Channel labels
let channelLabels = $derived(getChannelLabels(config.channels));

// Template options
const TEMPLATES: { key: SignalTemplate; label: string; desc: string }[] = [
  { key: "sine", label: "veeg.templateSine", desc: "veeg.templateSineDesc" },
  { key: "good_quality", label: "veeg.templateGoodQuality", desc: "veeg.templateGoodQualityDesc" },
  { key: "bad_quality", label: "veeg.templateBadQuality", desc: "veeg.templateBadQualityDesc" },
  { key: "interruptions", label: "veeg.templateInterruptions", desc: "veeg.templateInterruptionsDesc" },
  { key: "file", label: "veeg.templateFile", desc: "veeg.templateFileDesc" },
];

const QUALITY_OPTIONS: { key: SignalQuality; label: string }[] = [
  { key: "poor", label: "veeg.qualityPoor" },
  { key: "fair", label: "veeg.qualityFair" },
  { key: "good", label: "veeg.qualityGood" },
  { key: "excellent", label: "veeg.qualityExcellent" },
];

const CHANNEL_OPTIONS = [1, 2, 4, 8, 16, 32];
const RATE_OPTIONS = [128, 256, 512, 1000];

// ── Actions ────────────────────────────────────────────────────────────────

function start() {
  if (running) return;
  const rt = createRuntime(config);
  rt.onSamples = (electrode, samples, timestamp) => {
    // Inject samples into the daemon WS event pipeline so the
    // dashboard EEG chart renders them in real time.
    import("$lib/daemon/ws")
      .then(({ injectDaemonEvent }) => {
        injectDaemonEvent({
          type: "EegSample",
          ts_unix_ms: Math.round(timestamp * 1000),
          payload: { electrode, samples, timestamp },
        });
      })
      .catch(() => {});
  };
  startRuntime(rt);
  runtime = rt;
  running = true;
  startPreview();
}

function stop() {
  if (!running || !runtime) return;
  stopRuntime(runtime);
  runtime = null;
  running = false;
  stopPreview();
}

async function chooseFile() {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const path = await invoke<string | null>("pick_exg_weights_file");
    if (path) {
      config.fileName = path.split("/").pop() ?? path;
      // In a real implementation, load and parse the CSV/EDF file
      config.fileData = null; // placeholder
    }
  } catch {
    // Fallback: browser file picker
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".csv,.edf,.bdf";
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return;
      config.fileName = file.name;
      const reader = new FileReader();
      reader.onload = () => {
        const text = reader.result as string;
        const lines = text.trim().split("\n");
        const data: number[][] = [];
        for (const line of lines) {
          const vals = line
            .split(",")
            .map(Number)
            .filter((v) => !isNaN(v));
          if (vals.length > 0) {
            for (let ch = 0; ch < vals.length; ch++) {
              if (!data[ch]) data[ch] = [];
              data[ch].push(vals[ch]);
            }
          }
        }
        config.fileData = data;
        config.channels = Math.min(data.length, 32);
      };
      reader.readAsText(file);
    };
    input.click();
  }
}

// ── Preview ────────────────────────────────────────────────────────────────

function startPreview() {
  stopPreview();
  const previewCh = Math.min(config.channels, 4);
  previewBuffers = Array.from({ length: previewCh }, () => []);
  previewIdx = 0;

  previewTimer = setInterval(
    () => {
      for (let i = 0; i < 8; i++) {
        const samples = generateSamples(config, previewIdx++);
        for (let ch = 0; ch < previewCh; ch++) {
          previewBuffers[ch].push(samples[ch]);
          if (previewBuffers[ch].length > 512) previewBuffers[ch].shift();
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
  const chH = h / Math.max(previewBuffers.length, 1);

  ctx.fillStyle = "var(--background, #0c0c14)";
  ctx.fillRect(0, 0, w, h);

  const colors = ["#22c55e", "#3b82f6", "#f59e0b", "#ef4444"];

  for (let ch = 0; ch < previewBuffers.length; ch++) {
    const buf = previewBuffers[ch];
    if (buf.length < 2) continue;

    const yCenter = chH * ch + chH / 2;
    const scale = chH / (config.amplitudeUv * 4 || 200);

    ctx.strokeStyle = colors[ch % colors.length];
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let i = 0; i < buf.length; i++) {
      const x = (i / 512) * w;
      const y = yCenter - buf[i] * scale;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Channel label
    ctx.fillStyle = colors[ch % colors.length];
    ctx.font = "10px monospace";
    ctx.fillText(channelLabels[ch] ?? `Ch${ch + 1}`, 4, chH * ch + 12);
  }
}

// Start preview on mount for visual feedback
$effect(() => {
  if (!running) startPreview();
  return () => stopPreview();
});

onDestroy(() => {
  stop();
  stopPreview();
});
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="flex flex-col gap-4 px-1">

  <!-- Header -->
  <div class="flex flex-col gap-1">
    <h2 class="text-[0.72rem] font-bold tracking-tight text-foreground">{t("veeg.title")}</h2>
    <p class="text-[0.58rem] text-muted-foreground leading-relaxed">{t("veeg.desc")}</p>
  </div>

  <!-- Status + Start/Stop -->
  <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
    <CardContent class="flex items-center justify-between py-3">
      <div class="flex items-center gap-2">
        <span class="relative flex h-2.5 w-2.5">
          {#if running}
            <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-60"></span>
            <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-green-500"></span>
          {:else}
            <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-muted-foreground/30"></span>
          {/if}
        </span>
        <span class="text-[0.62rem] font-medium {running ? 'text-green-600 dark:text-green-400' : 'text-muted-foreground'}">
          {running ? t("veeg.running") : t("veeg.stopped")}
        </span>
        {#if running}
          <span class="text-[0.5rem] text-muted-foreground/50 tabular-nums">
            {config.channels}ch · {config.sampleRate} Hz
          </span>
        {/if}
      </div>
      <Button
        variant={running ? "destructive" : "default"}
        size="sm"
        class="h-7 text-[0.58rem] px-3"
        onclick={running ? stop : start}
      >
        {running ? t("veeg.stop") : t("veeg.start")}
      </Button>
    </CardContent>
  </Card>

  <!-- Signal Template -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("veeg.template")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-1 py-2">
        {#each TEMPLATES as tmpl}
          <button
            class="flex items-start gap-3 px-2 py-2 rounded-md text-left transition-colors cursor-pointer
                   {config.template === tmpl.key
                     ? 'bg-primary/10 border border-primary/20'
                     : 'hover:bg-muted/40 border border-transparent'}"
            onclick={() => { config.template = tmpl.key; }}
            disabled={running}
          >
            <span class="mt-0.5 flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border
                         {config.template === tmpl.key ? 'border-primary bg-primary' : 'border-muted-foreground/30'}">
              {#if config.template === tmpl.key}
                <span class="h-1.5 w-1.5 rounded-full bg-white"></span>
              {/if}
            </span>
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-[0.62rem] font-medium text-foreground">{t(tmpl.label)}</span>
              <span class="text-[0.54rem] text-muted-foreground leading-relaxed">{t(tmpl.desc)}</span>
            </div>
          </button>
        {/each}

        {#if config.template === "file"}
          <div class="flex items-center gap-2 px-2 py-2">
            <Button variant="outline" size="sm" class="h-7 text-[0.58rem]" onclick={chooseFile} disabled={running}>
              {t("veeg.chooseFile")}
            </Button>
            <span class="text-[0.54rem] text-muted-foreground truncate">
              {config.fileName ?? t("veeg.noFile")}
            </span>
          </div>
        {/if}
      </CardContent>
    </Card>
  </section>

  <!-- Channels + Sample Rate -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("veeg.channels")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-3 py-3">
        <div class="flex flex-col gap-1">
          <span class="text-[0.54rem] text-muted-foreground">{t("veeg.channelsDesc")}</span>
          <div class="flex gap-1.5 flex-wrap">
            {#each CHANNEL_OPTIONS as n}
              <button
                class="h-7 px-2.5 rounded-md text-[0.58rem] font-medium transition-colors cursor-pointer
                       {config.channels === n
                         ? 'bg-primary text-primary-foreground'
                         : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                onclick={() => { config.channels = n; }}
                disabled={running}
              >
                {n}ch
              </button>
            {/each}
          </div>
          {#if config.channels > 0}
            <span class="text-[0.48rem] text-muted-foreground/50 font-mono truncate">
              {channelLabels.join(", ")}
            </span>
          {/if}
        </div>

        <Separator class="bg-border dark:bg-white/[0.06]" />

        <div class="flex flex-col gap-1">
          <span class="text-[0.54rem] text-muted-foreground">{t("veeg.sampleRateDesc")}</span>
          <div class="flex gap-1.5">
            {#each RATE_OPTIONS as hz}
              <button
                class="h-7 px-2.5 rounded-md text-[0.58rem] font-medium transition-colors cursor-pointer
                       {config.sampleRate === hz
                         ? 'bg-primary text-primary-foreground'
                         : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                onclick={() => { config.sampleRate = hz; }}
                disabled={running}
              >
                {hz} Hz
              </button>
            {/each}
          </div>
        </div>
      </CardContent>
    </Card>
  </section>

  <!-- Signal Quality -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("veeg.quality")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col gap-2 py-3">
        <span class="text-[0.54rem] text-muted-foreground">{t("veeg.qualityDesc")}</span>
        <div class="flex gap-1.5">
          {#each QUALITY_OPTIONS as q}
            <button
              class="flex-1 h-7 rounded-md text-[0.58rem] font-medium transition-colors cursor-pointer
                     {config.quality === q.key
                       ? 'bg-primary text-primary-foreground'
                       : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
              onclick={() => { config.quality = q.key; }}
              disabled={running}
            >
              {t(q.label)}
            </button>
          {/each}
        </div>
      </CardContent>
    </Card>
  </section>

  <!-- Advanced -->
  <section class="flex flex-col gap-2">
    <button
      class="flex items-center gap-1 text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5 cursor-pointer"
      onclick={() => { showAdvanced = !showAdvanced; }}
    >
      <span class="transition-transform {showAdvanced ? 'rotate-90' : ''}" style="display:inline-block">▶</span>
      {t("veeg.advanced")}
    </button>

    {#if showAdvanced}
      <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
        <CardContent class="flex flex-col gap-3 py-3">

          <!-- Amplitude -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.58rem] font-medium text-foreground">{t("veeg.amplitudeUv")}</span>
              <span class="text-[0.5rem] text-muted-foreground">{t("veeg.amplitudeDesc")}</span>
            </div>
            <input type="number" min="1" max="500" step="5"
              bind:value={config.amplitudeUv}
              disabled={running}
              class="w-20 h-7 rounded-md border border-border bg-background px-2 text-[0.62rem] text-right
                     font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          </div>

          <Separator class="bg-border dark:bg-white/[0.06]" />

          <!-- Noise -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.58rem] font-medium text-foreground">{t("veeg.noiseUv")}</span>
              <span class="text-[0.5rem] text-muted-foreground">{t("veeg.noiseDesc")}</span>
            </div>
            <input type="number" min="0" max="100" step="1"
              bind:value={config.noiseUv}
              disabled={running}
              class="w-20 h-7 rounded-md border border-border bg-background px-2 text-[0.62rem] text-right
                     font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          </div>

          <Separator class="bg-border dark:bg-white/[0.06]" />

          <!-- Line noise -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.58rem] font-medium text-foreground">{t("veeg.lineNoise")}</span>
              <span class="text-[0.5rem] text-muted-foreground">{t("veeg.lineNoiseDesc")}</span>
            </div>
            <div class="flex gap-1">
              {#each [["none", "veeg.lineNoiseNone"], ["50hz", "veeg.lineNoise50"], ["60hz", "veeg.lineNoise60"]] as [val, label]}
                <button
                  class="h-7 px-2 rounded-md text-[0.54rem] font-medium cursor-pointer transition-colors
                         {config.lineNoise === val
                           ? 'bg-primary text-primary-foreground'
                           : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                  onclick={() => { config.lineNoise = val as LineNoise; }}
                  disabled={running}
                >
                  {t(label)}
                </button>
              {/each}
            </div>
          </div>

          <Separator class="bg-border dark:bg-white/[0.06]" />

          <!-- Dropout probability -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-[0.58rem] font-medium text-foreground">{t("veeg.dropoutProb")}</span>
              <span class="text-[0.5rem] text-muted-foreground">{t("veeg.dropoutDesc")}</span>
            </div>
            <input type="number" min="0" max="1" step="0.05"
              bind:value={config.dropoutProb}
              disabled={running}
              class="w-20 h-7 rounded-md border border-border bg-background px-2 text-[0.62rem] text-right
                     font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          </div>

        </CardContent>
      </Card>
    {/if}
  </section>

  <!-- Signal Preview -->
  <section class="flex flex-col gap-2">
    <span class="text-[0.56rem] font-semibold tracking-widest uppercase text-muted-foreground px-0.5">
      {t("veeg.preview")}
    </span>
    <Card class="border-border dark:border-white/[0.06] bg-white dark:bg-[#14141e] gap-0 py-0 overflow-hidden">
      <CardContent class="py-2">
        <canvas
          bind:this={previewCanvas}
          width="600"
          height="200"
          class="w-full h-[200px] rounded-md bg-[#0c0c14]"
        ></canvas>
        <p class="text-[0.48rem] text-muted-foreground/40 mt-1">{t("veeg.previewDesc")}</p>
      </CardContent>
    </Card>
  </section>

</div>
