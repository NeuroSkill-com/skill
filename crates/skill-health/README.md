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
| `location_samples` | GPS fixes from CoreLocation (WGS-84 lat/lon, altitude, accuracy, speed, course) |

## API

```rust
let store = skill_health::HealthStore::open(&skill_dir).unwrap();

// Idempotent batch sync from iOS companion
let result = store.sync(&payload);

// Query by type and time range
let sleep    = store.query_sleep(start_utc, end_utc, 100);
let hr       = store.query_heart_rate(start_utc, end_utc, 500);
let location = store.query_location(start_utc, end_utc, 1000);

// Aggregate summary
let summary = store.summary(start_utc, end_utc);

// List available metric types
let types = store.list_metric_types();
```

## GPS location

The `location` array in `HealthSyncPayload` accepts `LocationSample` objects from iOS CoreLocation. All fields follow `CLLocation` conventions:

| Field | Type | Description |
|---|---|---|
| `source_id` | `String` | Device identifier (e.g. `"iphone"`) |
| `timestamp` | `i64` | UTC unix seconds |
| `latitude` | `f64` | WGS-84 degrees |
| `longitude` | `f64` | WGS-84 degrees |
| `altitude` | `f64?` | Metres above sea level |
| `horizontal_accuracy` | `f64?` | Metres; negative = invalid |
| `vertical_accuracy` | `f64?` | Metres; negative = invalid |
| `speed` | `f64?` | m/s; negative = invalid |
| `course` | `f64?` | Degrees clockwise from true north; negative = invalid |

Fixes are deduplicated by `(source_id, timestamp)` — the same sync payload can be sent multiple times safely.

## Dependencies

- `rusqlite` (bundled SQLite)
- `serde` / `serde_json`

Zero Tauri dependencies — pure Rust library.
