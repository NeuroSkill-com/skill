### Build

- **Fix Tauri Build Cargo Target**: `scripts/tauri-build.js` now falls back to the `CARGO_BUILD_TARGET` env var when no explicit `--target` flag is passed, so the freshly built `skill-daemon` binary (and the macOS .app bundle during widget embedding) is located under the correct target-triple directory. Fixes the spurious "skill-daemon binary not found after build" warning and the skipped stale-daemon reap on every direnv-enabled dev run.
