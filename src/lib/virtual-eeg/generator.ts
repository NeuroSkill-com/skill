// SPDX-License-Identifier: GPL-3.0-only
// Virtual EEG signal generator — produces synthetic multichannel EEG data.

export type SignalTemplate = "sine" | "good_quality" | "bad_quality" | "interruptions" | "file";
export type SignalQuality = "poor" | "fair" | "good" | "excellent";
export type LineNoise = "none" | "50hz" | "60hz";

export interface VirtualEegConfig {
  channels: number;
  sampleRate: number;
  template: SignalTemplate;
  quality: SignalQuality;
  amplitudeUv: number;
  noiseUv: number;
  lineNoise: LineNoise;
  dropoutProb: number;
  fileData: number[][] | null; // [channel][sample]
  fileName: string | null;
}

export const DEFAULT_CONFIG: VirtualEegConfig = {
  channels: 4,
  sampleRate: 256,
  template: "good_quality",
  quality: "good",
  amplitudeUv: 50,
  noiseUv: 5,
  lineNoise: "none",
  dropoutProb: 0,
  fileData: null,
  fileName: null,
};

export const QUALITY_SNR: Record<SignalQuality, number> = {
  poor: 0.5,
  fair: 2,
  good: 5,
  excellent: 20,
};

export const CHANNEL_PRESETS: Record<number, string[]> = {
  1: ["Fp1"],
  2: ["Fp1", "Fp2"],
  4: ["TP9", "AF7", "AF8", "TP10"],
  8: ["Fp1", "Fp2", "F3", "F4", "C3", "C4", "O1", "O2"],
  16: ["Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "T3", "C3", "Cz", "C4", "T4", "P3", "Pz", "P4", "O1"],
  32: [
    "Fp1",
    "Fp2",
    "AF3",
    "AF4",
    "F7",
    "F3",
    "Fz",
    "F4",
    "F8",
    "FT7",
    "FC3",
    "FCz",
    "FC4",
    "FT8",
    "T7",
    "C3",
    "Cz",
    "C4",
    "T8",
    "TP7",
    "CP3",
    "CPz",
    "CP4",
    "TP8",
    "P7",
    "P3",
    "Pz",
    "P4",
    "P8",
    "O1",
    "Oz",
    "O2",
  ],
};

/** Get channel labels for a given channel count. */
export function getChannelLabels(n: number): string[] {
  if (n in CHANNEL_PRESETS) return CHANNEL_PRESETS[n];
  return Array.from({ length: n }, (_, i) => `Ch${i + 1}`);
}

/** Gaussian random using Box-Muller transform. */
function gaussianRandom(): number {
  const u1 = Math.random();
  const u2 = Math.random();
  return Math.sqrt(-2 * Math.log(u1 || 1e-10)) * Math.cos(2 * Math.PI * u2);
}

/** Generate a single sample buffer for all channels at a given time. */
export function generateSamples(config: VirtualEegConfig, sampleIndex: number): number[] {
  const { channels, sampleRate, template, amplitudeUv, noiseUv, lineNoise, dropoutProb } = config;
  const snr = QUALITY_SNR[config.quality];
  const t = sampleIndex / sampleRate;

  // Check for dropout
  if (dropoutProb > 0 && Math.random() < dropoutProb / sampleRate) {
    return new Array(channels).fill(0);
  }

  const samples = new Array(channels);

  for (let ch = 0; ch < channels; ch++) {
    let value = 0;
    const phaseOffset = ch * 0.3;

    switch (template) {
      case "sine": {
        // Standard EEG frequency bands
        const delta = Math.sin(2 * Math.PI * 2 * t + phaseOffset) * 0.4;
        const theta = Math.sin(2 * Math.PI * 6 * t + phaseOffset) * 0.3;
        const alpha = Math.sin(2 * Math.PI * 10 * t + phaseOffset) * 0.5;
        const beta = Math.sin(2 * Math.PI * 20 * t + phaseOffset) * 0.2;
        const gamma = Math.sin(2 * Math.PI * 40 * t + phaseOffset) * 0.1;
        value = (delta + theta + alpha + beta + gamma) * amplitudeUv;
        break;
      }

      case "good_quality": {
        // Dominant alpha rhythm (8–13 Hz) + pink noise background
        const alpha = Math.sin(2 * Math.PI * (10 + ch * 0.1) * t + phaseOffset) * 0.6;
        const theta = Math.sin(2 * Math.PI * 5.5 * t + phaseOffset) * 0.15;
        const beta = Math.sin(2 * Math.PI * 18 * t + phaseOffset) * 0.1;
        // Pink noise approximation
        const pink = (gaussianRandom() * 0.15) / (1 + Math.abs(gaussianRandom()) * 0.5);
        value = ((alpha + theta + beta + pink) * amplitudeUv * snr) / 5;
        break;
      }

      case "bad_quality": {
        // Muscle artefacts + electrode pops
        const alpha = Math.sin(2 * Math.PI * 10 * t + phaseOffset) * 0.2;
        const muscle = gaussianRandom() * 0.6; // High-frequency muscle
        const pop = Math.random() < 0.005 ? gaussianRandom() * 5 : 0; // Electrode pops
        const drift = Math.sin(2 * Math.PI * 0.1 * t + ch) * 2; // Slow drift
        value = (alpha + muscle + pop + drift) * amplitudeUv * 0.3;
        break;
      }

      case "interruptions": {
        // Good signal with periodic 1–3s dropouts every 8–15s
        const cycleLen = 10 + ch * 0.5;
        const dropoutLen = 1.5;
        const inDropout = t % cycleLen > cycleLen - dropoutLen;
        if (inDropout) {
          value = gaussianRandom() * noiseUv * 0.1;
        } else {
          const alpha = Math.sin(2 * Math.PI * 10 * t + phaseOffset) * 0.5;
          const theta = Math.sin(2 * Math.PI * 6 * t + phaseOffset) * 0.2;
          value = ((alpha + theta) * amplitudeUv * snr) / 5;
        }
        break;
      }

      case "file": {
        if (config.fileData && config.fileData[ch]) {
          const data = config.fileData[ch];
          value = data[sampleIndex % data.length] ?? 0;
        }
        break;
      }
    }

    // Add noise
    if (template !== "file") {
      value += gaussianRandom() * noiseUv;
    }

    // Add line noise
    if (lineNoise === "50hz") {
      value += Math.sin(2 * Math.PI * 50 * t) * noiseUv * 0.3;
    } else if (lineNoise === "60hz") {
      value += Math.sin(2 * Math.PI * 60 * t) * noiseUv * 0.3;
    }

    samples[ch] = value;
  }

  return samples;
}

// ── Runtime ────────────────────────────────────────────────────────────────

export interface VirtualEegRuntime {
  config: VirtualEegConfig;
  running: boolean;
  sampleIndex: number;
  timer: ReturnType<typeof setInterval> | null;
  onSamples: ((electrode: number, samples: number[], timestamp: number) => void) | null;
}

export function createRuntime(config: VirtualEegConfig): VirtualEegRuntime {
  return {
    config: { ...config },
    running: false,
    sampleIndex: 0,
    timer: null,
    onSamples: null,
  };
}

const BATCH_SIZE = 8; // samples per callback

export function startRuntime(rt: VirtualEegRuntime): void {
  if (rt.running) return;
  rt.running = true;
  const intervalMs = (1000 * BATCH_SIZE) / rt.config.sampleRate;

  rt.timer = setInterval(() => {
    const timestamp = Date.now() / 1000;
    for (let ch = 0; ch < rt.config.channels; ch++) {
      const samples: number[] = [];
      for (let i = 0; i < BATCH_SIZE; i++) {
        const allCh = generateSamples(rt.config, rt.sampleIndex + i);
        samples.push(allCh[ch]);
      }
      rt.onSamples?.(ch, samples, timestamp);
    }
    rt.sampleIndex += BATCH_SIZE;
  }, intervalMs);
}

export function stopRuntime(rt: VirtualEegRuntime): void {
  if (!rt.running) return;
  rt.running = false;
  if (rt.timer) {
    clearInterval(rt.timer);
    rt.timer = null;
  }
}
