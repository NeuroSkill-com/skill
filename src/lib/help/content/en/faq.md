## Where is my data stored?
Everything is stored locally in {dataDir}/ — raw CSV recordings, HNSW vector indices, embedding SQLite databases, labels, logs, and settings. Nothing is sent to the cloud.

## What does the ZUNA encoder do?
ZUNA is a GPU-accelerated transformer encoder that converts 5-second EEG epochs into compact embedding vectors. These vectors capture the neural signature of each moment and power the similarity search feature.

## Why does calibration require a connected device?
Calibration runs a timed task (e.g. eyes open / eyes closed) and records labelled EEG data. Without live streaming data, the calibration would have no neural signal to associate with each phase.

## How do I connect from Python / Node.js?
Discover the WebSocket port via mDNS (dns-sd -B _skill._tcp on macOS), then open a standard WebSocket connection. Send JSON commands and receive live event streams. See the API tab for wire-format details.

## What do the signal quality indicators mean?
Each dot represents one EEG electrode. Green = good skin contact, low noise. Yellow = some movement artifact or loose fit. Red = high noise, poor contact. Grey = no signal detected.

## Can I change the notch filter frequency?
Yes — go to Settings → Signal Processing and choose 50 Hz (Europe, most of Asia) or 60 Hz (Americas, Japan). This removes powerline interference from the display and band-power calculation.

## How do I reset a paired device?
Open Settings → Paired Devices, then click the × button next to the device you want to forget. You can then scan for it again.

## Why does the tray icon turn red?
Bluetooth is turned off on your system. Open System Settings → Bluetooth and enable it. {app} will reconnect automatically within ~1 second.

## The app keeps spinning but never connects — what should I do?
1. Make sure the device is powered on (Muse: hold until you feel a vibration; Ganglion/Cyton: check the blue LED). 2. Keep it within 5 m. 3. If it still fails, power-cycle the device.

## How do I grant Bluetooth permission?
macOS will show a permission dialog the first time {app} tries to connect. If you dismissed it, go to System Settings → Privacy & Security → Bluetooth and enable {app}.

## What metrics are stored in the database?
Every 2.5 s epoch stores: the ZUNA embedding vector (32-D), relative band powers (delta, theta, alpha, beta, gamma, high-gamma) averaged across channels, per-channel band powers as JSON, derived scores (relaxation, engagement), FAA, cross-band ratios (TAR, BAR, DTR), spectral shape (PSE, APF, BPS, SNR), coherence, Mu suppression, mood index, and PPG averages if available.

## What is Session Compare?
Compare (⌘⇧M) lets you pick two time ranges and compare them side-by-side: relative band-power bars with deltas, all derived scores and ratios, Frontal Alpha Asymmetry, sleep staging hypnograms, and Brain Nebula™ — a 3D UMAP embedding projection.

## What is Brain Nebula™?
Brain Nebula™ (technically: UMAP Embedding Distribution) projects high-dimensional EEG embeddings into 3D space so that similar brain states appear as nearby points. Range A (blue) and Range B (amber) form distinct clusters when the sessions differ. You can orbit, zoom, and click labelled points to trace temporal connections. Multiple labels can be highlighted simultaneously in different colours.

## Why does Brain Nebula™ show a random cloud at first?
The UMAP projection is computationally expensive and runs in a background job queue so the UI stays responsive. While computing, a random placeholder cloud is shown. Once the projection is ready, points animate smoothly to their final positions.

## What are labels and how are they used?
Labels are user-defined tags (e.g. 'meditation', 'reading') attached to a moment during recording. They're stored alongside EEG embeddings. In the UMAP viewer, labelled points appear larger with coloured rings — click one to trace that label through time across both sessions.

## What is Frontal Alpha Asymmetry (FAA)?
FAA is ln(AF8 α) − ln(AF7 α). Positive values suggest approach motivation (engagement, curiosity). Negative values suggest withdrawal (avoidance, anxiety).

## How does sleep staging work?
Each EEG epoch is classified as Wake, N1, N2, N3, or REM based on relative delta, theta, alpha, and beta power. The compare view shows a hypnogram for each session with stage breakdowns and time percentages.

## What are the keyboard shortcuts?
⌘⇧O — Open {app} window. ⌘⇧M — Open Session Compare. Customise shortcuts in Settings → Shortcuts.

## What is the WebSocket API?
{app} exposes a JSON WebSocket API on the local network (mDNS: _skill._tcp). Commands: status, label, search, compare (metrics + sleep + UMAP ticket), sessions, sleep, umap (enqueue 3D projection), umap_poll (retrieve result). Run 'node test.js' to smoke-test.

## What are Relaxation and Engagement scores?
Relaxation = α/(β+θ), measuring calm wakefulness. Engagement = β/(α+θ), measuring sustained mental involvement. Both are mapped to 0–100 via a sigmoid.

## What are TAR, BAR, and DTR?
TAR (Theta/Alpha) — higher = drowsier or more meditative. BAR (Beta/Alpha) — higher = more stressed or focused. DTR (Delta/Theta) — higher = deeper sleep or relaxation. All averaged across channels.

## What are PSE, APF, BPS, and SNR?
PSE (Power Spectral Entropy, 0–1) — spectral complexity. APF (Alpha Peak Frequency, Hz) — max alpha power frequency. BPS (Band-Power Slope) — 1/f aperiodic exponent. SNR (Signal-to-Noise Ratio, dB) — broadband vs line noise.

## What is the Theta/Beta Ratio (TBR)?
TBR is the ratio of absolute theta to absolute beta power. Higher values indicate reduced cortical arousal — elevated TBR is associated with drowsiness and attentional dysregulation. Reference: Angelidis et al. (2016).

## What are Hjorth parameters?
Three time-domain features from Hjorth (1970): Activity (signal variance / total power), Mobility (estimate of mean frequency), and Complexity (bandwidth / deviation from a pure sine). They're computationally cheap and widely used in EEG ML pipelines.

## What nonlinear complexity measures are computed?
Four measures: Permutation Entropy (ordinal pattern complexity, Bandt & Pompe 2002), Higuchi Fractal Dimension (signal fractal structure, Higuchi 1988), DFA Exponent (long-range temporal correlations, Peng et al. 1994), and Sample Entropy (signal regularity, Richman & Moorman 2000). All are averaged across the 4 EEG channels.

## What are SEF95, Spectral Centroid, PAC, and Laterality Index?
SEF95 (Spectral Edge Frequency) is the frequency below which 95% of total power lies — used in anaesthesia monitoring. Spectral Centroid is the power-weighted mean frequency (arousal indicator). PAC (Phase-Amplitude Coupling) measures theta-gamma cross-frequency interaction associated with memory encoding. Laterality Index is generalised left/right power asymmetry across all bands.

## What PPG metrics are computed?
On Muse 2/S (with PPG sensor): Heart Rate (bpm) from IR peak detection, RMSSD/SDNN/pNN50 (heart rate variability — parasympathetic tone), LF/HF Ratio (sympathovagal balance), Respiratory Rate (breaths/min from PPG envelope), SpO₂ Estimate (uncalibrated blood oxygen from red/IR ratio), Perfusion Index (peripheral blood flow), and Baevsky Stress Index (autonomic stress). These appear in the PPG Vitals section when a PPG-equipped headband is connected.

## How do I use the Focus Timer?
Open the Focus Timer via the tray menu, the Command Palette (⌘K → "Focus Timer"), or the global shortcut (⌘⇧P by default). Choose a preset — Pomodoro (25/5), Deep Work (50/10), or Short Focus (15/5) — or set custom durations. Enable "Auto-label EEG" to have NeuroSkill™ automatically tag EEG recordings at the start and end of each focus phase. Session dots track your completed rounds. Your preset and custom settings are saved automatically and restored next time you open the timer.

## How do I manage or edit my annotations?
Open the Labels window via the Command Palette (⌘K → "All Labels"). It shows all annotations with inline text editing (click a label, press ⌘↵ to save or Esc to cancel), delete (with confirmation), and metadata showing the EEG time-range. Use the search box to filter by text. Labels are paginated at 50 per page for large archives.

## How do I compare two specific sessions side-by-side?
From the History page, click "Quick Compare" to enter compare mode. Checkboxes appear on each session row — select exactly two, then click "Compare Selected" to open the Compare window pre-loaded with both sessions. Alternatively open Compare from the tray or Command Palette and use the session dropdowns manually.

## How does Text Embedding Search work?
Your query is converted to a vector by the same sentence-transformer model that indexes your labels. That vector is then searched against the HNSW label index using approximate nearest-neighbour lookup. Results are your own annotations ranked by semantic similarity — so searching "calm and focused" will surface labels like "deep reading" or "meditation" even if those exact words never appeared in your query. Requires the embedding model to be downloaded and the label index to be built (Settings → Embeddings).

## How does Interactive Cross-Modal Search work?
Interactive search bridges text, EEG, and time in a single query. Step 1: your text query is embedded. Step 2: the top text-k semantically similar labels are found. Step 3: for each label, {app} computes the mean EEG embedding over its recording window and retrieves the top eeg-k nearest EEG epochs from all daily indices — crossing from language into brain-state space. Step 4: for each EEG moment found, any annotations within ±reach minutes are collected as "found labels". The four node layers (Query → Text Matches → EEG Neighbors → Found Labels) are rendered as a 4-layer directed graph. Export as SVG for a static image or as DOT source for further processing in Graphviz.

## How do I trigger TTS speech from a script or automation tool?
Use the WebSocket or HTTP API. WebSocket: send {"command":"say","text":"your message"}. HTTP (curl): curl -X POST http://localhost:<port>/say -H 'Content-Type: application/json' -d '{"text":"your message"}'. The say command is fire-and-forget — it responds immediately while audio plays in the background.

## Why is there no sound from TTS?
Check that espeak-ng is installed on PATH (brew install espeak-ng on macOS, apt install espeak-ng on Ubuntu). Check that your system audio output is not muted or routed to a different device. On first run the model (~30 MB) must finish downloading before any sound is heard. Enable TTS debug logging in Settings → Voice to see synthesis events in the log file.

## Can I change the TTS voice or language?
The current version uses the Jasper English (en-us) voice from the KittenML/kitten-tts-mini-0.8 model. Only English text is phonemised correctly. Additional voices and language support are planned for future releases.

## Does TTS require an internet connection?
Only once, for the initial ~30 MB model download from HuggingFace Hub. After that, all synthesis runs fully offline. The model is cached in ~/.cache/huggingface/hub/ and reused on every subsequent launch.

## What OpenBCI boards does NeuroSkill™ support?
NeuroSkill™ supports all boards in the OpenBCI ecosystem via the published openbci crate (crates.io/crates/openbci): Ganglion (4ch, BLE), Ganglion + WiFi Shield (4ch, 1 kHz), Cyton (8ch, USB dongle), Cyton + WiFi Shield (8ch, 1 kHz), Cyton+Daisy (16ch, USB dongle), Cyton+Daisy + WiFi Shield (16ch, 1 kHz), and Galea (24ch, UDP). Any board can be used alongside another BCI device. Select the board in Settings → OpenBCI, then click Connect.

## How do I connect the Ganglion over Bluetooth?
1. Power on the Ganglion — the blue LED should blink slowly. 2. In Settings → OpenBCI select "Ganglion — 4ch · BLE". 3. Save settings, then click Connect. NeuroSkill™ scans for up to the configured timeout (default 10 s). Keep the board within 3–5 m. On macOS, grant Bluetooth permission when prompted (or go to System Settings → Privacy & Security → Bluetooth).

## My Ganglion is powered on but NeuroSkill™ can't find it — what should I try?
1. Confirm the blue LED is blinking (solid or off means it's not advertising — press the button to wake it). 2. Increase the BLE scan timeout in Settings → OpenBCI. 3. Move the board to within 2 m. 4. Quit NeuroSkill™ and reopen to reset the BLE adapter. 5. Toggle Bluetooth off and back on in System Settings. 6. Ensure no other app (OpenBCI GUI, another NeuroSkill™ instance) is already connected — BLE allows only one central at a time. 7. On macOS 14+, check that NeuroSkill™ has Bluetooth permission in System Settings → Privacy & Security → Bluetooth.

## How do I connect a Cyton over USB?
1. Plug the USB radio dongle into your computer (the dongle is the radio — the Cyton board itself has no USB port). 2. Power on the Cyton — slide the power switch to PC. 3. In Settings → OpenBCI select "Cyton — 8ch · USB serial". 4. Click Refresh to list serial ports, then select the port (/dev/cu.usbserial-… on macOS, /dev/ttyUSB0 on Linux, COM3 on Windows) or leave blank for auto-detect. 5. Save settings and click Connect.

## The serial port isn't listed or I get a permission denied error — how do I fix it?
macOS: The dongle appears as /dev/cu.usbserial-*. If absent, install the CP210x or FTDI VCP driver from the chip manufacturer's site. Linux: Run sudo usermod -aG dialout $USER, then log out and back in. Verify the device appears at /dev/ttyUSB0 or /dev/ttyACM0 after plugging in. Windows: Install the CP2104 USB-to-UART driver; the COM port will appear in Device Manager → Ports (COM & LPT).

## How do I connect via the OpenBCI WiFi Shield?
1. Stack the WiFi Shield on top of the Cyton or Ganglion and power the board on. 2. On your computer, connect to the WiFi network the shield broadcasts (SSID: OpenBCI-XXXX, typically no password). 3. In Settings → OpenBCI select the matching WiFi board variant. 4. Enter IP 192.168.4.1 (shield default) or leave blank to auto-discover. 5. Click Connect. The WiFi Shield streams at 1000 Hz — set the low-pass filter to ≤ 500 Hz in Signal Processing to avoid aliasing.

## What is the Galea board and how do I set it up?
Galea by OpenBCI is a 24-channel research biosignals headset combining EEG, EMG, and AUX sensors, streaming over UDP. To connect: 1. Power on Galea and connect it to your local network. 2. In Settings → OpenBCI select "Galea — 24ch · UDP". 3. Enter the Galea IP address (or leave blank to accept from any sender). 4. Click Connect. Channels 1–8 are EEG (drive real-time analysis); 9–16 are EMG; 17–24 are AUX. All 24 are saved to CSV.

## Can I use two BCI devices at the same time?
Yes — NeuroSkill™ can stream from both simultaneously. Whichever device connects first drives the live dashboard, band-power display, and ZUNA embedding pipeline. The second device's data is recorded to CSV for offline analysis. Simultaneous multi-device analysis in the real-time pipeline is planned for a future release.

## Only 4 of my Cyton's 8 channels are used for live analysis — why?
The real-time analysis pipeline (filters, band powers, ZUNA embeddings, signal quality dots) is currently designed for 4-channel inputs to match the Muse headset format. For Cyton (8ch) and Cyton+Daisy (16ch), channels 1–4 feed the live pipeline; all channels are written to CSV for offline work. Full multi-channel pipeline support is on the roadmap.

## How do I improve signal quality on an OpenBCI board?
1. Apply conductive gel or paste at each electrode site and part hair to make direct scalp contact. 2. Verify impedance with the OpenBCI GUI Impedance Check before recording — aim for < 20 kΩ. 3. Connect the SRB bias electrode to the mastoid (behind the ear) for a solid reference. 4. Keep electrode cables short and away from power supplies. 5. Use the notch filter in Settings → Signal Processing (50 Hz for Europe, 60 Hz for Americas). 6. For Ganglion BLE: move the board away from USB 3.0 ports, which emit 2.4 GHz interference.

## My OpenBCI connection drops repeatedly — how do I stabilise it?
Ganglion BLE: Keep the board within 2 m; plug the host computer's BLE adapter into a USB 2.0 port (USB 3.0 emits 2.4 GHz noise that can jam BLE). Cyton USB: Use a short, high-quality USB cable and connect directly to the computer rather than through a hub. WiFi Shield: Ensure the shield's 2.4 GHz channel does not overlap with your router; move the board closer. All boards: avoid running other wireless-intensive apps (video calls, file sync) during recordings.

## What exactly does Activity Tracking record?
Active-window tracking writes one row to activity.sqlite each time the frontmost app or window title changes. Each row contains: application display name (e.g. "Safari", "VS Code"), the full path to the binary or app bundle, the window title (e.g. document name or webpage title — may be empty for sandboxed apps), and a Unix-second timestamp of when it became active. Keyboard & mouse tracking writes a periodic sample every 60 seconds, but only when there has been activity since the last flush. Each sample stores two Unix-second timestamps — the last keyboard event and the last mouse/trackpad event. It does not record what keys you pressed, what text you typed, where the cursor was, or which buttons you clicked. Both features are enabled by default and can be turned off independently in Settings → Activity Tracking.

## Why does macOS ask for Accessibility access for input tracking?
Keyboard and mouse tracking uses a CGEventTap — a macOS API that intercepts system-wide input events before they reach individual apps. Apple requires the Accessibility permission for any application that reads global input, regardless of what that app does with it. Without Accessibility access the tap fails silently: NeuroSkill continues to work normally, but last-keyboard and last-mouse timestamps stay at zero. To grant access: System Settings → Privacy & Security → Accessibility → find NeuroSkill → toggle on. If you prefer not to grant it, disable the "Track Keyboard & Mouse Activity" toggle in Settings — this prevents the hook from being installed in the first place. Active-window tracking (app name and path) uses AppleScript/osascript and does not require Accessibility permission.

## How do I clear or delete Activity Tracking data?
All activity tracking data lives in a single file: ~/.skill/activity.sqlite. To delete everything: quit NeuroSkill, delete that file, then relaunch — an empty database is created automatically on the next start. To stop future collection without touching existing data, turn off both toggles in Settings → Activity Tracking; changes take effect immediately with no restart needed. To selectively remove rows you can open the file in any SQLite browser (e.g. DB Browser for SQLite) and DELETE from active_windows or input_activity.

## Why does {app} ask for Accessibility permission on macOS?
{app} uses the macOS CGEventTap API to record the last time a key was pressed or the mouse moved. This is used to compute keyboard and mouse activity timestamps shown in the Activity Tracking panel. Only the timestamp is stored — no keystrokes, no cursor positions. The feature degrades silently if permission is not granted.

## Does {app} need Bluetooth permission?
Yes. {app} uses Bluetooth Low Energy (BLE) to connect to your BCI headset. On macOS the system will show a one-time Bluetooth permission prompt when the app first tries to scan. On Linux and Windows no explicit Bluetooth permission is required.

## How do I grant Accessibility permission on macOS?
Open System Settings → Privacy & Security → Accessibility. Find {app} in the list and toggle it on. You can also click "Open Accessibility Settings" in the Permissions tab inside the app.

## What happens if I deny Accessibility permission?
Keyboard and mouse activity timestamps will not be recorded and will remain at zero. All other features — EEG streaming, band powers, calibration, TTS, search — continue to work normally. You can disable the feature entirely in Settings → Activity Tracking.

## Can I revoke permissions after granting them?
Yes. Open System Settings → Privacy & Security → Accessibility (or Notifications) and toggle off {app}. The relevant feature will stop working immediately without requiring a restart.
