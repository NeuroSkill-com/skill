// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Filesystem and formatting helpers for tool execution.

/// Resolve a path for filesystem tools.  Supports `~` expansion and relative
/// paths (resolved against the user's home directory).
pub fn resolve_tool_path(path: &str) -> std::path::PathBuf {
    let expanded = if path == "~" {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
    } else if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
            .join(rest)
    } else {
        std::path::PathBuf::from(path)
    };

    let raw = if expanded.is_absolute() {
        expanded
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/"))
            .join(expanded)
    };

    // Lexical normalization to collapse ./ and ../ without requiring existence.
    let mut normalized = std::path::PathBuf::new();
    for comp in raw.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let _ = normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn path_within(path: &std::path::Path, root: &std::path::Path) -> bool {
    let p = path.components().collect::<Vec<_>>();
    let r = root.components().collect::<Vec<_>>();
    p.len() >= r.len() && p.iter().zip(r.iter()).all(|(a, b)| a == b)
}

/// Strict path integrity check for file tools.
///
/// Allows paths under: current working directory, home directory, and temp dir.
/// Set `SKILL_DISABLE_STRICT_PATH_SAFETY=1` to disable this guard globally.
pub fn enforce_path_integrity(path: &std::path::Path) -> anyhow::Result<()> {
    if std::env::var("SKILL_DISABLE_STRICT_PATH_SAFETY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/"));
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
    let tmp = std::env::temp_dir();

    if path_within(path, &cwd) || path_within(path, &home) || path_within(path, &tmp) {
        return Ok(());
    }

    anyhow::bail!("path `{}` is outside trusted roots (cwd/home/tmp)", path.display())
}

/// Retry a fallible closure with exponential backoff.
///
/// Retries up to `max_retries` times on failure.  The delay between attempts
/// starts at `base_delay` and doubles each time.  Returns the first `Ok` value
/// or the last `Err` if all attempts fail.
///
/// Designed for network I/O in blocking contexts (e.g. inside
/// `tokio::task::spawn_blocking`).
pub fn retry_with_backoff<T, E, F>(max_retries: u32, base_delay: std::time::Duration, mut f: F) -> Result<T, E>
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
