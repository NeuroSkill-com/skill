### Reliability

- **Worker thread panic recovery with auto-restart**: all 4 activity worker threads (poller, input monitor, file watcher, clipboard monitor) now wrapped in `catch_unwind` via `spawn_resilient`. On panic, the worker logs the error and restarts after 5 s with freshly cloned `AppState` and `Arc<ActivityStore>`.
- **osascript timeout**: all `osascript` calls (active window poll, secondary windows, clipboard monitor) now use a 3-second timeout via `run_osascript` helper. Previously, a hung app could block the poller thread indefinitely, silently killing all activity tracking.
- **Daily report catch-up on late startup**: if the daemon starts after 18:00 local, the daily brain report fires on the first tick instead of waiting up to 1 hour for the hourly check.
- **WAL checkpoint on shutdown**: daemon now runs `PRAGMA optimize` on the activity database during graceful shutdown, ensuring the WAL is checkpointed and the database is clean for next start.
- **Missing WAL pragmas on EEG embedding store**: `day_store.rs` opened connections without `init_wal_pragmas`, missing both WAL mode and `busy_timeout`.
- **Retention pruning for new tables**: added `prune_terminal_commands`, `prune_ai_events`, `prune_zone_switches`, `prune_layout_snapshots` — wired into the hourly maintenance cycle so these tables don't grow unbounded.
- **TOCTOU race in calibration settings**: concurrent profile CRUD could lose writes. Added `modify_settings_blocking()` helper that runs under a global mutex on tokio's blocking thread pool — serializes read-modify-write without blocking the async runtime.
