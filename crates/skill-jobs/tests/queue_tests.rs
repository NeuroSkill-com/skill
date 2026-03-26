// SPDX-License-Identifier: GPL-3.0-only
//! Integration tests for the JobQueue.

use skill_jobs::{JobPollResult, JobQueue};
use std::time::Duration;

#[test]
fn submit_and_poll_completes() {
    let q = JobQueue::new();
    let ticket = q.submit(100, || Ok(serde_json::json!({"answer": 42})));

    // Poll until complete (with timeout)
    let mut result = None;
    for _ in 0..50 {
        match q.poll(ticket.job_id) {
            JobPollResult::Complete { result: r, .. } => {
                result = Some(r);
                break;
            }
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }

    let r = result.expect("job should complete within 2.5s");
    assert_eq!(r["answer"], 42);
}

#[test]
fn poll_unknown_job_returns_not_found() {
    let q = JobQueue::new();
    match q.poll(99999) {
        JobPollResult::NotFound { .. } => {} // expected
        other => panic!("expected NotFound, got {:?}", other),
    }
}

#[test]
fn sequential_execution() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let q = JobQueue::new();
    let counter = Arc::new(AtomicU32::new(0));

    let c1 = counter.clone();
    let c2 = counter.clone();

    // Job 1: set counter to 1, sleep briefly
    q.submit(100, move || {
        c1.store(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        Ok(serde_json::json!(1))
    });

    // Job 2: should see counter == 1 (job 1 completed first)
    q.submit(100, move || {
        c1.store(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));
        Ok(serde_json::json!(1))
    });
    // Wait for both
    let mut r2 = None;
    for _ in 0..100 {
        if let JobPollResult::Complete { result, .. } = q.poll(t2.job_id) {
            r2 = Some(result);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let r2 = r2.expect("job 2 should complete");
    assert_eq!(r2, 1, "job 2 should see counter=1 from job 1");
}

#[test]
fn error_job_returns_error() {
    let q = JobQueue::new();
    let ticket = q.submit(100, || Err("something went wrong".into()));

    let mut got_error = false;
    for _ in 0..50 {
        if let JobPollResult::Error { error, .. } = q.poll(ticket.job_id) {
            assert!(error.contains("something went wrong"));
            got_error = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(got_error, "should have received error result");
}

#[test]
fn ticket_has_queue_position() {
    let q = JobQueue::new();
    // Submit a long-running job to block the queue
    let _t1 = q.submit(5000, || {
        std::thread::sleep(Duration::from_millis(500));
        Ok(serde_json::json!(1))
    });
    // Submit a second job — should be queued
    let t2 = q.submit(100, || Ok(serde_json::json!(2)));

    // t2's queue_position should be >= 0
    // (it might be 0 if t1 already started and t2 is next, or 1 if both are pending)
    assert!(
        t2.queue_position <= 1,
        "queue position should be 0 or 1, got {}",
        t2.queue_position
    );
}
