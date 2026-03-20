### Performance

- **Remove excessive .clone() in hot loops**: Eliminated redundant `String`, `PathBuf`, and `Vec<f32>` allocations in search, index rebuild, and downsampling hot paths. Key changes:
  - `skill-commands`: Search functions now store indices into `date_dirs`/`day_indices` instead of cloning `(String, PathBuf)` per embedding and per HNSW hit; owned copies are only materialized for the final top-k candidates after truncation.
  - `skill-history`: `list_embedding_sessions` interns day-name strings via an index vector instead of cloning per DB row; session-gap loop references interned names by index.
  - `skill-history`: `downsample_timeseries` uses in-place `swap` + `truncate` instead of cloning large `EpochRow` structs into a new `Vec`.
  - `skill-history`: `analyze_search_results` borrows `&str` date references instead of cloning into the frequency map.
  - `skill-label-index`: `rebuild_indices` iterates `rows` by value so `Vec<f32>` embeddings are moved into HNSW insert instead of cloned.
  - `skill-screenshots`: HNSW rebuild loop iterates rows by value to move embeddings instead of cloning.
  - `skill-commands/graph`: Parent-ID dedup uses `as_deref()` + `&str` set instead of cloning `Option<String>` twice.
