// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! `skill-data` — pure data, storage, and utility modules for NeuroSkill.
//!
//! This crate contains modules with zero tauri/AppState coupling:
//!
//! - **active_window** — `ActiveWindowInfo` data type
//! - **activity_store** — SQLite activity persistence (windows, input buckets)
//! - **session_csv** — CSV writer for EEG/PPG/metrics recording sessions
//! - **label_store** — SQLite label persistence
//! - **screenshot_store** — SQLite screenshot metadata + embedding store
//! - **hooks_log** — SQLite hook-fire audit log
//! - **gpu_stats** — GPU hardware info queries
//! - **ppg_analysis** — PPG/heart-rate signal analysis
//! - **dnd** — Do Not Disturb platform automation
//! - **device** — BLE device types
//! - **util** — shared utilities (MutexExt, date_dirs, UTC formatters, open_readonly)

pub mod active_window;
pub mod activity_store;
pub mod device;
pub mod dnd;
pub mod error;
pub mod gpu_stats;
pub mod health_store;
pub mod hooks_log;
pub mod label_store;
pub mod oura_sync;
pub mod ppg_analysis;
pub mod screenshot_store;
pub mod session_csv;
#[cfg(feature = "parquet")]
pub mod session_parquet;
pub mod session_paths;
pub mod session_writer;
pub mod util;

pub use error::{SessionError, StoreError};
