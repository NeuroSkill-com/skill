# NeuroSkill™ — State of Mind Brain-Computer Interface system

[www.neuroskill.com](https://neuroskill.com)

[![Release](https://img.shields.io/github/v/release/NeuroSkill-com/skill?style=for-the-badge&logo=github&logoColor=white&label=Latest)](https://github.com/NeuroSkill-com/skill/releases/latest)
[![License](https://img.shields.io/badge/License-GPL--3.0-blue?style=for-the-badge)](https://github.com/NeuroSkill-com/skill/blob/main/LICENSE)
[![Stars](https://img.shields.io/github/stars/NeuroSkill-com/skill?style=for-the-badge&logo=github&logoColor=white)](https://github.com/NeuroSkill-com/skill/stargazers)
[![Discord](https://img.shields.io/badge/Discord-Join%20Community-5865F2?style=for-the-badge&logo=discord&logoColor=white)](https://discord.gg/nA6Xk5MV)
[![Homebrew](https://img.shields.io/badge/Homebrew-Install%20via%20Cask-FBB040?style=for-the-badge&logo=homebrew&logoColor=white)](https://github.com/NeuroSkill-com/skill#install-homebrew-macos-apple-silicon)

[![macOS](https://img.shields.io/badge/Download-macOS%20(Apple%20Silicon)-000000?style=for-the-badge&logo=apple&logoColor=white)](https://github.com/NeuroSkill-com/skill/releases/latest/download/NeuroSkill.dmg)
[![Windows](https://img.shields.io/badge/Download-Windows%20(x86__64)-0078D4?style=for-the-badge&logo=windows11&logoColor=white)](https://github.com/NeuroSkill-com/skill/releases/latest/download/NeuroSkill.exe)
[![Linux](https://img.shields.io/badge/Download-Linux%20(x86__64)-FCC624?style=for-the-badge&logo=linux&logoColor=black)](https://github.com/NeuroSkill-com/skill/releases/latest/download/NeuroSkill.AppImage)

> **⚠️ Research Use Only — Not a Medical Device**
>
> NeuroSkill™ is an open-source research tool for exploratory EXG analysis. It is **NOT** a medical device
> and has **NOT** been cleared or approved by the FDA, CE, or any regulatory body. It must not be
> used for clinical diagnosis, treatment decisions, or any medical purpose. All metrics are
> experimental research outputs — not validated clinical measurements. Do not rely on any output of
> this software for health-related decisions. Consult a qualified healthcare professional for any
> medical concerns.
>
> **This software is provided for non-commercial research and educational use only.**

**NeuroSkill™** is a desktop neurofeedback and brain-computer interface application for BCI devices. It streams, analyses, embeds, and visualises EXG data in real time — all processing runs locally on-device.

Built with **Tauri v2** (Rust backend) + **SvelteKit** (TypeScript/Svelte 5 frontend). Runs on **macOS** (Apple Silicon), **Windows** (x86-64 MSVC), and **Linux** (x86-64, experimental).

---

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [Project layout](#project-layout)
  - [Workspace crates](#workspace-crates)
  - [Vendored crates](#vendored-crates)
  - [Frontend](#frontend)
- [Documentation](#documentation)
- [Data storage](#data-storage)
- [WebSocket & REST API](#websocket--rest-api)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Development](#development)
  - [Prerequisites](#prerequisites)
  - [Setup](#setup)
  - [Install (Homebrew)](#install-homebrew-macos-apple-silicon)
  - [Build](#build)
  - [Build cache](#build-cache-optional-recommended)
  - [Linux packaging](#linux-packaging-quickstart)
  - [Pre-commit checks](#pre-commit-checks)
- [Versioning](#versioning)
- [Release](#release)
- [License](#license)

---

## Features

| Feature | Description |
|---------|-------------|
| **Live EXG Waveforms** | Multi-channel real-time scrolling waveform with glow effect, gradient fill, live-edge pulse dot, configurable bandpass filter, and signal-quality indicators |
| **GPU Band-Power Analysis** | Hann-windowed 512-sample FFT via `gpu_fft` — all 4 channels in a single GPU dispatch at ~4 Hz. Six clinical EXG bands (0.5–100 Hz) |
| **ZUNA Neural Embeddings** | GPU-accelerated transformer encoder (ZUNA) converts 5-second EXG epochs into 32-dimensional embedding vectors for similarity search |
| **Session Compare** | Side-by-side comparison of any two recording sessions: band powers, derived scores, FAA, sleep staging, and 3D UMAP embedding projection |
| **3D UMAP Viewer** | Interactive Three.js scatter plot of session embeddings projected to 3D. Auto-orbit, hover tooltips, click-to-connect labelled points |
| **Sleep Staging** | Automatic Wake/N1/N2/N3/REM classification from band-power ratios with hypnogram visualisation |
| **Label System** | Attach user-defined tags to moments during recording. Full CRUD, stored alongside embeddings and visualised in UMAP |
| **Focus Timer** | Pomodoro-style work/break timer with optional auto-label EXG at each phase transition |
| **Similarity Search** | Approximate nearest-neighbour search across daily HNSW indices with streaming results |
| **Screenshot Capture** | Periodic screenshots with CLIP vision embedding, OCR, and HNSW-based visual search |
| **Local LLM** | On-device chat with function calling via llama.cpp — Metal, CUDA, and Vulkan GPU backends |
| **Text-to-Speech** | KittenTTS and NeuTTS backends for voice feedback during sessions |
| **Proactive Hooks** | Background monitoring that triggers actions when brain-state matches configured labels |
| **DND Focus Mode** | Automatic Do Not Disturb toggling driven by real-time focus scores |
| **WebSocket API** | JSON-based LAN API with mDNS discovery (`_skill._tcp`) |
| **Keyboard Shortcuts** | Fully configurable global and in-app shortcuts. Press `?` for cheat sheet |
| **i18n** | English, German, French, Hebrew, Ukrainian |

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   SvelteKit Frontend                │
│  Svelte 5 · Tailwind · Three.js · shadcn-svelte     │
├─────────────────────────────────────────────────────┤
│                  Tauri v2 Bridge                    │
│         IPC commands · Event emitters               │
├─────────────────────────────────────────────────────┤
│                   Rust Backend                      │
│  CoreBluetooth/BlueZ · gpu_fft · ZUNA (wgpu)        │
│  rusqlite · fast_hnsw · umap_rs · job_queue         │
└─────────────────────────────────────────────────────┘
```

### Data flow

1. **BLE** → Raw EXG samples at 256 Hz (4 channels × 12 samples/packet)
2. **Signal Filter** → Bandpass + notch filter for display
3. **Band Analyzer** → GPU FFT every 64 samples (~4 Hz) → `BandSnapshot`
4. **ZUNA Encoder** → Every 5 s epoch → 32-D embedding vector (wgpu)
5. **Storage** → HNSW index + SQLite database per day in `~/.skill/YYYYMMDD/`

---

## Project layout

### Workspace crates

The Rust backend is split into focused, zero-Tauri-dependency crates under [`crates/`](crates/). Each has its own README with full API documentation.

| Crate | Description |
|---|---|
| [`skill-constants`](crates/skill-constants/) | Single source of truth for all constants (sample rates, bands, file names, HNSW params) |
| [`skill-eeg`](crates/skill-eeg/) | Real-time EEG signal processing — filter pipeline, band powers, quality monitor, artifact detection, head pose |
| [`skill-exg`](crates/skill-exg/) | EEG embedding helpers — cosine distance, fuzzy matching, HuggingFace weight management, GPU cache, epoch metrics |
| [`skill-devices`](crates/skill-devices/) | Device-session logic — composite EEG scores, battery EMA, DND focus-mode decision engine |
| [`skill-data`](crates/skill-data/) | Shared data layer — label store, activity store, hooks log, screenshot store, device types, DND, GPU stats |
| [`skill-commands`](crates/skill-commands/) | Embedding search engine — HNSW K-NN search, streaming results, graph generation (DOT/SVG), PCA projection |
| [`skill-label-index`](crates/skill-label-index/) | Cross-modal HNSW label indices (text, context, EEG) with rebuild/insert/search |
| [`skill-router`](crates/skill-router/) | UMAP projection (GPU), embedding loaders, cluster analysis, metric rounding, WS command registry |
| [`skill-jobs`](crates/skill-jobs/) | Sequential background job queue for expensive compute (UMAP, model downloads) |
| [`skill-settings`](crates/skill-settings/) | Persistent configuration types and JSON I/O — all user settings, calibration, hooks |
| [`skill-screenshots`](crates/skill-screenshots/) | Screenshot capture, CLIP vision embedding (ONNX), OCR (ocrs / Apple Vision), HNSW search |
| [`skill-llm`](crates/skill-llm/) | Local LLM inference engine — model catalog, chat store, llama.cpp, streaming generation |
| [`skill-tools`](crates/skill-tools/) | LLM function-calling — tool definitions, argument validation, execution, safety checks |
| [`skill-tts`](crates/skill-tts/) | Text-to-speech — KittenTTS and NeuTTS backends, voice management, audio playback |
| [`skill-tray`](crates/skill-tray/) | System tray helpers — progress-ring overlay, shortcut formatting, dedup (pure `std`) |
| [`skill-autostart`](crates/skill-autostart/) | Platform-specific launch-at-login registration (macOS/Linux/Windows) |
| [`skill-vision`](crates/skill-vision/) | Apple Vision framework OCR via compiled Objective-C FFI (macOS only) |

### Vendored crates

| Crate | Description |
|---|---|
| [`fast-hnsw`](src-tauri/vendor/fast-hnsw/) | Pure-Rust HNSW approximate nearest-neighbour search |
| [`rdev`](src-tauri/vendor/rdev/) | Cross-platform keyboard and mouse event listener |

### Tauri backend

[`src-tauri/`](src-tauri/) — application entry point, BLE integration, Tauri command wrappers, WebSocket/HTTP server, and mDNS discovery. See its [README](src-tauri/README.md).

### Frontend

```
src/
├── routes/          # SvelteKit pages (dashboard, compare, settings, history, …)
└── lib/             # Shared components, i18n, utilities
    ├── i18n/        # en, de, fr, he, uk
    ├── format.ts    # Shared formatting helpers
    ├── types.ts     # Shared TypeScript interfaces
    └── …            # UI components (EXGChart, UmapViewer3D, HelpFaq, …)
```

---

## Documentation

In-depth guides live in [`docs/`](docs/):

| Document | Description |
|---|---|
| [`CHANGELOG.md`](CHANGELOG.md) | All notable changes — compiled from [`changes/`](changes/) fragments at release time |
| [`METRICS.md`](docs/METRICS.md) | Full metrics & indices reference — band powers, derived scores, PPG, composites, with formulas and citations |
| [`HOOKS.md`](docs/HOOKS.md) | Proactive Hooks architecture — background brain-state monitoring and automated actions |
| [`LLM.md`](docs/LLM.md) | LLM engine architecture — actor pattern, model lifecycle, chat, function calling |
| [`LINUX.md`](docs/LINUX.md) | Linux (Ubuntu) build prerequisites and packaging |
| [`WINDOWS.md`](docs/WINDOWS.md) | Windows build instructions (Visual Studio, LLVM, CMake) |

---

## Data storage

All data is stored locally in `~/.skill/` organised by UTC date:

```
~/.skill/
  settings.json
  labels.sqlite
  screenshots.sqlite
  20260224/
    EXG.sqlite              ← embeddings, metrics, per-epoch scores
    EXG_embeddings.hnsw     ← daily HNSW approximate-NN index
    session_*.csv           ← raw EXG samples
  20260225/
    …
```

For the full SQLite schema and per-column documentation (60+ columns covering band powers, derived scores, cross-band ratios, spectral shape, Hjorth parameters, complexity measures, PPG, and composites), see [`docs/METRICS.md`](docs/METRICS.md).

---

## WebSocket & REST API

NeuroSkill™ broadcasts EXG data and accepts commands over a local WebSocket server, advertised via mDNS as `_skill._tcp`.

### Discovery

```bash
# macOS
dns-sd -B _skill._tcp

# Linux
avahi-browse _skill._tcp
```

### Broadcast events (server → client)

| Event | Rate | Description |
|-------|------|-------------|
| `EXG-bands` | ~4 Hz | Derived scores, band powers, heart rate, head pose — all 60+ fields |
| `muse-status` | ~1 Hz | Device heartbeat: battery, sample counts, connection state |
| `label-created` | on-demand | Fired when any client creates a label |

### Commands (client → server)

| Command | Description |
|---------|-------------|
| `status` | Device state, scores, embeddings count, sleep summary |
| `label` | Attach a label to the current moment |
| `search` | K-nearest EXG embedding search over a date range |
| `sessions` | List all recording sessions |
| `compare` | Full A/B session comparison (metrics, sleep, UMAP ticket) |
| `sleep` | Sleep staging for a time range |
| `umap` / `umap_poll` | Enqueue and poll 3D UMAP projection |
| `llm_status` | LLM server state |
| `llm_start` / `llm_stop` | Load or unload the active model |
| `llm_catalog` | Model catalog with download states |
| `llm_download` / `llm_cancel_download` / `llm_delete` | Model lifecycle |
| `llm_chat` | Streaming chat completion |
| `llm_logs` | Last 500 LLM server log lines |

### REST shortcuts

Every command is also available as an HTTP endpoint at `http://localhost:<port>`. Common routes:

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/status` | Status snapshot |
| `GET` | `/sessions` | List sessions |
| `POST` | `/label` | Create label |
| `POST` | `/search` | EXG ANN search |
| `POST` | `/compare` | A/B session comparison |
| `POST` | `/sleep` | Sleep staging |
| `POST` | `/say` | Speak text via TTS |
| `POST` | `/llm/chat` | Non-streaming chat completion |
| `GET` | `/llm/status` | LLM server status |
| `POST` | `/llm/start` / `/llm/stop` | Start/stop inference server |
| `GET` | `/dnd` | DND config + live eligibility |

### Testing

```bash
node test.js           # auto-discover via mDNS
node test.js 62853     # explicit port
```

---

## Keyboard shortcuts

### Global (system-wide, work even when window is hidden)

| Default (macOS) | Default (Win/Linux) | Action |
|----------------|---------------------|--------|
| ⌘⇧O | Ctrl+Shift+O | Open NeuroSkill™ window |
| ⌘⇧L | Ctrl+Shift+L | Add EXG label |
| ⌘⇧F | Ctrl+Shift+F | Open similarity search |
| ⌘⇧, | Ctrl+Shift+, | Open Settings |
| ⌘⇧C | Ctrl+Shift+C | Open Calibration |
| ⌘⇧M | Ctrl+Shift+M | Open Session Compare |
| ⌘⇧P | Ctrl+Shift+P | Open Focus Timer |
| ⌘⇧H | Ctrl+Shift+H | Open History |
| ⌘⇧A | Ctrl+Shift+A | Open API Status |
| ⌘⇧T | Ctrl+Shift+T | Toggle Theme |

All global shortcuts are configurable in **Settings → Shortcuts**.

### In-app

| Shortcut | Action |
|----------|--------|
| `?` | Keyboard shortcut cheat sheet |
| ⌘K / Ctrl+K | Command Palette |
| `Esc` | Close overlay / dialog |
| ⌘↵ / Ctrl+↵ | Submit label |

---

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) ≥ 18
- [Tauri CLI v2](https://v2.tauri.app/start/prerequisites/)
- ZUNA weights from Hugging Face (see below)
- **macOS** — Xcode Command Line Tools (`xcode-select --install`)
- **Windows** — see [`docs/WINDOWS.md`](docs/WINDOWS.md)
- **Linux** — see [`docs/LINUX.md`](docs/LINUX.md)

### Setup

```bash
npm install

# Download ZUNA encoder weights
python3 -c "from huggingface_hub import snapshot_download; snapshot_download('mariozechner/zuna-EXG-v1')"

# Run in development mode
npm run tauri dev
```

### Install (Homebrew, macOS Apple Silicon)

```bash
brew tap NeuroSkill-com/skill
brew install --cask neuroskill
```

Upgrade:

```bash
brew upgrade --cask neuroskill
```

### Build

```bash
npm run tauri build
```

### Build cache (optional, recommended)

Install **sccache** and **mold** to speed up Rust/C++ builds by ~50 %. The build system auto-detects these tools — no config changes needed.

```bash
# Interactive setup
npm run setup:build-cache
```

| Tool | Platform | What it does | Speedup |
|---|---|---|---|
| sccache | macOS, Linux, Windows | Caches rustc + cc/c++ outputs | ~50 % faster clean rebuilds |
| mold | Linux only | Fast linker (replaces ld/lld) | Faster link step |

To disable temporarily: `SKILL_NO_SCCACHE=1` or `SKILL_NO_MOLD=1`.

### Linux packaging quickstart

```bash
npm run tauri:build:linux:x64:native
npm run package:linux:system:x64:native -- --skip-build
```

For full details, see [`docs/LINUX.md`](docs/LINUX.md).

### Pre-commit checks

A Git pre-commit hook runs two fast sanity checks:

| Check | Command |
|---|---|
| `cargo clippy` | `cd src-tauri && cargo clippy --all-targets --all-features -- -D warnings` |
| `svelte-check` | `npm run check` |

Both must pass. Bypass in an emergency with `git commit --no-verify`.

---

## Versioning

The `bump` script keeps `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` in sync, and compiles changelog fragments:

```bash
npm run bump          # auto-increment patch (0.0.3 → 0.0.4)
npm run bump 1.2.0    # set exact version
```

Bump automatically:
1. Runs preflight checks (clippy, svelte-check, i18n sync)
2. Updates version in all three files
3. Compiles `changes/unreleased/*.md` fragments into `CHANGELOG.md`
4. Archives fragments to `changes/releases/<version>/`

---

## Release

Generate signing keys:

```shell
npm run tauri signer generate -- -w ~/.tauri/skill.key
```

### Required GitHub secrets

| Secret | What it is |
|---|---|
| APPLE_CERTIFICATE | `base64 -i cert.p12` output |
| APPLE_CERTIFICATE_PASSWORD | P12 export password |
| APPLE_SIGNING_IDENTITY | `"Developer ID Application: Name (TEAMID)"` |
| APPLE_ID | Apple ID email |
| APPLE_PASSWORD | App-specific password |
| APPLE_TEAM_ID | 10-character Team ID |
| TAURI_SIGNING_PRIVATE_KEY | Output of signer generate |
| TAURI_SIGNING_PRIVATE_KEY_PASSWORD | Key password (empty if none) |

### Local testing

```bash
act push                         # all CI jobs via Docker
bash release.sh --dry-run        # dry-run release
SKIP_UPLOAD=1 bash release.sh    # full local release (no upload)
```

---

## License

This program is free software: you can redistribute it and/or modify it under
the terms of the **GNU General Public License version 3** as published by the
Free Software Foundation.

This program is distributed in the hope that it will be useful, but **without
any warranty**; without even the implied warranty of merchantability or fitness
for a particular purpose. See the GNU General Public License for more details.

You should have received a copy of the GNU General Public License along with
this program. If not, see <https://www.gnu.org/licenses/>.

SPDX-License-Identifier: `GPL-3.0-only`
