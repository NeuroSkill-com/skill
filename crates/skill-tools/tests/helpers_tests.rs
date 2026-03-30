// SPDX-License-Identifier: GPL-3.0-only
//! Tests for tool execution helpers — path resolution, retry, UTC formatting.
#![allow(clippy::unwrap_used)]

use skill_tools::exec::resolve_tool_path;
use std::time::Duration;

// ── resolve_tool_path ────────────────────────────────────────────────────────

#[test]
fn resolve_absolute_path_unchanged() {
    let p = resolve_tool_path("/usr/bin/ls");
    assert_eq!(p.to_str().unwrap(), "/usr/bin/ls");
}

#[test]
fn resolve_tilde_expands_to_home() {
    let p = resolve_tool_path("~");
    assert!(p.is_absolute());
    // Should be the actual home directory, not literally "~"
    assert_ne!(p.to_str().unwrap(), "~");
}

#[test]
fn resolve_tilde_slash_expands() {
    let p = resolve_tool_path("~/Documents/file.txt");
    assert!(p.is_absolute());
    assert!(p.to_str().unwrap().ends_with("Documents/file.txt"));
    assert!(!p.to_str().unwrap().starts_with("~"));
}

#[test]
fn resolve_relative_becomes_absolute() {
    let p = resolve_tool_path("some/relative/path.txt");
    assert!(p.is_absolute());
    assert!(p.to_str().unwrap().ends_with("some/relative/path.txt"));
}

// ── retry_with_backoff ───────────────────────────────────────────────────────

#[test]
fn retry_succeeds_first_try() {
    let result = skill_tools::exec::retry_with_backoff(3, Duration::from_millis(1), || Ok::<_, &str>(42));
    assert_eq!(result.unwrap(), 42);
}

#[test]
fn retry_succeeds_after_failures() {
    let mut attempts = 0;
    let result = skill_tools::exec::retry_with_backoff(3, Duration::from_millis(1), || {
        attempts += 1;
        if attempts < 3 {
            Err("not yet")
        } else {
            Ok(99)
        }
    });
    assert_eq!(result.unwrap(), 99);
    assert_eq!(attempts, 3);
}

#[test]
fn retry_exhausts_all_attempts() {
    let mut attempts = 0;
    let result = skill_tools::exec::retry_with_backoff(2, Duration::from_millis(1), || {
        attempts += 1;
        Err::<(), _>("always fails")
    });
    assert!(result.is_err());
    assert_eq!(attempts, 3); // initial + 2 retries
}
