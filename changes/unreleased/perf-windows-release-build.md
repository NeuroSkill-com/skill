### Performance

- **Faster Windows release builds**: Added `CMAKE_C_COMPILER_LAUNCHER` and `CMAKE_CXX_COMPILER_LAUNCHER` env vars so sccache caches C/C++ compilations from cmake-based -sys crates (llama-cpp-sys-4, etc.), not just rustc. Added `[profile.release]` with `lto = "thin"` and `codegen-units = 8` for faster linking. Moved frontend build before SDK installs to reduce idle time. Combined NSIS and Vulkan SDK installs into a single parallel step. Added sccache GHA cache versioning and `--timings` cargo flag with artifact upload for build profiling.
