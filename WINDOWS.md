# Windows Build Instructions

> ⚠️ **Work in progress — not ready for production.**
> Windows support is experimental. Builds may be unstable, features may be
> missing or broken, and no Windows releases are published yet.

## Prerequisites

### 1. Rust

Install from https://rustup.rs. Accept the default toolchain (stable,
`x86_64-pc-windows-msvc`).

### 2. LLVM

`llama-cpp-sys` uses `bindgen` to generate Rust↔C bindings at build time.
`bindgen` requires `libclang.dll`, which ships with LLVM.

```powershell
winget install LLVM.LLVM
```

Then tell bindgen where to find it (adjust the path if LLVM installed
elsewhere, e.g. to a non-default drive):

```powershell
[System.Environment]::SetEnvironmentVariable(
    "LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
```

Restart your terminal after setting the variable.

Without this step `cargo build` will fail with:

```
Unable to find libclang: couldn't find any valid shared libraries matching:
['clang.dll', 'libclang.dll']
```

### 3. Node.js

Install from https://nodejs.org (LTS recommended).

### 4. CMake

Required by llama.cpp's build system:

```powershell
winget install Kitware.CMake
```

Make sure `cmake` is on your `PATH` (the installer offers this as an option).

## Building

```powershell
# Install JS dependencies
npm install

# Build (frontend + Rust, targeting the host triple x86_64-pc-windows-msvc)
npm run tauri:build
```

The build script (`scripts/tauri-build.js`) detects Windows automatically and
does **not** pass `--target aarch64-apple-darwin` — it builds for the host
triple instead.

## Known limitations

- espeak-ng (`kittentts` / `neutts`) has no Windows build path yet.
  `build.rs` will attempt to run `build-espeak-static.sh`, which requires
  a Bash environment (WSL or Git Bash). TTS features may not compile.
- No Windows CI pipeline exists yet; breakage may go undetected between
  commits.
- The app has only been tested on macOS. UI, tray behaviour, and BLE
  discovery are untested on Windows.
