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

  // ── Virtual Devices window ────────────────────────────────────────────────────
  "window.title.virtualDevices": "{app} – Virtual Devices",

  "vdev.title": "Virtual Devices",
  "vdev.desc":
    "Test NeuroSkill without physical EEG hardware. Pick a preset that matches a real device or configure your own synthetic signal source.",

  "vdev.presets": "Device Presets",
  "vdev.statusRunning": "Virtual device streaming",
  "vdev.statusStopped": "No virtual device running",
  "vdev.selected": "Ready",
  "vdev.configure": "configure",
  "vdev.customConfig": "Custom Configuration",

  "vdev.presetMuse": "Muse S",
  "vdev.presetMuseDesc": "4-channel headband layout — TP9, AF7, AF8, TP10.",
  "vdev.presetCyton": "OpenBCI Cyton",
  "vdev.presetCytonDesc": "8-channel research-grade signal, full frontal/central montage.",
  "vdev.presetCap32": "32-Ch EEG Cap",
  "vdev.presetCap32Desc": "Full 10-20 international system, 32 electrodes.",
  "vdev.presetAlpha": "Strong Alpha",
  "vdev.presetAlphaDesc": "Prominent 10 Hz alpha rhythm — relaxed eyes-closed baseline.",
  "vdev.presetArtifact": "Artifact Test",
  "vdev.presetArtifactDesc": "Noisy signal with muscle artifacts and 50 Hz line noise.",
  "vdev.presetDropout": "Dropout Test",
  "vdev.presetDropoutDesc": "Periodic signal loss simulating loose electrodes.",
  "vdev.presetMinimal": "Minimal (1ch)",
  "vdev.presetMinimalDesc": "Single-channel sine wave — lightest possible load.",
  "vdev.presetCustom": "Custom",
  "vdev.presetCustomDesc": "Define your own channel count, rate, template and noise level.",

  "vdev.lslSourceTitle": "Virtual LSL Source",
  "vdev.lslRunning": "Streaming synthetic EEG via LSL",
  "vdev.lslStopped": "Virtual LSL source stopped",
  "vdev.lslDesc": "Starts a local Lab Streaming Layer source so you can test LSL stream discovery and connection.",
  "vdev.lslHint":
    'Open the main Settings → LSL tab and click "Scan Network" to see SkillVirtualEEG in the stream list, then connect to it.',
  "vdev.lslStarted": "Virtual LSL source is now streaming on the local network.",

  // Status panel
  "vdev.statusSource": "LSL source",
  "vdev.statusSession": "Session",
  "vdev.sessionConnected": "Connected",
  "vdev.sessionConnecting": "Connecting…",
  "vdev.sessionDisconnected": "Disconnected",
  "vdev.startBtn": "Start Virtual Device",
  "vdev.stopBtn": "Stop Virtual Device",
  "vdev.autoConnect": "Auto-connect to dashboard",
  "vdev.autoConnectDesc": "Connect the dashboard to this source immediately after starting.",

  // Preview
  "vdev.previewOffline": "Signal preview (offline)",
  "vdev.previewOfflineDesc":
    "Client-side waveform preview — shows signal shape before connecting. No data is streamed yet.",

  // Custom preset — channel / rate
  "vdev.cfgChannels": "Channels",
  "vdev.cfgChannelsDesc": "Number of EEG electrodes to simulate.",
  "vdev.cfgRate": "Sample Rate",
  "vdev.cfgRateDesc": "Samples per second per channel.",

  // Custom preset — signal quality
  "vdev.cfgQuality": "Signal Quality",
  "vdev.cfgQualityDesc": "Signal-to-noise ratio. Higher = cleaner signal.",

  // Custom preset — template
  "vdev.cfgTemplate": "Signal Template",
  "vdev.cfgTemplateSine": "Sine waves",
  "vdev.cfgTemplateSineDesc": "Pure sine waves at delta, theta, alpha, beta and gamma frequencies.",
  "vdev.cfgTemplateGood": "Good quality EEG",
  "vdev.cfgTemplateGoodDesc": "Realistic resting-state with dominant alpha and pink noise background.",
  "vdev.cfgTemplateBad": "Bad quality EEG",
  "vdev.cfgTemplateBadDesc": "Noisy signal with muscle artefacts, line noise and electrode pops.",
  "vdev.cfgTemplateInterruptions": "Intermittent connection",
  "vdev.cfgTemplateInterruptionsDesc": "Good signal with periodic dropouts simulating loose electrodes.",

  // Custom preset — advanced
  "vdev.cfgAdvanced": "Advanced",
  "vdev.cfgAmplitude": "Amplitude (µV)",
  "vdev.cfgAmplitudeDesc": "Peak-to-peak amplitude of the simulated signal.",
  "vdev.cfgNoise": "Noise floor (µV)",
  "vdev.cfgNoiseDesc": "RMS amplitude of additive Gaussian background noise.",
  "vdev.cfgLineNoise": "Line noise",
  "vdev.cfgLineNoiseDesc": "Inject 50 Hz or 60 Hz mains interference.",
  "vdev.cfgLineNoiseNone": "None",
  "vdev.cfgLineNoise50": "50 Hz",
  "vdev.cfgLineNoise60": "60 Hz",
  "vdev.cfgDropout": "Dropout probability",
  "vdev.cfgDropoutDesc": "Chance of signal dropout per second (0 = never, 1 = constant).",
};

export default virtualEeg;
