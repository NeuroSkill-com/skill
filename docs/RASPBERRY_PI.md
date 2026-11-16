# Raspberry Pi

> ⚠️ **Experimental**, like the rest of Linux support (see [`LINUX.md`](LINUX.md)).
> aarch64 packages come from the `Trial — Linux arm64 packages` workflow, not
> from tagged releases yet.

NeuroSkill runs on a Raspberry Pi as an ordinary aarch64 Linux build: the same
app, daemon and packages as x86_64 Linux, built for `aarch64-unknown-linux-gnu`.

## Requirements

| | Supported | Notes |
|---|---|---|
| Board | Raspberry Pi 5, Pi 4, Compute Module 4/5 | Pi 5 recommended. Pi 3 and Zero 2 W lack the RAM |
| RAM | 8 GB | the daemon peaked at ~4.5 GB on a Pi 5 with chat and the default voice loaded; 4 GB boards are untested and likely too small |
| OS | **64-bit** Raspberry Pi OS Bookworm (Debian 12) or Trixie (Debian 13) | install tested on both; Ubuntu 22.04+ for Pi should work (glibc 2.35) but is untested. 32-bit (armhf) is not supported |
| Storage | SSD or a fast A2 microSD, 16 GB+ free | model downloads are 0.5–2 GB each |

The packages are built inside a `debian:bookworm` container
(`scripts/build-linux-container.sh`), so they need glibc 2.35 at most and
their dependencies name versions Bookworm has. Built on the Ubuntu 24.04
runner instead, they would need glibc 2.39 and refuse to start on Bookworm.

The 16 KB-page kernel that Pi 5 boots by default works; nothing in the app
assumes 4 KB pages.

## Install

Download the `linux-arm64-trial-packages` artifact from a run of
`Trial — Linux arm64 packages` (Actions tab, scope `app`), then:

```bash
sudo apt install ./neuroskill_*_arm64.deb
```

Bluetooth headsets go through BlueZ, so make sure it is running and your user
may use it:

```bash
sudo apt install -y bluez
sudo systemctl enable --now bluetooth
```

## What runs where

The Pi's VideoCore GPU is **not** used for inference. It shows up as a Vulkan
(wgpu) adapter, but it is slower than the Pi's CPU for these models, too small
for model weights, and has given wrong output. On a Pi the daemon's `auto`
device therefore means **CPU** for every model (LLM, speech, TTS, embeddings,
EEG). Detection reads `/proc/device-tree/model`.

- To try the GPU anyway, pick `wgpu` explicitly in the **Inference Device**
  setting. If it fails to load, the model falls back to CPU as usual.
- `SKILL_ACCEL_RASPBERRY_PI=0` gives a Pi the normal desktop GPU chain;
  `SKILL_ACCEL_RASPBERRY_PI=1` forces the Pi policy on any machine.

## What to expect

Measured on a Raspberry Pi 5 (8 GB, Raspberry Pi OS Trixie, 16 KB-page
kernel) through the installed daemon, with ~22-token prompts. A Pi 4 will be
slower.

| Feature | Model | Result |
|---|---|---|
| Chat (LLM) | Qwen3.5-0.8B Q4_K_M | loads in ~5–6 s; ~3–6 s to process a prompt, then ~1.6 tokens/s |
| Daemon, all models it loads at start | | ~4.5 GB peak |

Two things set the prompt time, and both are fixed in rlx-models (skill picks
them up with its next rlx-models update):

- **Prefill width.** CPU prefill costs what the compiled graph width costs, not
  what the prompt costs. The minimum width was 256 tokens, so a 22-token
  prompt took ~10 s even with every weight cached; at the new CPU minimum of
  32 it takes ~1.6 s, with identical output.
- **Weight cache.** The LLM keeps float copies of its weights in a cache
  capped at a quarter of RAM (2 GB on an 8 GB Pi), less than the 0.8B model
  needs, so later prompts re-dequantise weights: ~45 s per prompt at width
  256, ~5.6 s at width 32. Through the daemon, prompts went from 12 / 45 / 50 s
  to 3.1 / 5.6 / 5.6 s.

Measured in Docker (CPU-only arm64 on an Apple M4 Pro), not yet on a Pi:
speech recognition (whisper-tiny) runs ~1.3–1.5× faster than real time, and
StyleTTS2 + WaveRNN text to speech ~25× **slower** than real time.

**Cooling.** Sustained inference took the Pi 5 to 81 °C and into thermal
throttling (`vcgencmd get_throttled`). Use the active cooler or a heatsink case.

EEG streaming, band powers and the dashboard are light by comparison and run
fine.

**Memory.** On the CPU the LLM looks token embeddings up outside its compiled
graphs (`RLX_QWEN35_HOST_EMBED=1`, set automatically). Otherwise each graph
holds its own 1 GB float copy of the embedding table, and the 0.8B model
alone peaks at ~6.6 GB, which an 8 GB Pi 5 running the command-line runner
killed for running out of memory. The output is byte-identical either way.

### SIMD

Every 64-bit ARM chip has NEON, and the CPU backend (rlx) uses it throughout:
float matrix multiply, quantised (Q4_K, Q2_0) matrix-vector products for the
LLM, and vector math (exp, log, sin/cos, tanh). The packages are built for
the ARMv8.0 baseline, so one binary runs on both boards:

- **Pi 5** (Cortex-A76) also has the `dotprod` instruction, used by the int8
  Q4_K kernel after a run-time CPU check.
- **Pi 4** (Cortex-A72) has no `dotprod`, and takes plain NEON instead.

The build checks this: rlx's ISA portability scan flags any above-ARMv8.0
instruction outside a run-time-checked function, and the daemon is started
under an emulated Cortex-A72 and A76.

The int8 kernel (`RLX_Q4K_FUSED_MIN_N=1`) is not forced on for a Pi. With a
Pi's weight-cache budget, rlx already sends decode through it whenever the
cache is too small, and forcing it everywhere was no faster (measured within
noise) while agreeing with the default on 97.9% of next tokens rather than
100%.

## Build on the Pi

Follow [`LINUX.md`](LINUX.md) sections 1–5 for the system packages, then:

```bash
npm install
npm run tauri:build:linux:arm64                 # AppImage
npm run package:linux:system:arm64:native -- --skip-build   # .deb + .rpm
```

The workspace is large. On an 8 GB Pi 5, add swap and limit parallel jobs or
the final link runs out of memory:

```bash
sudo fallocate -l 8G /swapfile && sudo chmod 600 /swapfile
sudo mkswap /swapfile && sudo swapon /swapfile
export CARGO_BUILD_JOBS=2
```

`sudo apt install mold clang` makes the link much faster;
`scripts/tauri-build.js` uses them when they are installed.

A native build on a Pi takes hours. The practical route is the same
container build CI uses, on any faster arm64 machine with Docker (an Apple
Silicon Mac works; ~20 min there):

```bash
CARGO_BUILD_JOBS=8 bash scripts/build-linux-container.sh --target aarch64-unknown-linux-gnu
# → dist/linux-container/aarch64-unknown-linux-gnu/*.deb, *.rpm
```

## Troubleshooting

**Blank or white window.** The app already sets
`WEBKIT_DISABLE_DMABUF_RENDERER=1`. If the window is still blank (seen with some
Pi display stacks), also try:

```bash
WEBKIT_DISABLE_COMPOSITING_MODE=1 neuroskill
```

**The LLM is slow or the Pi swaps.** Stay with the default 0.8B model. Larger
models do not fit in a Pi's memory bandwidth or RAM. Check that the inference
device is `auto` or `cpu`.
