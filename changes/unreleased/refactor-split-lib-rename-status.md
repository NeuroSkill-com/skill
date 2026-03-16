### Refactor

- **Split `lib.rs` into `state.rs` + `helpers.rs`**: Extracted `AppState`, `DeviceStatus`, `LlmState`, IPC packet structs, and all `impl` blocks into `state.rs` (~410 lines). Extracted time helpers, emit/toast helpers, settings persistence, device upsert, and state access shortcuts into `helpers.rs` (~250 lines). `lib.rs` dropped from 2,217 to 1,557 lines (–660).

- **Renamed `MuseStatus` → `DeviceStatus`**: The status struct is used for all devices (Muse, MW75, Hermes, OpenBCI) — the old name was misleading. Updated all Rust and TypeScript/Svelte references. The Tauri event name `"muse-status"` is kept for backward compatibility with existing WS clients.
