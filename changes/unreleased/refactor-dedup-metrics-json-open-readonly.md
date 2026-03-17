### Refactor

- **Deduplicate metrics JSON serialization in `DayStore`**: Extracted the ~60-field metrics-to-JSON serialization into a shared `metrics_to_json()` function, eliminating the copy-pasted logic between `insert()` and `insert_metrics_only()`. Reduces `day_store.rs` by ~65 lines.

- **Replace raw `SQLITE_OPEN_READ_ONLY` flags with `open_readonly()` helper**: Replaced 11 inline `rusqlite::Connection::open_with_flags(…, READ_ONLY)` calls across 7 files with the existing `skill_data::util::open_readonly()` helper for consistency with the workspace crates.
