// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Apple HealthKit data store — `~/.skill/health.sqlite`.
//!
//! Stores data pushed from a companion iOS app over the HTTP/WS API:
//!
//! * **`sleep_samples`** — sleep analysis entries (asleep, awake, REM, core, deep)
//! * **`workouts`** — workout sessions (type, duration, calories, distance, HR)
//! * **`heart_rate_samples`** — discrete heart-rate readings (bpm + context)
//! * **`steps_samples`** — step-count aggregates over time ranges
//! * **`mindfulness_samples`** — mindful minutes / meditation sessions
//! * **`health_metrics`** — catch-all for any scalar HealthKit quantity
//!   (resting HR, HRV, VO2max, body mass, blood pressure, etc.)
//!
//! # Usage
//!
//! ```rust,ignore
//! let store = skill_health::HealthStore::open(&skill_dir).unwrap();
//!
//! // Sync data from iOS companion app
//! let payload = skill_health::HealthSyncPayload { /* ... */ ..Default::default() };
//! let result = store.sync(&payload);
//!
//! // Query sleep data
//! let sleep = store.query_sleep(start_utc, end_utc, 100);
//!
//! // Get aggregate summary
//! let summary = store.summary(start_utc, end_utc);
//! ```

mod store;

pub use store::*;
