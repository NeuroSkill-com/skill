// SPDX-License-Identifier: GPL-3.0-only
//! Tests for the HooksLog (hook fire event store).

use skill_data::hooks_log::{HooksLog, HookFireEntry};
use tempfile::tempdir;

#[test]
fn open_creates_db() {
    let dir = tempdir().expect("tmpdir");
    let log = HooksLog::open(dir.path());
    assert!(log.is_some(), "should open successfully");
}

#[test]
fn record_and_query() {
    let dir = tempdir().expect("tmpdir");
    let log = HooksLog::open(dir.path()).expect("open");

    log.record(HookFireEntry {
        triggered_at_utc: 1700000000,
        hook_json: r#"{"name":"test-hook"}"#,
        trigger_json: r#"{"type":"meditation_high"}"#,
        payload_json: r#"{"action":"notify"}"#,
    });

    let rows = log.query(10, 0);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].hook_json.contains("test-hook"));
    assert!(rows[0].trigger_json.contains("meditation_high"));
}

#[test]
fn count_tracks_entries() {
    let dir = tempdir().expect("tmpdir");
    let log = HooksLog::open(dir.path()).expect("open");

    assert_eq!(log.count(), 0);

    log.record(HookFireEntry {
        triggered_at_utc: 1700000001,
        hook_json: r#"{"name":"hook1"}"#,
        trigger_json: r#"{"type":"focus_low"}"#,
        payload_json: r#"{"action":"notify"}"#,
    });
    log.record(HookFireEntry {
        triggered_at_utc: 1700000002,
        hook_json: r#"{"name":"hook2"}"#,
        trigger_json: r#"{"type":"battery_low"}"#,
        payload_json: r#"{"action":"sound"}"#,
    });

    assert_eq!(log.count(), 2);
}

#[test]
fn query_with_offset() {
    let dir = tempdir().expect("tmpdir");
    let log = HooksLog::open(dir.path()).expect("open");

    for i in 0..5 {
        log.record(HookFireEntry {
            triggered_at_utc: 1700000000 + i,
            hook_json: &format!(r#"{{"name":"hook-{i}"}}"#),
            trigger_json: r#"{"type":"test"}"#,
            payload_json: r#"{}"#,
        });
    }

    let page1 = log.query(2, 0);
    let page2 = log.query(2, 2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 2);
    // Pages should have different entries
    assert_ne!(page1[0].id, page2[0].id);
}
