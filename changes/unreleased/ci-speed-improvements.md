### Build

- **CI/Release speed improvements**: Estimated 7-12 min savings per CI run via multiple optimizations:
  - Removed redundant `cargo check` step — `clippy` is a strict superset and already compiles everything.
  - Merged duplicate `cargo-audit` and `audit` jobs into a single security audit job.
  - Moved audit to run only on main/develop pushes (advisory, not PR-blocking).
  - Added concurrency groups to `ci.yml` to cancel superseded runs on the same branch/PR.
  - Replaced manual `--workspace -p crate1 -p crate2 ...` clippy invocations with `--workspace --exclude skill`.
  - Switched Linux CI from manual `actions/cache` to `Swatinem/rust-cache` for smarter per-crate invalidation (matching release workflows).
  - Cached Vulkan SDK on Linux CI (previously downloaded ~200 MB on every run).
  - Added `mold` fast linker + `clang` to Linux CI (previously only in release-linux).
  - Added `fetch-depth: 1` to CI jobs that don't need full git history.
  - Added `--locked` flag to all release cargo build commands for reproducible builds.
  - Added `--timings` flag and cargo-timings artifact upload to macOS and Linux release workflows (previously Windows only) for build profiling.
  - Removed redundant `cargo check` on Windows CI — `clippy` already covers it.
  - Fixed Discord notification to show "skipped" emoji for audit when it doesn't run on PRs.
  - Added `CMAKE_C_COMPILER_LAUNCHER` / `CMAKE_CXX_COMPILER_LAUNCHER` sccache integration to macOS, Linux CI, and preview builds (previously Windows release only) so cmake-based -sys crate compilations (llama-cpp-sys, espeak-ng) are cached across runs.
