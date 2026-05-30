// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com

/// Conditional log line for a subsystem (`log_enabled` + `write_log` function paths).
#[macro_export]
macro_rules! subsystem_log {
    ($log_enabled:path, $write_log:path, $tag:expr, $($arg:tt)*) => {
        if $log_enabled() {
            $write_log($tag, &format!($($arg)*));
        }
    };
}
