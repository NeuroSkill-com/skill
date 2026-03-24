### Bugfixes

- **CI: fix `ort-sys` link error on Linux/Windows**: `ort` was declared with `default-features = false` and no download strategy in the base `[dependencies]`, so `ort-sys` could not find `libonnxruntime` on Linux/Windows CI runners. Moved `ort` into target-specific sections — `download-binaries` for Linux/Windows (fetches the pre-built shared library at build time), `coreml` for macOS (unchanged).
