### Features

- **GPU memory safety guard**: Added pre-decode GPU memory checks to prevent Metal/CUDA `abort()` crashes when GPU memory is exhausted. The LLM engine now verifies sufficient free GPU/unified memory before starting prompt decode, multimodal decode, warmup, and periodically during token generation. When memory is too low, requests are rejected with a recoverable error message instead of crashing the entire application.

### UI

- **GPU memory threshold settings**: Added configurable GPU memory safety thresholds in Settings → LLM → Inference Settings. Users can set the minimum free GPU memory required before decode (default: 0.5 GB) and during generation (default: 0.3 GB), or disable the checks entirely.

### Bugfixes

- **LLM crash on Metal buffer allocation failure**: Fixed a crash (`SIGABRT` in `ggml_metal_synchronize` → `ggml_abort`) that occurred when the Metal GPU backend failed to allocate buffers during `llama_decode`. The ggml abort is unrecoverable in-process, so pre-flight memory checks now prevent reaching that code path.
- **Reduced dynamic context growth memory budget**: Lowered the memory headroom multiplier for dynamic context window resizing from 85% to 70% of available GPU memory, reducing the risk of Metal OOM during large context operations.

### i18n

- **GPU memory safety strings**: Added translation keys for the new GPU memory threshold settings (`llm.inference.gpuMemThreshold`, `gpuMemThresholdDesc`, `gpuMemDecode`, `gpuMemGen`) in all languages (en, fr, uk, de, he).
