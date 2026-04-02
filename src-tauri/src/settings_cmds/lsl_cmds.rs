// SPDX-License-Identifier: GPL-3.0-only
//! Tauri commands and shared helpers for LSL and rlsl-iroh stream management.
//!
//! Both the Tauri IPC commands (used by the settings UI) and the WebSocket
//! API commands delegate to the same core functions defined here.

use std::sync::Mutex;

use serde::Serialize;
use skill_settings::LslPairedStream;
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
    pub paired_streams: Vec<LslPairedStream>,
}

// ── Core helpers (shared by Tauri + WS commands) ─────────────────────────────

/// Discover LSL streams on the local network (~3 s blocking scan).
pub fn discover_streams_with_paired(paired: &[LslPairedStream]) -> Vec<LslStreamEntry> {
    let paired_ids: Vec<&str> = paired.iter().map(|p| p.source_id.as_str()).collect();
    skill_lsl::discover_streams(3.0)
        .into_iter()
        .map(|s| {
            let is_paired = paired_ids.contains(&s.source_id.as_str());
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
    let paired: Vec<LslPairedStream> = { state.lock_or_recover().lsl_paired_streams.clone() };
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

/// Pair an LSL stream for auto-connect (stores full stream metadata).
#[tauri::command]
pub fn lsl_pair_stream(
    source_id: String,
    name: String,
    stream_type: String,
    channels: usize,
    sample_rate: f64,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let mut s = state.lock_or_recover();
    // Update existing or insert new
    if let Some(existing) = s
        .lsl_paired_streams
        .iter_mut()
        .find(|p| p.source_id == source_id)
    {
        existing.name = name;
        existing.stream_type = stream_type;
        existing.channels = channels;
        existing.sample_rate = sample_rate;
    } else {
        s.lsl_paired_streams.push(LslPairedStream {
            source_id,
            name,
            stream_type,
            channels,
            sample_rate,
        });
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
    s.lsl_paired_streams.retain(|p| p.source_id != source_id);
    drop(s);
    crate::save_settings(&app);
}

/// Get the LSL idle-timeout setting.
///
/// Returns `None` when the watchdog is disabled, or `Some(secs)` when active.
#[tauri::command]
pub fn lsl_get_idle_timeout(state: tauri::State<'_, Mutex<Box<AppState>>>) -> Option<u64> {
    state.lock_or_recover().lsl_idle_timeout_secs
}

/// Set the LSL idle-timeout.
///
/// Pass `None` to disable (stream never times out), or `Some(secs)` to stop
/// the session after that many seconds of silence.
#[tauri::command]
pub fn lsl_set_idle_timeout(
    secs: Option<u64>,
    app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    {
        let mut s = state.lock_or_recover();
        s.lsl_idle_timeout_secs = secs;
    }
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

/// Switch from the current session to a new LSL stream (cancel + reconnect).
#[tauri::command]
pub async fn lsl_switch_session(name: String, app: AppHandle) -> Result<(), String> {
    crate::lifecycle::switch_session(&app, Some(format!("lsl:{name}")));
    Ok(())
}

/// Start an LSL stream as a secondary (background) recording session.
///
/// The primary session (if any) keeps running.  The secondary writes its
/// own CSV but does not drive the dashboard or embeddings.
#[tauri::command]
pub async fn lsl_start_secondary(name: String, app: AppHandle) -> Result<bool, String> {
    let session_id = format!("lsl:{name}");

    // Resolve the stream
    let name2 = name.clone();
    let info = tokio::task::spawn_blocking(move || {
        let streams = skill_lsl::resolve_eeg_streams(5.0);
        streams.into_iter().find(|s| s.name() == name2)
    })
    .await
    .map_err(|e| format!("spawn: {e}"))?
    .ok_or_else(|| format!("No LSL stream named '{name}' found"))?;

    let adapter = skill_lsl::LslAdapter::new(&info);
    Ok(crate::lifecycle::start_secondary_session(
        &app,
        session_id,
        Box::new(adapter),
    ))
}

/// Cancel a specific secondary session by ID.
#[tauri::command]
pub fn lsl_cancel_secondary(session_id: String, app: AppHandle) {
    crate::lifecycle::cancel_secondary_session(&app, &session_id);
}

/// List all active secondary sessions.
#[tauri::command]
pub fn list_secondary_sessions(app: AppHandle) -> Vec<crate::state::SecondarySessionInfo> {
    crate::lifecycle::list_secondary_sessions(&app)
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

            if !is_session_active && !paired.is_empty() {
                let paired2 = paired.clone();
                let streams = tokio::task::spawn_blocking(move || skill_lsl::discover_streams(3.0))
                    .await
                    .unwrap_or_default();

                let matched = streams
                    .iter()
                    .find(|s| paired2.iter().any(|p| p.source_id == s.source_id));

                if let Some(stream) = matched {
                    eprintln!(
                        "[lsl-auto] found paired stream '{}' (source_id={}), connecting",
                        stream.name, stream.source_id
                    );

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

                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    continue;
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }

        SCANNER_RUNNING.store(false, Ordering::Release);
        eprintln!("[lsl-auto] background scanner stopped");
    });
}

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
#[allow(clippy::unwrap_used)]
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
        let paired = vec![LslPairedStream {
            source_id: "some-id".into(),
            name: "Test".into(),
            stream_type: "EEG".into(),
            channels: 4,
            sample_rate: 256.0,
        }];
        let result = discover_streams_with_paired(&paired);
        for s in &result {
            assert_eq!(s.paired, s.source_id == "some-id");
        }
    }

    #[test]
    fn lsl_paired_stream_round_trips() {
        let p = LslPairedStream {
            source_id: "abc-123".into(),
            name: "MyEEG".into(),
            stream_type: "EEG".into(),
            channels: 8,
            sample_rate: 500.0,
        };
        let json = serde_json::to_string(&p).unwrap();
        let p2: LslPairedStream = serde_json::from_str(&json).unwrap();
        assert_eq!(p2.source_id, "abc-123");
        assert_eq!(p2.name, "MyEEG");
        assert_eq!(p2.channels, 8);
    }
}
