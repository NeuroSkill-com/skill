### Refactor

- **Extract `skill-history` crate**: Moved all session history, metrics, time-series, sleep staging, and analysis logic from `src-tauri/src/{history_cmds,session_analysis}.rs` into a new `crates/skill-history` workspace crate with zero Tauri dependencies. The Tauri files now contain only thin async IPC wrappers that delegate to `skill_history::*` and run on `spawn_blocking` threads. Types (`SessionEntry`, `SessionMetrics`, `EpochRow`, `CsvMetricsResult`, `SleepStages`, `HistoryStats`, `EmbeddingSession`) and all pure functions (`list_sessions_for_day`, `load_metrics_csv`, `get_session_metrics`, `get_sleep_stages`, `compute_compare_insights`, `analyze_sleep_stages`, `analyze_search_results`, `compute_status_history`, etc.) are now public API in the crate.

### Performance

- **Async history commands**: Converted `list_session_days`, `list_sessions_for_day`, `get_session_metrics`, `get_session_timeseries`, `get_csv_metrics`, `get_day_metrics_batch`, and `get_sleep_stages` from synchronous Tauri commands to async commands using `tokio::task::spawn_blocking`, preventing UI thread blocking during heavy file I/O and CSV/SQLite parsing.
