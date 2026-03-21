### Build

- **Flamegraph script: full clean + user directory fixes**: `npm run tauri:flamegraph` now runs `cargo clean` and removes SvelteKit/Vite caches (`.svelte-kit`, `node_modules/.vite`, `build`) before building, ensuring a completely fresh profiling run. The sudo `--preserve-env` list is expanded to include `CARGO_HOME`, `RUSTUP_HOME`, `DISPLAY`, `WAYLAND_DISPLAY`, `DBUS_SESSION_BUS_ADDRESS`, `XDG_RUNTIME_DIR`, and `LOGNAME`, so the profiled app uses the current user's directories instead of root's. Ownership fixup after profiling now correctly identifies the real user.
