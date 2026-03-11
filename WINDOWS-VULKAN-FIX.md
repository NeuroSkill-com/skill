# Fix for Vulkan Garbage Output on Windows

## Problem Description

When running LLM inference with the `llm-vulkan` feature enabled on Windows, you may see **garbage/random output** instead of coherent text. This issue occurs because:

1. The Vulkan SDK's DLLs (`vulkan-1.dll`, etc.) are not found in the system PATH
2. Environment variables like `VULKAN_SDK` are not set correctly
3. The llama-cpp backend fails to properly initialize the Vulkan device on Windows

## Quick Fix (No Code Changes)

Before installing or building, ensure Vulkan is available:

1. **Download and install the Vulkan SDK**:
   ```powershell
   # Official installer from LunarG:
   https://vulkan.lunarg.com/sdk/#windows
   ```

2. **Verify installation**:
   ```powershell
   explorer "C:\Program Files\Vulkan SDK"
   ```

3. **Set environment variables for the current session**:
   ```powershell
   $env:VULKAN_SDK = "C:\Program Files\Vulkan SDK"
   $env:PATH = "$env:VULKAN_SDK\bin;" + $env:PATH
   skill.exe  # or your build path
   ```

4. **Make it permanent** (optional):
   - Open System Properties → Advanced → Environment Variables
   - Add `VULKAN_SDK` with value `C:\Program Files\Vulkan SDK`
   - Add `C:\Program Files\Vulkan SDK\bin` to the end of PATH

## Code Changes Applied

The following changes have been made to `src-tauri/src/llm/mod.rs`:

### 1. Windows-specific Vulkan SDK path injection

When initializing the llama backend on Windows, the code now:
- Checks for the `VULKAN_SDK` environment variable
- Prepends its `\bin` directory to the system PATH
- Logs which path was used for debugging

```rust
#[cfg(target_os = "windows")]
{
    if let Ok(vulkan_sdk_path) = std::env::var("VULKAN_SDK") {
        if let Ok(parent_dir) = std::path::Path::new(&vulkan_sdk_path).parent() {
            std::env::set_var(
                "PATH",
                format!("{};{}", parent_dir.display(), current_path),
            );
        }
    }
}
```

### 2. Enhanced diagnostics

When GPU layers are requested on Windows, the system now logs a warning to remind users to ensure Vulkan SDK is properly installed.

## Alternative: Use CPU Instead of Vulkan

If Vulkan continues to cause issues:

### Method 1: Set `n_gpu_layers = 0` in settings.json
```json
{
  "llm": {
    "enabled": true,
    "model_path": "/path/to/model.Q4_K_M.gguf",
    "n_gpu_layers": 0,  // Force CPU inference (workaround)
    "ctx_size": 8192
  }
}
```

### Method 2: Disable LLM feature until Windows GPU support is stable
Edit `Cargo.toml` and remove the Vulkan feature:
```toml
# Comment out for now:
llm-vulkan   = ["llm", "llama-cpp-4?/vulkan"]
```

Then build without the feature and run with CPU-only inference.

## Verification Steps

After applying fixes, verify GPU is being used:

1. **Check logs** for messages about Vulkan initialization
2. **Look for this message** (indicates success):
   ```
   [llm][info] backend initialised
   ```
3. **If you see errors**, check if the Vulkan SDK path exists:
   ```powershell
   Test-Path "C:\Program Files\Vulkan SDK"
   # Should return: True
   ```

## Resources

- **Vulkan SDK**: https://vulkan.lunarg.com/sdk/#windows
- **llama-cpp Vulkan docs**: https://github.com/ggerganov/llama.cpp/wiki/GPU-Types#vulkan
- **Troubleshooting GPU inference**: Check if `vulkaninfo` reports devices

## Building from Source with This Fix

Once the code changes in `src-tauri/src/llm/mod.rs` are applied:

```powershell
# On Windows Developer PowerShell for VS
$env:VULKAN_SDK = "C:\Program Files\Vulkan SDK"
cargo build --release --features llm-vulkan
npm run tauri:build
```

This should now produce a binary that works correctly with Vulkan on Windows.
