### Refactor

- **Cross-crate deduplication**: consolidated shared utilities into `skill-data::util` — `date_dirs`, `MutexExt`, UTC timestamp formatters (`yyyymmdd_utc`, `yyyymmddhhmmss_utc`, `unix_to_ts`, `ts_to_unix`, `fmt_unix_utc`, `civil_from_unix`), and `open_readonly` SQLite helper. Removed duplicate implementations from `skill-commands`, `skill-label-index`, `skill-exg`, `skill-screenshots`, and `skill-router`.
- **Screenshot HNSW dedup**: replaced near-identical vision/OCR HNSW load/save/rebuild function pairs in `skill-screenshots` with a generic `load_or_rebuild_hnsw_generic` + `save_hnsw_to` parametrized by path and fetch closure.
- **Band-snapshot enrichment**: extracted `enrich_band_snapshot` + `SnapshotContext` into `skill-devices`, eliminating ~90 lines of duplicated PPG/artifact/head-pose/composite-score/GPU enrichment from `muse_session.rs` and `openbci_session.rs`.
- **DND decision dedup**: replaced ~200-line inline DND decision block in `muse_session.rs` with the existing `skill_devices::dnd_tick()` pure function and proper state round-tripping.
- **Constants consistency**: fixed hardcoded `"labels.sqlite"` in `skill-data::label_store` to use `LABELS_FILE` from `skill-constants`.
