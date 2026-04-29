### Bugfixes

- **Duration-averaged EEG**: `FileSnapshot` now samples EEG focus/mood every 5 s (at each edit-chunk tick) and writes the average to `file_interactions` at finalize, replacing the single snapshot captured at file-switch time. Falls back to the initial snapshot if no samples were collected (`COALESCE`).
- **EEG mood/focus sample count mismatch**: both `FileSnapshot` and `build_focus_sessions` used a single counter for focus and mood samples, inflating mood averages when they arrived independently. Split into separate `focus_count` and `mood_count`.
- **EEG ghost data after disconnect**: `latest_bands` was not cleared when an EEG device disconnected, causing stale focus/mood values to persist in reports and the activity pipeline. Now cleared on all 3 disconnect paths (idle timeout, stream end, explicit disconnect).
