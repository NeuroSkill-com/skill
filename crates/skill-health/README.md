# skill-health

Apple HealthKit data store for NeuroSkill.

Stores data pushed from a companion iOS app over the HTTP/WS API into a local SQLite database (`health.sqlite`).

## Tables

| Table | Description |
|---|---|
| `sleep_samples` | Sleep analysis (InBed, Asleep, Awake, REM, Core, Deep) |
| `workouts` | Workout sessions (type, duration, calories, distance, HR) |
| `heart_rate_samples` | Discrete heart-rate readings (bpm + context) |
| `steps_samples` | Step-count aggregates over time ranges |
| `mindfulness_samples` | Mindful minutes / meditation sessions |
| `health_metrics` | Catch-all for scalar HealthKit quantities (resting HR, HRV, VO2max, etc.) |

## API

```rust
let store = skill_health::HealthStore::open(&skill_dir).unwrap();

// Idempotent batch sync from iOS companion
let result = store.sync(&payload);

// Query by type and time range
let sleep = store.query_sleep(start_utc, end_utc, 100);
let hr    = store.query_heart_rate(start_utc, end_utc, 500);

// Aggregate summary
let summary = store.summary(start_utc, end_utc);

// List available metric types
let types = store.list_metric_types();
```

## Dependencies

- `rusqlite` (bundled SQLite)
- `serde` / `serde_json`

Zero Tauri dependencies — pure Rust library.
