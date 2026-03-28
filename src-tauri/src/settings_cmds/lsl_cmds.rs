// SPDX-License-Identifier: GPL-3.0-only
//! Tauri commands and shared helpers for LSL and rlsl-iroh stream management.
//!
//! Both the Tauri IPC commands (used by the settings UI) and the WebSocket
//! API commands delegate to the same core functions defined here.

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

// ── Core helpers (shared by Tauri + WS commands) ─────────────────────────────

/// Discover LSL streams on the local network (~3 s blocking scan).
pub fn discover_streams_with_paired(paired: &[String]) -> Vec<LslStreamEntry> {
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
}

/// Start an LSL session by stream name.
pub fn connect_lsl_by_name(app: &AppHandle, name: &str) {
    let target = format!("lsl:{name}");
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        crate::lifecycle::start_session(&app2, Some(target));
    });
}

/// Start the rlsl-iroh sink.  Returns `(endpoint_id, already_running)`.
pub async fn start_iroh_sink(app: &AppHandle) -> Result<(String, bool), String> {
    // Already running?
    {
        let r = app.app_state();
        let s = r.lock_or_recover();
        if let Some(ref eid) = s.lsl_iroh_endpoint_id {
            return Ok((eid.clone(), true));
        }
    }

    let (endpoint_id, adapter_fut) = skill_lsl::IrohLslAdapter::start_sink_two_phase()
        .await
        .map_err(|e| format!("rlsl-iroh bind failed: {e}"))?;

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
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

    Ok((endpoint_id, false))
}

/// Get iroh sink status from app state.
pub fn get_iroh_status(app: &AppHandle) -> LslIrohStatus {
    let r = app.app_state();
    let s = r.lock_or_recover();
    LslIrohStatus {
        running: s.lsl_iroh_endpoint_id.is_some(),
        endpoint_id: s.lsl_iroh_endpoint_id.clone(),
    }
}

/// Stop the iroh sink.
pub fn stop_iroh_sink(app: &AppHandle) {
    crate::lifecycle::cancel_session(app);
    let r = app.app_state();
    let mut s = r.lock_or_recover();
    s.lsl_iroh_endpoint_id = None;
}

// ── Tauri commands ───────────────────────────────────────────────────────────

/// Discover LSL streams on the local network (blocking scan, ~3 s).
#[tauri::command]
pub async fn lsl_discover(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<Vec<LslStreamEntry>, String> {
    let paired: Vec<String> = { state.lock_or_recover().lsl_paired_streams.clone() };
    tokio::task::spawn_blocking(move || discover_streams_with_paired(&paired))
        .await
        .map_err(|e| format!("lsl_discover: {e}"))
}

/// Connect to a specific LSL stream by name and start a recording session.
#[tauri::command]
pub async fn lsl_connect(name: String, app: AppHandle) -> Result<(), String> {
    connect_lsl_by_name(&app, &name);
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
    // Check already running via state (avoid double-bind)
    {
        let s = state.lock_or_recover();
        if let Some(ref eid) = s.lsl_iroh_endpoint_id {
            return Ok(LslIrohStatus {
                running: true,
                endpoint_id: Some(eid.clone()),
            });
        }
    }
    let (eid, _already) = start_iroh_sink(&app).await?;
    Ok(LslIrohStatus {
        running: true,
        endpoint_id: Some(eid),
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
///
/// The scanner also acts as the LSL reconnect mechanism: when a session ends
/// (source disconnected, stream stopped, etc.) and auto-connect is still
/// enabled, the next poll will detect the stream and reconnect automatically.
pub(crate) fn start_lsl_auto_scanner(app: AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static SCANNER_RUNNING: AtomicBool = AtomicBool::new(false);

    if SCANNER_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        eprintln!("[lsl-auto] background scanner started");

        loop {
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

            // Only scan when no session is active and there are paired streams.
            // This doubles as auto-reconnect: when a session ends the next poll
            // will find the stream again and reconnect.
            if !is_session_active && !paired.is_empty() {
                let streams = tokio::task::spawn_blocking(move || skill_lsl::discover_streams(3.0))
                    .await
                    .unwrap_or_default();

                let matched = streams.iter().find(|s| paired.contains(&s.source_id));

                if let Some(stream) = matched {
                    eprintln!(
                        "[lsl-auto] found paired stream '{}' (source_id={}), connecting",
                        stream.name, stream.source_id
                    );

                    // Emit event so the frontend can update immediately
                    let _ = tauri::Emitter::emit(
                        &app,
                        "lsl-auto-connect",
                        serde_json::json!({
                            "name": stream.name,
                            "source_id": stream.source_id,
                        }),
                    );

                    let target = format!("lsl:{}", stream.name);
                    crate::lifecycle::start_session(&app, Some(target));

                    // Wait before scanning again (session needs time to start)
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsl_stream_entry_serializes_type_as_type() {
        let entry = LslStreamEntry {
            name: "Test".into(),
            stream_type: "EEG".into(),
            channels: 4,
            sample_rate: 256.0,
            source_id: "src-001".into(),
            hostname: "lab-pc".into(),
            paired: true,
        };
        let json = serde_json::to_value(&entry).unwrap();
        // `stream_type` field serialises as `type` (serde rename)
        assert_eq!(json["type"], "EEG");
        assert!(json.get("stream_type").is_none());
        assert_eq!(json["paired"], true);
    }

    #[test]
    fn lsl_iroh_status_defaults() {
        let s = LslIrohStatus {
            running: false,
            endpoint_id: None,
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["running"], false);
        assert!(json["endpoint_id"].is_null());
    }

    #[test]
    fn lsl_config_defaults() {
        let c = LslConfig {
            auto_connect: false,
            paired_streams: vec![],
        };
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["auto_connect"], false);
        assert_eq!(json["paired_streams"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn discover_streams_marks_paired() {
        // No network streams expected in test, but the function shouldn't panic.
        let result = discover_streams_with_paired(&["some-id".into()]);
        // All returned streams should have correct paired flag
        for s in &result {
            assert_eq!(s.paired, s.source_id == "some-id");
        }
    }
}
