# Windows Build Instructions

> ⚠️ **Work in progress — not ready for production.**
> Windows support is experimental. Builds may be unstable, features may be
> missing or broken, and no Windows releases are published yet.

## Prerequisites

### 1. Visual Studio Build Tools (MSVC)

Install from https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022
and select the **Desktop development with C++** workload.  This provides:

- The MSVC compiler (`cl.exe`) and linker (`link.exe`)
- The static-library archiver (`lib.exe`) — required to merge espeak-ng
  companion libraries into a single `espeak-ng.lib`
- Windows SDK headers required by espeak-ng's CMake build

The build must be invoked from a **Developer PowerShell for VS** (or a terminal
where `vcvarsall.bat` has been sourced) so that `lib.exe` and `cl.exe` are on
`PATH`.

### 2. Rust

Install from https://rustup.rs. Accept the default toolchain (stable,
`x86_64-pc-windows-msvc`).

### 3. LLVM

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

### 4. Node.js

Install from https://nodejs.org (LTS recommended).

### 5. CMake

Required by llama.cpp **and** espeak-ng's build systems:

```powershell
winget install Kitware.CMake
```

Make sure `cmake` is on your `PATH` (the installer offers this as an option).

### 6. Git

Required to clone the espeak-ng source when building the static library:

```powershell
winget install Git.Git
```

## Building

```powershell
# Install JS dependencies
npm install

# Build (frontend + Rust, targeting the host triple x86_64-pc-windows-msvc).
# This also builds espeak-ng.lib automatically on first run.
npm run tauri:build
```

The build script (`scripts/tauri-build.js`) detects Windows automatically,
runs `scripts\build-espeak-static.ps1` to produce
`src-tauri\espeak-static\lib\espeak-ng.lib` (a no-op on subsequent runs), then
invokes `npx tauri build` for the host triple.

### Building espeak-ng manually

If you need to rebuild espeak-ng from scratch (e.g. after deleting
`src-tauri\espeak-static\`), run from a **Developer PowerShell for VS**:

```powershell
.\scripts\build-espeak-static.ps1
```

The script clones espeak-ng 1.52.0, builds it with CMake + MSVC in Release
mode (`-DBUILD_SHARED_LIBS=OFF`), merges companion archives (`libucd.lib`,
etc.) into a single self-contained `espeak-ng.lib` using `lib.exe`, and copies
`espeak-ng-data\` to `src-tauri\espeak-static\share\`.

To use a different espeak-ng tag:

```powershell
$env:ESPEAK_TAG_OVERRIDE = "1.51.1"
.\scripts\build-espeak-static.ps1
```

## Cross-compilation via MinGW

You can produce a Windows binary (targeting `x86_64-pc-windows-gnu`) from a
Linux or macOS host using the MinGW-w64 cross-toolchain.  This is an
alternative to building natively on Windows with MSVC.

### Install the cross-toolchain

**Linux (Debian/Ubuntu):**
```bash
sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 cmake git
```

**macOS:**
```bash
brew install mingw-w64 cmake git
```

**Windows (MSYS2 MinGW shell):**
```bash
pacman -S mingw-w64-x86_64-gcc mingw-w64-x86_64-cmake git
```

### Add the Rust target

```bash
rustup target add x86_64-pc-windows-gnu
```

### Build

```bash
# Build espeak-ng for MinGW (one-time; no-op if already built)
bash scripts/build-espeak-static-mingw.sh

# Full Tauri build (also runs build-espeak-static-mingw.sh automatically)
npm run tauri:build -- --target x86_64-pc-windows-gnu
```

The MinGW espeak archive is stored in `src-tauri/espeak-static-mingw/` — a
separate directory from the native build — so the two never overwrite each
other.

### Differences from the MSVC build

| | MSVC (`x86_64-pc-windows-msvc`) | MinGW (`x86_64-pc-windows-gnu`) |
|---|---|---|
| Static lib name | `espeak-ng.lib` | `libespeak-ng.a` |
| C++ runtime | MSVC CRT (auto-linked) | libstdc++ |
| Build tool | `cl.exe` + `lib.exe` | `x86_64-w64-mingw32-g++` + `ar` |
| Merge companion libs | `lib.exe /OUT:` | `ar -rcs` |
| Build script | `build-espeak-static.ps1` | `build-espeak-static-mingw.sh` |

### Tauri bundling note

The Tauri CLI bundles the installer using Windows-native tools (WiX Toolset,
NSIS, etc.) which cannot run on Linux/macOS.  Cross-compilation therefore
produces the raw binary but cannot generate an `.msi`/`.exe` installer in one
shot on a non-Windows host.  Use a Windows CI runner (e.g. GitHub Actions
`windows-latest`) for full installer packaging.

## Known limitations

- No Windows CI pipeline exists yet; breakage may go undetected between
  commits.
- The app has only been tested on macOS. UI, tray behaviour, and BLE
  discovery are untested on Windows.
- Tauri installer bundling requires a Windows host; see cross-compilation note
  above.
