// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Active-window data type — shared across the workspace.

use serde::{Deserialize, Serialize};

/// Everything we know about the currently active window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActiveWindowInfo {
    /// Display name of the application (e.g. `"Safari"`, `"code"`).
    pub app_name: String,
    /// Filesystem path to the application bundle / executable.
    /// Empty string when the OS doesn't provide it.
    pub app_path: String,
    /// Title of the focused window / document (empty if unavailable).
    pub window_title: String,
    /// Unix-second timestamp at which this window became active.
    pub activated_at: u64,
}
