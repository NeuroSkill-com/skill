// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Muse BLE session loop and per-event handler.

use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use muse_rs::prelude::*;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    AppState, EegPacket, ImuPacket, MutexExt, PpgPacket, SessionDsp,
    ToastLevel, emit_status, refresh_tray, send_toast, upsert_paired, unix_secs,
};
use crate::ble_scanner::{bluetooth_ok, classify_bt_error};
use crate::eeg_bands::BandSnapshot;
use crate::eeg_filter::SpectrogramColumn;
use crate::session_csv::{CsvState, EEG_SAMPLE_RATE, write_session_meta};
use crate::ws_server::WsBroadcaster;
use crate::constants::EEG_CHANNELS;

// ── Muse session entry-point ──────────────────────────────────────────────────

pub(crate) async fn run_muse_session(
    app:          AppHandle,
    cancel_rx:    tokio::sync::oneshot::Receiver<()>,
    csv_path:     PathBuf,
    preferred_id: Option<String>,
) {
    tokio::pin!(cancel_rx);

    // 0. BT check
    if let Err((msg, is_bt)) = bluetooth_ok().await {
        crate::go_disconnected(&app, Some(msg), is_bt); return;
    }

    // 1. → "scanning"
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.session_start_utc = Some(unix_secs());
        s.status.reset_for_scanning("muse", &csv_path, preferred_id.as_deref());
    }
    refresh_tray(&app); emit_status(&app);

    // 2. Scan
    let config = MuseClientConfig { scan_timeout_secs: 10, enable_ppg: true, ..Default::default() };
    let client = MuseClient::new(config);
    let all_devices = tokio::select! {
        biased;
        _ = &mut cancel_rx => { crate::go_disconnected(&app, None, false); return; }
        r = client.scan_all() => match r {
            Err(e) => { let (m,b) = classify_bt_error(&e.to_string()); crate::go_disconnected(&app, Some(m), b); return; }
            Ok(d)  => d,
        }
    };

    // 3. Pick device
    let paired_ids: Vec<String> = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        s.status.paired_devices.iter().map(|d| d.id.clone()).collect()
    };
    let first_time = paired_ids.is_empty();

    let device = if first_time {
        all_devices.into_iter().next()
    } else {
        match &preferred_id {
            Some(id) => all_devices.iter().find(|d| &d.id == id).cloned(),
            None     => all_devices.into_iter().find(|d| paired_ids.contains(&d.id)),
        }
    };
    let device = match device {
        Some(d) => d,
        None => {
            crate::go_disconnected(&app, Some("NO_MUSE_NEARBY".into()), false);
            return;
        }
    };

    // 3b. Pin the real BLE ID into status before connect_to() takes ownership.
    {
        let sr = app.state::<Mutex<Box<AppState>>>();
        let mut g = sr.lock_or_recover();
        g.status.device_id = Some(device.id.clone());
        g.retry_attempt    = 0;
    }

    // 4. Connect
    let (mut rx, handle) = tokio::select! {
        biased;
        _ = &mut cancel_rx => { crate::go_disconnected(&app, None, false); return; }
        r = client.connect_to(device) => match r {
            Err(e) => { let (m,b) = classify_bt_error(&e.to_string()); crate::go_disconnected(&app, Some(m), b); return; }
            Ok(v)  => v,
        }
    };

    // 5. Start streaming
    tokio::select! {
        biased;
        _ = &mut cancel_rx => { let _ = handle.disconnect().await; crate::go_disconnected(&app, None, false); return; }
        r = handle.start(false, false) => { if let Err(e) = r { app_log!(app, "bluetooth", "[muse] start: {e}"); } }
    }

    // 6. Open CSV
    let mut csv = match CsvState::open(&csv_path) {
        Ok(c)  => c,
        Err(e) => {
            let _ = handle.disconnect().await;
            write_session_meta(&app, &csv_path);
            crate::go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };
    write_session_meta(&app, &csv_path);

    // 7. Create session-local DSP (lock-free after this point for all DSP).
    let mut dsp = SessionDsp::new(&app);

    // 8. Event loop
    let mut user_cancelled = false;
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
                        let is_disconnect = matches!(e, MuseEvent::Disconnected);
                        handle_event(e, &app, &mut csv, &csv_path, &mut dsp).await;
                        if is_disconnect {
                            app_log!(app, "bluetooth", "[muse] event loop: received MuseEvent::Disconnected, breaking");
                            let _ = handle.disconnect().await;
                            break;
                        }
                    }
                    None => {
                        app_log!(app, "bluetooth", "[muse] event loop: channel closed");
                        let _ = handle.disconnect().await;
                        break;
                    }
                }
            }
        }
    }

    // Yield so CoreBluetooth delegate callbacks can drain before the client drops.
    tokio::time::sleep(Duration::from_millis(250)).await;

    // 8. Finalise
    csv.flush();
    write_session_meta(&app, &csv_path);

    if !user_cancelled {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        if s.status.sample_count > 0 { s.pending_reconnect = true; }
    }
    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };
    crate::go_disconnected(&app, error_msg, false);
}

// ── Per-event handler ─────────────────────────────────────────────────────────

pub(crate) async fn handle_event(
    event:    MuseEvent,
    app:      &AppHandle,
    csv:      &mut CsvState,
    csv_path: &std::path::Path,
    dsp:      &mut SessionDsp,
) {
    match event {
        // ── Connected ────────────────────────────────────────────────────────
        MuseEvent::Connected(name) => {
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
            // DSP update — no lock needed (session-local).
            dsp.accumulator.update_device(Some(dev_id.clone()), Some(name.clone()));
            app_log!(app, "bluetooth", "[muse] connected: {name} (id={dev_id})");
            upsert_paired(app, &dev_id, &name);
            refresh_tray(app); emit_status(app); crate::emit_devices(app);
            write_session_meta(app, csv_path);

            let connect_payload = serde_json::json!({
                "device_name": name,
                "device_id":   dev_id,
                "timestamp":   unix_secs(),
            });
            let _ = app.emit("device-connected", &connect_payload);
            app.state::<WsBroadcaster>().send("device-connected", &connect_payload);
            send_toast(app, ToastLevel::Success, "Connected",
                &format!("{name} is now streaming EEG data."));
        }

        MuseEvent::Disconnected => {
            let (name, device_id) = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let g  = sr.lock_or_recover();
                (
                    g.status.device_name.clone().unwrap_or_else(|| "unknown".into()),
                    g.status.device_id.clone(),
                )
            };
            app_log!(app, "bluetooth", "[muse] disconnected: {name}");
            let disconnect_payload = serde_json::json!({
                "device_name": name,
                "device_id":   device_id,
                "timestamp":   unix_secs(),
                "reason":      "device_disconnected",
            });
            let _ = app.emit("device-disconnected", &disconnect_payload);
            app.state::<WsBroadcaster>().send("device-disconnected", &disconnect_payload);
            send_toast(app, ToastLevel::Warning, "Connection Lost",
                &format!("{name} disconnected."));
        }

        // ── EEG ──────────────────────────────────────────────────────────────
        MuseEvent::Eeg(r) => {
            let packet_ts_s = if r.timestamp > 0.0 {
                r.timestamp / 1000.0
            } else {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            };

            // ── Sync config changes from UI (brief read lock, no DSP) ────────
            dsp.sync_config(app);

            // ── Status write-back: raw electrode value + sample count ────────
            // Lock is held only for two field writes — no DSP inside.
            let (ipc_ch, _count) = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                if r.electrode < 4 {
                    if let Some(&v) = r.samples.last() {
                        s.status.eeg[r.electrode] = v;
                    }
                }
                s.status.sample_count += r.samples.len() as u64;
                (s.eeg_channel.clone(), s.status.sample_count)
            }; // lock released — all DSP below is lock-free

            // ── CSV write (lock-free) ────────────────────────────────────────
            csv.push_eeg(r.electrode, &r.samples, packet_ts_s, EEG_SAMPLE_RATE);

            // ── DSP pipeline (entirely lock-free) ───────────────────────────
            let filter_fired = dsp.filter.push(r.electrode, &r.samples);

            let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                (0..EEG_CHANNELS)
                    .map(|ch| (ch, dsp.filter.drain(ch)))
                    .filter(|(_, v)| !v.is_empty())
                    .collect()
            } else { Vec::new() };

            let spec_col: Option<SpectrogramColumn> = dsp.filter.take_spec_col();

            let band_fired = dsp.band_analyzer.push(r.electrode, &r.samples);
            let band_snap: Option<BandSnapshot> = if band_fired {
                let snap = dsp.band_analyzer.latest.clone();
                if let Some(ref sn) = snap { dsp.accumulator.update_bands(sn.clone()); }
                snap
            } else { None };

            dsp.quality.push(r.electrode, &r.samples);
            dsp.artifact_detector.push(r.electrode, &r.samples);

            let samples_f32: Vec<f32> = r.samples.iter().map(|&v| v as f32).collect();
            dsp.accumulator.push(r.electrode, &samples_f32);

            // ── Write quality back (brief lock, after DSP completes) ─────────
            if filter_fired {
                let qualities = dsp.quality.all_qualities();
                let sr = app.state::<Mutex<Box<AppState>>>();
                sr.lock_or_recover().status.channel_quality = qualities;
            }

            if !drained.is_empty() {
                let now_ts_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64() * 1000.0;
                for (ch, samples) in drained {
                    let pkt = EegPacket { electrode: ch, samples, timestamp: now_ts_ms };
                    if let Some(ref ipc_ch) = ipc_ch { let _ = ipc_ch.send(pkt); }
                }
            }

            if let Some(col) = spec_col {
                let _ = app.emit("eeg-spectrogram", &col);
            }

            if let Some(mut snap) = band_snap {
                // ── Enrich snap via shared skill-devices logic ───────────────
                let temperature_raw = {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    let val = sr.lock_or_recover().status.temperature_raw;
                    val
                };
                let enrich_ctx = skill_devices::SnapshotContext {
                    ppg:             dsp.accumulator.latest_ppg().cloned(),
                    artifacts:       Some(dsp.artifact_detector.metrics()),
                    head_pose:       Some(dsp.head_pose.metrics()),
                    temperature_raw,
                    gpu:             crate::gpu_stats::read(),
                };
                skill_devices::enrich_band_snapshot(&mut snap, &enrich_ctx);

                csv.push_metrics(csv_path, &snap);

                // ── Auto Do Not Disturb (using skill_devices::dnd_tick) ──────
                let engage_raw = skill_devices::compute_engagement_raw(&snap);
                let focus_score = skill_devices::focus_score(engage_raw);
                let snr_db = snap.snr;

                // Brief lock: read DND config + state, run pure dnd_tick, write state back.
                let d = {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    let mut s = sr.lock_or_recover();
                    let cfg = skill_devices::DndConfig {
                        enabled:               s.dnd_config.enabled,
                        focus_threshold:        s.dnd_config.focus_threshold as f64,
                        duration_secs:          s.dnd_config.duration_secs,
                        exit_duration_secs:     s.dnd_config.exit_duration_secs,
                        focus_lookback_secs:    s.dnd_config.focus_lookback_secs,
                        exit_notification:      s.dnd_config.exit_notification,
                        focus_mode_identifier:  s.dnd_config.focus_mode_identifier.clone(),
                    };
                    let mut dnd_state = skill_devices::DndState {
                        active:        s.dnd_active,
                        focus_samples: std::mem::take(&mut s.dnd_focus_samples),
                        score_history: std::mem::take(&mut s.dnd_score_history),
                        below_ticks:   s.dnd_below_ticks,
                        snr_low_ticks: s.dnd_snr_low_ticks,
                        os_active:     s.dnd_os_active,
                    };
                    let decision = skill_devices::dnd_tick(&cfg, &mut dnd_state, focus_score, snr_db);
                    // Write mutated state back.
                    s.dnd_focus_samples = dnd_state.focus_samples;
                    s.dnd_score_history = dnd_state.score_history;
                    s.dnd_below_ticks   = dnd_state.below_ticks;
                    s.dnd_snr_low_ticks = dnd_state.snr_low_ticks;
                    decision
                }; // lock released — OS DND call runs below

                // Perform OS DND change outside the lock.
                if let Some((enable, mode_id)) = d.set_dnd_to {
                    let ok = crate::dnd::set_dnd(enable, &mode_id);
                    if ok {
                        {
                            let sr = app.state::<Mutex<Box<AppState>>>();
                            let mut s = sr.lock_or_recover();
                            s.dnd_active        = enable;
                            s.dnd_below_ticks   = 0;
                            s.dnd_snr_low_ticks = 0;
                        }
                        let _ = app.emit("dnd-state-changed", enable);
                        app.state::<WsBroadcaster>().send("dnd-state-changed", &enable);
                        if !enable && d.send_exit_notification {
                            send_toast(app, ToastLevel::Info,
                                "Focus mode exited", d.exit_body);
                        }
                    }
                }

                let emit_active = d.emit_active;
                let exit_secs_remaining: f64 =
                    if emit_active && d.avg_score < d.threshold && !d.exit_held {
                        let remaining = d.exit_window.saturating_sub(d.below_ticks as usize);
                        remaining as f64 / 4.0
                    } else { 0.0 };

                // Write the latest band snapshot back.
                {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    sr.lock_or_recover().latest_bands = Some(snap.clone());
                }

                let eligibility = serde_json::json!({
                    "enabled":               d.enabled,
                    "focus_score":           focus_score,
                    "avg_score":             d.avg_score,
                    "sample_count":          d.sample_count,
                    "window_size":           d.window,
                    "threshold":             d.threshold,
                    "dnd_active":            emit_active,
                    "below_ticks":           d.below_ticks,
                    "exit_window_size":      d.exit_window,
                    "exit_secs_remaining":   exit_secs_remaining,
                    "exit_duration_secs":    d.exit_duration_secs,
                    "exit_held_by_lookback": d.exit_held,
                    "focus_lookback_secs":   d.focus_lookback_secs,
                    "os_active":             d.os_active,
                });
                let _ = app.emit("dnd-eligibility", &eligibility);
                app.state::<WsBroadcaster>().send("dnd-eligibility", &eligibility);

                let _ = app.emit("eeg-bands", &snap);
                app.state::<WsBroadcaster>().send("eeg-bands", &snap);
            }

            if _count % 256 == 0 { emit_status(app); }
        }

        // ── PPG ───────────────────────────────────────────────────────────────
        MuseEvent::Ppg(r) => {
            let packet_ts_s = if r.timestamp > 0.0 {
                r.timestamp / 1000.0
            } else {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            };
            let samples_f64: Vec<f64> = r.samples.iter().map(|&v| v as f64).collect();

            // Brief lock: status write-back + IPC channel clone only.
            let ipc = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                if r.ppg_channel < 3 {
                    if let Some(last) = samples_f64.last() { s.status.ppg[r.ppg_channel] = *last; }
                }
                s.status.ppg_sample_count += samples_f64.len() as u64;
                s.ppg_channel.clone()
            }; // lock released — PPG DSP below is lock-free

            dsp.accumulator.push_ppg(r.ppg_channel, &samples_f64);
            let ppg_vitals = dsp.accumulator.latest_ppg().cloned();

            csv.push_ppg(csv_path, r.ppg_channel, &samples_f64, packet_ts_s, ppg_vitals.as_ref());
            if let Some(ch) = ipc {
                let _ = ch.send(PpgPacket {
                    channel:   r.ppg_channel,
                    samples:   samples_f64,
                    timestamp: packet_ts_s * 1000.0,
                });
            }
        }

        // ── Accelerometer ─────────────────────────────────────────────────────
        MuseEvent::Accelerometer(imu) => {
            let last = imu.samples[2];
            let ipc = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                s.status.accel = [last.x, last.y, last.z];
                s.imu_channel.clone()
            };
            if let Some(ch) = ipc {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64() * 1000.0;
                let _ = ch.send(ImuPacket {
                    sensor: "accel".into(),
                    samples: [
                        [imu.samples[0].x, imu.samples[0].y, imu.samples[0].z],
                        [imu.samples[1].x, imu.samples[1].y, imu.samples[1].z],
                        [imu.samples[2].x, imu.samples[2].y, imu.samples[2].z],
                    ],
                    timestamp: now_ms,
                });
            }
        }

        // ── Gyroscope ─────────────────────────────────────────────────────────
        MuseEvent::Gyroscope(imu) => {
            let last = imu.samples[2];
            let (accel, ipc) = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                s.status.gyro = [last.x, last.y, last.z];
                (s.status.accel, s.imu_channel.clone())
            };
            // head_pose.update is lock-free (session-local DSP).
            for sample in &imu.samples {
                dsp.head_pose.update(accel, [sample.x, sample.y, sample.z]);
            }
            if let Some(ch) = ipc {
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64() * 1000.0;
                let _ = ch.send(ImuPacket {
                    sensor: "gyro".into(),
                    samples: [
                        [imu.samples[0].x, imu.samples[0].y, imu.samples[0].z],
                        [imu.samples[1].x, imu.samples[1].y, imu.samples[1].z],
                        [imu.samples[2].x, imu.samples[2].y, imu.samples[2].z],
                    ],
                    timestamp: now_ms,
                });
            }
        }

        // ── Telemetry (battery) ───────────────────────────────────────────────
        MuseEvent::Telemetry(t) => {
            const ALPHA: f32 = 0.1;
            let r = app.state::<Mutex<Box<AppState>>>();
            let mut s = r.lock_or_recover();
            let prev_battery  = s.status.battery;
            let first_reading = s.battery_ema.is_none();
            let smoothed = match s.battery_ema {
                None    => t.battery_level,
                Some(v) => ALPHA * t.battery_level + (1.0 - ALPHA) * v,
            };
            s.battery_ema           = Some(smoothed);
            s.status.battery        = smoothed;
            s.status.fuel_gauge_mv  = t.fuel_gauge_voltage;
            s.status.temperature_raw = t.temperature;
            drop(s);
            emit_status(app);
            if first_reading { write_session_meta(app, csv_path); }
            if smoothed < 10.0 && prev_battery >= 10.0 {
                send_toast(app, ToastLevel::Error, "Battery Critical",
                    &format!("Battery at {:.0}% — charge soon.", smoothed));
            } else if smoothed < 20.0 && prev_battery >= 20.0 {
                send_toast(app, ToastLevel::Warning, "Low Battery",
                    &format!("Battery at {:.0}% — consider charging.", smoothed));
            }
        }

        MuseEvent::Control(c) => {
            app_log!(app, "bluetooth", "[muse] ctrl: {}", c.raw);
            let sn = c.fields.get("sn").and_then(|v| v.as_str()).map(str::to_owned);
            let ma = c.fields.get("ma").and_then(|v| v.as_str()).map(str::to_owned);
            let fw = c.fields.get("fw").and_then(|v| v.as_str()).map(str::to_owned);
            let hw = c.fields.get("hw").and_then(|v| v.as_str()).map(str::to_owned);
            let bl = c.fields.get("bl").and_then(|v| v.as_str()).map(str::to_owned);
            let tp = c.fields.get("tp").and_then(|v| v.as_str()).map(str::to_owned);
            if sn.is_some() || ma.is_some() || fw.is_some() || hw.is_some() {
                let r = app.state::<Mutex<Box<AppState>>>();
                let mut s = r.lock_or_recover();
                if let Some(v) = sn { s.status.serial_number      = Some(v); }
                if let Some(v) = ma { s.status.mac_address         = Some(v); }
                if let Some(v) = fw { s.status.firmware_version    = Some(v); }
                if let Some(v) = hw { s.status.hardware_version    = Some(v); }
                if let Some(v) = bl { s.status.bootloader_version  = Some(v); }
                if let Some(v) = tp { s.status.headset_preset      = Some(v); }
                drop(s);
                emit_status(app);
                write_session_meta(app, csv_path);
            }
        }
    }
}
