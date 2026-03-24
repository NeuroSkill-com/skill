### Refactor

- **Split `skill-llm/src/catalog.rs` into focused sub-modules**: decomposed the 1059-line monolithic file into 5 modules under `catalog/` — `mod.rs` (re-exports), `types.rs` (data types), `persistence.rs` (load/save/merge/queries), `memory.rs` (memory estimation, context-size recommendation), `download.rs` (resumable HuggingFace downloader). All public API re-exports preserved.

### Bugfixes

- **Add missing tests for under-tested crates**: added 28 new unit tests across three crates:
  - `skill-llm`: 6 tests for `estimate_memory_gb` and `recommend_ctx_size` (catalog/memory), 7 tests for `ThinkTracker` budget enforcement (engine/think_tracker).
  - `skill-commands`: 11 tests for `query_slug`, `file_ts`, `pca_2d`, and `pca_3d` pure utility functions.
  - `skill-screenshots`: 4 tests for `ScreenshotMetrics` atomics and `MetricsSnapshot` serialization.
