# skill-history

Session history, metrics, time-series, sleep staging, and cross-session analysis.

Pure library crate — **zero Tauri dependencies**. Thin IPC wrappers live in
`src-tauri/src/{history_cmds,session_analysis}.rs`.

## Modules

| Module | Description |
|---|---|
| `lib.rs` | Session listing, type definitions, raw metrics CSV parsing |
| `cache.rs` | Disk-cached metrics, downsampling, sleep staging, cross-session analysis |

## API

### Session listing

| Function | Description |
|---|---|
| `list_session_days(skill_dir)` | Return `YYYYMMDD` day dirs (newest first) that contain sessions |
| `list_sessions_for_day(day, skill_dir, label_store)` | Load all sessions for a single day, hydrate labels |
| `delete_session(csv_path)` | Remove CSV + JSON sidecar + metrics cache files |
| `find_session_csv_for_timestamp(skill_dir, ts)` | Find the session CSV closest to a timestamp |
| `list_embedding_sessions(skill_dir)` | Discover recording sessions from embedding databases |
| `get_history_stats(skill_dir)` | Aggregate stats: total sessions, hours, week-over-week |

### Metrics & time-series

| Function | Description |
|---|---|
| `load_metrics_csv(csv_path)` | Parse `_metrics.csv` → `CsvMetricsResult` (summary + timeseries) |
| `load_csv_metrics_cached(csv_path)` | Same as above but with `_metrics_cache.json` disk cache |
| `get_day_metrics_batch(paths, max_points)` | Batch-load + downsample for multiple sessions |
| `downsample_timeseries(ts, max)` | Reduce timeseries to ≤ `max` points (stride-based) |
| `get_session_metrics(skill_dir, start, end)` | Aggregated metrics from SQLite embedding databases |
| `get_session_timeseries(skill_dir, start, end)` | Per-epoch timeseries from SQLite |

### Sleep staging

| Function | Description |
|---|---|
| `get_sleep_stages(skill_dir, start, end)` | Classify epochs into Wake/N1/N2/N3/REM stages |

### Analysis

| Function | Description |
|---|---|
| `compute_compare_insights(skill_dir, …)` | A/B session comparison: per-metric stats, deltas, trends |
| `analyze_sleep_stages(stages)` | Derived sleep quality: efficiency, latency, bouts |
| `analyze_search_results(result)` | Search result insights: distance stats, temporal distribution |
| `compute_status_history(skill_dir, now, sessions)` | Recording history: totals, streak, today vs 7-day average |

## Types

- `SessionEntry` — one recording session (from JSON sidecar or orphaned CSV)
- `SessionMetrics` — aggregated band powers, scores, PPG vitals, composites
- `EpochRow` — single epoch with all metrics fields
- `CsvMetricsResult` — summary + timeseries from `_metrics.csv`
- `SleepStages` / `SleepEpoch` / `SleepSummary` — sleep classification
- `HistoryStats` — aggregate session/time statistics
- `EmbeddingSession` — contiguous recording range from embedding timestamps
