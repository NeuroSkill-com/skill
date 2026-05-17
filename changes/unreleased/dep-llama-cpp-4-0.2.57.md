### Dependencies

- **Bump llama-cpp-4 to 0.2.57**: bumped `llama-cpp-4` and `llama-cpp-sys-4` from 0.2.56 to 0.2.57, picking up the Windows MSVC bindgen fix (the `LLAMA_CONTEXT_TYPE_*` constants are `i32` on MSVC but the `LlamaContextType` enum is `#[repr(u32)]`, which broke the Windows release build). Pinned `LLAMA_PREBUILT_TAG` in `scripts/ci.mjs` to `v0.2.57` so the prebuilt llama libs ship the same MTP symbols (`mtp_session_new`, `mtp_session_draft`, etc.) the crate now expects — the previous `0.2.46` pin caused undefined-symbol link failures for `skill-daemon` on macOS and Linux after the 0.2.56 MTP upgrade.

### Build

- **Fix release retry: cargo failures inside `run_cmd` were silently ignored**: `release-mac.yml`, `release-linux.yml`, and `release-windows.yml` call `run_cmd` via `if ! run_cmd; then`, which inhibits `set -e` inside the function body. A failing `cargo build -p skill-daemon` (e.g. link error against stale prebuilt llama libs) would silently continue to the next `cargo build`, the function would return 0, and the prebuilt→source-build fallback would never fire — leaving the assemble/package step to fail later with a confusing "missing daemon binary" error. Added explicit `|| return $?` after each cargo invocation so failures propagate regardless of bash's `set -e` inhibition rules.
