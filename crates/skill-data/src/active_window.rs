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
    /// OS-reported path of the frontmost document, when the application
    /// exposes it (e.g. via macOS AppleScript).  `None` when unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_path: Option<String>,
    /// Unix-second timestamp at which this window became active.
    pub activated_at: u64,
    /// Page title extracted from browser window titles (e.g. "GitHub - Pull Request #123").
    /// `None` when the active app is not a browser or title extraction fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_title: Option<String>,
}
