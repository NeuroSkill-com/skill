### Performance

- **Faster prompt processing on the CPU**: rlx-models now compiles CPU prefill graphs from a 32-token minimum instead of 256. CPU prefill cost follows the compiled width rather than the prompt, so a short chat prompt paid for 256 tokens. On a Raspberry Pi 5 (Qwen3.5-0.8B, ~22-token prompts, through the daemon) prompts took 3.1 / 5.6 / 5.6 s instead of 12 / 45 / 50 s, with identical output. GPU backends keep the 256 minimum.

### Build

- **macOS DuckDB link fix applies again**: the vendored `libduckdb-sys` patch (`-bundle` on the static-extension autoregister archive, the fix for the v0.1.0-rc.4 "not 8-byte aligned" link failure and the `unable to open object file` warnings) had stopped applying when `duckdb` moved to 1.20000.2 — Cargo reported it as an unused patch. Re-vendored as `patches/libduckdb-sys-1.20000.2`; upstream moved the link line into `build.rs`, so the change is now in one place.
- **Release bump preflight passes again**: Cargo's new `unused_dependencies` lint printed 25 manifest warnings, which `npm run bump` treats as fatal. Unused dependencies removed; dependencies used only behind a feature (`axum`, `async-stream`, `anyhow`, `hf-hub`, `serde_json`, `tokio`) made optional under that feature; `llmfit-core` limited to non-macOS targets; test-only dependencies moved to `[dev-dependencies]`; `src-tauri` takes the workspace `rustls`; the iroh crates inherit the workspace lints.
- **CI**: the integration-test step no longer errors on test targets with `required-features` (skill-eeg's `fft_*` need `slow-tests`), and the release-notes / test-target scripts type-check under `svelte-check`.
- **Advisory ignores pruned**: 23 entries in `.cargo/audit.toml` and one in `deny.toml` matched nothing — crates gone from the graph, fixed by upgrades, or (the gtk-rs GTK3 set) withdrawn by RustSec on 2026-08-14 — and a dead ignore hides a future advisory with the same ID. `cargo-deny`'s `advisory-not-detected` warning is gone.

### Dependencies

- **rlx** → `f6575a59`, **rlx-models** → `7935c8e4` (CPU prefill floor above).
