### Bugfixes

- **Fix Windows SxS "side-by-side configuration incorrect" error**: Added `CMAKE_MSVC_RUNTIME_LIBRARY = "MultiThreaded"` to the workspace `.cargo/config.toml` `[env]` section so that cmake-based C/C++ dependencies (llama-cpp-sys, espeak-ng) use static CRT (`/MT`) matching Rust's `+crt-static` target feature. Previously this env var was only set in CI, causing local Windows builds to produce a CRT mismatch that triggered the SxS error on machines without the Visual C++ Redistributable.
