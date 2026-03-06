// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Sequential job queue for expensive compute tasks (e.g. UMAP).
//!
//! Only one job runs at a time.  Callers receive a `JobTicket` with an
//! estimated completion time.  The frontend can poll for results by job id.
//!
//! ## Design
//!
//! - `JobQueue` holds a `VecDeque` of pending jobs and an `Arc<Mutex>` result
//!   map.  A dedicated background thread drains the queue serially.
//! - When a new job is submitted, it returns immediately with the ticket.
//! - The frontend polls `poll_job` with the ticket id to retrieve the result
//!   once it's ready.
//! - Old completed results are pruned after 5 minutes.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use crate::MutexExt;

// ── Public types ──────────────────────────────────────────────────────────────

/// Ticket returned immediately when a job is enqueued.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobTicket {
    /// Unique job identifier (monotonic u64).
    pub job_id: u64,
    /// Estimated unix-second UTC when the result should be ready.
    pub estimated_ready_utc: u64,
    /// Number of jobs ahead in the queue (0 = running now).
    pub queue_position: usize,
    /// Estimated seconds until result is available.
    pub estimated_secs: u64,
}

/// Live progress info for a running job.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct JobProgress {
    /// Current epoch (0-indexed).
    pub epoch: usize,
    /// Total epochs configured.
    pub total_epochs: usize,
    /// Current loss value.
    pub loss: f64,
    /// Best loss seen so far.
    pub best_loss: f64,
    /// Wall-clock seconds since training started.
    pub elapsed_secs: f64,
    /// Average milliseconds per epoch.
    pub epoch_ms: f64,
}

/// Result of polling a job.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum JobPollResult {
    /// Job is still pending or running.
    #[serde(rename = "pending")]
    Pending {
        job_id: u64,
        queue_position: usize,
        estimated_secs: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<JobProgress>,
    },
    /// Job completed successfully.
    #[serde(rename = "complete")]
    Complete {
        job_id: u64,
        result: serde_json::Value,
        elapsed_ms: u64,
    },
    /// Job failed.
    #[serde(rename = "error")]
    Error {
        job_id: u64,
        error: String,
    },
    /// Job ID not found (expired or invalid).
    #[serde(rename = "not_found")]
    NotFound { job_id: u64 },
}

// ── Internal ──────────────────────────────────────────────────────────────────

type JobFn = Box<dyn FnOnce() -> Result<serde_json::Value, String> + Send + 'static>;

struct PendingJob {
    id:            u64,
    work:          JobFn,
    estimated_ms:  u64,
}

struct CompletedJob {
    result:     Result<serde_json::Value, String>,
    elapsed_ms: u64,
    completed:  Instant,
}

struct Inner {
    queue:       VecDeque<PendingJob>,
    next_id:     u64,
    /// Average job duration (EMA) for time estimation.
    avg_job_ms:  f64,
    /// The job currently being executed (popped from queue, not yet in results).
    running_id:  Option<u64>,
}

/// Thread-safe job queue.  Create one at app startup and share via Tauri state.
pub struct JobQueue {
    inner:    Mutex<Inner>,
    results:  Arc<Mutex<HashMap<u64, CompletedJob>>>,
    progress: Arc<Mutex<HashMap<u64, JobProgress>>>,
    condvar:  Condvar,
}

impl JobQueue {
    pub fn new() -> Arc<Self> {
        let q = Arc::new(Self {
            inner: Mutex::new(Inner {
                queue:      VecDeque::new(),
                next_id:    1,
                avg_job_ms: 5_000.0, // initial estimate: 5 s
                running_id: None,
            }),
            results:  Arc::new(Mutex::new(HashMap::new())),
            progress: Arc::new(Mutex::new(HashMap::new())),
            condvar:  Condvar::new(),
        });

        // Spawn the worker thread.
        let q2 = Arc::clone(&q);
        std::thread::Builder::new()
            .name("job-queue".into())
            .spawn(move || q2.worker_loop())
            .expect("[job-queue] failed to spawn worker thread");

        // Spawn a pruner that cleans old results every 60 s.
        let results = Arc::clone(&q.results);
        std::thread::Builder::new()
            .name("job-pruner".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(60));
                let mut map = results.lock_or_recover();
                let cutoff = Instant::now() - Duration::from_secs(300);
                map.retain(|_, v| v.completed > cutoff);
            })
            .expect("[job-queue] failed to spawn pruner thread");

        q
    }

    /// Submit a job.  Returns a ticket immediately.
    pub fn submit<F>(&self, estimated_ms: u64, work: F) -> JobTicket
    where
        F: FnOnce() -> Result<serde_json::Value, String> + Send + 'static,
    {
        let mut inner = self.inner.lock_or_recover();
        let id = inner.next_id;
        inner.next_id += 1;

        let queue_position = inner.queue.len();
        // Estimate: sum of estimated durations for jobs ahead + this one.
        let ahead_ms: u64 = inner.queue.iter().map(|j| j.estimated_ms).sum();
        let total_est_ms = ahead_ms + estimated_ms;
        let est_secs = total_est_ms.div_ceil(1000);

        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        inner.queue.push_back(PendingJob {
            id,
            work: Box::new(work),
            estimated_ms,
        });

        self.condvar.notify_one();

        JobTicket {
            job_id: id,
            estimated_ready_utc: now_unix + est_secs,
            queue_position,
            estimated_secs: est_secs,
        }
    }

    /// Get a clone of the progress map `Arc` so callers can update it
    /// from inside a job closure.
    pub fn progress_map(&self) -> Arc<Mutex<HashMap<u64, JobProgress>>> {
        Arc::clone(&self.progress)
    }

    /// Submit a job that receives its own job ID inside the closure.
    /// This allows the job to update the shared progress map.
    pub fn submit_with_id<F>(&self, estimated_ms: u64, work: F) -> JobTicket
    where
        F: FnOnce(u64) -> Result<serde_json::Value, String> + Send + 'static,
    {
        let mut inner = self.inner.lock_or_recover();
        let id = inner.next_id;
        inner.next_id += 1;

        let queue_position = inner.queue.len();
        let ahead_ms: u64 = inner.queue.iter().map(|j| j.estimated_ms).sum();
        let total_est_ms = ahead_ms + estimated_ms;
        let est_secs = total_est_ms.div_ceil(1000);

        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Wrap the id-aware closure into the standard JobFn.
        let job_id = id;
        inner.queue.push_back(PendingJob {
            id,
            work: Box::new(move || work(job_id)),
            estimated_ms,
        });

        self.condvar.notify_one();

        JobTicket {
            job_id: id,
            estimated_ready_utc: now_unix + est_secs,
            queue_position,
            estimated_secs: est_secs,
        }
    }

    /// Return a snapshot of the current queue state for the UI context menu.
    pub fn stats(&self) -> serde_json::Value {
        let inner = self.inner.lock_or_recover();
        let pending = inner.queue.len();
        let running = inner.running_id.is_some();
        let total_est_ms: u64 = inner.queue.iter().map(|j| j.estimated_ms).sum();
        let active_count = if running { 1 } else { 0 };
        serde_json::json!({
            "queued":       pending,
            "running":      running,
            "total_active": pending + active_count,
            "est_secs":     total_est_ms.div_ceil(1000),
        })
    }

    /// Poll for a job result.
    pub fn poll(&self, job_id: u64) -> JobPollResult {
        // Check completed results first.
        {
            let map = self.results.lock_or_recover();
            if let Some(completed) = map.get(&job_id) {
                return match &completed.result {
                    Ok(val) => JobPollResult::Complete {
                        job_id,
                        result: val.clone(),
                        elapsed_ms: completed.elapsed_ms,
                    },
                    Err(e) => JobPollResult::Error {
                        job_id,
                        error: e.clone(),
                    },
                };
            }
        }

        // Check if still in the queue or currently running.
        let inner = self.inner.lock_or_recover();

        // Currently executing — report as running (position 0) with progress.
        if inner.running_id == Some(job_id) {
            let prog = self.progress.lock_or_recover().get(&job_id).cloned();
            return JobPollResult::Pending {
                job_id,
                queue_position: 0,
                estimated_secs: 0,
                progress: prog,
            };
        }

        if let Some(pos) = inner.queue.iter().position(|j| j.id == job_id) {
            let ahead_ms: u64 = inner.queue.iter().take(pos + 1).map(|j| j.estimated_ms).sum();
            return JobPollResult::Pending {
                job_id,
                queue_position: pos,
                estimated_secs: ahead_ms.div_ceil(1000),
                progress: None,
            };
        }

        JobPollResult::NotFound { job_id }
    }

    /// Blocking worker loop — runs on the dedicated thread.
    fn worker_loop(&self) {
        loop {
            let job = {
                let mut inner = self.inner.lock_or_recover();
                while inner.queue.is_empty() {
                    inner = self.condvar.wait(inner).unwrap();
                }
                let job = inner.queue.pop_front().unwrap();
                inner.running_id = Some(job.id);
                job
            };

            eprintln!("[job-queue] running job #{}", job.id);
            let start = Instant::now();
            let result = (job.work)();
            let elapsed = start.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;
            eprintln!("[job-queue] job #{} done in {}ms", job.id, elapsed_ms);

            // Update EMA + clear running_id.
            {
                let mut inner = self.inner.lock_or_recover();
                inner.avg_job_ms = inner.avg_job_ms * 0.7 + elapsed_ms as f64 * 0.3;
                inner.running_id = None;
            }

            // Store result & clean up progress.
            {
                self.progress.lock_or_recover().remove(&job.id);
                let mut map = self.results.lock_or_recover();
                map.insert(job.id, CompletedJob {
                    result,
                    elapsed_ms,
                    completed: Instant::now(),
                });
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_empty_queue() {
        let q = JobQueue::new();
        let s = q.stats();
        assert_eq!(s["queued"].as_u64(),   Some(0));
        assert_eq!(s["running"].as_bool(), Some(false));
        assert_eq!(s["total_active"].as_u64(), Some(0));
        assert_eq!(s["est_secs"].as_u64(), Some(0));
    }

    #[test]
    fn stats_after_submit() {
        let q = JobQueue::new();
        // Submit a job with 2000 ms estimated time
        let _ticket = q.submit(2000, || Ok(serde_json::json!("done")));
        let s = q.stats();
        // The job may be running or still queued depending on thread scheduling,
        // but total_active must be ≥ 1.
        let total = s["total_active"].as_u64().unwrap_or(0);
        assert!(total >= 1, "expected at least 1 active job, got {total}");
    }

    #[test]
    fn est_secs_rounds_up() {
        // Two queued jobs of 500 ms each = 1000 ms = exactly 1 s (ceiling)
        // We can't easily test est_secs without injecting pending jobs directly,
        // so verify the formula: ⌈(total_est_ms) / 1000⌉
        let ceil = |ms: u64| ms.div_ceil(1000);
        assert_eq!(ceil(0),    0);
        assert_eq!(ceil(1),    1);
        assert_eq!(ceil(1000), 1);
        assert_eq!(ceil(1001), 2);
        assert_eq!(ceil(5000), 5);
    }
}
