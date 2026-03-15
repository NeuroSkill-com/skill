// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Hermes V1 EEG headset session — 8-channel ADS1299 over BLE.
//
// Connection lifecycle (same as the hermes-ble CLI binary):
//   1. BLE scan for devices whose name starts with "Hermes"
//   2. BLE connect + subscribe to GATT characteristics
//   3. Send "start streaming" command
//   4. Receive EEG (8ch @ 250 Hz) and IMU data via BLE notifications
//
// No RFCOMM needed — all data flows over BLE GATT.

use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use skill_devices::hermes_ble::prelude::*;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    AppState, EegPacket, ImuPacket, MutexExt, SessionDsp, ToastLevel,
    emit_status, refresh_tray, send_toast, upsert_paired, unix_secs,
};
use crate::ble_scanner::{bluetooth_ok, classify_bt_error};
use crate::eeg_bands::BandSnapshot;
use crate::session_csv::{CsvState, write_session_meta};
use crate::ws_server::WsBroadcaster;
use crate::constants::EEG_CHANNELS;

const HERMES_SAMPLE_RATE: f64 = skill_constants::HERMES_SAMPLE_RATE;

// ── Hermes session entry-point ────────────────────────────────────────────────

pub(crate) async fn run_hermes_session(
    app:          AppHandle,
    cancel_rx:    tokio::sync::oneshot::Receiver<()>,
    csv_path:     PathBuf,
    _preferred_id: Option<String>,
) {
    tokio::pin!(cancel_rx);

    // 0. BT check
    if let Err((msg, is_bt)) = bluetooth_ok().await {
        crate::go_disconnected(&app, Some(msg), is_bt);
        return;
    }

    // 1. → "scanning"
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.session_start_utc = Some(unix_secs());
        s.status.reset_for_scanning("hermes", &csv_path, _preferred_id.as_deref());
    }
    refresh_tray(&app);
    emit_status(&app);

    // 2. BLE discover + connect (single step).
    let config = HermesClientConfig {
        scan_timeout_secs: 15,
        ..Default::default()
    };
    let client = HermesClient::new(config);

    app_log!(app, "bluetooth", "[hermes] connecting…");

    let connect_result = tokio::select! {
        biased;
        _ = &mut cancel_rx => { crate::go_disconnected(&app, None, false); return; }
        r = client.connect() => r.map_err(|e| format!("{e}")),
    };

    let (mut rx, handle) = match connect_result {
        Ok(v) => v,
        Err(msg) => {
            app_log!(app, "bluetooth", "[hermes] connect failed: {msg}");
            let (m, b) = classify_bt_error(&msg);
            crate::go_disconnected(&app, Some(m), b);
            return;
        }
    };

    app_log!(app, "bluetooth", "[hermes] BLE connected, starting streaming…");

    // 3. Start streaming (sends the start command via BLE GATT write).
    tokio::select! {
        biased;
        _ = &mut cancel_rx => {
            let _ = handle.disconnect().await;
            crate::go_disconnected(&app, None, false);
            return;
        }
        r = handle.start() => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[hermes] start failed: {e}");
                let _ = handle.disconnect().await;
                crate::go_disconnected(&app, Some(format!("Hermes start failed: {e}")), false);
                return;
            }
        }
    }

    app_log!(app, "bluetooth", "[hermes] streaming started — 8ch EEG at {HERMES_SAMPLE_RATE} Hz");

    // 4. Open CSV with Hermes channel labels.
    let ch_labels = skill_constants::HERMES_CHANNEL_NAMES;
    let label_refs: Vec<&str> = ch_labels.to_vec();
    let mut csv = match CsvState::open_with_labels(&csv_path, &label_refs) {
        Ok(c)  => c,
        Err(e) => {
            let _ = handle.disconnect().await;
            write_session_meta(&app, &csv_path);
            crate::go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };
    write_session_meta(&app, &csv_path);

    // 5. Session-local DSP.
    let mut dsp = SessionDsp::new(&app);
    let pipeline_ch = skill_constants::HERMES_EEG_CHANNELS.min(EEG_CHANNELS);

    // 6. Event loop.
    let mut user_cancelled = false;
    let mut first_eeg_logged = false;
    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                let _ = handle.disconnect().await;
                user_cancelled = true;
                break;
            }
            ev = rx.recv() => {
                match ev {
                    Some(e) => {
                        if !first_eeg_logged {
                            if let HermesEvent::Eeg(ref s) = e {
                                first_eeg_logged = true;
                                app_log!(app, "bluetooth",
                                    "[hermes] first EEG: {} ch, pkt={}, samp={}",
                                    s.channels.len(), s.packet_index, s.sample_index);
                            }
                        }
                        let is_disconnect = matches!(e, HermesEvent::Disconnected);
                        handle_hermes_event(e, &app, &mut csv, &csv_path, &mut dsp, pipeline_ch).await;
                        if is_disconnect {
                            app_log!(app, "bluetooth", "[hermes] disconnected");
                            break;
                        }
                    }
                    None => {
                        app_log!(app, "bluetooth", "[hermes] event channel closed");
                        break;
                    }
                }
            }
        }
    }

    tokio::time::sleep(Duration::from_millis(250)).await;

    // 7. Finalise.
    csv.flush();
    write_session_meta(&app, &csv_path);

    if !user_cancelled {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        if s.status.sample_count > 0 {
            s.pending_reconnect = true;
        }
    }
    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };
    crate::go_disconnected(&app, error_msg, false);
}

// ── Per-event handler ─────────────────────────────────────────────────────────

async fn handle_hermes_event(
    event:       HermesEvent,
    app:         &AppHandle,
    csv:         &mut CsvState,
    csv_path:    &std::path::Path,
    dsp:         &mut SessionDsp,
    pipeline_ch: usize,
) {
    match event {
        HermesEvent::Connected(name) => {
            let dev_id = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let g  = sr.lock_or_recover();
                g.status.device_id.clone().unwrap_or_else(|| name.clone())
            };
            {
                let r = app.state::<Mutex<Box<AppState>>>();
                let mut s = r.lock_or_recover();
                s.status.state       = "connected".into();
                s.status.device_name = Some(name.clone());
                s.status.bt_error    = None;
                s.status.target_name = None;
                s.retry_attempt               = 0;
                s.status.retry_attempt        = 0;
                s.status.retry_countdown_secs = 0;
            }
            dsp.accumulator.update_device(Some(dev_id.clone()), Some(name.clone()));
            app_log!(app, "bluetooth", "[hermes] connected: {name} (id={dev_id})");
            upsert_paired(app, &dev_id, &name);
            refresh_tray(app);
            emit_status(app);
            crate::emit_devices(app);
            write_session_meta(app, csv_path);

            let payload = serde_json::json!({
                "device_name": name, "device_id": dev_id, "timestamp": unix_secs(),
            });
            let _ = app.emit("device-connected", &payload);
            app.state::<WsBroadcaster>().send("device-connected", &payload);
            send_toast(app, ToastLevel::Success, "Connected",
                &format!("{name} is now streaming EEG data."));
        }

        HermesEvent::Disconnected => {
            let (name, device_id) = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let g  = sr.lock_or_recover();
                (
                    g.status.device_name.clone().unwrap_or_else(|| "unknown".into()),
                    g.status.device_id.clone(),
                )
            };
            app_log!(app, "bluetooth", "[hermes] disconnected: {name}");
            let payload = serde_json::json!({
                "device_name": name, "device_id": device_id,
                "timestamp": unix_secs(), "reason": "device_disconnected",
            });
            let _ = app.emit("device-disconnected", &payload);
            app.state::<WsBroadcaster>().send("device-disconnected", &payload);
            send_toast(app, ToastLevel::Warning, "Connection Lost",
                &format!("{name} disconnected."));
        }

        HermesEvent::Eeg(sample) => {
            let packet_ts_s = sample.timestamp / 1000.0;

            dsp.sync_config(app);

            let ipc_ch = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                for (ch, &uv) in sample.channels.iter().enumerate() {
                    if ch < s.status.eeg.len() {
                        s.status.eeg[ch] = uv;
                    }
                }
                s.status.sample_count += 1;
                s.eeg_channel.clone()
            };

            let mut filter_fired = false;
            let mut band_fired   = false;

            for (ch, &uv) in sample.channels.iter().enumerate() {
                let one = [uv];
                csv.push_eeg(ch, &one, packet_ts_s, HERMES_SAMPLE_RATE);

                if ch < pipeline_ch {
                    if dsp.filter.push(ch, &one)        { filter_fired = true; }
                    if dsp.band_analyzer.push(ch, &one) { band_fired   = true; }
                    dsp.quality.push(ch, &one);
                    dsp.artifact_detector.push(ch, &one);
                    dsp.accumulator.push(ch, &[uv as f32]);
                }
            }

            let ts_ms = packet_ts_s * 1000.0;

            let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                (0..pipeline_ch)
                    .map(|ch| (ch, dsp.filter.drain(ch)))
                    .filter(|(_, v)| !v.is_empty())
                    .collect()
            } else { Vec::new() };

            let spec_col = dsp.filter.take_spec_col();

            let band_snap: Option<BandSnapshot> = if band_fired {
                let snap = dsp.band_analyzer.latest.clone();
                if let Some(ref sn) = snap { dsp.accumulator.update_bands(sn.clone()); }
                snap
            } else { None };

            if filter_fired {
                let qualities = dsp.quality.all_qualities();
                let sr = app.state::<Mutex<Box<AppState>>>();
                sr.lock_or_recover().status.channel_quality = qualities;
            }

            if !drained.is_empty() {
                for (ch, samples) in drained {
                    let pkt = EegPacket { electrode: ch, samples, timestamp: ts_ms };
                    if let Some(ref ipc_ch) = ipc_ch { let _ = ipc_ch.send(pkt); }
                }
            }

            if let Some(col) = spec_col {
                let _ = app.emit("eeg-spectrogram", &col);
            }

            if let Some(mut snap) = band_snap {
                let enrich_ctx = skill_devices::SnapshotContext {
                    ppg:             None,
                    artifacts:       Some(dsp.artifact_detector.metrics()),
                    head_pose:       Some(dsp.head_pose.metrics()),
                    temperature_raw: 0,
                    gpu:             crate::gpu_stats::read(),
                };
                skill_devices::enrich_band_snapshot(&mut snap, &enrich_ctx);
                csv.push_metrics(csv_path, &snap);

                {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    sr.lock_or_recover().latest_bands = Some(snap.clone());
                }
                let _ = app.emit("eeg-bands", &snap);
                app.state::<WsBroadcaster>().send("eeg-bands", &snap);
            }

            let count = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let c = sr.lock_or_recover().status.sample_count;
                c
            };
            if count % 256 == 0 { emit_status(app); }
        }

        HermesEvent::Motion(m) => {
            let accel = [m.accel.x, m.accel.y, m.accel.z];
            let gyro  = [m.gyro.x,  m.gyro.y,  m.gyro.z];
            {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                s.status.accel = accel;
                s.status.gyro  = gyro;
            }
            // Feed head pose tracker with accel + gyro.
            dsp.head_pose.update(accel, gyro);

            // Emit IMU events for the frontend chart.
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64() * 1000.0;
            let ipc = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let c = sr.lock_or_recover().imu_channel.clone();
                c
            };
            if let Some(ch) = ipc {
                let _ = ch.send(ImuPacket {
                    sensor: "accel".into(),
                    samples: [accel, accel, accel],
                    timestamp: now_ms,
                });
                let _ = ch.send(ImuPacket {
                    sensor: "gyro".into(),
                    samples: [gyro, gyro, gyro],
                    timestamp: now_ms,
                });
            }
        }

        HermesEvent::PacketsDropped(n) => {
            app_log!(app, "bluetooth", "[hermes] dropped {n} EEG packet(s)");
        }

        _ => {}
    }
}
