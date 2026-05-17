// SPDX-License-Identifier: GPL-3.0-only
//! Background idle reembedding loop.
//!
//! Monitors the EEG device connection state.  When the device has been
//! disconnected for a configurable period (default 30 min), starts slowly
//! processing un-embedded epochs in the background.  Immediately pauses
//! when a device reconnects (real-time embedding takes priority).

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::state::AppState;

const NO_PROGRESS_BACKOFF: Duration = Duration::from_secs(60 * 60);

/// Sample system memory usage and return (used_percent, used_bytes, total_bytes).
/// Cheap enough to call once per 10s tick.
fn sample_memory_percent() -> (u8, u64, u64) {
    let sys = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_memory(sysinfo::MemoryRefreshKind::everything()),
    );
    let used = sys.used_memory();
    let total = sys.total_memory();
    if total == 0 {
        return (0, used, total);
    }
    let pct = ((used as u128 * 100) / total as u128).min(100) as u8;
    (pct, used, total)
}

/// Whether to record a no-progress cooldown after a run finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackoffAfterRun {
    /// Do not start the 1h cooldown (cancelled or embeddings were written).
    Cleared,
    /// Same or higher missing count — avoid restarting every 10s.
    Set { remaining: i64 },
}

fn backoff_after_run(remaining: i64, started_missing: i64, cancelled: bool) -> BackoffAfterRun {
    if cancelled {
        BackoffAfterRun::Cleared
    } else if remaining >= started_missing {
        BackoffAfterRun::Set { remaining }
    } else {
        BackoffAfterRun::Cleared
    }
}

fn should_back_off_no_progress(
    no_progress_backoff: &Mutex<Option<(i64, Instant)>>,
    needed: i64,
    backoff_for: Duration,
) -> bool {
    no_progress_backoff
        .lock()
        .map(|mut guard| {
            let (active, stale) = match guard.as_ref() {
                Some((missing, started_at)) => (*missing == needed && started_at.elapsed() < backoff_for, true),
                None => (false, false),
            };
            if stale && !active {
                *guard = None;
            }
            active
        })
        .unwrap_or(false)
}

/// Spawn the background idle-reembed loop.
/// Runs forever, checking device state every 10 seconds.
pub fn spawn_idle_reembed_loop(state: AppState) {
    tokio::spawn(async move {
        // Wait for daemon to fully initialize before starting.
        tokio::time::sleep(Duration::from_secs(10)).await;

        let mut last_connected = Instant::now();
        let reembed_running = Arc::new(AtomicBool::new(false));
        let no_progress_backoff: Arc<Mutex<Option<(i64, Instant)>>> = Arc::new(Mutex::new(None));
        let mut last_throttle_log = Instant::now()
            .checked_sub(Duration::from_secs(600))
            .unwrap_or_else(Instant::now);

        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            // Heartbeat marks the polling loop tick (not the actual embed run,
            // which spawn_blocking does separately and updates `idle_reembed_state`).
            state.record_task_heartbeat("idle-reembed", 0);

            // Load current settings every tick (user may change them).
            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let settings = skill_settings::load_settings(&skill_dir);
            let cfg = &settings.reembed;

            if !cfg.idle_reembed_enabled {
                if reembed_running.load(Ordering::Relaxed) {
                    state.idle_reembed_cancel.store(true, Ordering::Relaxed);
                }
                continue;
            }

            // Check device state.
            let device_state = state.status.lock().map(|s| s.state.clone()).unwrap_or_default();

            let is_connected = matches!(device_state.as_str(), "connected" | "connecting" | "scanning");

            if is_connected {
                last_connected = Instant::now();
                // Cancel any running background reembed immediately.
                if reembed_running.load(Ordering::Relaxed) {
                    info!("[idle-reembed] device connected — pausing background reembed");
                    state.idle_reembed_cancel.store(true, Ordering::Relaxed);
                }
                continue;
            }

            // Check if we've been idle long enough.
            let idle_secs = last_connected.elapsed().as_secs();

            // Always update observable idle state (so the UI shows countdown).
            if let Ok(mut st) = state.idle_reembed_state.lock() {
                st.idle_secs = idle_secs;
                st.delay_secs = cfg.idle_reembed_delay_secs;
                if !reembed_running.load(Ordering::Relaxed) {
                    st.active = false;
                }
            }

            if idle_secs < cfg.idle_reembed_delay_secs {
                continue;
            }

            // Check if there's work to do.
            if reembed_running.load(Ordering::Relaxed) {
                continue; // Already processing.
            }

            // Check if there are un-embedded epochs.
            let sd = skill_dir.clone();
            let needed: i64 = tokio::task::spawn_blocking(move || count_missing_embeddings(&sd))
                .await
                .unwrap_or(0);

            if needed == 0 {
                if let Ok(mut st) = state.idle_reembed_state.lock() {
                    st.active = false;
                    st.total = 0;
                    st.done = 0;
                    st.memory_throttled = false;
                }
                continue;
            }

            if should_back_off_no_progress(&no_progress_backoff, needed, NO_PROGRESS_BACKOFF) {
                continue;
            }

            // Memory backpressure: skip the run if system memory is already
            // saturated. Embedding (especially with GPU/Metal) can add hundreds
            // of MB of resident memory and OOM the user's machine.
            let (mem_pct, mem_used, mem_total) = sample_memory_percent();
            let limit = cfg.max_resident_memory_percent.min(100);
            if limit < 100 && mem_pct >= limit {
                if let Ok(mut st) = state.idle_reembed_state.lock() {
                    st.memory_throttled = true;
                    st.memory_percent = mem_pct;
                    st.active = false;
                }
                // Rate-limit the warning so we don't spam the log every 10s.
                if last_throttle_log.elapsed() >= Duration::from_secs(300) {
                    warn!(
                        "[idle-reembed] deferring: system memory {mem_pct}% \
                         ({} / {} MiB) >= max_resident_memory_percent={limit}",
                        mem_used / (1024 * 1024),
                        mem_total / (1024 * 1024),
                    );
                    last_throttle_log = Instant::now();
                }
                continue;
            }
            if let Ok(mut st) = state.idle_reembed_state.lock() {
                st.memory_throttled = false;
                st.memory_percent = mem_pct;
            }

            info!(
                "[idle-reembed] device idle for {}s, {} epochs need embeddings — starting background reembed",
                idle_secs, needed
            );

            // Reset cancel flag and start.
            state.idle_reembed_cancel.store(false, Ordering::Relaxed);
            reembed_running.store(true, Ordering::Relaxed);

            if let Ok(mut st) = state.idle_reembed_state.lock() {
                st.active = true;
                st.total = needed as u64;
                st.done = 0;
                st.current_day = String::new();
            }

            let state_clone = state.clone();
            let running_flag = reembed_running.clone();
            let backoff_state = no_progress_backoff.clone();
            let started_missing = needed;
            let skill_dir_for_backoff = skill_dir.clone();
            let cancel_for_backoff = state.idle_reembed_cancel.clone();
            let use_gpu = cfg.idle_reembed_gpu;
            let throttle_ms = cfg.idle_reembed_throttle_ms;
            let batch_size = cfg.batch_size.max(1);

            let handle = tokio::task::spawn_blocking(move || {
                // Wrap the reembed body in catch_unwind so a panic in encoder
                // load, encode, or label-index rebuild surfaces as a logged
                // join error rather than a silently orphaned task.
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Err(e) = run_idle_reembed(&state_clone, use_gpu, throttle_ms, batch_size) {
                        warn!("[idle-reembed] failed: {e}");
                    }
                    // Rebuild label EEG index so interactive search picks up new embeddings.
                    let skill_dir = state_clone.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
                    let stats = skill_label_index::rebuild(&skill_dir, &state_clone.label_index);
                    info!(
                        "[idle-reembed] label index rebuilt: {} text, {} eeg ({} skipped)",
                        stats.text_nodes, stats.eeg_nodes, stats.eeg_skipped
                    );
                }));

                let panicked = result.is_err();

                // Always drop the active flag and signal completion, even on panic,
                // so the UI doesn't get stuck in an "active" state forever.
                if let Ok(mut st) = state_clone.idle_reembed_state.lock() {
                    st.active = false;
                }
                let status = if panicked { "idle_panic" } else { "idle_done" };
                let _ = state_clone.events_tx.send(skill_daemon_common::EventEnvelope {
                    r#type: "reembed-progress".into(),
                    ts_unix_ms: now_unix_ms(),
                    correlation_id: None,
                    payload: serde_json::json!({ "status": status }),
                });

                if panicked {
                    warn!("[idle-reembed] worker panicked — task body unwound, state cleared");
                }
            });

            // Watcher: log if the spawn_blocking task itself fails to join
            // (panic that escaped the catch_unwind, or runtime cancellation).
            tokio::spawn(async move {
                let join_result = handle.await;
                if let Err(join_err) = &join_result {
                    if join_err.is_panic() {
                        warn!("[idle-reembed] task panicked outside catch_unwind: {join_err}");
                    } else if join_err.is_cancelled() {
                        info!("[idle-reembed] task cancelled");
                    }
                }
                let cancelled = cancel_for_backoff.load(Ordering::Relaxed);
                let remaining = if cancelled {
                    started_missing
                } else {
                    tokio::task::spawn_blocking(move || count_missing_embeddings(&skill_dir_for_backoff))
                        .await
                        .unwrap_or(started_missing)
                };
                match backoff_after_run(remaining, started_missing, cancelled) {
                    BackoffAfterRun::Cleared => {
                        if let Ok(mut guard) = backoff_state.lock() {
                            *guard = None;
                        }
                    }
                    BackoffAfterRun::Set { remaining } => {
                        if let Ok(mut guard) = backoff_state.lock() {
                            *guard = Some((remaining, Instant::now()));
                        }
                        warn!(
                            "[idle-reembed] made no embedding progress ({remaining} still missing); backing off for {} minutes",
                            NO_PROGRESS_BACKOFF.as_secs() / 60
                        );
                    }
                }
                running_flag.store(false, Ordering::Relaxed);
            });
        }
    });
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn count_missing_embeddings(skill_dir: &std::path::Path) -> i64 {
    let Ok(entries) = std::fs::read_dir(skill_dir) else {
        return 0;
    };
    let mut total = 0i64;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let db_path = path.join(skill_constants::SQLITE_FILE);
        if !db_path.exists() {
            continue;
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        else {
            continue;
        };
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM embeddings WHERE eeg_embedding IS NULL OR length(eeg_embedding) < 4",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        total += n;
    }
    total
}

/// Run the idle reembed, checking the cancel flag between each batch.
fn run_idle_reembed(state: &AppState, use_gpu: bool, throttle_ms: u64, batch_size: usize) -> anyhow::Result<()> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let cancel = &state.idle_reembed_cancel;
    let idle_state = &state.idle_reembed_state;

    // Subscribe to progress events so we can mirror them into the observable state.
    let mut rx = state.events_tx.subscribe();

    // Spawn a helper thread to update idle_reembed_state from progress events
    // *and* record a real heartbeat for each batch — so the activity panel
    // shows actual embed throughput (e.g. "took 240 ms · 1234 ticks") rather
    // than the 0-ms ticks of the outer 10s polling loop.
    let idle_state_clone = idle_state.clone();
    let state_for_hb = state.clone();
    let updater = std::thread::spawn(move || {
        let mut prev_done: u64 = 0;
        let mut prev_progress_at = std::time::Instant::now();
        while let Ok(ev) = rx.blocking_recv() {
            if ev.r#type != "reembed-progress" {
                continue;
            }
            let done = ev.payload.get("done").and_then(|v| v.as_u64()).unwrap_or(0);
            let total = ev.payload.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
            let day = ev.payload.get("day").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let status = ev.payload.get("status").and_then(|v| v.as_str()).unwrap_or("");
            if let Ok(mut st) = idle_state_clone.lock() {
                st.done = done;
                if total > 0 {
                    st.total = total;
                }
                if !day.is_empty() {
                    st.current_day = day;
                }
            }

            // If `done` advanced, that batch finished work — record a heartbeat
            // with the elapsed wall-clock for that batch. We use saturating
            // arithmetic in case events arrive out of order.
            if done > prev_done {
                let elapsed_ms = prev_progress_at.elapsed().as_millis() as u64;
                state_for_hb.record_task_heartbeat("idle-reembed", elapsed_ms);
                prev_done = done;
                prev_progress_at = std::time::Instant::now();
            }

            if matches!(
                status,
                "done" | "idle_done" | "complete" | "paused" | "error" | "idle_panic"
            ) {
                break;
            }
        }
    });

    // Delegate to the existing batch reembed function but with cancel checking.
    let result = crate::routes::settings_exg::run_batch_reembed_with_cancel(
        &skill_dir,
        &state.events_tx,
        cancel,
        use_gpu,
        throttle_ms,
        batch_size,
    );

    if let Err(e) = &result {
        let _ = state.events_tx.send(skill_daemon_common::EventEnvelope {
            r#type: "reembed-progress".into(),
            ts_unix_ms: now_unix_ms(),
            correlation_id: None,
            payload: serde_json::json!({ "status": "error", "message": e.to_string() }),
        });
    }

    let _ = updater.join();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_reembed_no_backoff_when_state_unset() {
        let backoff = Mutex::new(None);
        assert!(!should_back_off_no_progress(&backoff, 42, Duration::from_secs(60 * 60)));
    }

    #[test]
    fn idle_reembed_backoff_active_for_same_missing_count() {
        let backoff = Mutex::new(Some((42, Instant::now())));

        assert!(should_back_off_no_progress(&backoff, 42, Duration::from_secs(60 * 60)));
        assert!(backoff.lock().unwrap().is_some());
    }

    #[test]
    fn idle_reembed_backoff_clears_when_missing_count_changes() {
        let backoff = Mutex::new(Some((42, Instant::now())));

        assert!(!should_back_off_no_progress(&backoff, 41, Duration::from_secs(60 * 60)));
        assert!(backoff.lock().unwrap().is_none());
    }

    #[test]
    fn idle_reembed_backoff_clears_after_cooldown() {
        let backoff = Mutex::new(Some((
            42,
            Instant::now()
                .checked_sub(Duration::from_secs(60 * 61))
                .expect("test duration is within Instant range"),
        )));

        assert!(!should_back_off_no_progress(&backoff, 42, Duration::from_secs(60 * 60)));
        assert!(backoff.lock().unwrap().is_none());
    }

    #[test]
    fn idle_reembed_backoff_after_run_when_no_progress() {
        assert_eq!(
            backoff_after_run(100, 100, false),
            BackoffAfterRun::Set { remaining: 100 }
        );
    }

    #[test]
    fn idle_reembed_backoff_after_run_when_count_increased() {
        assert_eq!(
            backoff_after_run(110, 100, false),
            BackoffAfterRun::Set { remaining: 110 }
        );
    }

    #[test]
    fn idle_reembed_backoff_after_run_cleared_on_partial_progress() {
        assert_eq!(backoff_after_run(80, 100, false), BackoffAfterRun::Cleared);
    }

    #[test]
    fn idle_reembed_backoff_after_run_cleared_when_cancelled() {
        assert_eq!(backoff_after_run(100, 100, true), BackoffAfterRun::Cleared);
        assert_eq!(backoff_after_run(110, 100, true), BackoffAfterRun::Cleared);
    }
}
