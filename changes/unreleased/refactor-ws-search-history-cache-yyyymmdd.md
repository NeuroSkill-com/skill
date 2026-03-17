### Refactor

- **Extract `ws_commands/search.rs` (525 lines)**: Moved search_labels, search, compare, session_metrics, and interactive_search handlers out of `ws_commands/mod.rs`, reducing it from 1,687 to 1,162 lines.

- **Extract `skill-history/cache.rs` (751 lines)**: Moved disk cache, metrics computation, sleep staging, and batch loading out of `skill-history/lib.rs`, reducing it from 1,384 to 654 lines.

- **Deduplicate `yyyymmdd_utc()`**: Replaced the 29-line hand-rolled calendar calculation in `helpers.rs` with a one-line delegation to the canonical `skill_data::util::yyyymmdd_utc()`.
