### Features

- **Flamegraph profiling script**: Added `npm run tauri:flamegraph` to profile the Tauri app with `cargo flamegraph` and produce an interactive SVG. Supports optional duration argument (e.g. `npm run tauri:flamegraph -- 60` for 60s, or `0` for until exit).
