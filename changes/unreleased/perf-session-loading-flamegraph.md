### Performance

- **Session listing 10x faster**: Replaced `serde_json::Value` (BTreeMap-backed) with a typed `SessionJsonMeta` struct for parsing session JSON sidecars, eliminating expensive BTreeMap construction and recursive drop overhead.
- **Metrics timestamp lookup O(1) instead of O(n)**: `read_metrics_csv_time_range` now reads only the first and last 4 KB of the file (via seek) instead of parsing every CSV record. For a 100 MB metrics file this reduces I/O from ~100 MB to ~8 KB.
- **Skip redundant timestamp patching**: `patch_session_timestamps` now skips sessions that already have valid start/end timestamps from their JSON sidecar, avoiding unnecessary metrics file reads on every session listing.
