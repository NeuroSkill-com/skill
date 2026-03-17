### Refactor

- **Shared calibration CRUD service**: Extracted `calibration_service.rs` with `create_profile`, `update_profile`, `delete_profile`, `list_profiles`, and `get_profile` functions. Both the Tauri IPC commands (`window_cmds`) and the WebSocket API (`ws_commands`) now delegate to this single service, eliminating duplicated state mutation logic.

- **Extract LLM HTTP handlers from `engine.rs` (2,502 → 2,079 lines)**: Moved all axum HTTP handlers, auth helpers, and the router builder into a new `handlers.rs` (449 lines) in the `skill-llm` crate. `engine.rs` retains the inference actor, tool orchestration, and shared state.

- **Extract web search backends from `skill-tools/exec.rs` (1,551 → 944 lines)**: Moved DuckDuckGo HTML, Brave API, SearXNG, and headless fetch code into `search.rs` (616 lines).

- **Extract platform capture from `skill-screenshots/capture.rs` (1,527 → 1,145 lines)**: Moved macOS/Linux/Windows window capture and image decoding into `platform.rs` (392 lines).

- **Extract EEG band metrics from `skill-eeg/eeg_bands.rs` (1,510 → 1,222 lines)**: Moved advanced metric functions (SEF, Hjorth, entropy, DFA, consciousness indices) into `band_metrics.rs` (298 lines).
