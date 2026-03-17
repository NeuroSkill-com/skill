### Refactor

- **Extract lifecycle and quit modules from lib.rs**: Moved session lifecycle (start/cancel/disconnect/reconnect backoff) into `lifecycle.rs` and quit-confirmation dialogs into `quit.rs`, reducing `lib.rs` from 1,778 to 1,495 lines.
- **Add `DeviceStatus::reset_disconnected()` method**: Replaces 15+ manual field resets in `go_disconnected` with a single method call, preventing missed fields when new status fields are added.
- **Consolidate mutex lock acquisitions in `setup_app`**: Merged 4 separate lock/unlock cycles for LLM autostart, embedding model, model status, and HF repo into a single critical section.
- **Extract device-kind detection into constants**: Replaced inline string matching (`starts_with("ganglion")`, `contains("mw75")`) with named constants and a `detect_device_kind()` function with unit tests.

### Bugfixes

- **Fix `llama-cpp-4` version mismatch**: Non-macOS platforms used `0.2.10` while macOS used `0.2.12`; aligned to `0.2.12` everywhere.
- **Fix `package.json` dead script**: Removed duplicate `taur:build:win:nsis` key (typo missing `i`).
- **Replace silent `catch {}` in chat page**: All 18 empty catch blocks in `chat/+page.svelte` now log warnings/errors to the console instead of silently swallowing failures.

### Build

- **Add `rustfmt.toml` and `clippy.toml`**: Added formatting and lint configuration for consistent Rust code style.
