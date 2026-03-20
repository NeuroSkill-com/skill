// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Re-export from the standalone `skill-health` crate.
// Kept for backward compatibility so `skill_data::health_store::*` continues
// to work across the codebase without updating every import path.

pub use skill_health::*;
