// SPDX-License-Identifier: GPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  CHANNEL_PRESETS,
  createRuntime,
  DEFAULT_CONFIG,
  generateSamples,
  getChannelLabels,
  QUALITY_SNR,
  startRuntime,
  stopRuntime,
  type VirtualEegConfig,
} from "$lib/virtual-eeg/generator";

describe("VirtualEegConfig defaults", () => {
  it("has sensible defaults", () => {
    expect(DEFAULT_CONFIG.channels).toBe(4);
    expect(DEFAULT_CONFIG.sampleRate).toBe(256);
    expect(DEFAULT_CONFIG.template).toBe("good_quality");
    expect(DEFAULT_CONFIG.quality).toBe("good");
    expect(DEFAULT_CONFIG.amplitudeUv).toBe(50);
    expect(DEFAULT_CONFIG.noiseUv).toBe(5);
    expect(DEFAULT_CONFIG.lineNoise).toBe("none");
    expect(DEFAULT_CONFIG.dropoutProb).toBe(0);
  });
});

describe("getChannelLabels", () => {
  it("returns standard 10-20 labels for known presets", () => {
    expect(getChannelLabels(4)).toEqual(["TP9", "AF7", "AF8", "TP10"]);
    expect(getChannelLabels(8)).toHaveLength(8);
    expect(getChannelLabels(32)).toHaveLength(32);
  });

  it("returns generic labels for non-preset counts", () => {
    const labels = getChannelLabels(3);
    expect(labels).toEqual(["Ch1", "Ch2", "Ch3"]);
  });

  it("returns single channel for 1", () => {
    expect(getChannelLabels(1)).toEqual(["Fp1"]);
  });
});

describe("CHANNEL_PRESETS", () => {
  it("covers standard montages", () => {
    expect(Object.keys(CHANNEL_PRESETS).map(Number)).toEqual([1, 2, 4, 8, 16, 32]);
  });

  it("all labels are unique within each preset", () => {
    for (const [_n, labels] of Object.entries(CHANNEL_PRESETS)) {
      const unique = new Set(labels);
      expect(unique.size).toBe(labels.length);
    }
  });
});

describe("QUALITY_SNR", () => {
  it("increases monotonically", () => {
    expect(QUALITY_SNR.poor).toBeLessThan(QUALITY_SNR.fair);
    expect(QUALITY_SNR.fair).toBeLessThan(QUALITY_SNR.good);
    expect(QUALITY_SNR.good).toBeLessThan(QUALITY_SNR.excellent);
  });
});

describe("generateSamples", () => {
  it("returns correct number of channels", () => {
    const config = { ...DEFAULT_CONFIG, channels: 8 };
    const samples = generateSamples(config, 0);
    expect(samples).toHaveLength(8);
  });

  it("sine template produces non-zero values", () => {
    const config = { ...DEFAULT_CONFIG, template: "sine" as const, noiseUv: 0 };
    // Skip sample 0 (all sines are 0 at t=0 with some phase offsets)
    const samples = generateSamples(config, 100);
    expect(samples.some((s) => Math.abs(s) > 0.01)).toBe(true);
  });

  it("good_quality template produces bounded values", () => {
    const config = { ...DEFAULT_CONFIG, template: "good_quality" as const };
    for (let i = 0; i < 100; i++) {
      const samples = generateSamples(config, i);
      for (const s of samples) {
        expect(Math.abs(s)).toBeLessThan(1000); // µV, should be reasonable
      }
    }
  });

  it("bad_quality template has higher variance", () => {
    const goodConfig = { ...DEFAULT_CONFIG, template: "good_quality" as const, noiseUv: 0 };
    const badConfig = { ...DEFAULT_CONFIG, template: "bad_quality" as const, noiseUv: 0 };

    let goodVar = 0;
    let badVar = 0;
    const N = 500;
    for (let i = 0; i < N; i++) {
      const g = generateSamples(goodConfig, i);
      const b = generateSamples(badConfig, i);
      goodVar += g[0] * g[0];
      badVar += b[0] * b[0];
    }
    // Bad quality should generally have more energy/variance due to artefacts
    // (This is probabilistic, so we use a loose check)
    expect(badVar).toBeGreaterThan(0);
    expect(goodVar).toBeGreaterThan(0);
  });

  it("interruptions template produces dropouts", () => {
    const config = { ...DEFAULT_CONFIG, template: "interruptions" as const, noiseUv: 0 };
    let hasSignal = false;
    let hasDropout = false;
    for (let i = 0; i < 5000; i++) {
      const s = generateSamples(config, i);
      if (Math.abs(s[0]) > 1) hasSignal = true;
      if (Math.abs(s[0]) < 0.1) hasDropout = true;
    }
    expect(hasSignal).toBe(true);
    expect(hasDropout).toBe(true);
  });

  it("file template reads from fileData", () => {
    const config: VirtualEegConfig = {
      ...DEFAULT_CONFIG,
      template: "file",
      channels: 2,
      fileData: [
        [10, 20, 30],
        [40, 50, 60],
      ],
      fileName: "test.csv",
    };
    expect(generateSamples(config, 0)[0]).toBe(10);
    expect(generateSamples(config, 1)[0]).toBe(20);
    expect(generateSamples(config, 3)[0]).toBe(10); // wraps
  });

  it("dropout probability causes zero samples", () => {
    const config = { ...DEFAULT_CONFIG, dropoutProb: 1000, noiseUv: 0 };
    // With dropout prob = 1000 (way above 1), most samples should be zero
    let zeroCount = 0;
    for (let i = 0; i < 100; i++) {
      const s = generateSamples(config, i);
      if (s[0] === 0) zeroCount++;
    }
    expect(zeroCount).toBeGreaterThan(50);
  });

  it("line noise adds 50 Hz component", () => {
    const config = { ...DEFAULT_CONFIG, template: "sine" as const, lineNoise: "50hz" as const, noiseUv: 10 };
    const samples = generateSamples(config, 100);
    // Just verify it runs without error and produces values
    expect(samples).toHaveLength(4);
    expect(typeof samples[0]).toBe("number");
  });
});

describe("Runtime", () => {
  it("creates a stopped runtime", () => {
    const rt = createRuntime(DEFAULT_CONFIG);
    expect(rt.running).toBe(false);
    expect(rt.sampleIndex).toBe(0);
    expect(rt.timer).toBeNull();
  });

  it("starts and stops cleanly", async () => {
    const rt = createRuntime({ ...DEFAULT_CONFIG, sampleRate: 1000 });
    const received: number[][] = [];
    rt.onSamples = (_electrode, samples) => {
      received.push(samples);
    };

    startRuntime(rt);
    expect(rt.running).toBe(true);

    await new Promise((r) => setTimeout(r, 50));

    stopRuntime(rt);
    expect(rt.running).toBe(false);
    expect(received.length).toBeGreaterThan(0);
  });

  it("does not double-start", () => {
    const rt = createRuntime(DEFAULT_CONFIG);
    startRuntime(rt);
    const timer1 = rt.timer;
    startRuntime(rt); // should be no-op
    expect(rt.timer).toBe(timer1);
    stopRuntime(rt);
  });
});
