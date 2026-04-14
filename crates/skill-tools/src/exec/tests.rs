// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tests for tool execution sub-modules.

use super::helpers::*;
use super::safety::*;
use super::safety::{clear_bash_edit_hook, request_bash_edit, set_bash_edit_hook};
use super::status::format_status_as_text;
use super::tools_system::exec_date;
use super::tools_web::{exec_web_fetch, exec_web_search};
use super::truncate::*;
use crate::types::LlmToolConfig;
use serde_json::json;
use tokio;

fn mock_llm_tool_config() -> LlmToolConfig {
    let mut config = LlmToolConfig::default();
    config.web_search_provider.backend = "duckduckgo".to_string();
    config.retry.max_retries = 0;
    config.retry.base_delay_ms = 1;
    config
}

#[tokio::test]
async fn test_exec_web_search_missing_query() {
    let args = json!({});
    let config = mock_llm_tool_config();
    let result = exec_web_search(&args, &config).await;
    assert_eq!(result["ok"], false);
    assert_eq!(result["tool"], "web_search");
    assert!(result["error"].as_str().unwrap().contains("missing query"));
}

#[tokio::test]
async fn test_exec_web_search_valid_query() {
    let args = json!({"query": "rust programming"});
    let config = mock_llm_tool_config();
    let result = exec_web_search(&args, &config).await;
    assert_eq!(result["ok"], true);
    assert_eq!(result["tool"], "web_search");
    assert!(result["results"].is_array() || result["compact"].is_string());
}

#[tokio::test]
async fn test_exec_web_fetch_invalid_url() {
    let args = json!({"url": "ftp://example.com"});
    let config = mock_llm_tool_config();
    let result = exec_web_fetch(&args, &config).await;
    assert_eq!(result["ok"], false);
    assert_eq!(result["tool"], "web_fetch");
    assert!(result["error"].as_str().unwrap().contains("http"));
}

#[tokio::test]
async fn test_exec_web_fetch_missing_url() {
    let args = json!({});
    let config = mock_llm_tool_config();
    let result = exec_web_fetch(&args, &config).await;
    assert_eq!(result["ok"], false);
    assert_eq!(result["tool"], "web_fetch");
    assert!(result["error"].as_str().unwrap().contains("http"));
}

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
    assert_eq!(
        truncate_text("\u{1f9e0}\u{1f52c}\u{1f9ec}\u{1f9ea}", 2),
        "\u{1f9e0}\u{1f52c}"
    );
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

// ── bash edit hook ────────────────────────────────────────────────────

/// Tests for the bash-edit hook. Run serially since they share global state.
#[test]
fn bash_edit_hook_lifecycle() {
    use std::sync::Arc;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    // 1. Without a hook set, request_bash_edit returns Some(original)
    clear_bash_edit_hook();
    let result = rt.block_on(request_bash_edit("echo hello"));
    assert_eq!(result, Some("echo hello".to_string()));

    // 2. Hook can modify
    set_bash_edit_hook(Arc::new(|cmd: &str| Some(format!("{} --safe", cmd))));
    let result = rt.block_on(request_bash_edit("rm -rf /tmp/test"));
    assert_eq!(result, Some("rm -rf /tmp/test --safe".to_string()));

    // 3. Hook can cancel
    set_bash_edit_hook(Arc::new(|_cmd: &str| None));
    let result = rt.block_on(request_bash_edit("dangerous command"));
    assert_eq!(result, None);

    // 4. Clearing hook restores passthrough
    clear_bash_edit_hook();
    let result = rt.block_on(request_bash_edit("safe command"));
    assert_eq!(result, Some("safe command".to_string()));
}

// ── retry_with_backoff ────────────────────────────────────────────────

#[test]
fn retry_succeeds_immediately() {
    let result = retry_with_backoff(3, std::time::Duration::from_millis(1), || Ok::<&str, &str>("ok"));
    assert_eq!(result, Ok("ok"));
}

#[test]
fn retry_succeeds_on_second_attempt() {
    let mut attempts = 0u32;
    let result = retry_with_backoff(3, std::time::Duration::from_millis(1), || {
        attempts += 1;
        if attempts < 2 {
            Err("fail")
        } else {
            Ok("ok")
        }
    });
    assert_eq!(result, Ok("ok"));
    assert_eq!(attempts, 2);
}

#[test]
fn retry_exhausts_all_attempts() {
    let mut attempts = 0u32;
    let result = retry_with_backoff(2, std::time::Duration::from_millis(1), || {
        attempts += 1;
        Err::<(), &str>("always fails")
    });
    assert_eq!(result, Err("always fails"));
    assert_eq!(attempts, 3); // initial + 2 retries
}

#[test]
fn retry_zero_retries_runs_once() {
    let mut attempts = 0u32;
    let result = retry_with_backoff(0, std::time::Duration::from_millis(1), || {
        attempts += 1;
        Err::<(), &str>("fail")
    });
    assert_eq!(result, Err("fail"));
    assert_eq!(attempts, 1);
}

// ── enforce_path_integrity ──────────────────────────────────────────────

#[test]
fn enforce_path_integrity_allows_home() {
    let home = dirs::home_dir().unwrap();
    let p = home.join("some/file.txt");
    assert!(enforce_path_integrity(&p).is_ok());
}

#[test]
fn enforce_path_integrity_allows_cwd() {
    let cwd = std::env::current_dir().unwrap();
    let p = cwd.join("test.txt");
    assert!(enforce_path_integrity(&p).is_ok());
}

#[test]
fn enforce_path_integrity_allows_tmp() {
    let tmp = std::env::temp_dir();
    let p = tmp.join("test.txt");
    assert!(enforce_path_integrity(&p).is_ok());
}

#[test]
fn enforce_path_integrity_rejects_outside_roots() {
    // /etc is unlikely to be under home/cwd/tmp
    let p = std::path::PathBuf::from("/etc/passwd");
    std::env::remove_var("SKILL_DISABLE_STRICT_PATH_SAFETY");
    let result = enforce_path_integrity(&p);
    assert!(result.is_err(), "should reject /etc/passwd");
}

#[test]
fn enforce_path_integrity_disabled_by_env() {
    // This test must run alone (env var race). Use a path under /tmp which is
    // always allowed, then verify the env-var bypass *additionally* allows /etc.
    // We test the bypass indirectly: if the var is "1", any path should pass.
    let prev = std::env::var("SKILL_DISABLE_STRICT_PATH_SAFETY").ok();
    std::env::set_var("SKILL_DISABLE_STRICT_PATH_SAFETY", "1");
    // /usr/bin is outside cwd/home/tmp on most systems
    let p = std::path::PathBuf::from("/usr/bin/env");
    let result = enforce_path_integrity(&p);
    // Restore
    match prev {
        Some(v) => std::env::set_var("SKILL_DISABLE_STRICT_PATH_SAFETY", v),
        None => std::env::remove_var("SKILL_DISABLE_STRICT_PATH_SAFETY"),
    }
    assert!(result.is_ok(), "env bypass should allow any path");
}

// ── exec_date ───────────────────────────────────────────────────────────

#[test]
fn exec_date_returns_structured_json() {
    let result = exec_date();
    assert_eq!(result["ok"], true);
    assert_eq!(result["tool"], "date");
    assert!(result["unix"].as_u64().unwrap() > 1700000000);
    assert!(result["unix_ms"].as_u64().unwrap() > 1700000000000u64);
    assert!(result["iso_utc"].as_str().unwrap().contains("T"));
    assert!(result["iso_local"].as_str().is_some());
    assert!(result["timezone"]["offset"].as_str().is_some());
    assert!(result["timezone"]["offset_seconds"].is_number());
}

#[test]
fn exec_date_iso_utc_ends_with_z() {
    let result = exec_date();
    let utc = result["iso_utc"].as_str().unwrap();
    assert!(utc.ends_with('Z'), "expected UTC ISO to end with Z: {utc}");
}

#[test]
fn exec_date_timezone_offset_format() {
    let result = exec_date();
    let offset = result["timezone"]["offset"].as_str().unwrap();
    // Should match +HH:MM or -HH:MM
    assert!(
        offset.len() == 6 && (offset.starts_with('+') || offset.starts_with('-')),
        "unexpected offset format: {offset}"
    );
}
