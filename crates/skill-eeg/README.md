# skill-eeg

EEG signal processing pipeline for NeuroSkill.

## Overview

Real-time digital signal processing for 4-channel EEG (Muse / OpenBCI). Covers the full path from raw samples to frequency-band powers, spectrogram columns, signal-quality classification, artifact detection, and head-pose estimation. Uses GPU-accelerated FFT (`gpu-fft` with wgpu) for the heavy lifting.

## Modules

| Module | Description |
|---|---|
| `eeg_filter` | `EegFilter` — per-channel overlap-add pipeline: high-pass, low-pass, notch (50/60 Hz), and spectrogram extraction. Configurable via `FilterConfig` with presets (`full_band_us`, `full_band_eu`, `passthrough`). Outputs `SpectrogramColumn` for live visualization. |
| `eeg_bands` | `BandAnalyzer` — accumulates filtered samples and computes `BandPowers` / `BandSnapshot` (delta through high-gamma) across all channels |
| `band_metrics` | Advanced metric functions: spectral edge frequency, spectral centroid, Hjorth parameters, permutation/sample entropy, DFA, Higuchi fractal dimension, PAC |
| `eeg_quality` | `QualityMonitor` — per-channel signal-quality classification (`Good` / `Fair` / `Bad`) based on amplitude statistics |
| `artifact_detection` | `ArtifactDetector` — detects blink, jaw-clench, and motion artifacts; outputs `ArtifactMetrics` |
| `head_pose` | Head orientation estimation from electrode impedance asymmetry |
| `eeg_model_config` | `EegModelConfig` — on-disk model configuration (HF repo, weights file); `EegModelStatus` — download/load state; `LatestEpochMetrics` — most recent per-epoch scores |
| `constants` | DSP constants re-exported from `skill-constants` |

## Key types

| Type | Description |
|---|---|
| `EegFilter` | Stateful per-channel filter with overlap-add FFT |
| `FilterConfig` | Low-pass, high-pass, notch frequencies and powerline selector |
| `PowerlineFreq` | `Hz50` / `Hz60` / `None` enum |
| `BandAnalyzer` | Accumulates samples → `BandSnapshot` |
| `BandSnapshot` | Per-channel, per-band absolute and relative power |
| `SignalQuality` | `Good` / `Fair` / `Bad` per channel |

## Dependencies

- `skill-constants` — sample rates, window sizes, band definitions
- `gpu-fft` (wgpu) — GPU-accelerated FFT
- `serde` / `serde_json` — serialization
