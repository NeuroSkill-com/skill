// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Neurable MW75 Neuro EEG headphone session.
//
// Connection lifecycle (mirrors the `mw75` CLI binary):
//
// 0. **Power on + pairing** — hold the power button for 4+ seconds until a
//    sound is heard.  On first use, continue holding to enter pairing mode,
//    then pair the headphones via the OS Bluetooth settings (required for
//    audio playback and RFCOMM data transport).
//
// 1. **BLE discover + connect** — scan for the MW75 by name / service UUID,
//    connect BLE, and subscribe to GATT notifications.
//
// 2. **BLE activation** — enable EEG mode, optionally enable raw mode (500 Hz),
//    and query the battery level.
//
// 3. **Disconnect BLE** — required on macOS before RFCOMM can connect
//    (CoreBluetooth and IOBluetooth share the radio).
//
// 4. **RFCOMM streaming** — EEG data (12 channels at 500 Hz) flows over
//    Bluetooth Classic RFCOMM channel 25.
//
// The entire session runs on a spawned async task — it never blocks the UI.
//
// ## Cargo feature: `mw75-rfcomm`
//
// The RFCOMM transport (IOBluetooth on macOS, bluer on Linux) is behind the
// `mw75-rfcomm` feature flag because linking IOBluetooth.framework adds ~2 s
// to process startup (dyld loads the framework before `main()`).  Without
// the feature, the session receives EEG data via BLE GATT notifications
// (lower throughput but no framework startup cost).

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
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

    // 2. BLE discover + connect (single step, like the mw75 CLI binary).
    //
    //    Uses client.connect() which creates its own btleplug Manager,
    //    scans with the MW75 service UUID filter, and connects to the first
    //    device found.  We do NOT stop the background scanner — two
    //    CBCentralManagers coexist fine on macOS, and stopping the old one
    //    would destroy its peripheral cache, causing the new one's scan to
    //    find 0 devices.
    let config = Mw75ClientConfig {
        scan_timeout_secs: 10,
        ..Default::default()
    };
    let client = Mw75Client::new(config);

    app_log!(app, "bluetooth", "[mw75] connecting (preferred_id={preferred_id:?})…");

    let connect_result = tokio::select! {
        biased;
        _ = &mut cancel_rx => { crate::go_disconnected(&app, None, false); return; }
        r = client.connect() => r.map_err(|e| format!("{e}")),
    };

    let (mut rx, handle) = match connect_result {
        Ok(v) => v,
        Err(msg) => {
            app_log!(app, "bluetooth", "[mw75] connect failed: {msg}");
            let (m, b) = classify_bt_error(&msg);
            crate::go_disconnected(
                &app,
                Some(format!(
                    "{m}\n\n\
                     To pair MW75: hold the power button for 4+ seconds,\n\
                     then pair in System Bluetooth Settings."
                )),
                b,
            );
            return;
        }
    };

    app_log!(app, "bluetooth", "[mw75] BLE connected, starting activation…");

    // 4. BLE activation — enables EEG + raw mode, queries battery.
    tokio::select! {
        biased;
        _ = &mut cancel_rx => {
            let _ = handle.disconnect().await;
            crate::go_disconnected(&app, None, false);
            return;
        }
        r = handle.start() => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[mw75] BLE activation failed: {e}");
                let _ = handle.disconnect().await;
                crate::go_disconnected(&app, Some(format!("MW75 activation failed: {e}")), false);
                return;
            }
        }
    }
    app_log!(app, "bluetooth", "[mw75] activation complete");

    // 5. Disconnect BLE before RFCOMM — required on macOS.
    let bt_address = handle.peripheral_id();
    app_log!(app, "bluetooth", "[mw75] disconnecting BLE (addr={bt_address})…");
    if let Err(e) = handle.disconnect_ble().await {
        app_log!(app, "bluetooth", "[mw75] BLE disconnect warning: {e}");
    }

    // 6. Start RFCOMM data stream (if feature enabled) or stay on BLE
    //    notifications for EEG data.
    #[allow(unused_variables)]
    let handle = Arc::new(handle);

    #[cfg(feature = "mw75-rfcomm")]
    let _rfcomm_guard = {
        app_log!(app, "bluetooth", "[mw75] starting RFCOMM stream…");
        let rfcomm = tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                crate::go_disconnected(&app, None, false);
                return;
            }
            r = skill_devices::mw75::rfcomm::start_rfcomm_stream(handle.clone(), &bt_address) => match r {
                Err(e) => {
                    app_log!(app, "bluetooth", "[mw75] RFCOMM failed: {e}");
                    crate::go_disconnected(
                        &app,
                        Some(format!(
                            "MW75 RFCOMM failed: {e}\n\n\
                             Make sure the headphones are paired in System Bluetooth Settings.\n\
                             To pair: hold the power button for 4+ seconds to enter pairing mode."
                        )),
                        false,
                    );
                    return;
                }
                Ok(r) => r,
            }
        };
        app_log!(app, "bluetooth", "[mw75] RFCOMM connected — streaming EEG at {MW75_SAMPLE_RATE} Hz");

        // Drain any garbled EEG events from BLE notifications that arrived
        // during the BLE→RFCOMM transition.  BLE GATT notifications from
        // non-EEG characteristics can be misinterpreted as EEG packets with
        // wrong channel counts or extreme values.
        let mut drained = 0u32;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, Mw75Event::Eeg(_)) { drained += 1; }
        }
        if drained > 0 {
            app_log!(app, "bluetooth", "[mw75] drained {drained} stale BLE EEG events");
        }

        Some(rfcomm)
    };

    #[cfg(not(feature = "mw75-rfcomm"))]
    let _rfcomm_guard: Option<()> = {
        app_log!(app, "bluetooth",
            "[mw75] RFCOMM feature disabled — receiving EEG via BLE notifications");
        None
    };

    // 7. Open CSV with MW75 channel labels.
    let ch_labels = skill_constants::MW75_CHANNEL_NAMES;
    let label_refs: Vec<&str> = ch_labels.iter().copied().collect();
    let mut csv = match CsvState::open_with_labels(&csv_path, &label_refs) {
        Ok(c)  => c,
        Err(e) => {
            #[cfg(feature = "mw75-rfcomm")]
            if let Some(ref r) = _rfcomm_guard { r.shutdown(); }
            write_session_meta(&app, &csv_path);
            crate::go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };
    write_session_meta(&app, &csv_path);

    // 8. Session-local DSP (lock-free after this point).
    let mut dsp = SessionDsp::new(&app);
    let pipeline_ch = skill_constants::MW75_EEG_CHANNELS.min(EEG_CHANNELS);

    // 9. Event loop — data arrives as Mw75Event::Eeg from RFCOMM or BLE.
    let mut user_cancelled = false;
    let mut first_eeg_logged = false;
    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                #[cfg(feature = "mw75-rfcomm")]
                if let Some(ref r) = _rfcomm_guard { r.shutdown(); }
                user_cancelled = true;
                break;
            }
            ev = rx.recv() => {
                match ev {
                    Some(e) => {
                        // Log first EEG packet for diagnostics.
                        if !first_eeg_logged {
                            if let Mw75Event::Eeg(ref pkt) = e {
                                first_eeg_logged = true;
                                app_log!(app, "bluetooth",
                                    "[mw75] first EEG: {} ch, counter={}, valid={}",
                                    pkt.channels.len(), pkt.counter, pkt.checksum_valid);
                                let preview: Vec<String> = pkt.channels.iter().enumerate()
                                    .take(6).map(|(i, v)| format!("ch{}={:.1}", i, v)).collect();
                                app_log!(app, "bluetooth", "[mw75]   {}", preview.join(" "));
                            }
                        }
                        let is_disconnect = matches!(e, Mw75Event::Disconnected);
                        handle_mw75_event(e, &app, &mut csv, &csv_path, &mut dsp, pipeline_ch).await;
                        if is_disconnect {
                            app_log!(app, "bluetooth", "[mw75] disconnected");
                            #[cfg(feature = "mw75-rfcomm")]
                            if let Some(ref r) = _rfcomm_guard { r.shutdown(); }
                            break;
                        }
                    }
                    None => {
                        app_log!(app, "bluetooth", "[mw75] event channel closed");
                        #[cfg(feature = "mw75-rfcomm")]
                        if let Some(ref r) = _rfcomm_guard { r.shutdown(); }
                        break;
                    }
                }
            }
        }
    }

    tokio::time::sleep(Duration::from_millis(250)).await;

    // 10. Finalise.
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
    event:       Mw75Event,
    app:         &AppHandle,
    csv:         &mut CsvState,
    csv_path:    &std::path::Path,
    dsp:         &mut SessionDsp,
    pipeline_ch: usize,
) {
    match event {
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

        Mw75Event::Eeg(pkt) => {
            let packet_ts_s = if pkt.timestamp > 0.0 {
                pkt.timestamp
            } else {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            };

            dsp.sync_config(app);

            let ipc_ch = {
                let sr = app.state::<Mutex<Box<AppState>>>();
                let mut s = sr.lock_or_recover();
                for (ch, &uv) in pkt.channels.iter().enumerate() {
                    if ch < s.status.eeg.len() {
                        s.status.eeg[ch] = uv as f64;
                    }
                }
                s.status.sample_count += 1;
                s.eeg_channel.clone()
            };

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
                            send_toast(app, ToastLevel::Info, "Focus mode exited", d.exit_body);
                        }
                    }
                }

                let emit_active = d.emit_active;
                let exit_secs_remaining: f64 =
                    if emit_active && d.avg_score < d.threshold && !d.exit_held {
                        let remaining = d.exit_window.saturating_sub(d.below_ticks as usize);
                        remaining as f64 / 4.0
                    } else { 0.0 };

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
            if count % 256 == 0 { emit_status(app); }
        }

        Mw75Event::Battery(bat) => {
            app_log!(app, "bluetooth", "[mw75] battery: {}%", bat.level);
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
            if first_reading { write_session_meta(app, csv_path); }
            if smoothed < 10.0 && prev_battery >= 10.0 {
                send_toast(app, ToastLevel::Error, "Battery Critical",
                    &format!("Battery at {:.0}% — charge soon.", smoothed));
            } else if smoothed < 20.0 && prev_battery >= 20.0 {
                send_toast(app, ToastLevel::Warning, "Low Battery",
                    &format!("Battery at {:.0}% — consider charging.", smoothed));
            }
        }

        Mw75Event::Activated(status) => {
            app_log!(app, "bluetooth",
                "[mw75] activated: eeg={} raw={}",
                status.eeg_enabled, status.raw_mode_enabled);
        }

        _ => {}
    }
}
