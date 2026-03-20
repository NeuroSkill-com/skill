// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Cross-platform GPU utilisation and memory reading.
//!
//! # Usage
//!
//! ```rust,ignore
//! if let Some(stats) = skill_gpu::read() {
//!     println!("GPU: {} — {:.0} MB free / {:.0} MB total",
//!              stats.name, stats.free_memory_bytes as f64 / 1e6,
//!              stats.total_memory_bytes as f64 / 1e6);
//! }
//! ```

mod stats;
pub use stats::*;
