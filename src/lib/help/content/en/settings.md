# Settings Tab
Configure device preferences, signal processing, embedding parameters, calibration, shortcuts, and logging.

## Paired Devices
Lists all BCI devices the app has seen. You can set a preferred device (auto-connect target), forget devices, or scan for new ones. RSSI signal strength is shown for recently seen devices.

## Signal Processing
Configure the real-time EEG filter chain: low-pass cutoff (removes high-frequency noise), high-pass cutoff (removes DC drift), and powerline notch filter (removes 50 or 60 Hz mains hum and harmonics). Changes apply immediately to the waveform display and band powers.

## EEG Embedding
Adjust the overlap between consecutive 5-second embedding epochs. Higher overlap means more embeddings per minute (finer temporal resolution in search) at the cost of more storage and compute.

## Calibration
Configure the calibration task: action labels (e.g. "eyes open", "eyes closed"), phase durations, number of repetitions, and whether to auto-start calibration on app launch.

## Calibration Voice Guidance (TTS)
During calibration the app announces each phase by name using on-device English text-to-speech. The engine is powered by KittenTTS (tract-onnx, ~30 MB) with espeak-ng phonemisation. The model is downloaded from HuggingFace Hub on first launch and cached locally — no data leaves your device after that. Speech fires for: session start, each action phase, every break ("Break. Next: …"), and session completion. Requires espeak-ng on PATH (brew / apt / apk install espeak-ng). English only.

## Global Shortcuts
Set system-wide keyboard shortcuts to open the Label, Search, Settings, and Calibration windows from any application. Uses the standard accelerator format (e.g. CmdOrCtrl+Shift+L).

## Debug Logging
Toggle per-subsystem logging to the daily log file at {dataDir}/logs/. Subsystems include embedder, devices, websocket, csv, filter, and bands.

## Updates
Check for and install app updates. Uses Tauri's built-in updater with Ed25519 signature verification.

## Appearance
Choose a colour mode (System / Light / Dark), enable High Contrast for stronger borders and text, and pick a chart colour scheme for EEG waveforms and band-power visualisations. Colourblind-safe palettes are available. Language is also changed here via the locale picker.

## Goals
Set a daily recording target in minutes. A progress bar appears on the dashboard while streaming, and a notification fires when you hit your goal. The last-30-days chart shows which days you reached (green), reached halfway (amber), made some progress (dim), or missed (none).

## Text Embeddings
Labels and search queries are embedded using nomic-embed-text-v1.5 (~130 MB ONNX model, 768-dim). The model is downloaded once from HuggingFace Hub and cached locally. It powers both text similarity search and the semantic label index used by Proactive Hooks.

## Shortcuts
Configure global keyboard shortcuts (system-wide hotkeys) for opening the Label, Search, Settings, and Calibration windows. Also shows all in-app shortcuts (⌘K for command palette, ? for shortcuts overlay, ⌘↵ to submit a label). Shortcuts use the standard accelerator format — e.g. CmdOrCtrl+Shift+L.

# Activity Tracking
NeuroSkill can optionally record which app is in the foreground and when the keyboard and mouse were last used. Both features are off-by-default opt-ins, fully local, and independently configurable in Settings → Activity Tracking.

## Active Window Tracking
A background thread wakes every second and asks the OS which application is currently in the foreground. When the app name or window title changes, one row is inserted into activity.sqlite: the application display name (e.g. "Safari"), the full path to the application bundle or executable, the frontmost window title (e.g. the document name or current webpage), and a Unix-second timestamp recording when that window became active. If you stay in the same window, no new row is written — idle time in a single app produces no database activity. On macOS the tracker calls osascript; no Accessibility permission is needed for the app name and path, but the window title may be empty for sandboxed apps. On Linux it uses xdotool and xprop (requires an X11 session). On Windows it uses a PowerShell GetForegroundWindow call.

## Keyboard & Mouse Activity Tracking
A global input hook (rdev) listens for every key press and mouse or trackpad event system-wide. It does not record what you typed, which keys you pressed, or where the cursor moved — it only updates two Unix-second timestamps in memory: one for the most recent keyboard event and one for the most recent mouse/trackpad event. These are flushed to activity.sqlite every 60 seconds, but only when at least one value has changed since the last flush, so idle periods leave no trace. The Settings panel receives a live update event (throttled to at most once per second) so the "Last keyboard" and "Last mouse" fields reflect activity in near-real-time.

## Where Data Is Stored
All activity data lives in a single SQLite file: ~/.skill/activity.sqlite. It is never transmitted, synced, or included in any analytics. Two tables are maintained: active_windows (one row per window-focus change, with app name, path, title, and timestamp) and input_activity (one row per 60-second flush when activity was detected, with last-keyboard and last-mouse timestamps). Both tables have a descending index on the timestamp column. WAL journal mode is enabled so background writes never block reads. You can open, inspect, export, or delete the file at any time with any SQLite browser.

## Required OS Permissions
macOS — Active-window tracking (app name and path) requires no special permissions. Keyboard and mouse tracking uses a CGEventTap which requires Accessibility access: open System Settings → Privacy & Security → Accessibility, find NeuroSkill in the list, and toggle it on. Without this permission the input hook fails silently — timestamps stay at zero and the rest of the app is completely unaffected. You can disable the toggle in Settings → Activity Tracking to prevent the permission prompt entirely. Linux — Both features require an X11 session. Active-window tracking uses xdotool and xprop, which are pre-installed on most desktop distributions. Input tracking uses the XRecord extension from libxtst. If either tool is missing, that feature logs a warning and disables itself. Windows — No special permissions are required. Active-window tracking uses GetForegroundWindow via PowerShell; input tracking uses SetWindowsHookEx.

## Disabling & Clearing Data
Both toggles in Settings → Activity Tracking take effect immediately — no restart is required. Disabling active-window tracking stops new rows from being inserted into active_windows and clears the in-memory current-window state. Disabling input tracking stops the rdev callback from updating timestamps and prevents future flushes to input_activity; existing rows are not removed automatically. To delete all collected history: quit the app, delete ~/.skill/activity.sqlite, then relaunch. An empty database will be created automatically on the next start.

# UMAP

## UMAP
Control parameters for the 3D UMAP projection used in Session Compare: number of neighbours (controls local vs. global structure), minimum distance (how tightly points cluster), and the metric (cosine or euclidean). Higher neighbour counts preserve more global topology; lower counts reveal fine-grained local clusters. Projections run in a background job and results are cached.

# EEG Model Tab
Monitor the EEG embedding encoder and HNSW vector index status.

## Encoder Status
Shows whether the EEG embedding encoder is loaded, the architecture summary (dimension, layers, heads), and the path to the .safetensors weight file. The encoder runs entirely on-device using your GPU.

## Embeddings Today
A live counter of how many 5-second EEG epochs have been embedded into today's HNSW index. Each embedding is a compact vector that captures the neural signature of that moment.

## HNSW Parameters
M (connections per node) and ef_construction (search width during build) control the quality/speed trade-off of the nearest-neighbour index. Higher values give better recall but use more memory. Defaults (M=16, ef=200) are a good balance.

## Data Normalisation
The data_norm scaling factor applied to raw EEG before encoding. The default (10) is tuned for Muse 2 / Muse S headsets.

# OpenBCI Boards
Connect and configure any OpenBCI board — Ganglion, Cyton, Cyton+Daisy, WiFi Shield variants, or Galea — standalone or alongside another BCI device.

## Board Selection
Choose which OpenBCI board to use. Ganglion (4 channels, BLE) is the most portable option. Cyton (8 channels, USB serial) adds higher channel count. Cyton+Daisy doubles this to 16 channels. WiFi Shield variants replace the USB/BLE link with a 1 kHz Wi-Fi stream. Galea (24 channels, UDP) is a high-density research board. All variants can run standalone or alongside another BCI device.

## Ganglion BLE
The Ganglion connects over Bluetooth Low Energy. Press Connect and NeuroSkill™ scans for the nearest advertising Ganglion for up to the configured scan timeout. Keep the board within 3–5 m and powered on (blue LED blinking). Only one Ganglion can be active per Bluetooth adapter. Extend the BLE scan timeout in Settings if the board is slow to advertise.

## Serial Port (Cyton / Cyton+Daisy)
Cyton boards communicate via a USB radio dongle. Leave the serial port field blank to auto-detect the first available port, or enter it explicitly (/dev/cu.usbserial-… on macOS, /dev/ttyUSB0 on Linux, COM3 on Windows). Plug in the dongle before clicking Connect and ensure you have serial port permissions — on Linux add your user to the dialout group.

## WiFi Shield
The OpenBCI WiFi Shield creates its own 2.4 GHz access point (SSID: OpenBCI-XXXX). Connect your computer to that network, then set the IP to 192.168.4.1 (the shield's default gateway). Alternatively, the shield can join your home network — enter its assigned IP instead. Leave the IP field blank to attempt auto-discovery via mDNS. WiFi Shield streams at 1 kHz — set the low-pass filter cutoff to ≤ 500 Hz in Signal Processing settings.

## Galea
Galea is a 24-channel research-grade biosignals headset (EEG + EMG + AUX) that streams over UDP. Enter the Galea device's IP address, or leave blank to accept packets from any sender on the local network. Channels 1–8 are EEG and drive real-time analysis; channels 9–16 are EMG; 17–24 are AUX. All 24 channels are saved to CSV.

## Channel Labels & Presets
Assign standard 10-20 electrode names to each physical channel so band-power metrics, Frontal Alpha Asymmetry, and electrode visualisations are electrode-aware. Use a preset (Frontal, Motor, Occipital, Full 10-20) to populate labels automatically, or type custom names. Channels beyond the first 4 are recorded to CSV only and do not drive the real-time analysis pipeline.
