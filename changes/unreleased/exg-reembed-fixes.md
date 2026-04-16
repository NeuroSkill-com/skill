### Bugfixes

- Fix EXG reembedding: resolve GPU f16 TypeMismatch bug by using f32 GPU encoder for batch reembed, fix per-segment channel metadata for mixed-device sessions, require full 5s epoch extraction, skip channel counts with consecutive failures, and process most recent days first.
- Fix reembed progress events not reaching UI (Tauri `listen` → daemon WebSocket `onDaemonEvent`), fix `"day"` → `"date"` field in progress payload, and emit `loading_encoder`/`scanning` status immediately for responsive feedback.
- Rebuild label EEG HNSW index after manual and idle reembed so interactive search can bridge text labels to EEG epochs.
- Add 32 unit tests for reembed edge cases: mixed devices, relative timestamps, partial rows, empty files, boundary extraction, and channel count handling.
