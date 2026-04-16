### Features

- **Session sidecar `device_kind`**: JSON sidecars now include `device_kind` (e.g. "muse", "awear", "openbci") alongside `device_name` and `channel_names`. Enables reliable device identification during reembedding when a day directory contains sessions from different devices. Backward compatible — old sidecars without this field are handled gracefully.

### Bugfixes

- **Reembed silent failures**: Added diagnostic logging on first extract failure and first encode failure per batch. Previously all epoch failures were completely silent, making it impossible to debug why 27K+ epochs would fail with zero output.
- **CSV channel length mismatch**: The CSV parser now skips entire rows when any channel value fails to parse, preventing channels from accumulating different sample counts. Previously, individual parse failures were silently skipped per-channel, creating mismatched-length arrays that the ZUNA encoder rejected.
- **Mixed-device reembedding**: CSVs with fewer columns than the metadata expects (e.g. different device in the same day directory) now use available columns instead of skipping all rows. The column count is detected per-file from the CSV header.
- **Reconnect respects test mode**: The BLE reconnect loop now pauses when `test_mode` is active, preventing background connection attempts from interfering with E2E tests.
- **`cpu-only` feature build**: Fixed compilation errors in `skill-daemon` when built with `--no-default-features --features cpu-only`. The `#[cfg(not(feature = "llm"))]` fallback paths referenced `AppState` fields that don't exist without the `llm` feature. Replaced with static stubs. This fixes the CI coverage job.
- **Clippy 1.95**: Fixed `sort_by` → `sort_by_key(Reverse)` in skill-history, `collapsible_match` in connect_wired, `checked_div` in iroh_remote.
- **Prebuilt llama naming**: Updated download URL from `q1-metal`/`q1-vulkan` to `metal-static`/`vulkan-static` to match current release asset names.
