// SPDX-License-Identifier: GPL-3.0-only
//! Unit tests for the InterceptStore and network log types.

use skill_headless::{
    InterceptStore, InterceptedRequest, InterceptedResponse, NavigationEvent, NetworkLog,
};

#[test]
fn store_push_and_snapshot() {
    let store = InterceptStore::new();

    store.push_request(InterceptedRequest {
        seq: 1,
        method: "GET".into(),
        url: "https://example.com".into(),
        headers: "{}".into(),
        body: String::new(),
        timestamp_ms: 1000.0,
    });

    store.push_response(InterceptedResponse {
        seq: 1,
        status: 200,
        status_text: "OK".into(),
        headers: "{}".into(),
        body: "<html></html>".into(),
        body_base64: false,
        url: "https://example.com".into(),
        timestamp_ms: 1001.0,
    });

    store.push_navigation(NavigationEvent {
        url: "https://example.com".into(),
        allowed: true,
        timestamp_ms: 999.0,
    });

    let snap = store.snapshot(false);
    assert_eq!(snap.requests.len(), 1);
    assert_eq!(snap.responses.len(), 1);
    assert_eq!(snap.navigations.len(), 1);

    // Non-clearing snapshot should leave data in place
    let snap2 = store.snapshot(false);
    assert_eq!(snap2.requests.len(), 1);
}

#[test]
fn snapshot_with_clear() {
    let store = InterceptStore::new();
    store.push_request(InterceptedRequest {
        seq: 1,
        method: "POST".into(),
        url: "https://api.example.com/data".into(),
        headers: r#"{"content-type":"application/json"}"#.into(),
        body: r#"{"key":"value"}"#.into(),
        timestamp_ms: 2000.0,
    });

    let snap = store.snapshot(true);
    assert_eq!(snap.requests.len(), 1);

    // After clearing, should be empty
    let snap2 = store.snapshot(false);
    assert!(snap2.requests.is_empty());
}

#[test]
fn store_clear() {
    let store = InterceptStore::new();
    store.push_request(InterceptedRequest {
        seq: 1,
        method: "GET".into(),
        url: "https://example.com".into(),
        headers: "{}".into(),
        body: String::new(),
        timestamp_ms: 3000.0,
    });

    store.clear();
    let snap = store.snapshot(false);
    assert!(snap.requests.is_empty());
    assert!(snap.responses.is_empty());
    assert!(snap.navigations.is_empty());
}

#[test]
fn network_log_default_is_empty() {
    let log = NetworkLog::default();
    assert!(log.requests.is_empty());
    assert!(log.responses.is_empty());
    assert!(log.navigations.is_empty());
}

#[test]
fn network_log_serializes_roundtrip() {
    let mut log = NetworkLog::default();
    log.requests.push(InterceptedRequest {
        seq: 42,
        method: "GET".into(),
        url: "https://example.com/page".into(),
        headers: "{}".into(),
        body: String::new(),
        timestamp_ms: 5000.0,
    });
    let json = serde_json::to_string(&log).expect("serialize");
    let deserialized: NetworkLog = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.requests.len(), 1);
    assert_eq!(deserialized.requests[0].seq, 42);
}

// interception_init_script is pub(crate) — tested via internal unit tests

