// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tests for tool execution sub-modules.

use serde_json::json;
use super::truncate::*;
use super::helpers::*;
use super::safety::*;
use super::status::format_status_as_text;

// ── truncate_text ─────────────────────────────────────────────────────────

#[test]
fn truncate_text_within_limit() {
    assert_eq!(truncate_text("hello", 10), "hello");
}

#[test]
fn truncate_text_at_limit() {
    assert_eq!(truncate_text("hello", 5), "hello");
}

#[test]
fn truncate_text_over_limit() {
    assert_eq!(truncate_text("hello world", 5), "hello");
}

#[test]
fn truncate_text_empty() {
    assert_eq!(truncate_text("", 5), "");
}

#[test]
fn truncate_text_unicode() {
    // Each emoji is one char
    assert_eq!(truncate_text("\u{1f9e0}\u{1f52c}\u{1f9ec}\u{1f9ea}", 2), "\u{1f9e0}\u{1f52c}");
}

// ── truncate_tool_output (tail) ───────────────────────────────────────

#[test]
fn truncate_output_no_truncation() {
    let out = truncate_tool_output("line1\nline2\nline3", 10, 1000);
    assert!(!out.was_truncated);
    assert_eq!(out.total_lines, 3);
    assert_eq!(out.output_lines, 3);
    assert_eq!(out.text, "line1\nline2\nline3");
}

#[test]
fn truncate_output_by_lines() {
    let content = (0..100).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
    let out = truncate_tool_output(&content, 5, 100_000);
    assert!(out.was_truncated);
    assert_eq!(out.total_lines, 100);
    assert_eq!(out.output_lines, 5);
    // Should keep the LAST 5 lines
    assert!(out.text.contains("line99"));
    assert!(out.text.contains("line95"));
    assert!(!out.text.contains("line0"));
}

#[test]
fn truncate_output_by_bytes() {
    let content = (0..100).map(|i| format!("line{i:03}")).collect::<Vec<_>>().join("\n");
    let out = truncate_tool_output(&content, 1000, 50);
    assert!(out.was_truncated);
    // Should have truncated to fit under 50 bytes
    assert!(out.text.len() <= 50);
}

// ── truncate_tool_output_head ─────────────────────────────────────────

#[test]
fn truncate_head_no_truncation() {
    let out = truncate_tool_output_head("a\nb\nc", 10, 1000);
    assert!(!out.was_truncated);
    assert_eq!(out.text, "a\nb\nc");
}

#[test]
fn truncate_head_by_lines() {
    let content = (0..100).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
    let out = truncate_tool_output_head(&content, 5, 100_000);
    assert!(out.was_truncated);
    assert_eq!(out.output_lines, 5);
    // Should keep the FIRST 5 lines
    assert!(out.text.contains("line0"));
    assert!(out.text.contains("line4"));
    assert!(!out.text.contains("line99"));
}

#[test]
fn truncate_head_by_bytes() {
    let content = (0..100).map(|i| format!("line{i:03}")).collect::<Vec<_>>().join("\n");
    let out = truncate_tool_output_head(&content, 1000, 50);
    assert!(out.was_truncated);
    assert!(out.text.len() <= 50);
    assert!(out.text.starts_with("line000"));
}

// ── resolve_tool_path ─────────────────────────────────────────────────

#[test]
fn resolve_absolute_path() {
    let p = resolve_tool_path("/tmp/test.txt");
    assert_eq!(p, std::path::PathBuf::from("/tmp/test.txt"));
}

#[test]
fn resolve_tilde() {
    let p = resolve_tool_path("~");
    assert!(p.is_absolute());
    // Should be the home directory
    assert_eq!(p, dirs::home_dir().unwrap());
}

#[test]
fn resolve_tilde_slash() {
    let p = resolve_tool_path("~/Documents/file.txt");
    assert!(p.is_absolute());
    assert!(p.ends_with("Documents/file.txt"));
}

#[test]
fn resolve_relative_path() {
    let p = resolve_tool_path("some/relative/path");
    assert!(p.is_absolute()); // Should be resolved to absolute via home
}

// ── check_bash_safety ─────────────────────────────────────────────────

#[test]
fn bash_safety_safe_command() {
    assert!(check_bash_safety("ls -la").is_none());
    assert!(check_bash_safety("echo hello").is_none());
    assert!(check_bash_safety("cat file.txt").is_none());
    assert!(check_bash_safety("grep pattern file").is_none());
}

#[test]
fn bash_safety_dangerous_rm() {
    assert!(check_bash_safety("rm -rf /").is_some());
    assert!(check_bash_safety("rm file.txt").is_some());
}

#[test]
fn bash_safety_dangerous_sudo() {
    assert!(check_bash_safety("sudo apt install").is_some());
}

#[test]
fn bash_safety_dangerous_dd() {
    assert!(check_bash_safety("dd if=/dev/zero of=/dev/sda").is_some());
}

#[test]
fn bash_safety_dangerous_shutdown() {
    assert!(check_bash_safety("shutdown -h now").is_some());
    assert!(check_bash_safety("reboot").is_some());
}

#[test]
fn bash_safety_case_insensitive() {
    assert!(check_bash_safety("SUDO apt install").is_some());
    assert!(check_bash_safety("Rm -rf /").is_some());
}

#[test]
fn bash_safety_no_false_positive_on_skill() {
    // "skill" contains "kill" but should NOT trigger the "kill " pattern.
    assert!(check_bash_safety("skill --help").is_none());
    assert!(check_bash_safety("skill status").is_none());
    assert!(check_bash_safety("neuroskill-status").is_none());
    // Actual "kill" commands should still be caught.
    assert!(check_bash_safety("kill 1234").is_some());
    assert!(check_bash_safety("killall firefox").is_some());
    assert!(check_bash_safety("pkill node").is_some());
    // "kill" after a pipe/semicolon should be caught.
    assert!(check_bash_safety("echo hi; kill 1234").is_some());
    assert!(check_bash_safety("ps aux | kill 42").is_some());
}

// ── check_path_safety ─────────────────────────────────────────────────

#[test]
fn path_safety_safe() {
    assert!(check_path_safety(std::path::Path::new("/home/user/file.txt")).is_none());
    assert!(check_path_safety(std::path::Path::new("/tmp/test")).is_none());
}

#[test]
fn path_safety_sensitive() {
    assert!(check_path_safety(std::path::Path::new("/etc/passwd")).is_some());
    assert!(check_path_safety(std::path::Path::new("/boot/vmlinuz")).is_some());
    assert!(check_path_safety(std::path::Path::new("/usr/bin/ls")).is_some());
    assert!(check_path_safety(std::path::Path::new("/sys/class")).is_some());
}

// ── format_utc_offset ─────────────────────────────────────────────────

#[test]
fn utc_offset_positive() {
    assert_eq!(format_utc_offset(3600), "+01:00");
    assert_eq!(format_utc_offset(19800), "+05:30"); // India
}

#[test]
fn utc_offset_negative() {
    assert_eq!(format_utc_offset(-18000), "-05:00"); // EST
    assert_eq!(format_utc_offset(-28800), "-08:00"); // PST
}

#[test]
fn utc_offset_zero() {
    assert_eq!(format_utc_offset(0), "+00:00");
}

// ── format_status_as_text ─────────────────────────────────────────────

#[test]
fn status_text_disconnected_device() {
    let v = json!({
        "device": { "state": "disconnected", "connected": false, "streaming": false },
        "session": {},
        "embeddings": { "today": 10, "total": 500, "recording_days": 30, "encoder_loaded": true },
        "labels": { "total": 42, "embedded": 38, "recent": [], "top_all_time": [], "top_7d": [], "top_24h": [] },
        "apps": { "top_all_time": [], "top_7d": [], "top_24h": [] },
        "screenshots": { "total": 0, "with_embedding": 0, "with_ocr": 0, "with_ocr_embedding": 0 },
        "signal_quality": [],
        "scores": null,
        "hooks": { "total": 2, "enabled": 1, "latest_trigger": null },
        "sleep": { "total_epochs": 0 },
        "history": null,
        "calibration": { "last_calibration_utc": null },
    });
    let text = format_status_as_text(&v);
    assert!(text.contains("# Device"));
    assert!(text.contains("disconnected"));
    assert!(text.contains("# EEG Embeddings"));
    assert!(text.contains("Today: 10"));
    assert!(text.contains("Total: 42"));
    assert!(text.contains("With text embeddings: 38"));
    assert!(text.contains("# Hooks"));
    // No screenshots section when total=0
    assert!(!text.contains("# Screenshots"));
}

#[test]
fn status_text_connected_with_scores() {
    let v = json!({
        "device": {
            "state": "connected", "connected": true, "streaming": true,
            "name": "Muse 2", "battery": 85.0,
            "sample_count": 12345, "ppg_sample_count": 500,
        },
        "session": { "duration_secs": 125 },
        "embeddings": { "today": 5, "total": 100, "recording_days": 10, "encoder_loaded": true },
        "labels": {
            "total": 20, "embedded": 15,
            "recent": [{"text": "focus"}, {"text": "relax"}],
            "top_all_time": [{"text": "focus", "count": 8}, {"text": "relax", "count": 5}],
            "top_7d": [], "top_24h": [],
        },
        "apps": {
            "top_all_time": [{"app_name": "Firefox", "switches": 50}],
            "top_7d": [], "top_24h": [],
        },
        "screenshots": {
            "total": 200, "with_embedding": 180, "with_ocr": 150, "with_ocr_embedding": 140,
            "top_apps_all_time": [{"app_name": "Firefox", "count": 80}],
            "top_apps_24h": [],
        },
        "signal_quality": [],
        "scores": {
            "meditation": 72.5, "relaxation": 65.3, "engagement": 55.0,
            "bands": { "rel_delta": 0.25, "rel_theta": 0.15, "rel_alpha": 0.30, "rel_beta": 0.20, "rel_gamma": 0.10 },
        },
        "hooks": { "total": 0, "enabled": 0, "latest_trigger": null },
        "sleep": { "total_epochs": 0 },
        "history": { "total_sessions": 15, "total_hours": 42.5, "streak_days": 7 },
        "calibration": { "last_calibration_utc": null },
    });
    let text = format_status_as_text(&v);
    assert!(text.contains("Muse 2"));
    assert!(text.contains("Streaming: yes"));
    assert!(text.contains("Battery: 85%"));
    assert!(text.contains("PPG samples: 500"));
    assert!(text.contains("Duration: 2m 5s"));
    assert!(text.contains("Recent: focus, relax"));
    assert!(text.contains("Top labels (all time): focus (8x), relax (5x)"));
    assert!(text.contains("# Most Used Apps"));
    assert!(text.contains("Firefox (50x)"));
    assert!(text.contains("# Screenshots"));
    assert!(text.contains("With OCR text: 150"));
    assert!(text.contains("Meditation: 72.5"));
    assert!(text.contains("Bands: Delta: 0.250"));
    assert!(text.contains("Total sessions: 15"));
}
