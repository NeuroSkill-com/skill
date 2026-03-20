### Features

- **LUNA model backend for EXG embeddings**: Added support for the [luna-rs](https://crates.io/crates/luna-rs) EEG foundation model as an alternative embedding backend alongside ZUNA. Users can now select between ZUNA (default) and LUNA in the EEG Model settings tab, with LUNA offering `base`, `large`, and `huge` model size variants. The selected backend and model size are persisted in `model_config.json`.

- **Embedding speed tracking**: Every embedding inference now tracks wall-clock time in milliseconds. The last embedding speed and an exponential moving average are displayed in the EEG Model tab and published via the WebSocket status endpoint. SQLite stores per-row `embed_speed_ms` for post-hoc performance analysis.

- **Model provenance in SQLite and HNSW**: Each embedding row now records which model backend (`zuna` or `luna`) produced it via the new `model_backend` TEXT column in the daily `embeddings` table. Historical rows without the column are auto-migrated on open. This enables filtering, auditing, and future re-embedding by model.

- **Re-embed historical data**: Added `estimate_reembed` and `trigger_reembed` Tauri commands with a UI section in the EEG Model settings tab. Currently tags all legacy embeddings with model metadata (`model_backend = 'zuna'`). Progress is streamed to the frontend via the `reembed-progress` event.

### Bugfixes

- **Screenshot LLM token receiver**: Fixed `blocking_recv()` return type mismatch (`Ok(tok)` → `Some(tok)`) in `screenshot.rs` that prevented compilation with newer tokio mpsc channel API.
