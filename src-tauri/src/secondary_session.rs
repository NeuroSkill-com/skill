// SPDX-License-Identifier: GPL-3.0-only
//! Lightweight secondary session runner for concurrent multi-device recording.
//!
//! A secondary session writes its own CSV/Parquet file and emits periodic
//! `secondary-sessions` events but does NOT drive the dashboard, DSP pipeline,
//! embeddings, or hooks — those belong to the primary session.

use std::path::PathBuf;

use skill_data::session_writer::{SessionWriter, StorageFormat};
use skill_devices::session::{DeviceAdapter, DeviceEvent};
use tauri::AppHandle;
use tokio::time::Instant;

use crate::state::SecondarySessionInfo;
use crate::ws_server::WsBroadcaster;
use crate::{AppStateExt, MutexExt};

/// Run a secondary session to completion.
///
/// The caller must have already inserted a `SecondarySessionHandle` into
/// `app_state.secondary_sessions` before calling this function.
pub(crate) async fn run_secondary_session(
    app: AppHandle,
    session_id: String,
    cancel: tokio_util::sync::CancellationToken,
    csv_path: PathBuf,
    mut adapter: Box<dyn DeviceAdapter>,
) {
    let desc = adapter.descriptor().clone();

    let storage_format = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        StorageFormat::parse(&s.settings_storage_format)
    };
    let mut csv: Option<SessionWriter> = None;

    // Write session meta sidecar
    crate::session_csv::write_session_meta(&app, &csv_path);

    let mut sample_count: u64 = 0;
    let mut last_emit = Instant::now();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                eprintln!("[secondary:{session_id}] cancelled");
                break;
            }
            event = adapter.next_event() => {
                let Some(event) = event else {
                    eprintln!("[secondary:{session_id}] adapter stream ended");
                    break;
                };

                match event {
                    DeviceEvent::Connected(info) => {
                        eprintln!(
                            "[secondary:{session_id}] connected: {} ({})",
                            info.name, info.id
                        );
                        let r = app.app_state();
                        let mut s = r.lock_or_recover();
                        if let Some(handle) = s.secondary_sessions.get_mut(&session_id) {
                            handle.info.device_name = info.name;
                        }
                    }

                    DeviceEvent::Eeg(frame) => {
                        if csv.is_none() {
                            let labels: Vec<&str> =
                                desc.channel_names.iter().map(String::as_str).collect();
                            csv = SessionWriter::open(
                                &csv_path,
                                &labels,
                                storage_format,
                            )
                            .ok();
                        }
                        if let Some(ref mut w) = csv {
                            let sr = desc.eeg_sample_rate;
                            for (ch, &uv) in frame.channels.iter().enumerate() {
                                w.push_eeg(ch, &[uv], frame.timestamp_s, sr);
                            }
                        }
                        sample_count += 1;

                        if last_emit.elapsed() >= std::time::Duration::from_millis(500) {
                            last_emit = Instant::now();
                            update_and_emit(&app, &session_id, sample_count);
                        }
                    }

                    DeviceEvent::Battery(frame) => {
                        let r = app.app_state();
                        let mut s = r.lock_or_recover();
                        if let Some(handle) = s.secondary_sessions.get_mut(&session_id) {
                            handle.info.battery = frame.level_pct;
                        }
                    }

                    DeviceEvent::Disconnected => {
                        eprintln!("[secondary:{session_id}] disconnected");
                        break;
                    }

                    // PPG, IMU, Meta — skip for secondary (no DSP)
                    _ => {}
                }
            }
        }
    }

    // Final update
    update_and_emit(&app, &session_id, sample_count);

    // Flush CSV
    drop(csv);

    // Remove from state
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.secondary_sessions.remove(&session_id);
    }
    emit_secondary_status(&app);
    eprintln!(
        "[secondary:{session_id}] ended — {sample_count} samples written to {}",
        csv_path.display()
    );
}

fn update_and_emit(app: &AppHandle, session_id: &str, sample_count: u64) {
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        if let Some(handle) = s.secondary_sessions.get_mut(session_id) {
            handle.info.sample_count = sample_count;
        }
    }
    emit_secondary_status(app);
}

/// Emit the list of all secondary sessions to the frontend.
pub(crate) fn emit_secondary_status(app: &AppHandle) {
    let infos: Vec<SecondarySessionInfo> = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.secondary_sessions
            .values()
            .map(|h| h.info.clone())
            .collect()
    };
    let _ = tauri::Emitter::emit(app, "secondary-sessions", &infos);
    use tauri::Manager;
    app.state::<WsBroadcaster>()
        .send("secondary_sessions", &infos);
}
