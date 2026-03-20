### Features

- **LUNA model backend for EXG embeddings**: Added support for the [luna-rs](https://crates.io/crates/luna-rs) EEG foundation model as an alternative embedding backend alongside ZUNA. Users can now select between ZUNA (default) and LUNA in the EEG Model settings tab, with LUNA offering `base`, `large`, and `huge` model size variants. The selected backend and model size are persisted in `model_config.json`.

- **Embedding speed tracking**: Every embedding inference now tracks wall-clock time in milliseconds. The last embedding speed and an exponential moving average are displayed in the EEG Model tab and published via the WebSocket status endpoint. SQLite stores per-row `embed_speed_ms` for post-hoc performance analysis.

- **Model provenance in SQLite and HNSW**: Each embedding row now records which model backend (`zuna` or `luna`) produced it via the new `model_backend` TEXT column in the daily `embeddings` table. Historical rows without the column are auto-migrated on open. This enables filtering, auditing, and future re-embedding by model.

- **Per-model HNSW indices**: Each model backend now gets its own HNSW index file per day (`eeg_embeddings.hnsw` for ZUNA, `eeg_embeddings_luna.hnsw` for LUNA) and globally (`eeg_global.hnsw` / `eeg_global_luna.hnsw`). This prevents dimension mismatches when switching backends and allows side-by-side nearest-neighbor search for each model. The daily SQLite remains shared with a `model_backend` column to differentiate rows. Search APIs accept an optional model backend parameter to load the correct index.

- **Re-embed from raw EXG data**: Added `estimate_reembed` and `trigger_reembed` Tauri commands. The re-embed worker reads raw EEG samples from session CSV files (`exg_*.csv` / `muse_*.csv`), reads channel names and sample rate from the JSON sidecar, chunks data into 5-second epochs with 50% overlap, resamples to model input size, runs the selected encoder (ZUNA or LUNA) on the GPU, and writes new embedding rows to SQLite. Per-model HNSW indices are rebuilt per day and globally. Progress is streamed to the frontend via the `reembed-progress` event.

### Bugfixes

- **Screenshot LLM token receiver**: Fixed `blocking_recv()` return type mismatch (`Ok(tok)` → `Some(tok)`) in `screenshot.rs` that prevented compilation with newer tokio mpsc channel API.
