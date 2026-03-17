### Refactor

- **Split `llm.rs` into module directory**: Refactored the 1537-line `src-tauri/src/llm.rs` into `src-tauri/src/llm/` with focused sub-modules: `mod.rs` (re-exports, logger, emitter), `cmds/catalog.rs` (catalog queries), `cmds/downloads.rs` (download lifecycle), `cmds/selection.rs` (model selection), `cmds/server.rs` (server lifecycle), `cmds/chat.rs` (chat persistence), `cmds/streaming.rs` (IPC streaming), `cmds/hardware_fit.rs` (hardware prediction). All public API paths remain unchanged.
