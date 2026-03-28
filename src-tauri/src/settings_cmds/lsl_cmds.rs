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
    pub paired: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LslIrohStatus {
    pub running: bool,
    pub endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LslConfig {
    pub auto_connect: bool,
    pub paired_streams: Vec<String>,
}

// ── Commands ────────────────────────────────────────────────────────────────

/// Discover LSL streams on the local network (blocking scan, ~3 s).
#[tauri::command]
pub async fn lsl_discover(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<LslStreamEntry>, String> {
    let paired: Vec<String> = { state.lock_or_recover().lsl_paired_streams.clone() };
    tokio::task::spawn_blocking(move || {
        skill_lsl::discover_streams(3.0)
            .into_iter()
            .map(|s| {
                let is_paired = paired.contains(&s.source_id);
                LslStreamEntry {
                    name: s.name,
                    stream_type: s.stream_type,
                    channels: s.channel_count,
                    sample_rate: s.sample_rate,
                    source_id: s.source_id,
                    hostname: s.hostname,
                    paired: is_paired,
                }
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

/// Pair an LSL stream (by source_id) for auto-connect.
#[tauri::command]
pub fn lsl_pair_stream(
    source_id: String,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    if !s.lsl_paired_streams.contains(&source_id) {
        s.lsl_paired_streams.push(source_id);
    }
    drop(s);
    crate::save_settings(&app);
}

/// Unpair an LSL stream.
#[tauri::command]
pub fn lsl_unpair_stream(
    source_id: String,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    s.lsl_paired_streams.retain(|id| id != &source_id);
    drop(s);
    crate::save_settings(&app);
}

/// Get LSL auto-connect config.
#[tauri::command]
pub fn lsl_get_config(state: tauri::State<'_, Mutex<Box<AppState>>>) -> LslConfig {
    let s = state.lock_or_recover();
    LslConfig {
        auto_connect: s.lsl_auto_connect,
        paired_streams: s.lsl_paired_streams.clone(),
    }
}

/// Toggle LSL auto-connect on/off.
#[tauri::command]
pub fn lsl_set_auto_connect(
    enabled: bool,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    {
        let mut s = state.lock_or_recover();
        s.lsl_auto_connect = enabled;
    }
    crate::save_settings(&app);
    // Start or stop the background scanner
    if enabled {
        start_lsl_auto_scanner(app);
    }
}

/// Start the rlsl-iroh sink to accept remote LSL streams over QUIC.
#[tauri::command]
pub async fn lsl_iroh_start(
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<LslIrohStatus, String> {
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

// ── Background auto-scanner ──────────────────────────────────────────────────

/// Start the background LSL auto-scanner that periodically discovers streams
/// and auto-connects paired ones.  Safe to call multiple times — only one
/// scanner runs at a time.
pub(crate) fn start_lsl_auto_scanner(app: AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static SCANNER_RUNNING: AtomicBool = AtomicBool::new(false);

    // Only one scanner at a time
    if SCANNER_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        eprintln!("[lsl-auto] background scanner started");

        loop {
            // Check if auto-connect is still enabled
            let (enabled, paired, is_session_active) = {
                let r = app.app_state();
                let s = r.lock_or_recover();
                (
                    s.lsl_auto_connect,
                    s.lsl_paired_streams.clone(),
                    s.stream.is_some(),
                )
            };

            if !enabled {
                eprintln!("[lsl-auto] auto-connect disabled, stopping scanner");
                break;
            }

            // Skip scanning if a session is already active or no paired streams
            if !is_session_active && !paired.is_empty() {
                // Scan for LSL streams (blocking)
                let streams = tokio::task::spawn_blocking(|| skill_lsl::discover_streams(3.0))
                    .await
                    .unwrap_or_default();

                // Check if any discovered stream matches a paired source_id
                let matched = streams.iter().find(|s| paired.contains(&s.source_id));

                if let Some(stream) = matched {
                    eprintln!(
                        "[lsl-auto] found paired stream '{}' (source_id={}), connecting",
                        stream.name, stream.source_id
                    );
                    let target = format!("lsl:{}", stream.name);
                    let app2 = app.clone();
                    crate::lifecycle::start_session(&app2, Some(target));

                    // Wait a bit after starting session before scanning again
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    continue;
                }
            }

            // Poll every 10 seconds
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }

        SCANNER_RUNNING.store(false, Ordering::Release);
        eprintln!("[lsl-auto] background scanner stopped");
    });
}

/// Called at app startup to resume the background scanner if auto-connect
/// was enabled in settings.
pub(crate) fn maybe_start_lsl_auto_scanner(app: &AppHandle) {
    let r = app.app_state();
    let s = r.lock_or_recover();
    let enabled = s.lsl_auto_connect;
    drop(s);
    if enabled {
        start_lsl_auto_scanner(app.clone());
    }
}
