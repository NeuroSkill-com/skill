### Features

- **Flamegraph profiling script**: Added `npm run tauri:flamegraph` to profile the Tauri app with `flamegraph` and produce an interactive SVG. Works on Linux (perf), macOS (dtrace), and Windows (dtrace/xperf). Supports optional duration argument (e.g. `npm run tauri:flamegraph -- 60`) and `--release` flag (default: dev profile to match `tauri dev`).

### Bugfixes

- **Flamegraph permission errors on macOS**: Separated build (normal user) from profiling (`sudo flamegraph`) so dtrace runs as root and owns its trace files. Fixes "Trace file already exists" (exit 42) and "Permission denied" errors caused by root-owned artifacts from previous runs.
- **Flamegraph builds dev profile by default**: Now matches `tauri dev` behavior. Pass `--release` explicitly for optimized profiling.
