# Linux (Ubuntu) Build Prerequisites

> ⚠️ **Work in progress — not ready for production.**
> Linux support is experimental. Builds may be unstable, features may be
> missing or broken, and no Linux releases are published yet.

This guide lists what you should install **before** running a build on Ubuntu.

## Supported Ubuntu versions

- Ubuntu 24.04 LTS (noble)
- Ubuntu 22.04 LTS (jammy)
- Ubuntu 20.04 LTS (focal)

## 1) Base build toolchain

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  curl \
  wget \
  file \
  pkg-config \
  libssl-dev \
  clang \
  cmake \
  git \
  libtool
```

## 2) Node.js (LTS) + npm

Install from https://nodejs.org (LTS recommended), then verify:

```bash
node -v
npm -v
```

## 3) Rust toolchain

Install Rust with rustup:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
```

Verify:

```bash
rustc -V
cargo -V
```

## 4) Tauri/Linux GUI dependencies

Install the Linux system libraries used by Tauri/WebKit:

```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libjavascriptcoregtk-4.1-dev \
  libsoup-3.0-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  patchelf
```

If you are on an older Ubuntu image that does not provide `4.1`/`libsoup-3.0`,
use the legacy equivalents:

```bash
sudo apt install -y \
  libwebkit2gtk-4.0-dev \
  libjavascriptcoregtk-4.0-dev \
  libsoup2.4-dev
```

## 5) Bluetooth / device communication deps

NeuroSkill uses BLE on Linux (BlueZ stack):

```bash
sudo apt install -y \
  bluez \
  libbluetooth-dev \
  dbus
```

## 6) Vulkan prerequisites (LLM GPU backend)

The Linux build uses `llm-vulkan` by default via `scripts/tauri-build.js`.
Install Vulkan prerequisites with the repo script:

```bash
bash scripts/install-vulkan-sdk.sh
```

That script ensures Vulkan headers/loader + shader tooling are present
(`libvulkan-dev`, `glslang-tools`/`shaderc`, `spirv-tools`, etc.).

Optional verification:

```bash
ls /usr/include/vulkan/vulkan.h
command -v glslc || command -v glslangValidator
```

## 7) Optional local tooling

Useful for diagnostics in this repo:

```bash
sudo apt install -y sqlite3
```

(`verify_sqlite.sh` uses `sqlite3`.)

## 8) Build after prerequisites are installed

From repo root:

```bash
npm install
npm run tauri:build
```

The build wrapper automatically:

- checks/installs Vulkan prerequisites (`scripts/install-vulkan-sdk.sh`)
- builds static `espeak-ng` if missing (`scripts/build-espeak-static.sh`)
- injects `--features llm-vulkan` if no explicit `--features` was provided

## Runtime warning troubleshooting

### `pkg-config has not been configured to support cross-compilation`

If you run on an ARM Linux host (for example `aarch64`) and force:

```bash
npx tauri build --target x86_64-unknown-linux-gnu
```

Cargo switches to cross-compilation mode. GTK/WebKit sys crates (`glib-sys`,
`gobject-sys`) then require a full x86_64 sysroot + cross `pkg-config`
configuration (`PKG_CONFIG_SYSROOT_DIR`, `PKG_CONFIG_PATH`, etc.).

For normal local builds, use your native target instead:

```bash
npm run tauri build -- --target aarch64-unknown-linux-gnu
```

For x86_64 release artifacts, build on an x86_64 Linux runner (recommended),
or set up a complete x86_64 cross toolchain + sysroot before invoking Tauri.

### `libEGL warning: egl: failed to create dri2 screen`

These warnings usually come from Mesa/WebKit probing GPU backends. On some
Linux setups (remote sessions, mixed Wayland/X11 stacks, unsupported drivers)
the probe fails before falling back.

If the app otherwise works, this warning is typically non-fatal.

To force a software-rendering fallback in dev:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 LIBGL_ALWAYS_SOFTWARE=1 npm run tauri dev
```

### `[input-monitor] xprintidle not found`

Install `xprintidle` on X11 sessions:

```bash
sudo apt install xprintidle
```

On Wayland sessions, `xprintidle` does not work (X11-only), so idle
keyboard/mouse tracking is unavailable.

### `[updater] ... fallback platforms ... not found`

Linux release artifacts are currently not published in the updater feed, so
automatic update checks can be unavailable depending on architecture.
