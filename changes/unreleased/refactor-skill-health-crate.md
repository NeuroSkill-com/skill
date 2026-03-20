### Refactor

- **Extract `skill-health` crate**: Moved HealthKit data store from `skill-data::health_store` into its own standalone `skill-health` crate with zero Tauri dependencies. The new crate has 9 unit tests covering sync idempotency, per-table queries, metric type listing, and aggregate summaries. `skill-data` re-exports `skill_health::*` for backward compatibility — no consumer changes needed.
