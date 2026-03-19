# NeuroSkill — User Interface Guide

> Visual walkthrough of every screen, panel, and interaction in the app.
> GIFs show animated recordings of real UI interactions; PNGs show static light/dark snapshots.

---

## Dashboard

The dashboard is your home base while recording. It shows the connected device, live EEG signal quality, brain-state metrics, and real-time charts — everything you need at a glance to know your session is running well and what your brain is doing right now.

![Dashboard overview](screenshots/gifs/dashboard-full-scroll-light.gif)

The dashboard scrolls through: GPU utilisation bar, connected device hero with battery and signal quality indicators, electrode placement guide, Brain State scores (relaxation and engagement), Frontal Alpha Asymmetry gauge, EEG Indices grid (TAR, BAR, DTR, TBR, PSE, APF, coherence, Hjorth parameters, entropy measures), Composite Scores (meditation, cognitive load, drowsiness), Consciousness metrics, PPG/heart-rate metrics, IMU head-pose data, live EEG channel waveform chart, and the recording status bar with daily goal progress.

<details>
<summary>Electrode Placement Guide (expand/collapse)</summary>

When you're unsure about sensor placement, the Electrode Placement Guide unfolds directly inside the dashboard. It shows the standard 10-20 positions used by your headset, with per-channel quality indicators so you can adjust fit in real time without leaving the main view.

![Electrode guide](screenshots/gifs/dashboard-electrode-guide-light.gif)

</details>

<details>
<summary>Collapsible metric sections</summary>

Each metric card on the dashboard can be collapsed to reduce clutter. This is useful when you only care about a subset of metrics — for example, you might collapse EEG Indices and Consciousness during a simple meditation session and keep only Brain State and Composite Scores visible.

![Collapsible sections](screenshots/gifs/dashboard-collapse-sections-light.gif)

</details>

<details>
<summary>Dashboard — dark mode</summary>

![Dashboard dark](screenshots/dashboard-dark.png)

</details>

---

## Settings

Settings is organised into 18 sub-tabs covering every aspect of the app. The sidebar on the left lets you jump directly to any section.

<details>
<summary>All settings tabs (cycle overview)</summary>

A quick visual tour through every settings tab, showing the sidebar navigation and the variety of configuration options available.

![Settings tabs cycle](screenshots/gifs/settings-all-tabs-cycle-light.gif)

</details>

### Goals & Do Not Disturb

<details>
<summary>Daily Recording Goal</summary>

Set a daily recording target (5 minutes to 8 hours) with quick presets. The 30-day bar chart tracks your streak — green for days that met the goal, blue for over halfway, indigo for some progress. A notification fires when you hit your daily target, and consecutive goal-met days build a visible streak counter.

![Goals slider and chart](screenshots/gifs/settings-goals-slider-and-chart-light.gif)

</details>

<details>
<summary>Do Not Disturb Automation</summary>

When enabled, the app automatically activates your system's Focus/DND mode once your engagement score stays above a configurable threshold for a sustained duration. This section reveals: engagement threshold slider (10–95), sustained duration presets (30s to 5min), exit delay presets (1–60min), focus lookback window, macOS Focus mode picker, exit notification toggle, SNR exit threshold, and a live activation progress bar that shows how close you are to triggering DND.

![DND full settings](screenshots/gifs/settings-goals-dnd-full-light.gif)

</details>

### Devices

<details>
<summary>Paired & Discovered Devices</summary>

View all paired EEG headsets, see BLE-discovered nearby devices, and browse the full catalog of supported devices by manufacturer (InterAxon, OpenBCI, Emotiv, Neurosity, IDUN, BrainBit). Each company section expands to show pairing instructions specific to that hardware.

![Devices list](screenshots/gifs/settings-devices-list-light.gif)

</details>

<details>
<summary>OpenBCI Configuration</summary>

Expandable section for OpenBCI boards: choose board type (Ganglion, Cyton, Cyton+Daisy, Galea), configure connection method (BLE scan, serial port, WiFi shield IP), and set board-specific parameters.

![OpenBCI config](screenshots/gifs/settings-devices-openbci-light.gif)

</details>

### Sleep

<details>
<summary>Sleep Schedule</summary>

Configure your bedtime and wake time with a visual clock dial. Quick presets let you pick common schedules (early bird, night owl, shift worker). The sleep analysis engine uses these times to detect overnight sessions and compute sleep staging metrics.

![Sleep settings](screenshots/gifs/settings-sleep-light.gif)

</details>

### Calibration

<details>
<summary>Calibration Profiles</summary>

View and manage calibration profiles. Each profile defines a sequence of timed actions (eyes closed, eyes open, deep breathing, mental arithmetic, music listening) with configurable durations, break intervals, and loop counts. The last-calibrated timestamp helps you know when to recalibrate.

![Calibration profiles](screenshots/gifs/settings-calibration-profiles-light.gif)

</details>

<details>
<summary>Profile Editor</summary>

Click "New Profile" to open the full editor: name the profile, add/remove/reorder actions, pick duration presets per action (5s to 30s), set break duration between actions, loop count, and auto-start toggle. The visual timeline at the bottom previews the full calibration sequence.

![Calibration editor](screenshots/gifs/settings-calibration-editor-light.gif)

</details>

### Voice (TTS)

<details>
<summary>Text-to-Speech Engine</summary>

Choose between KittenTTS (lightweight, local) and NeuTTS (neural, higher quality) backends. Browse the voice picker grid, select preset voices or configure a custom voice with a reference WAV file, and test-speak any text. The engine status indicator shows whether the model is loaded, loading, or idle.

![Voice settings](screenshots/gifs/settings-voice-light.gif)

</details>

### LLM (Local Language Model)

<details>
<summary>Server Controls</summary>

Enable the local LLM server, toggle auto-start on launch, start/stop the inference server, see the active model and its status badge, and view the OpenAI-compatible API endpoints (/v1/chat/completions, /v1/completions, /v1/embeddings, /v1/models, /health). The server log shows real-time inference requests with timing.

![LLM server](screenshots/gifs/settings-llm-server-light.gif)

</details>

<details>
<summary>Model Library</summary>

Browse model families (Qwen, Llama, Phi, Gemma) via the dropdown, see available quantisations with size, parameter count, and context length. "Recommended" badges highlight the best quality/size tradeoff. Hardware fit indicators show whether each model fits in your GPU's VRAM. Download, delete, or activate models with one click.

![LLM models](screenshots/gifs/settings-llm-models-light.gif)

</details>

<details>
<summary>Advanced Inference Settings</summary>

Expand the "Inference Settings" section to reveal: GPU layer offload presets (CPU-only, 8, 16, 32, All), context size picker (auto, 2K–128K tokens), verbose logging toggle, API key field, and multimodal projection (mmproj) toggles for vision models.

![LLM advanced](screenshots/gifs/settings-llm-advanced-light.gif)

</details>

### Tools (Function Calling)

<details>
<summary>Tool Toggles</summary>

Master enable switch plus per-tool toggles for: Date, Location, Web Search, Web Fetch, Bash (with security warning), Read File, Write File (warning), Edit File (warning), and Skill API. Each tool shows a description and hint text explaining what it allows the LLM to do.

![Tools toggles](screenshots/gifs/settings-tools-toggles-light.gif)

</details>

<details>
<summary>Web Search Provider</summary>

When Web Search is enabled, choose the search backend: DuckDuckGo (default, no key needed), Brave Search (requires free API key), or SearXNG (self-hosted, requires instance URL). Provider-specific configuration fields appear based on your selection.

![Tools web search](screenshots/gifs/settings-tools-web-search-light.gif)

</details>

<details>
<summary>Execution & Compression</summary>

Configure tool execution mode (sequential or parallel), maximum tool-call rounds per conversation turn, maximum calls per round, and context compression settings (off/normal/aggressive with max search results and max characters per result). These controls balance thoroughness against speed and context-window usage.

![Tools execution](screenshots/gifs/settings-tools-execution-light.gif)

</details>

<details>
<summary>Skills</summary>

View installed skill plugins with enable/disable toggles, see the skills license, trigger manual sync, and configure auto-refresh intervals. Skills extend the LLM's capabilities with domain-specific tools beyond the built-in set.

![Tools skills](screenshots/gifs/settings-tools-skills-light.gif)

</details>

### EEG Model

<details>
<summary>EEG Embedding Model</summary>

View the ZUNA EEG embedding model's download and encoder status, weights file path, and configure HNSW index parameters (M and ef_construction) that control the speed/accuracy tradeoff for similarity search.

![EEG model](screenshots/gifs/settings-eeg-model-light.gif)

</details>

### Embeddings

<details>
<summary>Embedding Model Selection</summary>

Choose which embedding model to use from a family-grouped dropdown, see the currently active model, check for stale labels that need re-embedding, and trigger a bulk re-embed operation with progress tracking.

![Embeddings](screenshots/gifs/settings-embeddings-light.gif)

</details>

### Screenshots

<details>
<summary>Capture Configuration</summary>

Toggle screenshot capture on/off, restrict to recording sessions only, and fine-tune: capture interval (1–30 seconds), image size (224–1536px), JPEG quality (10–100%), embedding backend (ONNX or FastEmbed), and OCR toggle. These settings control how the app builds its visual activity timeline.

![Screenshots config](screenshots/gifs/settings-screenshots-config-light.gif)

</details>

<details>
<summary>OCR & Pipeline Metrics</summary>

Configure the OCR engine (ocrs or Apple Vision on macOS), trigger model downloads, manage re-embedding when you change the vision model, and monitor the live pipeline metrics dashboard: captures per minute, OCR processing rate, timing breakdown (capture/OCR/resize/save), and error counts.

![Screenshots OCR and metrics](screenshots/gifs/settings-screenshots-ocr-and-metrics-light.gif)

</details>

### Proactive Hooks

<details>
<summary>Hook Configuration</summary>

Define proactive hooks that trigger actions (TTS announcements, notifications) based on EEG patterns. Each hook has: name, enable toggle, trigger keywords with auto-suggestions, EEG distance thresholds with suggested ranges, cooldown timers, and scenario examples (focus drop alert, break reminder) to get started quickly.

![Hooks](screenshots/gifs/settings-hooks-light.gif)

</details>

### Appearance

<details>
<summary>Theme & Colors</summary>

Customise the visual style: font size presets (compact to large), theme picker (light, dark, system-follow), high contrast toggle for accessibility, accent color palette (violet, blue, indigo, sky, and more), and chart color scheme picker that lets you choose how EEG bands and metrics are colored in all charts.

![Appearance](screenshots/gifs/settings-appearance-light.gif)

</details>

### General Settings

<details>
<summary>Data & Device Management</summary>

Set the data storage directory, manage paired devices with pair/forget/prefer controls, reveal/hide serial numbers and MAC addresses, and configure the signal processing filter pipeline (sample rate, low/high pass, notch filter).

![General devices](screenshots/gifs/settings-general-devices-light.gif)

</details>

<details>
<summary>OpenBCI Board Setup</summary>

The expandable OpenBCI section in general settings mirrors the Devices tab setup: board type selection, serial port picker with refresh, WiFi shield IP, Galea IP, and BLE scan timeout configuration.

![General OpenBCI](screenshots/gifs/settings-general-openbci-light.gif)

</details>

### Shortcuts

<details>
<summary>Global Keyboard Shortcuts</summary>

Record custom global keyboard shortcuts for each window: Settings, Help, History, Label, Search, Calibration, Focus Timer, and API. Click "Record" to capture any key combination, or clear to remove. These shortcuts work system-wide even when the app is in the background.

![Shortcuts](screenshots/gifs/settings-shortcuts-light.gif)

</details>

### UMAP

<details>
<summary>UMAP Projection Parameters</summary>

Fine-tune the UMAP dimensionality reduction used for the 3D EEG embedding visualisation: repulsion strength slider with presets, negative sample rate, neighbor count, epoch count, computation timeout, and cooldown between recalculations. A "Reset Defaults" button restores the recommended values.

![UMAP](screenshots/gifs/settings-umap-light.gif)

</details>

### Updates

<details>
<summary>Auto-Update Configuration</summary>

Check for updates manually, see available update details with release notes, configure the automatic check interval (hourly to weekly, or disabled), and toggle launch-at-login. When an update is ready, a countdown timer auto-installs it unless you dismiss.

![Updates](screenshots/gifs/settings-updates-light.gif)

</details>

### Permissions

<details>
<summary>System Permissions</summary>

View and manage OS-level permissions required by the app: Accessibility (for global shortcuts and input monitoring), Screen Recording (for screenshot capture), Bluetooth (for EEG headset connection), and Notifications (for goal alerts and hook triggers). Each permission shows its current status with a button to open the relevant system settings panel.

![Permissions](screenshots/gifs/settings-permissions-light.gif)

</details>

---

## Chat

The built-in chat interface connects to the local LLM server for private, on-device conversations about your EEG data. No data leaves your machine.

<details>
<summary>Conversation View</summary>

The chat window has a session sidebar on the left (create, rename, delete, archive conversations) and the message thread on the right with full Markdown rendering, code syntax highlighting, and inline tool-call result cards. The input bar at the bottom supports multi-line input.

![Chat conversation](screenshots/gifs/chat-conversation-light.gif)

</details>

<details>
<summary>Settings & Tools Panels</summary>

The chat header provides toggle buttons to open overlay panels: the Settings panel (model selection, system prompt, temperature, top-p) and the Tools panel (see which function-calling tools are active, enable/disable individual tools for the current conversation).

![Chat settings panel](screenshots/gifs/chat-settings-panel-light.gif)

![Chat tools panel](screenshots/gifs/chat-tools-panel-light.gif)

</details>

<details>
<summary>Chat — dark mode</summary>

![Chat dark](screenshots/chat-dark.png)

</details>

---

## Search

Search lets you query your entire EEG history using three complementary modes: neural EEG similarity, semantic text labels, and visual screenshot search.

<details>
<summary>Mode Switching</summary>

Toggle between EEG, Text, and Images modes. Each mode has its own query interface and result format, but they all search across the same unified timeline so you can cross-reference findings.

![Search mode switching](screenshots/gifs/search-mode-switching-light.gif)

</details>

<details>
<summary>EEG Similarity Search</summary>

Select a time range (with quick presets: last hour, today, this week), set the number of neighbors (K) and search accuracy (ef), then hit Search. Results stream in as the HNSW index is scanned across daily indices. Each result card shows the query epoch, its nearest neighbors with cosine distance scores, matched labels, and per-epoch brain-state metrics.

![Search EEG](screenshots/gifs/search-eeg-mode-light.gif)

</details>

<details>
<summary>Text Label Search</summary>

Type a natural-language query and press Ctrl+Enter to search across all your label embeddings. Results are ranked by semantic similarity — so searching "deep focus coding" finds labels like "Programming flow state" or "Concentrated debugging session" even without exact word matches.

![Search text](screenshots/gifs/search-text-mode-light.gif)

</details>

<details>
<summary>Screenshot / Image Search</summary>

Search your screenshot history by describing what was on screen. The CLIP vision model matches your text query against embedded screenshots, and OCR text is used as a secondary signal. Results show thumbnail previews, app names, window titles, and OCR excerpts.

![Search images](screenshots/gifs/search-images-mode-light.gif)

</details>

<details>
<summary>Search — dark mode</summary>

| EEG | Text | Images |
|:---:|:----:|:------:|
| ![](screenshots/search-eeg-dark.png) | ![](screenshots/search-text-dark.png) | ![](screenshots/search-images-dark.png) |

</details>

---

## History

Browse your complete recording history organised by day, with session cards showing duration, device, labels, and one-click expansion into full session analytics.

<details>
<summary>Day View & Navigation</summary>

The history page shows sessions grouped by day with prev/next navigation. A streak counter and aggregate stats (total sessions, total hours, this-week vs last-week comparison) are displayed at the top. Each session card shows start/end times, device name, battery level, sample count, file size, and colour-coded label chips.

![History overview](screenshots/gifs/history-overview-light.gif)

</details>

<details>
<summary>Session Expansion</summary>

Click any session card to expand it in-place, revealing the full SessionDetail view: band-power summary table, time-series charts for all frequency bands and composite scores (focus, relaxation, meditation, cognitive load, drowsiness), label annotation timeline, and sleep staging analysis if the session spans overnight hours.

![History session expand](screenshots/gifs/history-session-expand-light.gif)

</details>

<details>
<summary>History — dark mode</summary>

![History dark](screenshots/history-dark.png)

</details>

---

## Session Detail

A dedicated full-page view for deep-diving into a single recording session.

<details>
<summary>Full Session Analysis</summary>

The session detail page opens with a metadata header (device, start time, duration, firmware, total samples), followed by the band-power summary metrics, multi-panel time-series charts with zoom and pan, sleep-stage hypnogram (for overnight sessions), and an interactive label annotation timeline. This is where you go to understand what happened during a specific recording.

![Session detail](screenshots/gifs/session-detail-full-light.gif)

</details>

<details>
<summary>Session — dark mode</summary>

![Session dark](screenshots/session-dark.png)

</details>

---

## Compare

Side-by-side session comparison for tracking changes over time.

<details>
<summary>Dual Timeline View</summary>

Pick two days using the dual calendar pickers, then select sessions from each day's timeline bar. The overlay charts align both sessions' metrics for direct visual comparison — useful for tracking meditation progress, comparing focus sessions across weeks, or evaluating the effect of different environments on your brain state.

![Compare overview](screenshots/gifs/compare-overview-light.gif)

</details>

<details>
<summary>Compare — dark mode</summary>

![Compare dark](screenshots/compare-dark.png)

</details>

---

## Help

Built-in documentation organised into 11 topic tabs, so you never need to leave the app to understand a feature.

<details>
<summary>All Help Tabs</summary>

Cycle through every help section: Dashboard (metrics explained), Electrodes (placement and quality), Settings (every config option), Windows (window management), API (WebSocket and CLI), TTS (voice synthesis), LLM (local inference), Hooks (proactive automation), Privacy (data handling), References (academic papers), FAQ (common questions).

![Help tabs cycle](screenshots/gifs/help-all-tabs-cycle-light.gif)

</details>

<details>
<summary>Dashboard Help</summary>

Detailed explanations of every dashboard element: status hero, battery indicator, signal quality badges, EEG channel grid, band power bars, Frontal Alpha Asymmetry, waveform chart, GPU utilisation, tray icon states (grey/amber/green/red), and what each metric means for your cognitive state.

![Help dashboard](screenshots/gifs/help-dashboard-scroll-light.gif)

</details>

<details>
<summary>Settings Help</summary>

Covers all settings topics: paired devices, signal processing pipeline, EEG embedding model, calibration procedures, TTS calibration, global shortcuts, debug logging, updates, appearance options, goals, embeddings, UMAP, encoder status, HNSW parameters, data normalisation, and OpenBCI board configuration.

![Help settings](screenshots/gifs/help-settings-scroll-light.gif)

</details>

<details>
<summary>Privacy & Data Handling</summary>

Explains exactly what data is stored, where it lives, what network access the app uses (and doesn't use), third-party dependencies, telemetry policy (there is none), and the permissions the app requests and why.

![Help privacy](screenshots/gifs/help-privacy-scroll-light.gif)

</details>

<details>
<summary>Academic References</summary>

A curated list of academic papers and citations that underpin the signal processing, neurofeedback metrics, and machine learning models used in the app. Useful for researchers who want to understand the scientific basis of each metric.

![Help references](screenshots/gifs/help-references-scroll-light.gif)

</details>

<details>
<summary>FAQ</summary>

Answers to the most common questions: device compatibility, data export, session recording, metric interpretation, troubleshooting connectivity, and more.

![Help FAQ](screenshots/gifs/help-faq-scroll-light.gif)

</details>

---

## Calibration

Guided calibration sessions that establish your personal EEG baselines.

<details>
<summary>Electrode Quality & Calibration Run</summary>

The calibration page shows live per-electrode quality indicators (colour-coded: green/amber/red), lets you switch between Muse and standard 10-20 electrode layouts, select a calibration profile, and view the action timeline. During a run, each action step is timed with TTS voice prompts, and progress indicators show the current loop and action.

![Calibration](screenshots/gifs/calibration-electrode-tabs-light.gif)

</details>

<details>
<summary>Calibration — dark mode</summary>

![Calibration dark](screenshots/calibration-dark.png)

</details>

---

## Onboarding

A guided wizard that walks new users through the initial setup.

<details>
<summary>Setup Wizard Steps</summary>

The onboarding flow progresses through: Welcome (feature overview), Bluetooth (device pairing with live scanning), Fit (electrode placement quality check), Calibration (run your first baseline), and Models (download EEG embedding, LLM, and OCR models). Each step includes contextual guidance and can be revisited via the step indicators at the top.

![Onboarding wizard](screenshots/gifs/onboarding-wizard-light.gif)

</details>

<details>
<summary>Onboarding — dark mode</summary>

![Onboarding dark](screenshots/onboarding-dark.png)

</details>

---

## Labels

Annotate your EEG timeline with text labels that become searchable semantic embeddings.

<details>
<summary>Label Browser</summary>

Browse all your labels with search (exact text match or semantic similarity toggle), see label text, context, timestamps, and pagination. Semantic search lets you find related labels even when the wording differs — searching "concentration" will find labels tagged "deep focus" or "intense coding".

![Labels search modes](screenshots/gifs/labels-search-modes-light.gif)

</details>

<details>
<summary>Quick Label Dialog</summary>

The quick-label popup (triggered by global shortcut) lets you type a label without leaving your current workflow. Recent labels appear as chips for one-click reuse, and the textarea shows a live character count. Labels are instantly embedded and linked to the current EEG timestamp range.

![Label quick entry](screenshots/gifs/label-quick-entry-light.gif)

</details>

<details>
<summary>Labels — dark mode</summary>

| Browser | Quick entry |
|:-------:|:----------:|
| ![](screenshots/labels-dark.png) | ![](screenshots/label-dark.png) |

</details>

---

## Focus Timer

A built-in Pomodoro-style timer integrated with EEG recording and auto-labeling.

<details>
<summary>Timer & Configuration</summary>

The focus timer shows a large countdown display with start/pause/skip controls. Below it: work/break/long-break duration inputs, preset picker (Pomodoro 25/5, Deep Work 50/10, Short Focus 15/5), auto-label toggle (automatically creates labels for each work session), TTS toggle (announces work/break transitions aloud), session counter, and an expandable session log.

![Focus timer](screenshots/gifs/focus-timer-config-light.gif)

</details>

<details>
<summary>Focus Timer — dark mode</summary>

![Focus timer dark](screenshots/focus-timer-dark.png)

</details>

---

## Downloads

Centralised download manager for all model files.

<details>
<summary>Download Manager</summary>

View all model downloads (LLM, EEG, OCR, TTS) in one place with: filename, size, progress bars for active downloads, and pause/resume/cancel controls. Completed downloads show the local file path; failed downloads show error messages with retry options.

![Downloads](screenshots/gifs/downloads-manager-light.gif)

</details>

<details>
<summary>Downloads — dark mode</summary>

![Downloads dark](screenshots/downloads-dark.png)

</details>

---

## API

Real-time status and code examples for the WebSocket and HTTP API.

<details>
<summary>Status & Connected Clients</summary>

Shows the WebSocket URL, mDNS discovery command for LAN auto-discovery, a live table of connected clients with peer addresses and connection timestamps, and a request log with command names, timestamps, and success/failure indicators.

![API status](screenshots/gifs/api-status-and-clients-light.gif)

</details>

<details>
<summary>Code Examples</summary>

Tabbed code examples with copy-to-clipboard: Overview (capabilities summary), neuroskill CLI tool usage, raw WebSocket protocol, Python client, and Node.js client. Each example shows how to connect, send commands, and handle streaming responses.

![API code examples](screenshots/gifs/api-code-examples-light.gif)

</details>

<details>
<summary>API — dark mode</summary>

![API dark](screenshots/api-dark.png)

</details>

---

## About

<details>
<summary>App Information</summary>

The About page shows the app icon, version number, tagline, links to the website, repository, and Discord, the authors list with roles, license information (GPL-3.0), and acknowledgements. Useful for checking your installed version and finding community resources.

![About](screenshots/gifs/about-scroll-light.gif)

</details>

---

## What's New

<details>
<summary>Changelog Viewer</summary>

The What's New page shows the changelog for the current version with a version selector dropdown to browse past releases. Release notes are rendered as Markdown with categorised sections (Features, Bugfixes, Performance, etc.). A dismiss button marks the current version as seen so the badge in the dashboard clears.

![What's New](screenshots/gifs/whats-new-light.gif)

</details>
