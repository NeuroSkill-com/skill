// SPDX-License-Identifier: GPL-3.0-only
//! Unit tests for ScreenshotMetrics and MetricsSnapshot.

use skill_screenshots::capture::ScreenshotMetrics;
use std::sync::atomic::Ordering;

#[test]
fn new_metrics_all_zero() {
    let m = ScreenshotMetrics::new();
    let s = m.snapshot();
    assert_eq!(s.captures, 0);
    assert_eq!(s.capture_errors, 0);
    assert_eq!(s.drops, 0);
    assert_eq!(s.embeds, 0);
    assert_eq!(s.embed_errors, 0);
    assert_eq!(s.queue_depth, 0);
    assert_eq!(s.backoff_multiplier, 1); // default backoff = 1
}

#[test]
fn metrics_snapshot_reflects_updates() {
    let m = ScreenshotMetrics::new();
    m.captures.store(42, Ordering::Relaxed);
    m.embeds.store(10, Ordering::Relaxed);
    m.capture_us.store(500, Ordering::Relaxed);
    m.queue_depth.store(3, Ordering::Relaxed);

    let s = m.snapshot();
    assert_eq!(s.captures, 42);
    assert_eq!(s.embeds, 10);
    assert_eq!(s.capture_us, 500);
    assert_eq!(s.queue_depth, 3);
}

#[test]
fn metrics_default_equals_new() {
    let d = ScreenshotMetrics::default();
    let n = ScreenshotMetrics::new();
    assert_eq!(d.snapshot().captures, n.snapshot().captures);
    assert_eq!(d.snapshot().backoff_multiplier, n.snapshot().backoff_multiplier);
}

#[test]
fn metrics_snapshot_serializes_to_json() {
    let m = ScreenshotMetrics::new();
    m.captures.store(5, Ordering::Relaxed);
    let s = m.snapshot();
    let json = serde_json::to_string(&s).expect("serialize");
    assert!(json.contains("\"captures\":5"));
}
