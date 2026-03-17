### Bugfixes

- **Enable GPU acceleration in Linux and macOS release builds**: Added missing `--features llm-vulkan` to the Linux CI cargo build and `--features llm-metal` to the macOS CI cargo build, ensuring GPU-accelerated LLM inference is included in release binaries (matching what `tauri-build.js` injects for local builds).
