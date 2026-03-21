### Features

- **Flamegraph profiling script**: Added `npm run tauri:flamegraph` to profile the Tauri app with `cargo flamegraph` and produce an interactive SVG. Works on Linux (perf), macOS (dtrace), and Windows (dtrace/xperf). Supports optional duration argument (e.g. `npm run tauri:flamegraph -- 60` for 60s, or default until app exit).

### Bugfixes

- **Flamegraph stale trace file**: Auto-remove leftover `cargo-flamegraph.trace` files from previous runs that caused exit code 42 ("Trace file already exists").
- **Flamegraph missing symbols**: Automatically set `CARGO_PROFILE_RELEASE_DEBUG=true` so release flamegraphs contain resolved function names instead of hex addresses.
