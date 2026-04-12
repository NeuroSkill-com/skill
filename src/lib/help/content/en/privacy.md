# Privacy Overview
{app} is designed to be fully local-first. Your EEG data, embeddings, labels, and settings never leave your machine unless you explicitly choose to share them.

# Data Storage

## All data stays on your device
Every piece of data {app} records — raw EEG samples (CSV), ZUNA embeddings (SQLite + HNSW index), text labels, calibration timestamps, logs, and settings — is stored locally in {dataDir}/. No data is uploaded to any cloud service, server, or third party.

## No user accounts
{app} does not require sign-up, login, or any form of account creation. There are no user identifiers, tokens, or authentication credentials stored or transmitted.

## Data location
All files are stored under {dataDir}/ on macOS and Linux. Each recording day gets its own YYYYMMDD subdirectory containing the EEG SQLite database and HNSW vector index. Labels are in {dataDir}/labels.sqlite. Logs are in {dataDir}/logs/. You can delete any of these files at any time.

# Network Activity

## No telemetry or analytics
{app} does not collect usage analytics, crash reports, telemetry, or any form of behavioural tracking. There are no analytics SDKs, tracking pixels, or phone-home beacons embedded in the application.

## Local-only WebSocket server
{app} runs a WebSocket server bound to your local network interface for LAN streaming to companion tools. This server is not exposed to the internet. It broadcasts derived EEG metrics (band powers, scores, heart rate) and status updates to clients on the same local network. Raw EEG/PPG/IMU sample streams are not broadcast.

## mDNS / Bonjour service
{app} registers a _skill._tcp.local. mDNS service so LAN clients can discover the WebSocket port automatically. This advertisement is local-only (multicast DNS) and is not visible outside your network.

## Update checks
When you click 'Check for Updates' in Settings, {app} contacts the configured update endpoint to check for a newer version. This is the only outbound internet request the app makes, and it only happens when you explicitly trigger it. Update bundles are verified with an Ed25519 signature before installation.

# Bluetooth & Device Security

## Bluetooth Low Energy (BLE)
{app} communicates with your BCI device over Bluetooth Low Energy or USB serial. The connection uses the standard CoreBluetooth (macOS) or BlueZ (Linux) system stack. No custom Bluetooth drivers or kernel modules are installed.

## OS-level permissions
Bluetooth access requires explicit system permission. On macOS, you must grant Bluetooth access in System Settings → Privacy & Security → Bluetooth. {app} cannot access Bluetooth without your consent.

## Device identifiers
The device serial number and MAC address are received from the BCI headset and displayed in the UI. These identifiers are stored only in the local settings file and are never transmitted over the network.

# On-Device Processing

## GPU inference stays local
The ZUNA embedding encoder runs entirely on your local GPU via wgpu. Model weights are loaded from the local Hugging Face cache (~/.cache/huggingface/). No EEG data is sent to any external inference API or cloud GPU.

## Filtering and analysis
All signal processing — overlap-save filtering, FFT band-power computation, spectrogram generation, and signal quality monitoring — runs locally on your CPU/GPU. No raw or processed EEG data leaves your machine.

## Nearest-neighbour search
The HNSW vector index used for similarity search is built and queried entirely on your device. Search queries never leave your machine.

# Your Data, Your Control

## Access
All your data is in {dataDir}/ in standard formats (CSV, SQLite, binary HNSW). You can read, copy, or process it with any tool.

## Delete
Delete any file or directory under {dataDir}/ at any time. There are no cloud backups to worry about. Uninstalling the app removes only the application binary — your data in {dataDir}/ is untouched unless you delete it.

## Export
CSV recordings and SQLite databases are portable standard formats. Copy them to any machine or import into Python, R, MATLAB, or any analysis tool.

## Encrypt
{app} does not encrypt data at rest. If you need disk-level encryption, use your operating system's full-disk encryption (FileVault on macOS, LUKS on Linux).

# Activity Tracking

## Activity Tracking
When enabled, NeuroSkill records which application is in the foreground and the last time the keyboard and mouse were used. This data stays entirely on your device in ~/.skill/activity.sqlite — it is never sent to any server, logged remotely, or included in any form of analytics. Active-window tracking captures: application name, executable path, window title, and the Unix timestamp at which that window became active. Keyboard and mouse tracking captures only two timestamps (last keyboard event, last mouse event) — never keystrokes, typed text, cursor coordinates, or click targets. Both features can be independently disabled in Settings → Activity Tracking; disabling a feature immediately stops collection. Existing rows are not deleted automatically, but you can remove them at any time by deleting activity.sqlite.

## Accessibility Permission (macOS)
On macOS, keyboard and mouse tracking requires the Accessibility permission because it installs a CGEventTap — a system-level hook that intercepts input events. Apple mandates this permission for any app that reads global input. The permission is requested only when the feature is enabled. If you decline or revoke it, the hook fails silently: the rest of the app continues normally and only the input-activity timestamps stay at zero. Active-window tracking (app name/path) does not require Accessibility — it uses AppleScript/osascript which operates within normal app entitlements.

# Summary

## No cloud
No cloud. All EEG data, embeddings, labels, and settings are stored locally in {dataDir}/.

## No telemetry
No telemetry. No analytics, crash reports, or usage tracking of any kind.

## No accounts
No accounts. No sign-up, login, or user identifiers.

## One optional network request
One optional network request. Update checks, only when you explicitly trigger them.

## Fully on-device
Fully on-device. GPU inference, signal processing, and search all run locally.

## Activity tracking is local-only
Activity tracking is local-only. Window focus and input timestamps are written to activity.sqlite on your device and never leave it.
