// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
/** English "onboarding" namespace — reference translation. */
const onboarding: Record<string, string> = {
  "onboarding.title": "Welcome to {app}",
  "onboarding.step.welcome": "Welcome",
  "onboarding.step.bluetooth": "Bluetooth",
  "onboarding.step.fit": "Fit Check",
  "onboarding.step.calibration": "Calibration",
  "onboarding.step.models": "Models",
  "onboarding.step.tray": "Tray",
  "onboarding.step.permissions": "Permissions",
  "onboarding.step.extensions": "Extensions",
  "onboarding.step.enable_bluetooth": "Enable Bluetooth",
  "onboarding.step.done": "Done",
  "onboarding.newBadge": "New",
  "onboarding.fontSizeLabel": "Text size",
  "onboarding.fontSizeDecrease": "Decrease text size",
  "onboarding.fontSizeIncrease": "Increase text size",
  "onboarding.welcomeTitle": "Welcome to {app}",
  "onboarding.welcomeBackTitle": "Welcome back to {app}",
  "onboarding.welcomeBody":
    "{app} records, analyzes, and indexes your EEG data from any supported BCI device. Let's get you set up in a few quick steps.",
  "onboarding.whatsNewTitle": "What's new since you last set up",
  "onboarding.whatsNewBody":
    'We\'ve added a few new steps since the last time you ran this wizard. Your existing setup (Bluetooth, calibration, models) is unchanged — feel free to skim through. The new steps are flagged below and tagged "NEW" in the progress bar:',
  "onboarding.bluetoothHint": "Connect your BCI device",
  "onboarding.fitHint": "Check sensor contact quality",
  "onboarding.calibrationHint": "Run a quick calibration session",
  "onboarding.modelsHint": "Download recommended local AI models",
  "onboarding.trayHint": "Find the app icon in your menu bar / tray",
  "onboarding.permissionsHint": "Optional: let the app see active app, files, clipboard",
  "onboarding.extensionsHint": "Optional: install VS Code, browser, and shell helpers",
  "onboarding.bluetoothTitle": "Connect Your BCI Device",
  "onboarding.bluetoothBody":
    "Turn on your BCI device and wear it. {app} will scan for nearby devices and connect automatically.",
  "onboarding.enableBluetoothTitle": "Enable Bluetooth on Your Mac",
  "onboarding.enableBluetoothBody":
    "{app} needs your Mac's Bluetooth adapter powered on to find and connect to your BCI device. Please enable Bluetooth in System Settings if it is turned off.",
  "onboarding.enableBluetoothStatus": "Bluetooth adapter",
  "onboarding.enableBluetoothHint":
    "Open Bluetooth settings and turn Bluetooth on. If running in development via Terminal, ensure the system adapter is enabled.",
  "onboarding.enableBluetoothOpen": "Open Bluetooth Settings",
  "onboarding.btConnected": "Connected to {name}",
  "onboarding.btScanning": "Scanning…",
  "onboarding.btReady": "Ready to scan",
  "onboarding.btScan": "Scan",
  "onboarding.btInstructions": "How to connect",
  "onboarding.btStep1":
    "Turn on your BCI device (hold the power button, flip the switch, or press the button depending on your headset).",
  "onboarding.btStep2":
    "Place the headset on your head — the sensors should rest behind your ears and on your forehead.",
  "onboarding.btStep3": "Click Scan above. {app} will find and connect to the nearest BCI device automatically.",
  "onboarding.btSuccess": "Headset connected! You can continue.",
  "onboarding.fitTitle": "Check Headset Fit",
  "onboarding.fitBody":
    "Good sensor contact is essential for clean EEG data. All four sensors should show green or yellow.",
  "onboarding.sensorQuality": "Live Sensor Quality",
  "onboarding.quality.good": "Good",
  "onboarding.quality.fair": "Fair",
  "onboarding.quality.poor": "Poor",
  "onboarding.quality.no_signal": "No Signal",
  "onboarding.fitNeedsBt": "Connect your headset first to see live sensor data.",
  "onboarding.fitTips": "Tips for better contact",
  "onboarding.fitTip1":
    "Ear sensors (TP9/TP10): tuck behind and slightly above your ears. Brush away any hair covering the sensors.",
  "onboarding.fitTip2":
    "Forehead sensors (AF7/AF8): should sit flat against clean skin — wipe with a dry cloth if needed.",
  "onboarding.fitTip3":
    "If contact is poor, lightly moisten the sensors with a damp finger. This improves conductivity.",
  "onboarding.fitGood": "Great fit! All sensors have good contact.",
  "onboarding.calibrationTitle": "Run Calibration",
  "onboarding.calibrationBody":
    "Calibration records labeled EEG while you alternate between two mental states. This helps {app} learn your brain's baseline patterns.",
  "onboarding.openCalibration": "Open Calibration",
  "onboarding.calibrationNeedsBt": "Connect your headset first to run calibration.",
  "onboarding.calibrationSkip": "You can skip this and calibrate later from the tray menu or settings.",
  "onboarding.modelsTitle": "Download Recommended Models",
  "onboarding.modelsBody":
    "For the best local experience, download these defaults now: Qwen3.5 4B (Q4_K_M), ZUNA encoder, NeuTTS, and Kitten TTS.",
  "onboarding.models.downloadAll": "Download Recommended Set",
  "onboarding.models.download": "Download",
  "onboarding.models.downloading": "Downloading…",
  "onboarding.models.downloaded": "Downloaded",
  "onboarding.models.qwenTitle": "Qwen3.5 4B (Q4_K_M)",
  "onboarding.models.qwenDesc":
    "Recommended chat model. Uses Q4_K_M for the best quality/speed balance on most laptops.",
  "onboarding.models.zunaTitle": "ZUNA EEG Encoder",
  "onboarding.models.zunaDesc": "Needed for EEG embeddings, semantic history, and downstream brain-state analytics.",
  "onboarding.models.neuttsTitle": "NeuTTS (Nano Q4)",
  "onboarding.models.neuttsDesc": "Recommended multilingual voice engine with better quality and cloning support.",
  "onboarding.models.kittenTitle": "Kitten TTS",
  "onboarding.models.kittenDesc":
    "Lightweight fast voice backend, useful as a quick fallback and for low-resource systems.",
  "onboarding.models.ocrTitle": "OCR Models",
  "onboarding.models.ocrDesc":
    "Text detection + recognition models for extracting text from screenshots. Enables text search across captured screens (~10 MB each).",
  "onboarding.screenRecTitle": "Screen Recording Permission",
  "onboarding.screenRecDesc":
    "Required on macOS to capture other application windows for the screenshot system. Without it, screenshots may be blank.",
  "onboarding.screenRecOpen": "Open Settings",
  "onboarding.trayTitle": "Find the App in Your Tray",
  "onboarding.trayBody":
    "{app} runs quietly in the background. After setup, the icon in your menu bar (macOS) or system tray (Windows/Linux) is your entry point back into the app.",
  "onboarding.tray.states": "The icon changes colour to show status:",
  "onboarding.tray.grey": "Grey — disconnected",
  "onboarding.tray.amber": "Amber — scanning or connecting",
  "onboarding.tray.green": "Green — connected and recording",
  "onboarding.tray.red": "Red — Bluetooth is off",
  "onboarding.tray.open": "Click the tray icon anytime to show or hide the main dashboard.",
  "onboarding.tray.menu":
    "Right-click the icon (or left-click on Windows/Linux) for quick actions — connect, label, calibrate, and more.",
  "onboarding.extensionsTitle": "Companion Extensions",
  "onboarding.extensionsBody":
    "{app} can pull in extra context from your editor, browser, and terminal. Each integration is a separate piece you can install or skip independently — none are required for the EEG features to work.",
  "onboarding.extensionsPrivacy":
    "Same privacy guarantee as everything else: every extension reports to the local daemon over a localhost port, and that data is written to activity.sqlite on this computer. Nothing is uploaded to NeuroSkill or anyone else.",
  "onboarding.extensionsSkip":
    "All optional. You can install, update, or remove any of these later in Settings → Extensions and Settings → Terminal.",
  "onboarding.extensions.vscodeTitle": "VS Code-family editor",
  "onboarding.extensions.vscodeDesc":
    "Adds per-file edit tracking, AI inline suggestions, and integration with the dev loop. Works with VS Code, VSCodium, Cursor, Windsurf, Trae, Positron — any installed fork is auto-detected.",
  "onboarding.extensions.browserTitle": "Browser extension",
  "onboarding.extensions.browserDesc":
    "Records active tab, page focus time, and reading patterns from your browser. Sideload supported for Chrome, Firefox, Edge, and Safari (Safari needs an extra signing step).",
  "onboarding.extensions.terminalTitle": "Terminal / shell hooks",
  "onboarding.extensions.terminalDesc":
    "Adds a small preexec/precmd hook to your shell so the app can correlate command timing with focus state. Pick zsh, bash, fish, or PowerShell — modifies your rc file with a single source line, fully removable later.",

  "onboarding.permissionsTitle": "Optional Activity Tracking",
  "onboarding.permissionsBody":
    '{app} can record what you were working on so it can correlate your EEG/focus data with the actual context — "I lost focus while writing this PR" rather than just "I lost focus at 3 pm." These are off by default and entirely optional.',
  "onboarding.permissionsPrivacy":
    "Everything stays on this computer. Recorded activity is written to a local activity.sqlite file and is never sent to any server — not to NeuroSkill, not to anyone. You can turn each option off at any time and the recorded data stays on disk until you delete it.",
  "onboarding.permissionsSkip":
    "All off by default. You can enable any of these later in Settings → Activity Tracking.",
  "onboarding.permissionsActiveWindowDesc":
    "Captures the foreground app, window title, active browser tab, and open editor file path. macOS will ask for Accessibility / Automation access for each browser and editor.",
  "onboarding.permissionsInputDesc":
    "Records only the timestamps of keyboard/mouse use — never which keys, never positions, never content. Needs no OS permission.",
  "onboarding.permissionsFileDesc":
    "Watches Documents, Desktop, Downloads, and common dev folders for create/modify/delete events. Records only paths and timestamps — file contents are never read. macOS may prompt for Full Disk Access.",
  "onboarding.permissionsScreenshotsDesc":
    'Captures the screen on an interval, runs OCR on text, and indexes both for visual search and "what was on my screen at 3pm" lookups. macOS asks for Screen Recording. Tune interval, quality, and OCR in Settings → Screenshots.',
  "onboarding.permissionsLocationDesc":
    "Records device location alongside focus blocks (home vs office vs cafe) so context switches across places can be correlated with your focus state. macOS asks for Location Services. Stored locally; never uploaded.",
  "onboarding.permissionsCalendarDesc":
    "Reads calendar event metadata (title, time, duration, attendees count) to correlate meeting density with focus dips. macOS asks for Calendar Access on first use. Event content is never uploaded.",
  "onboarding.permissionsClipboardDesc":
    "Records when the clipboard changes (which app, content type, size). The contents are never read. macOS only; will ask for Automation access.",
  "onboarding.downloadsComplete": "All Downloads Complete!",
  "onboarding.downloadsCompleteBody":
    "The recommended models are downloaded and ready to use. To download more models or switch to different ones, open",
  "onboarding.downloadMoreSettings": "app settings",
  "onboarding.doneTitle": "You're All Set!",
  "onboarding.doneBody": "{app} is running in your menu bar. Here are a few things to know:",
  "onboarding.doneTip.tray": "{app} lives in your menu bar tray. Click the icon to show/hide the dashboard.",
  "onboarding.doneTip.shortcuts": "Use ⌘K to open the command palette, or ? to see all keyboard shortcuts.",
  "onboarding.doneTip.help": "Open Help from the tray menu for a full reference of every feature.",
  "onboarding.back": "Back",
  "onboarding.next": "Next",
  "onboarding.getStarted": "Get Started",
  "onboarding.finish": "Finish",
};

export default onboarding;
