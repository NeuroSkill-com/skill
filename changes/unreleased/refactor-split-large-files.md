### Refactor

- **Split `eeg_embeddings.rs` (2,434 → 3 files)**: Removed 624 lines of dead code (`#[cfg(any())]` blocks). Split the remaining 1,810 lines into `mod.rs` (414 lines, public API + EegAccumulator), `day_store.rs` (372 lines, per-day HNSW + SQLite), and `worker.rs` (1,051 lines, background embed worker + hook matcher + weight helpers).

- **Split `ws_commands.rs` (2,417 → 3 files)**: Extracted `hooks.rs` (315 lines, hooks_get/set/status/suggest/log handlers) and `llm_cmds.rs` (418 lines, all LLM WebSocket commands), reducing `mod.rs` to 1,714 lines. The central `dispatch()` function remains in `mod.rs`.

- **Split `settings_cmds.rs` (1,789 → 3 files)**: Extracted `dnd_cmds.rs` (207 lines, Do Not Disturb automation commands) and `hook_cmds.rs` (274 lines, hook distance suggestion and audit log), reducing `mod.rs` to 1,341 lines.
