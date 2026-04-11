// SPDX-License-Identifier: GPL-3.0-only
// Tests for skill-tools/src/log.rs

use skill_tools::log;
use std::sync::{Arc, Mutex};

#[test]
fn test_log_enabled_toggle() {
    log::reset_log_callback_for_test();
    log::set_log_enabled(false);
    assert!(!log::log_enabled());
    log::set_log_enabled(true);
    assert!(log::log_enabled());
}

#[test]
fn test_set_log_callback_and_write_log() {
    log::reset_log_callback_for_test();
    let received: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(vec![]));
    let received_clone = received.clone();
    // Only the first set_log_callback call is honored
    log::set_log_callback(move |tag, msg| {
        received_clone.lock().unwrap().push((tag.to_string(), msg.to_string()));
    });
    log::set_log_enabled(true);
    log::write_log("test", "hello");
    let logs = received.lock().unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0], ("test".to_string(), "hello".to_string()));
}

#[test]
fn test_write_log_disabled() {
    log::reset_log_callback_for_test();
    let received: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(vec![]));
    let received_clone = received.clone();
    log::set_log_callback(move |tag, msg| {
        received_clone.lock().unwrap().push((tag.to_string(), msg.to_string()));
    });
    log::set_log_enabled(false);
    log::write_log("test", "should not log");
    let logs = received.lock().unwrap();
    // Should not log when disabled
    assert!(logs.is_empty());
}
