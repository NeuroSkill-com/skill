### Bugfixes

- **Fix LUNA crash on channels with mixed-case names**: LUNA's channel vocabulary uses uppercase names (e.g. `PZ`) but some devices like EMOTIV INSIGHT send mixed-case (e.g. `Pz`), causing a panic in `channel_indices_unwrap`. The embed worker now normalises channel names to uppercase and filters out any channels not in the LUNA vocabulary instead of panicking.

- **Fix screenshot HNSW panic on vision model change**: Switching the screenshot embedding backend (e.g. fastembed 512-dim → mmproj 2048-dim) caused a panic when inserting into the existing HNSW index built with a different dimension. Both vision and OCR HNSW indices now detect dimension mismatches and reset to a fresh index, with a log message suggesting re-embed to backfill.

- **Fix metrics-only SQLite insert failing with NOT NULL constraint**: When the GPU device was poisoned and the embedder fell back to metrics-only mode, `insert_metrics_only` tried to insert NULL for `eeg_embedding` which has a NOT NULL constraint in existing databases. Now inserts an empty blob for backward compatibility, and new databases use a nullable column.
