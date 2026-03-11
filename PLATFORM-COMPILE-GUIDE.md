# Platform-Specific Build Guide

## Vulkan Feature Availability

| Platform | Supported GPU Backend | Feature Flag          |
|----------|----------------------|-----------------------|
| macOS    | Metal               | `llm-metal`          |
| Linux    | Vulkan / ROCm       | `llm-vulkan` (Vulkan), `llm-cuda` (NVIDIA)/`llm-rocm`(AMD) |
| Windows  | Vulkan              | `llm-vulkan`         |

## Building for macOS (Metal Backend)

macOS uses **Metal**, not Vulkan. To build with GPU acceleration:

```bash
#!/bin/bash
# Build with Metal on macOS

cargo build --release \
    --features "llm-metal,tts-kitten,tts-neutts" \
    --no-default-features
npm run tauri:build
```

**Do NOT use `llm-vulkan` or `llm-cuda` on macOS** — Metal is the only supported GPU backend.

---

## Building for Linux (Vulkan Backend)

Linux supports Vulkan by default (if you have the Vulkan SDK and headers installed):

```bash
#!/bin/bash
# Build with Vulkan on Linux

sudo apt install vulkan-sdk libvulkan-dev  # or equivalent for your distro

cargo build --release \
    --features "llm-vulkan,tts-kitten,tts-neutts" \
    --no-default-features
npm run tauri:build
```

---

## Building for Windows (Vulkan Backend)

Windows uses Vulkan GPU acceleration via the **Vulkan SDK**:

```powershell
# On Windows with Developer PowerShell for Visual Studio

# Install Vulkan SDK if needed
winget install Lunarg.VulkanSDK  # or download from lunarg.com

cargo build --release \
    --features "llm-vulkan,tts-kitten,tts-neutts" \
    --no-default-features
npm run tauri:build
```

The code we fixed ensures proper Vulkan SDK detection on Windows.

---

## Cross-Platform Build Script (Recommended)

Create a `build.sh` script that auto-detects the platform and uses the correct feature:

```bash
#!/bin/bash
set -e

# Auto-detect platform and build with appropriate GPU backend
if [ "$(uname)" = "Darwin" ]; then
    # macOS — use Metal
    BUILT_FROM="macOS (Metal)"
elif [[ "$OSTYPE" == "linux-gnu"* || "$OSTYPE" == "linux-musl"* ]]; then
    # Linux — use Vulkan
    BUILT_FROM="Linux (Vulkan)"
elif [ "$(uname | tr '[:upper:]' '[:lower:]')" = "mswin*nt" ]; then
    # Windows
    BUILT_FROM="Windows (Vulkan)"
else
    echo "Unsupported platform: $(uname)"
    exit 1
fi

# Set up environment variables for GPU backend
case "$BUILT_FROM" in
    macOS\ \(Metal\))
        FEATURES="--features llm-metal,tts-kitten,tts-neutts"
        ;;
    Linux\ \(Vulkan\)|Linux)
        FEATURES="--features llm-vulkan,tts-kitten,tts-neutts"
        ;;
    Windows*)
        # Windows — let environment handle Vulkan SDK path
        FEATURES="--features llm-vulkan,tts-kitten,tts-neutts"
        ;;
esac

echo "Building on $BUILT_FROM with GPU acceleration enabled..."
echo "Using features: $FEATURES"

cargo build --release $FEATURES --no-default-features || exit 1
npm run tauri:build || exit 1
```

---

## Building Without GPU Acceleration (CPU-only)

If you encounter issues with GPU backends or want to force CPU inference:

```bash
# Build without GPU acceleration
cargo build --release \
    --no-default-features \
    --features "tts-kitten,tts-neutts"  # Just TTS, no LLM GPU
npm run tauri:build
```

Or in `src-tauri/settings.json`:

```json
{
  "llm": {
    "enabled": true,
    "n_gpu_layers": 0  // Force CPU inference
  }
}
```

---

## Feature Flags Reference

In `src-tauri/Cargo.toml`:

- `llm-metal` — Metal GPU (macOS only) ✓
- `llm-vulkan` — Vulkan GPU (Linux/Windows) ✓  
- `llm-cuda` — CUDA GPU (Linux NVIDIA, Windows NVIDIA)
- `llm-rocm` — ROCm GPU (Linux AMD)
- `llm-mtmd` — Multimodal projector (any platform with GPU)

---

## Quick Fix for Your Current Situation

You're on macOS and tried to build with `llm-vulkan`:

1. **For macOS**: Use Metal, not Vulkan
   ```bash
   cargo build --release \
       --features "llm-metal,tts-kitten,tts-neutts" \
       --no-default-features
   ```

2. **OR** build with CPU-only (safe for any platform):
   ```bash
   cargo build --release \
       --no-default-features \
       --features "tts-kitten,tts-neutts" \
       --features llm  # LLM feature without GPU
   ```

---

For the **Windows fix** we implemented earlier, that code is platform-gated:

```rust
#[cfg(target_os = "windows")]
{
    // Windows-only Vulkan SDK path injection
}
```

This means it will only compile on Windows and won't affect macOS builds. The fix is already in place for Windows users!

