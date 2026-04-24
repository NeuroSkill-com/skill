### Bugfixes

- **Deadlock in daily-report endpoint**: `daily_brain_report` held the database mutex while calling `productivity_score`, which re-acquires the same mutex. Scoped the lock to drop before the nested call.
- **Deadlock in struggle-predict endpoint**: same pattern in `predict_struggle` — held mutex while calling `get_recent_files`.
- **`overall_focus` returned `-0.0`**: always wrapped in `Some` even with no EEG data. Now returns `null` when no focus samples exist.
- **`best_period` arbitrary without EEG**: picked the first SQL result when all `avg_focus` were `None`. Now returns empty string when no EEG data to compare.
- **`weekly_avg_deep_mins` returned `-0.0`**: divided by hardcoded 7 regardless of actual days. Now divides by actual day count, returns `0.0` when empty.
- **Notification text ugly without EEG**: daily report notification showed "Best: . Score: 25. Focus: 0." — now conditionally includes only available fields.
- **Brain endpoints return HTTP 200 on errors**: all 18 brain handlers silently returned `null` with 200 OK when the activity store was offline or a query failed. Refactored to use `run_query` helper that returns 503 (db_unavailable) or 500 (task_error) with structured `ApiError` JSON.
