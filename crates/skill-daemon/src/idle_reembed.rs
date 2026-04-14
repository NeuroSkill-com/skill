// SPDX-License-Identifier: GPL-3.0-only
//! Background idle reembedding loop.
//!
//! Monitors the EEG device connection state.  When the device has been
//! disconnected for a configurable period (default 30 min), starts slowly
//! processing un-embedded epochs in the background.  Immediately pauses
//! when a device reconnects (real-time embedding takes priority).

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::state::AppState;

/// Spawn the background idle-reembed loop.
/// Runs forever, checking device state every 10 seconds.
pub fn spawn_idle_reembed_loop(state: AppState) {
    tokio::spawn(async move {
        // Wait for daemon to fully initialize before starting.
        tokio::time::sleep(Duration::from_secs(10)).await;

        let mut last_connected = Instant::now();
        let mut reembed_running = false;

        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;

            // Load current settings every tick (user may change them).
            let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
            let settings = skill_settings::load_settings(&skill_dir);
            let cfg = &settings.reembed;

            if !cfg.idle_reembed_enabled {
                if reembed_running {
                    state.idle_reembed_cancel.store(true, Ordering::Relaxed);
                    reembed_running = false;
                }
                continue;
            }

            // Check device state.
            let device_state = state.status.lock().map(|s| s.state.clone()).unwrap_or_default();

            let is_connected = matches!(device_state.as_str(), "connected" | "connecting" | "scanning");

            if is_connected {
                last_connected = Instant::now();
                // Cancel any running background reembed immediately.
                if reembed_running {
                    info!("[idle-reembed] device connected — pausing background reembed");
                    state.idle_reembed_cancel.store(true, Ordering::Relaxed);
                    reembed_running = false;
                }
                continue;
            }

            // Check if we've been idle long enough.
            let idle_secs = last_connected.elapsed().as_secs();
            if idle_secs < cfg.idle_reembed_delay_secs {
                continue;
            }

            // Check if there's work to do.
            if reembed_running {
                continue; // Already processing.
            }

            // Check if there are un-embedded epochs.
            let sd = skill_dir.clone();
            let needed: i64 = tokio::task::spawn_blocking(move || count_missing_embeddings(&sd))
                .await
                .unwrap_or(0);

            if needed == 0 {
                continue;
            }

            info!(
                "[idle-reembed] device idle for {}s, {} epochs need embeddings — starting background reembed",
                idle_secs, needed
            );

            // Reset cancel flag and start.
            state.idle_reembed_cancel.store(false, Ordering::Relaxed);
            reembed_running = true;

            let state_clone = state.clone();
            let use_gpu = cfg.idle_reembed_gpu;
            let throttle_ms = cfg.idle_reembed_throttle_ms;
            let batch_size = cfg.batch_size.max(1);

            tokio::task::spawn_blocking(move || {
                if let Err(e) = run_idle_reembed(&state_clone, use_gpu, throttle_ms, batch_size) {
                    warn!("[idle-reembed] failed: {e}");
                }
                // Signal completion.
                let _ = state_clone.events_tx.send(skill_daemon_common::EventEnvelope {
                    r#type: "reembed-progress".into(),
                    ts_unix_ms: now_unix_ms(),
                    correlation_id: None,
                    payload: serde_json::json!({ "status": "idle_done" }),
                });
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

    // Delegate to the existing batch reembed function but with cancel checking.
    // Re-use the same infrastructure from settings_exg.
    crate::routes::settings_exg::run_batch_reembed_with_cancel(
        &skill_dir,
        &state.events_tx,
        cancel,
        use_gpu,
        throttle_ms,
        batch_size,
    )
}
