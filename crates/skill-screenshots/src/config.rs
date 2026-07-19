// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Screenshot configuration — re-exported from `skill-settings`.
//!
//! The canonical [`ScreenshotConfig`] struct lives in `skill-settings` so
//! that other crates can read/write it without pulling in `xcap`/`pipewire`.

pub use skill_settings::ScreenshotConfig;
