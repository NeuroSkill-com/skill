// SPDX-License-Identifier: GPL-3.0-only
/** English "virtual-eeg" namespace — virtual EEG device simulator. */
const virtualEeg: Record<string, string> = {
  "settingsTabs.virtualEeg": "Virtual EEG",

  "veeg.title": "Virtual EEG Device",
  "veeg.desc":
    "Simulate an EEG headset for testing, demos, and development. Generates synthetic data that flows through the full signal pipeline.",

  "veeg.status": "Status",
  "veeg.running": "Running",
  "veeg.stopped": "Stopped",
  "veeg.start": "Start",
  "veeg.stop": "Stop",

  "veeg.channels": "Channels",
  "veeg.channelsDesc": "Number of EEG electrodes to simulate.",
  "veeg.sampleRate": "Sample Rate (Hz)",
  "veeg.sampleRateDesc": "Samples per second per channel.",

  "veeg.template": "Signal Template",
  "veeg.templateDesc": "Choose the type of synthetic signal to generate.",
  "veeg.templateSine": "Sine waves",
  "veeg.templateSineDesc": "Clean sine waves at standard frequency bands (delta, theta, alpha, beta, gamma).",
  "veeg.templateGoodQuality": "Good quality EEG",
  "veeg.templateGoodQualityDesc": "Realistic resting-state EEG with dominant alpha rhythm and pink noise background.",
  "veeg.templateBadQuality": "Bad quality EEG",
  "veeg.templateBadQualityDesc": "Noisy signal with muscle artefacts, 50/60 Hz line noise, and electrode pops.",
  "veeg.templateInterruptions": "Intermittent connection",
  "veeg.templateInterruptionsDesc":
    "Good signal with periodic dropouts simulating loose electrodes or wireless interference.",
  "veeg.templateFile": "From file",
  "veeg.templateFileDesc": "Replay samples from a CSV or EDF file.",

  "veeg.quality": "Signal Quality",
  "veeg.qualityDesc": "Adjust signal-to-noise ratio. Higher = cleaner signal.",
  "veeg.qualityPoor": "Poor",
  "veeg.qualityFair": "Fair",
  "veeg.qualityGood": "Good",
  "veeg.qualityExcellent": "Excellent",

  "veeg.chooseFile": "Choose File",
  "veeg.noFile": "No file selected",
  "veeg.fileLoaded": "{name} ({channels}ch, {samples} samples)",

  "veeg.advanced": "Advanced",
  "veeg.amplitudeUv": "Amplitude (µV)",
  "veeg.amplitudeDesc": "Peak-to-peak amplitude of generated signals.",
  "veeg.noiseUv": "Noise floor (µV)",
  "veeg.noiseDesc": "RMS amplitude of additive Gaussian noise.",
  "veeg.lineNoise": "Line noise",
  "veeg.lineNoiseDesc": "Add 50 Hz or 60 Hz mains interference.",
  "veeg.lineNoise50": "50 Hz",
  "veeg.lineNoise60": "60 Hz",
  "veeg.lineNoiseNone": "None",
  "veeg.dropoutProb": "Dropout probability",
  "veeg.dropoutDesc": "Chance of signal dropout per second (0 = none, 1 = constant).",

  "veeg.preview": "Signal Preview",
  "veeg.previewDesc": "Live preview of the first 4 channels.",
};

export default virtualEeg;
