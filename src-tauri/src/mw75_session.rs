// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Neurable MW75 Neuro EEG headphone session — BLE activation + data streaming.
//
// The MW75 has 12 EEG channels at 500 Hz.  The first `min(12, EEG_CHANNELS)`
// channels are routed through the existing filter / band / embedding pipeline;
// all 12 channels are written to the session CSV.

use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use skill_devices::mw75::prelude::*;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    AppState, EegPacket, MutexExt, SessionDsp, ToastLevel,
    emit_status, refresh_tray, send_toast, upsert_paired, unix_secs,
};
use crate::ble_scanner::{bluetooth_ok, classify_bt_error};
use crate::eeg_bands::BandSnapshot;
use crate::session_csv::{CsvState, write_session_meta};
use crate::ws_server::WsBroadcaster;
use crate::constants::EEG_CHANNELS;

/// MW75 hardware sample rate (500 Hz).
const MW75_SAMPLE_RATE: f64 = skill_constants::MW75_SAMPLE_RATE;

// ── MW75 session entry-point ──────────────────────────────────────────────────

pub(crate) async fn run_mw75_session(
    app:          AppHandle,
    cancel_rx:    tokio::sync::oneshot::Receiver<()>,
    csv_path:     PathBuf,
    preferred_id: Option<String>,
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
        s.status.reset_for_scanning("mw75", &csv_path, preferred_id.as_deref());
    }
    refresh_tray(&app);
    emit_status(&app);

    // 2. Scan
    let config = Mw75ClientConfig {
        scan_timeout_secs: 10,
        ..Default::default()
    };
    let client = Mw75Client::new(config);
    let all_devices = tokio::select! {
        biased;
        _ = &mut cancel_rx => { crate::go_disconnected(&app, None, false); return; }
        r = client.scan_all() => match r {
            Err(e) => {
                let (m, b) = classify_bt_error(&e.to_string());
                crate::go_disconnected(&app, Some(m), b);
                return;
            }
            Ok(d) => d,
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
            crate::go_disconnected(&app, Some("NO_MW75_NEARBY".into()), false);
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
            Err(e) => {
                let (m, b) = classify_bt_error(&e.to_string());
                crate::go_disconnected(&app, Some(m), b);
                return;
            }
            Ok(v) => v,
        }
    };

    // 5. Start BLE activation (enables EEG + raw mode, queries battery)
    tokio::select! {
        biased;
        _ = &mut cancel_rx => {
            let _ = handle.disconnect().await;
            crate::go_disconnected(&app, None, false);
            return;
        }
        r = handle.start() => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[mw75] start: {e}");
            }
        }
    }

    // 6. Open CSV with MW75 channel labels
    let ch_labels = skill_constants::MW75_CHANNEL_NAMES;
    let label_refs: Vec<&str> = ch_labels.iter().copied().collect();
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

    // 7. Create session-local DSP (lock-free after this point for all DSP).
    let mut dsp = SessionDsp::new(&app);
    let pipeline_ch = skill_constants::MW75_EEG_CHANNELS.min(EEG_CHANNELS);

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
                        let is_disconnect = matches!(e, Mw75Event::Disconnected);
                        handle_mw75_event(e, &app, &mut csv, &csv_path, &mut dsp, pipeline_ch).await;
                        if is_disconnect {
                            app_log!(app, "bluetooth", "[mw75] event loop: received Mw75Event::Disconnected, breaking");
                            let _ = handle.disconnect().await;
                            break;
                        }
                    }
                    None => {
                        app_log!(app, "bluetooth", "[mw75] event loop: channel closed");
                        let _ = handle.disconnect().await;
                        break;
                    }
                }
            }
        }
    }

    // Yield so BLE delegate callbacks can drain.
    tokio::time::sleep(Duration::from_millis(250)).await;

    // 9. Finalise
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

async fn handle_mw75_event(
    event:      Mw75Event,
    app:        &AppHandle,
    csv:        &mut CsvState,
    csv_path:   &std::path::Path,
    dsp:        &mut SessionDsp,
    pipeline_ch: usize,
) {
    match event {
        // ── Connected ────────────────────────────────────────────────────────
        Mw75Event::Connected(name) => {
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
            app_log!(app, "bluetooth", "[mw75] connected: {name} (id={dev_id})");
            upsert_paired(app, &dev_id, &name);
            refresh_tray(app);
            emit_status(app);
            crate::emit_devices(app);
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

        Mw75Event::Disconnected => {
            let (name, device_id) = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let g  = sr.lock_or_recover();
                (
                    g.status.device_name.clone().unwrap_or_else(|| "unknown".into()),
                    g.status.device_id.clone(),
                )
            };
            app_log!(app, "bluetooth", "[mw75] disconnected: {name}");
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
        Mw75Event::Eeg(pkt) => {
            let packet_ts_s = if pkt.timestamp > 0.0 {
                pkt.timestamp
            } else {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            };

            // ── Sync config changes from UI ──────────────────────────────────
            dsp.sync_config(app);

            // ── Status write-back: raw electrode values + sample count ───────
            let ipc_ch = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                for (ch, &uv) in pkt.channels.iter().enumerate().take(pipeline_ch) {
                    s.status.eeg[ch] = uv as f64;
                }
                s.status.sample_count += 1;
                s.eeg_channel.clone()
            }; // lock released — all DSP below is lock-free

            // ── CSV write + DSP pipeline ─────────────────────────────────────
            let mut filter_fired = false;
            let mut band_fired   = false;

            for (ch, &uv) in pkt.channels.iter().enumerate() {
                let sample_f64 = uv as f64;
                let one = [sample_f64];
                csv.push_eeg(ch, &one, packet_ts_s, MW75_SAMPLE_RATE);

                if ch < pipeline_ch {
                    if dsp.filter.push(ch, &one)        { filter_fired = true; }
                    if dsp.band_analyzer.push(ch, &one) { band_fired   = true; }
                    dsp.quality.push(ch, &one);
                    dsp.artifact_detector.push(ch, &one);
                    dsp.accumulator.push(ch, &[uv]);
                }
            }

            let ts_ms = packet_ts_s * 1000.0;

            let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                (0..pipeline_ch)
                    .map(|ch| (ch, dsp.filter.drain(ch)))
                    .filter(|(_, v)| !v.is_empty())
                    .collect()
            } else {
                Vec::new()
            };

            let spec_col = dsp.filter.take_spec_col();

            let band_snap: Option<BandSnapshot> = if band_fired {
                let snap = dsp.band_analyzer.latest.clone();
                if let Some(ref sn) = snap {
                    dsp.accumulator.update_bands(sn.clone());
                }
                snap
            } else {
                None
            };

            // ── Write quality back (brief lock, after DSP completes) ─────────
            if filter_fired {
                let qualities = dsp.quality.all_qualities();
                let sr = app.state::<Mutex<Box<AppState>>>();
                sr.lock_or_recover().status.channel_quality = qualities;
            }

            if !drained.is_empty() {
                for (ch, samples) in drained {
                    let pkt = EegPacket { electrode: ch, samples, timestamp: ts_ms };
                    if let Some(ref ipc_ch) = ipc_ch {
                        let _ = ipc_ch.send(pkt);
                    }
                }
            }

            if let Some(col) = spec_col {
                let _ = app.emit("eeg-spectrogram", &col);
            }

            if let Some(mut snap) = band_snap {
                // ── Enrich snap via shared skill-devices logic ───────────────
                let enrich_ctx = skill_devices::SnapshotContext {
                    ppg:             None, // MW75 has no PPG
                    artifacts:       Some(dsp.artifact_detector.metrics()),
                    head_pose:       Some(dsp.head_pose.metrics()),
                    temperature_raw: 0,
                    gpu:             crate::gpu_stats::read(),
                };
                skill_devices::enrich_band_snapshot(&mut snap, &enrich_ctx);

                csv.push_metrics(csv_path, &snap);

                // ── Auto Do Not Disturb ──────────────────────────────────────
                let engage_raw = skill_devices::compute_engagement_raw(&snap);
                let focus_score = skill_devices::focus_score(engage_raw);
                let snr_db = snap.snr;

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
                    s.dnd_focus_samples = dnd_state.focus_samples;
                    s.dnd_score_history = dnd_state.score_history;
                    s.dnd_below_ticks   = dnd_state.below_ticks;
                    s.dnd_snr_low_ticks = dnd_state.snr_low_ticks;
                    decision
                };

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
                    } else {
                        0.0
                    };

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

            let count = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let c = sr.lock_or_recover().status.sample_count;
                c
            };
            if count % 256 == 0 {
                emit_status(app);
            }
        }

        // ── Battery ──────────────────────────────────────────────────────────
        Mw75Event::Battery(bat) => {
            let level = bat.level as f32;
            let r = app.state::<Mutex<Box<AppState>>>();
            let mut s = r.lock_or_recover();
            let prev_battery  = s.status.battery;
            let first_reading = s.battery_ema.is_none();
            let smoothed = match s.battery_ema {
                None    => level,
                Some(v) => 0.1 * level + 0.9 * v,
            };
            s.battery_ema    = Some(smoothed);
            s.status.battery = smoothed;
            drop(s);
            emit_status(app);
            if first_reading {
                write_session_meta(app, csv_path);
            }
            if smoothed < 10.0 && prev_battery >= 10.0 {
                send_toast(app, ToastLevel::Error, "Battery Critical",
                    &format!("Battery at {:.0}% — charge soon.", smoothed));
            } else if smoothed < 20.0 && prev_battery >= 20.0 {
                send_toast(app, ToastLevel::Warning, "Low Battery",
                    &format!("Battery at {:.0}% — consider charging.", smoothed));
            }
        }

        // ── Activated ────────────────────────────────────────────────────────
        Mw75Event::Activated(status) => {
            app_log!(app, "bluetooth",
                "[mw75] activated: eeg={} raw={}",
                status.eeg_enabled, status.raw_mode_enabled);
        }

        // ── Raw data / other events — ignore ─────────────────────────────────
        _ => {}
    }
}
