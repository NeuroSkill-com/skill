// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Re-export from the standalone `skill-gpu` crate.
// Kept for backward compatibility so `skill_data::gpu_stats::*` continues
// to work across the codebase without updating every import path.

pub use skill_gpu::*;
