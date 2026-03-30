# skill-oura

Oura Ring V2 Cloud API integration for NeuroSkill.

Fetches sleep, activity, readiness, heart rate, SpO2, workouts, and mindfulness
data from the [Oura V2 REST API](https://cloud.ouraring.com/v2/docs) and converts
it into the unified `HealthSyncPayload` format so it flows through the same
storage and query pipeline as Apple HealthKit data (`health.sqlite`).

## Supported data

| Oura endpoint       | Health pipeline target                                            |
|----------------------|-------------------------------------------------------------------|
| Sleep (detailed)     | `sleep_samples` + `heart_rate_samples` + `health_metrics` (HRV…) |
| Daily Sleep          | `health_metrics` (oura_sleep_score + contributor sub-scores)      |
| Daily Activity       | `steps_samples` + `health_metrics` (calories, activity score)     |
| Daily Readiness      | `health_metrics` (readiness score, temperature deviation)         |
| Heart Rate           | `heart_rate_samples` (5-min intervals)                            |
| Daily SpO2           | `health_metrics` (spo2)                                           |
| Workouts             | `workouts`                                                        |
| Sessions             | `mindfulness_samples`                                             |

## Usage

```rust
use skill_oura::OuraSync;

let sync = OuraSync::new("your-oura-personal-access-token");
let payload = sync.fetch("2026-03-01", "2026-03-28").unwrap();
// payload is a HealthSyncPayload ready for health_store.sync(&payload)
```
