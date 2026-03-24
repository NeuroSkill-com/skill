// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Filesystem and formatting helpers for tool execution.

/// Resolve a path for filesystem tools.  Supports `~` expansion and relative
/// paths (resolved against the user's home directory).
pub fn resolve_tool_path(path: &str) -> std::path::PathBuf {
    let expanded = if path == "~" {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
    } else if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")).join(rest)
    } else {
        std::path::PathBuf::from(path)
    };

    if expanded.is_absolute() {
        expanded
    } else {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")).join(expanded)
    }
}

/// Retry a fallible closure with exponential backoff.
///
/// Retries up to `max_retries` times on failure.  The delay between attempts
/// starts at `base_delay` and doubles each time.  Returns the first `Ok` value
/// or the last `Err` if all attempts fail.
///
/// Designed for network I/O in blocking contexts (e.g. inside
/// `tokio::task::spawn_blocking`).
pub fn retry_with_backoff<T, E, F>(
    max_retries: u32,
    base_delay: std::time::Duration,
    mut f: F,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut last_err: Option<E> = None;
    for attempt in 0..=max_retries {
        match f() {
            Ok(val) => return Ok(val),
            Err(e) => {
                last_err = Some(e);
                if attempt < max_retries {
                    let delay = base_delay * 2u32.saturating_pow(attempt);
                    std::thread::sleep(delay);
                }
            }
        }
    }
    Err(last_err.expect("at least one attempt was made"))
}

/// Format a UTC offset in seconds as `+HH:MM` / `-HH:MM`.
pub(crate) fn format_utc_offset(offset_seconds: i32) -> String {
    let sign = if offset_seconds >= 0 { '+' } else { '-' };
    let total = offset_seconds.unsigned_abs();
    let hours = total / 3600;
    let mins = (total % 3600) / 60;
    format!("{sign}{hours:02}:{mins:02}")
}
