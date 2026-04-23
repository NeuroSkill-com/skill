<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Virtual EEG Device — streams synthetic EEG via daemon LSL pipeline. -->
<script lang="ts">
import { onDestroy } from "svelte";
import { Button } from "$lib/components/ui/button";
import { CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { Separator } from "$lib/components/ui/separator";
import { SettingsCard } from "$lib/components/ui/settings-card";
import { daemonInvoke } from "$lib/daemon/invoke-proxy";
import { lslConnect, lslStartVirtualSourceConfigured, lslStopVirtualSource } from "$lib/daemon/lsl";
import { t } from "$lib/i18n/index.svelte";
import {
  DEFAULT_CONFIG,
  generateSamples,
  getChannelLabels,
  type LineNoise,
  type SignalQuality,
  type SignalTemplate,
  type VirtualEegConfig,
} from "$lib/virtual-eeg/generator";

// ── State ──────────────────────────────────────────────────────────────────

let config = $state<VirtualEegConfig>({ ...DEFAULT_CONFIG });
let running = $state(false);
let busy = $state(false);
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
// Start/stop use the daemon LSL pipeline so the full DSP chain (filter →
// FFT → band analyzer → enrichment) runs on the backend, producing all
// EEG metrics identically to a real hardware device.

function emitVirtualStatus(cfg: VirtualEegConfig, connected: boolean) {
  const labels = getChannelLabels(cfg.channels);
  const payload = connected
    ? {
        state: "connected",
        device_name: "Virtual EEG",
        device_id: "virtual-eeg",
        device_kind: "lsl",
        serial_number: null,
        mac_address: null,
        csv_path: null,
        sample_count: 0,
        battery: 0,
        eeg: new Array(cfg.channels).fill(0),
        paired_devices: [],
        device_error: null,
        target_name: null,
        filter_config: {
          sample_rate: cfg.sampleRate,
          low_pass_hz: null,
          high_pass_hz: null,
          notch: null,
          notch_bandwidth_hz: 1,
        },
        channel_quality: new Array(cfg.channels).fill("good"),
        retry_attempt: 0,
        retry_countdown_secs: 0,
        ppg: [],
        ppg_sample_count: 0,
        imu_sample_count: 0,
        accel: [0, 0, 0] as [number, number, number],
        gyro: [0, 0, 0] as [number, number, number],
        fuel_gauge_mv: 0,
        temperature_raw: 0,
        hardware_version: null,
        has_ppg: false,
        has_imu: false,
        has_central_electrodes: cfg.channels >= 8,
        has_full_montage: cfg.channels >= 32,
        channel_names: labels,
        eeg_channel_count: cfg.channels,
        eeg_sample_rate_hz: cfg.sampleRate,
      }
    : {
        state: "disconnected",
        device_name: null,
        device_id: null,
        device_kind: "",
        channel_names: [],
        eeg_channel_count: 0,
        eeg_sample_rate_hz: 0,
      };
  import("@tauri-apps/api/event").then(({ emit }) => emit("virtual-device-status", payload)).catch(() => {});
}

async function start() {
  if (busy || running) return;
  busy = true;
  try {
    // 1. Start the daemon-side virtual LSL source with the configured params.
    await lslStartVirtualSourceConfigured({
      channels: config.channels,
      sampleRate: config.sampleRate,
      template: config.template,
      quality: config.quality,
      amplitudeUv: config.amplitudeUv,
      noiseUv: config.noiseUv,
      lineNoise: config.lineNoise,
      dropoutProb: config.dropoutProb,
    });
    running = true;

    // 2. Notify the dashboard so it shows CONNECTED immediately.
    emitVirtualStatus(config, true);

    // 3. Give the LSL source a moment to announce itself on the network.
    await new Promise<void>((r) => setTimeout(r, 600));

    // 4. Connect the dashboard to it through the normal LSL session path.
    //    The daemon discovers the source, opens a session, and streams
    //    EEG through the full DSP pipeline → all metrics are computed.
    await lslConnect("SkillVirtualEEG");
  } catch (e: unknown) {
    if (running) {
      await lslStopVirtualSource().catch(() => {});
      running = false;
    }
    console.error("[VirtualEegTab] start failed:", e);
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
    running = false;
    emitVirtualStatus(config, false);
  } catch (e: unknown) {
    console.error("[VirtualEegTab] stop failed:", e);
  } finally {
    busy = false;
  }
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
            .filter((v) => !Number.isNaN(v));
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

  // CSS variables are not resolved by the canvas 2D context — use a
  // literal colour so fillStyle is always valid.
  ctx.fillStyle = "#0c0c14";
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

// Keep the preview running at all times (both idle and while the virtual
// device is active).  The effect re-fires whenever `running` changes so
// the timer is always restarted with the correct config after start/stop.
$effect(() => {
  void running;
  startPreview();
  return () => stopPreview();
});

onDestroy(() => {
  // Fire-and-forget cleanup — stop the daemon session + LSL source if running.
  if (running) {
    daemonInvoke("cancel_session", {}).catch(() => {});
    lslStopVirtualSource().catch(() => {});
  }
  stopPreview();
});
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="flex flex-col gap-4 px-1">

  <!-- Header -->
  <div class="flex flex-col gap-1">
    <h2 class="text-ui-md font-bold tracking-tight text-foreground">{t("veeg.title")}</h2>
    <p class="text-ui-sm text-muted-foreground leading-relaxed">{t("veeg.desc")}</p>
  </div>

  <!-- Status + Start/Stop -->
  <SettingsCard>
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
        <span class="text-ui-sm font-medium {running ? 'text-green-600 dark:text-green-400' : 'text-muted-foreground'}">
          {running ? t("veeg.running") : t("veeg.stopped")}
        </span>
        {#if running}
          <span class="text-ui-2xs text-muted-foreground/50 tabular-nums">
            {config.channels}ch · {config.sampleRate} Hz
          </span>
        {/if}
      </div>
      <Button
        variant={running ? "destructive" : "default"}
        size="sm"
        class="h-7 text-ui-sm px-3"
        disabled={busy}
        onclick={running ? stop : start}
      >
        {#if busy}
          <span class="h-3 w-3 border-2 border-current border-t-transparent rounded-full animate-spin"></span>
        {:else}
          {running ? t("veeg.stop") : t("veeg.start")}
        {/if}
      </Button>
    </CardContent>
  </SettingsCard>

  <!-- Signal Template -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("veeg.template")}</SectionHeader>
    <SettingsCard>
      <CardContent class="flex flex-col gap-1 py-2">
        {#each TEMPLATES as tmpl}
          <button
            class="flex items-start gap-3 px-2 py-2 rounded-md text-left transition-colors cursor-pointer
                   {config.template === tmpl.key
                     ? 'bg-violet-500/10 border border-violet-500/20'
                     : 'hover:bg-muted/40 border border-transparent'}"
            onclick={() => { config.template = tmpl.key; }}
            disabled={running}
          >
            <span class="mt-0.5 flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border
                         {config.template === tmpl.key ? 'border-violet-500 bg-violet-600' : 'border-muted-foreground/30'}">
              {#if config.template === tmpl.key}
                <span class="h-1.5 w-1.5 rounded-full bg-white"></span>
              {/if}
            </span>
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-ui-sm font-medium text-foreground">{t(tmpl.label)}</span>
              <span class="text-ui-xs text-muted-foreground leading-relaxed">{t(tmpl.desc)}</span>
            </div>
          </button>
        {/each}

        {#if config.template === "file"}
          <div class="flex items-center gap-2 px-2 py-2">
            <Button variant="outline" size="sm" class="h-7 text-ui-sm" onclick={chooseFile} disabled={running}>
              {t("veeg.chooseFile")}
            </Button>
            <span class="text-ui-xs text-muted-foreground truncate">
              {config.fileName ?? t("veeg.noFile")}
            </span>
          </div>
        {/if}
      </CardContent>
    </SettingsCard>
  </section>

  <!-- Channels + Sample Rate -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("veeg.channels")}</SectionHeader>
    <SettingsCard>
      <CardContent class="flex flex-col gap-3 py-3">
        <div class="flex flex-col gap-1">
          <span class="text-ui-xs text-muted-foreground">{t("veeg.channelsDesc")}</span>
          <div class="flex gap-1.5 flex-wrap">
            {#each CHANNEL_OPTIONS as n}
              <button
                class="h-7 px-2.5 rounded-md text-ui-sm font-medium transition-colors cursor-pointer
                       {config.channels === n
                         ? 'bg-violet-600 text-white'
                         : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
                onclick={() => { config.channels = n; }}
                disabled={running}
              >
                {n}ch
              </button>
            {/each}
          </div>
          {#if config.channels > 0}
            <span class="text-ui-2xs text-muted-foreground/50 font-mono truncate">
              {channelLabels.join(", ")}
            </span>
          {/if}
        </div>

        <Separator class="bg-border dark:bg-white/[0.06]" />

        <div class="flex flex-col gap-1">
          <span class="text-ui-xs text-muted-foreground">{t("veeg.sampleRateDesc")}</span>
          <div class="flex gap-1.5">
            {#each RATE_OPTIONS as hz}
              <button
                class="h-7 px-2.5 rounded-md text-ui-sm font-medium transition-colors cursor-pointer
                       {config.sampleRate === hz
                         ? 'bg-violet-600 text-white'
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
    </SettingsCard>
  </section>

  <!-- Signal Quality -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("veeg.quality")}</SectionHeader>
    <SettingsCard>
      <CardContent class="flex flex-col gap-2 py-3">
        <span class="text-ui-xs text-muted-foreground">{t("veeg.qualityDesc")}</span>
        <div class="flex gap-1.5">
          {#each QUALITY_OPTIONS as q}
            <button
              class="flex-1 h-7 rounded-md text-ui-sm font-medium transition-colors cursor-pointer
                     {config.quality === q.key
                       ? 'bg-violet-600 text-white'
                       : 'bg-muted/40 text-muted-foreground hover:bg-muted'}"
              onclick={() => { config.quality = q.key; }}
              disabled={running}
            >
              {t(q.label)}
            </button>
          {/each}
        </div>
      </CardContent>
    </SettingsCard>
  </section>

  <!-- Advanced -->
  <section class="flex flex-col gap-2">
    <button
      class="flex items-center gap-1 text-ui-xs font-semibold tracking-widest uppercase text-muted-foreground px-0.5 cursor-pointer"
      onclick={() => { showAdvanced = !showAdvanced; }}
    >
      <span class="transition-transform {showAdvanced ? 'rotate-90' : ''}" style="display:inline-block">▶</span>
      {t("veeg.advanced")}
    </button>

    {#if showAdvanced}
      <SettingsCard>
        <CardContent class="flex flex-col gap-3 py-3">

          <!-- Amplitude -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-ui-sm font-medium text-foreground">{t("veeg.amplitudeUv")}</span>
              <span class="text-ui-2xs text-muted-foreground">{t("veeg.amplitudeDesc")}</span>
            </div>
            <input type="number" min="1" max="500" step="5"
              aria-label="Amplitude in microvolts"
              bind:value={config.amplitudeUv}
              disabled={running}
              class="w-20 h-7 rounded-md border border-border bg-background px-2 text-ui-sm text-right
                     font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          </div>

          <Separator class="bg-border dark:bg-white/[0.06]" />

          <!-- Noise -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-ui-sm font-medium text-foreground">{t("veeg.noiseUv")}</span>
              <span class="text-ui-2xs text-muted-foreground">{t("veeg.noiseDesc")}</span>
            </div>
            <input type="number" min="0" max="100" step="1"
              aria-label="Noise in microvolts"
              bind:value={config.noiseUv}
              disabled={running}
              class="w-20 h-7 rounded-md border border-border bg-background px-2 text-ui-sm text-right
                     font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          </div>

          <Separator class="bg-border dark:bg-white/[0.06]" />

          <!-- Line noise -->
          <div class="flex items-center justify-between gap-2">
            <div class="flex flex-col gap-0.5">
              <span class="text-ui-sm font-medium text-foreground">{t("veeg.lineNoise")}</span>
              <span class="text-ui-2xs text-muted-foreground">{t("veeg.lineNoiseDesc")}</span>
            </div>
            <div class="flex gap-1">
              {#each [["none", "veeg.lineNoiseNone"], ["50hz", "veeg.lineNoise50"], ["60hz", "veeg.lineNoise60"]] as [val, label]}
                <button
                  class="h-7 px-2 rounded-md text-ui-xs font-medium cursor-pointer transition-colors
                         {config.lineNoise === val
                           ? 'bg-violet-600 text-white'
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
              <span class="text-ui-sm font-medium text-foreground">{t("veeg.dropoutProb")}</span>
              <span class="text-ui-2xs text-muted-foreground">{t("veeg.dropoutDesc")}</span>
            </div>
            <input type="number" min="0" max="1" step="0.05"
              aria-label="Dropout probability"
              bind:value={config.dropoutProb}
              disabled={running}
              class="w-20 h-7 rounded-md border border-border bg-background px-2 text-ui-sm text-right
                     font-mono text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />
          </div>

        </CardContent>
      </SettingsCard>
    {/if}
  </section>

  <!-- Signal Preview -->
  <section class="flex flex-col gap-2">
    <SectionHeader>{t("veeg.preview")}</SectionHeader>
    <SettingsCard>
      <CardContent class="py-2">
        <canvas
          bind:this={previewCanvas}
          width="600"
          height="200"
          class="w-full h-[200px] rounded-md bg-[#0c0c14]"
        ></canvas>
        <p class="text-ui-2xs text-muted-foreground/40 mt-1">{t("veeg.previewDesc")}</p>
      </CardContent>
    </SettingsCard>
  </section>

</div>
