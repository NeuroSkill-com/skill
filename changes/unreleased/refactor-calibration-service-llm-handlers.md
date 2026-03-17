### Refactor

- **Shared calibration CRUD service**: Extracted `calibration_service.rs` with `create_profile`, `update_profile`, `delete_profile`, `list_profiles`, and `get_profile` functions. Both the Tauri IPC commands (`window_cmds`) and the WebSocket API (`ws_commands`) now delegate to this single service, eliminating duplicated state mutation logic.

- **Extract LLM HTTP handlers from `engine.rs` (2,502 → 2,079 lines)**: Moved all axum HTTP handlers, auth helpers, and the router builder into a new `handlers.rs` (449 lines) in the `skill-llm` crate. `engine.rs` retains the inference actor, tool orchestration, and shared state.
