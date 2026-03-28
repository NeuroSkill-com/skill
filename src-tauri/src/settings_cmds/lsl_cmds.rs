// SPDX-License-Identifier: GPL-3.0-only
//! Tauri commands for LSL and rlsl-iroh stream management.

use std::sync::Mutex;

use serde::Serialize;
use tauri::AppHandle;

use crate::state::AppState;
use crate::{AppStateExt, MutexExt};

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct LslStreamEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub stream_type: String,
    pub channels: usize,
    pub sample_rate: f64,
    pub source_id: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LslIrohStatus {
    pub running: bool,
    pub endpoint_id: Option<String>,
}

// ── Commands ────────────────────────────────────────────────────────────────

/// Discover LSL streams on the local network (blocking scan, ~3 s).
#[tauri::command]
pub async fn lsl_discover() -> Result<Vec<LslStreamEntry>, String> {
    tokio::task::spawn_blocking(|| {
        skill_lsl::discover_streams(3.0)
            .into_iter()
            .map(|s| LslStreamEntry {
                name: s.name,
                stream_type: s.stream_type,
                channels: s.channel_count,
                sample_rate: s.sample_rate,
                source_id: s.source_id,
                hostname: s.hostname,
            })
            .collect()
    })
    .await
    .map_err(|e| format!("lsl_discover: {e}"))
}

/// Connect to a specific LSL stream by name and start a recording session.
#[tauri::command]
pub async fn lsl_connect(name: String, app: AppHandle) -> Result<(), String> {
    let target = format!("lsl:{name}");
    tauri::async_runtime::spawn(async move {
        crate::lifecycle::start_session(&app, Some(target));
    });
    Ok(())
}

/// Start the rlsl-iroh sink to accept remote LSL streams over QUIC.
///
/// Returns the iroh endpoint ID immediately.  A background task waits for
/// a remote source to connect (up to 120 s), then starts the recording
/// session automatically.
#[tauri::command]
pub async fn lsl_iroh_start(
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<LslIrohStatus, String> {
    // Already running?
    {
        let s = state.lock_or_recover();
        if let Some(ref eid) = s.lsl_iroh_endpoint_id {
            return Ok(LslIrohStatus {
                running: true,
                endpoint_id: Some(eid.clone()),
            });
        }
    }

    let (endpoint_id, adapter_fut) = skill_lsl::IrohLslAdapter::start_sink_two_phase()
        .await
        .map_err(|e| format!("rlsl-iroh bind failed: {e}"))?;

    {
        let mut s = state.lock_or_recover();
        s.lsl_iroh_endpoint_id = Some(endpoint_id.clone());
    }

    // Background: wait for remote → start session → cleanup
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        match adapter_fut.await {
            Ok(Ok(adapter)) => {
                eprintln!("[lsl-iroh] source connected, starting session");
                let csv = crate::session_csv::new_csv_path(&app2);
                let cancel = tokio_util::sync::CancellationToken::new();
                let app3 = app2.clone();
                crate::session_runner::run_device_session(app3, cancel, csv, Box::new(adapter))
                    .await;
            }
            Ok(Err(e)) => eprintln!("[lsl-iroh] sink resolve failed: {e}"),
            Err(e) => eprintln!("[lsl-iroh] task panicked: {e}"),
        }
        let r = app2.app_state();
        let mut s = r.lock_or_recover();
        s.lsl_iroh_endpoint_id = None;
    });

    Ok(LslIrohStatus {
        running: true,
        endpoint_id: Some(endpoint_id),
    })
}

/// Return the current rlsl-iroh sink status.
#[tauri::command]
pub fn lsl_iroh_status(state: tauri::State<'_, Mutex<Box<AppState>>>) -> LslIrohStatus {
    let s = state.lock_or_recover();
    LslIrohStatus {
        running: s.lsl_iroh_endpoint_id.is_some(),
        endpoint_id: s.lsl_iroh_endpoint_id.clone(),
    }
}

/// Stop the rlsl-iroh sink and cancel any pending/active session.
#[tauri::command]
pub fn lsl_iroh_stop(app: AppHandle, state: tauri::State<'_, Mutex<Box<AppState>>>) {
    crate::lifecycle::cancel_session(&app);
    let mut s = state.lock_or_recover();
    s.lsl_iroh_endpoint_id = None;
}
