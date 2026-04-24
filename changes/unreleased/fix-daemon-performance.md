### Performance

- **`open_readonly` for read-only handlers**: added `ActivityStore::open_readonly()` that skips 15+ ALTER TABLE migrations and opens in read-only mode. Switched 38+ HTTP handlers and 3 background tasks from `open` to `open_readonly`.
- **`PRAGMA busy_timeout=5000`**: added to `init_wal_pragmas` and `util::open_readonly` so all database connections (activity, labels, screenshots, EEG embeddings) wait up to 5 s on lock contention instead of failing immediately with SQLITE_BUSY.
- **Composite indexes**: added `(seen_at, file_path)` and `(seen_at, project)` indexes on `file_interactions` for queries that filter by time range and group by path or project.
- **Lock consolidation**: `productivity_score` reduced from 3 separate mutex acquisitions to 1 via internal `_q` query helpers. `weekly_digest` reduced from 12 to 4 locks. Introduced `daily_summary_q`, `context_switch_rate_q`, `get_focus_sessions_in_range_q` static methods that take `&Connection` directly.
- **`PRAGMA optimize` after pruning**: runs after hourly retention pruning to keep query planner statistics fresh.
- **Batch label lookup in interactive search**: replaced 15 per-epoch `get_labels_near` calls (each opening a new DB connection) with a single `get_labels_near_batch` call per text label, then in-memory filtering. Reduces search DB round-trips from ~28 to ~16.
