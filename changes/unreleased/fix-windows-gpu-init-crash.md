### Bugfixes

- **Fix Windows STATUS_ACCESS_VIOLATION crash during GPU init**: Added a process-wide GPU initialisation mutex (`gpu_init_lock`) that serialises DirectML (ONNX, screenshot CLIP) and wgpu/Vulkan (cubecl, ZUNA/LUNA EEG encoder) framework startup. On Windows, simultaneously initialising both GPU backends could trigger a segfault in the Vulkan driver. The lock is held only during model load; once both frameworks are initialised they run concurrently without contention.

### Refactor

- **`ScreenshotContext::gpu_init_guard()`**: New optional trait method allowing the screenshot crate to acquire the app-level GPU init lock without depending on Tauri directly.
