### UI

- **Search mode dropdown**: replaced 4-tab bar with a styled dropdown supporting 7 search modes — Interactive, EEG, Text, Images, Code, Meetings, Brain.
- **Diamond EEG nodes**: EEG epochs rendered as faceted octahedrons (diamonds) in the 3D search graph. Meeting nodes rendered as tetrahedrons (pyramids). Visual distinction by data type.
- **New search modes**: Code (file activity + brain state), Meetings (meeting events + recovery), Brain (flow/fatigue/struggle insights).
- **3D graph cleanup**: fixed event listener leaks (pointerdown, dblclick) on component unmount in InteractiveGraph3D.

### i18n

- Search mode labels (Code, Meetings, Brain) translated in all 9 locales.

### Daemon — Bug Fixes

- **Deadlock in daily-report endpoint**: `daily_brain_report` held the database mutex while calling `productivity_score`, which re-acquires the same mutex. Scoped the lock to drop before the nested call.
- **Deadlock in struggle-predict endpoint**: same pattern in `predict_struggle` — held mutex while calling `get_recent_files`.
- **EEG ghost data after disconnect**: `latest_bands` was not cleared when an EEG device disconnected, causing stale focus/mood values to persist in reports and the activity pipeline.
- **Timezone-wrong period classification**: `daily_brain_report`, `hourly_edit_heatmap`, and `optimal_hours` used `seen_at % 86400` (UTC hour) instead of local hour. Daily report now uses `seen_at - day_start`; heatmap and optimal-hours accept a timezone offset computed server-side.
- **UTC midnight in all clients**: search page, CLI (`cmdActivity`, `cmdBrain`), VS Code extension, and Swift widget all computed `day_start` as UTC midnight instead of local midnight. Fixed to use `Date.setHours(0,0,0,0)` (JS/TS) and `Calendar.startOfDay` (Swift).
- **Widget empty daily-report body**: `DaemonClient.fetchDailyReport()` sent an empty POST body; the endpoint requires `dayStart`. Now sends local midnight.
- **`overall_focus` returned `-0.0`**: always wrapped in `Some` even with no EEG data. Now returns `null` when no focus samples exist.
- **`best_period` arbitrary without EEG**: picked the first SQL result when all `avg_focus` were `None`. Now returns empty string when no EEG data to compare.
- **`weekly_avg_deep_mins` returned `-0.0`**: divided by hardcoded 7 regardless of actual days. Now divides by actual day count, returns `0.0` when empty.
- **Path traversal in `delete_session`**: `csv_path` parameter was passed unsanitized to `fs::remove_file`. Now validates the path is within `skill_dir` via canonicalize + starts_with.
- **Timing-vulnerable token comparison**: default bearer token checks in auth middleware used `==` (variable-time). Switched to `constant_time_eq` for both in-memory and on-disk token paths.
- **Notification text ugly without EEG**: daily report notification showed "Best: . Score: 25. Focus: 0." — now conditionally includes only available fields.
- **Focus session mood averaging**: `build_focus_sessions` used a single counter for both focus and mood samples, inflating mood averages when focus arrived independently. Split into separate counters.
- **EEG mood/focus sample count mismatch in FileSnapshot**: same single-counter bug in the per-interaction EEG accumulator. Split into `eeg_focus_count` and `eeg_mood_count`.
- **Memory leak in terminal_command_end labeling**: `Box::leak` was used to format non-zero exit codes in EEG auto-labeling, permanently leaking memory for every failed command. Replaced with owned `String`.

### Daemon — Performance

- **`open_readonly` for read-only handlers**: added `ActivityStore::open_readonly()` that skips 15+ ALTER TABLE migrations and opens in read-only mode. Switched 38+ HTTP handlers and 3 background tasks from `open` to `open_readonly`.
- **`PRAGMA busy_timeout=5000`**: added to `init_wal_pragmas` so concurrent connections wait up to 5 s instead of failing immediately with SQLITE_BUSY.
- **Composite indexes**: added `(seen_at, file_path)` and `(seen_at, project)` indexes on `file_interactions` for queries that filter by time range and group by path or project.
- **Lock consolidation in `productivity_score`**: reduced from 3 separate mutex acquisitions to 1 via internal `_q` query helpers. Same for `weekly_digest` (12 → 4 locks), with 7× `daily_summary` + `get_focus_sessions_in_range` batched under a single lock.
- **`PRAGMA optimize` after pruning**: runs after hourly retention pruning to keep query planner statistics fresh.
- **Retention pruning for new tables**: added `prune_terminal_commands`, `prune_ai_events`, `prune_zone_switches`, `prune_layout_snapshots` — wired into the hourly maintenance cycle so these tables don't grow unbounded.

### Daemon — Reliability

- **Worker thread panic recovery with auto-restart**: all 4 activity worker threads (poller, input monitor, file watcher, clipboard monitor) now wrapped in `catch_unwind` via `spawn_resilient`. On panic, the worker logs the error and restarts after 5 s. Previous implementation only caught the panic without restarting.
- **Daily report catch-up on late startup**: if the daemon starts after 18:00 local, the daily brain report fires on the first tick instead of waiting up to 1 hour for the hourly check.
- **WAL checkpoint on shutdown**: daemon now runs `PRAGMA optimize` on the activity database during graceful shutdown, ensuring the WAL is checkpointed and the database is clean for next start.

### App

- **401 auto-retry**: daemon HTTP client now retries once with a fresh token on 401 (stale token after daemon restart) instead of failing immediately.

### Daemon — EEG Pipeline

- **Duration-averaged EEG**: `FileSnapshot` now samples EEG focus/mood every 5 s (at each edit-chunk tick) and writes the average to `file_interactions` at finalize, replacing the single snapshot captured at file-switch time. Falls back to the initial snapshot if no samples were collected (`COALESCE`).
