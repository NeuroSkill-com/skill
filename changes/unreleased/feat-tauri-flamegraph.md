### Features

- **Flamegraph profiling script**: Added `npm run tauri:flamegraph` to profile the Tauri app with `cargo flamegraph` and produce an interactive SVG. Works on Linux (perf), macOS (dtrace), and Windows (dtrace/xperf). Supports optional duration argument (e.g. `npm run tauri:flamegraph -- 60` for 60s, or default until app exit).

### Bugfixes

- **Flamegraph stale trace file**: Use `sudo rm` fallback to clean root-owned `cargo-flamegraph.trace` left by macOS dtrace from previous runs (exit code 42).
- **Flamegraph missing symbols**: Automatically set `CARGO_PROFILE_RELEASE_DEBUG=true` so release flamegraphs contain resolved function names instead of hex addresses.
- **Flamegraph sudo timing**: Warm the sudo credential cache before compilation on macOS so the password prompt appears upfront, not after a 5-minute build.
