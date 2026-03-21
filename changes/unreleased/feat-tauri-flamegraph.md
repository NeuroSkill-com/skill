### Features

- **Flamegraph profiling script**: Added `npm run tauri:flamegraph` to profile the Tauri app with `cargo flamegraph` and produce an interactive SVG. Works on Linux (perf), macOS (dtrace), and Windows (dtrace/xperf). Supports optional duration argument (e.g. `npm run tauri:flamegraph -- 60` for 60s, or default until app exit).
