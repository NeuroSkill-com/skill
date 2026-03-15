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
        s.session_start_utc        = Some(unix_secs());
        s.status.state             = "scanning".into();
        s.status.device_kind       = "muse".into();
        s.status.device_name       = None;
        s.status.device_id         = None;
        s.status.serial_number     = None;
        s.status.mac_address       = None;
        s.status.firmware_version  = None;
        s.status.hardware_version  = None;
        s.status.bootloader_version = None;
        s.status.headset_preset    = None;
        s.status.csv_path          = Some(csv_path.to_string_lossy().into_owned());
        s.status.bt_error          = None;
        s.status.battery           = 0.0;
        s.status.eeg               = vec![f64::NAN; 4];
        s.status.sample_count      = 0;
        s.status.ppg               = vec![0.0; 3];
        s.status.ppg_sample_count  = 0;
        s.status.target_name = preferred_id.as_ref().and_then(|id|
            s.status.paired_devices.iter().find(|d| &d.id == id).map(|d| d.name.clone())
        );
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
                // ── Enrich snap from session-local DSP (all lock-free) ───────
                if let Some(ppg) = dsp.accumulator.latest_ppg() {
                    snap.hr               = Some(ppg.hr);
                    snap.rmssd            = Some(ppg.rmssd);
                    snap.sdnn             = Some(ppg.sdnn);
                    snap.pnn50            = Some(ppg.pnn50);
                    snap.lf_hf_ratio      = Some(ppg.lf_hf_ratio);
                    snap.respiratory_rate = Some(ppg.respiratory_rate);
                    snap.spo2_estimate    = Some(ppg.spo2_estimate);
                    snap.perfusion_index  = Some(ppg.perfusion_index);
                    snap.stress_index     = Some(ppg.stress_index);
                }

                let art = dsp.artifact_detector.metrics();
                snap.blink_count = Some(art.blink_count);
                snap.blink_rate  = Some(art.blink_rate);

                let hp = dsp.head_pose.metrics();
                snap.head_pitch  = Some(hp.pitch);
                snap.head_roll   = Some(hp.roll);
                snap.stillness   = Some(hp.stillness);
                snap.nod_count   = Some(hp.nod_count);
                snap.shake_count = Some(hp.shake_count);

                // Brief read lock only for temperature (one scalar copy).
                let temperature_raw = {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    let g = sr.lock_or_recover();
                    g.status.temperature_raw
                };
                if temperature_raw > 0 {
                    snap.temperature_raw = Some(temperature_raw);
                }

                // ── Composite scores (pure computation, lock-free) ───────────
                let rmssd_opt = dsp.accumulator.latest_ppg().map(|p| p.rmssd);
                let meditation = skill_devices::compute_meditation(&snap, hp.stillness, rmssd_opt);
                snap.meditation = Some((meditation * 10.0).round() / 10.0);

                let cognitive_load = skill_devices::compute_cognitive_load(&snap);
                snap.cognitive_load = Some((cognitive_load * 10.0).round() / 10.0);

                let drowsiness = skill_devices::compute_drowsiness(&snap);
                snap.drowsiness = Some((drowsiness * 10.0).round() / 10.0);

                if let Some(gpu) = crate::gpu_stats::read() {
                    snap.gpu_overall = Some(gpu.overall as f64);
                    snap.gpu_render  = Some(gpu.render  as f64);
                    snap.gpu_tiler   = Some(gpu.tiler   as f64);
                }

                csv.push_metrics(csv_path, &snap);

                // ── Auto Do Not Disturb (lock-free scoring, brief locks for state) ──
                let engage_raw: f32 = if snap.channels.is_empty() {
                    0.5
                } else {
                    let n = snap.channels.len() as f32;
                    snap.channels.iter().map(|ch| {
                        let d = ch.rel_alpha + ch.rel_theta;
                        if d > 1e-6 { ch.rel_beta / d } else { 0.5 }
                    }).sum::<f32>() / n
                };
                let focus_score: f64 =
                    (100.0_f32 / (1.0 + (-2.0 * (engage_raw - 0.8)).exp())) as f64;

                // Current SNR (dB) from the band snapshot.
                let snr_db = snap.snr;

                // SNR threshold below which signal quality is too poor to
                // sustain focus mode.  After SNR_LOW_TICKS consecutive ticks
                // (~1 minute at 4 Hz) below this level the focus mode exits.
                const SNR_LOW_DB:    f32 = 5.0;
                const SNR_LOW_TICKS: u32 = 240; // 60 s × 4 Hz

                // Read DND config + current state, update rolling windows,
                // decide action — all in one brief lock.
                struct DndDecision {
                    dnd_enabled:           bool,
                    threshold:             f64,
                    exit_duration_secs:    u32,
                    focus_lookback_secs:   u32,
                    window:                usize,
                    exit_window:           usize,
                    sample_count:          usize,
                    avg_score:             f64,
                    emit_active:           bool,
                    below_ticks:           u32,
                    exit_held:             bool,
                    os_active:             Option<bool>,
                    /// `Some(true/false)` → call set_dnd(value) after lock release.
                    set_dnd_to:            Option<(bool, String)>,
                    /// Whether to send a native exit notification after the OS call.
                    send_exit_notification: bool,
                    /// Human-readable exit reason for the notification body.
                    exit_body:             &'static str,
                }

                let d = {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    let mut s = sr.lock_or_recover();

                    let dnd_enabled   = s.dnd_config.enabled;
                    let threshold     = s.dnd_config.focus_threshold as f64;
                    let duration_secs = s.dnd_config.duration_secs;
                    let window        = (duration_secs as usize * 4).max(8);
                    let exit_notif_cfg = s.dnd_config.exit_notification;

                    s.dnd_focus_samples.push_back(focus_score);
                    while s.dnd_focus_samples.len() > window { s.dnd_focus_samples.pop_front(); }
                    let sample_count = s.dnd_focus_samples.len();
                    let avg_score = s.dnd_focus_samples.iter().sum::<f64>() / sample_count as f64;

                    let exit_duration_secs  = s.dnd_config.exit_duration_secs;
                    let focus_lookback_secs = s.dnd_config.focus_lookback_secs;
                    let exit_window     = (exit_duration_secs  as usize * 4).max(4);
                    let lookback_window = (focus_lookback_secs as usize * 4).max(4);

                    s.dnd_score_history.push_back(focus_score);
                    while s.dnd_score_history.len() > lookback_window { s.dnd_score_history.pop_front(); }

                    // ── SNR low-signal tracking ───────────────────────────────
                    if snr_db < SNR_LOW_DB {
                        s.dnd_snr_low_ticks = s.dnd_snr_low_ticks.saturating_add(1);
                    } else {
                        s.dnd_snr_low_ticks = 0;
                    }
                    // Exit immediately if SNR has been below threshold for 1 min.
                    let snr_forced_exit = dnd_enabled
                        && s.dnd_active
                        && s.dnd_snr_low_ticks >= SNR_LOW_TICKS;

                    let mut emit_active = s.dnd_active;
                    let mut below_ticks = s.dnd_below_ticks;
                    let mut exit_held   = false;
                    let mut set_dnd_to: Option<(bool, String)> = None;
                    let mut send_exit_notification = false;
                    let mut exit_body: &'static str = "";

                    if snr_forced_exit {
                        // Signal quality too low for 1 minute: drop focus mode
                        // immediately, bypassing the normal exit-delay logic.
                        // Cap below_ticks so we retry next tick if the OS call fails.
                        s.dnd_below_ticks  = exit_window as u32;
                        below_ticks        = exit_window as u32;
                        // NOTE: s.dnd_active stays true until post-lock OS call succeeds.
                        emit_active        = false;
                        set_dnd_to         = Some((false, String::new()));
                        send_exit_notification = exit_notif_cfg;
                        exit_body          = "Signal quality (SNR) dropped below 5 dB for 1 minute. Focus mode deactivated.";
                    } else if dnd_enabled {
                        if avg_score >= threshold {
                            s.dnd_below_ticks = 0;
                            below_ticks       = 0;
                            if !s.dnd_active && snr_db >= SNR_LOW_DB && sample_count >= window {
                                let mode_id = s.dnd_config.focus_mode_identifier.clone();
                                set_dnd_to = Some((true, mode_id));
                            }
                        } else if s.dnd_active {
                            let recent_had_focus = s.dnd_score_history.iter().any(|&v| v >= threshold);
                            if recent_had_focus {
                                s.dnd_below_ticks = 0; below_ticks = 0; exit_held = true;
                            } else {
                                s.dnd_below_ticks += 1;
                                below_ticks        = s.dnd_below_ticks;
                                if s.dnd_below_ticks as usize >= exit_window {
                                    // Cap at exit_window so next tick retries if OS call fails.
                                    // NOTE: s.dnd_active stays true until post-lock OS call succeeds.
                                    s.dnd_below_ticks      = exit_window as u32;
                                    emit_active            = false;
                                    set_dnd_to             = Some((false, String::new()));
                                    send_exit_notification = exit_notif_cfg;
                                    exit_body              = "Your focus score dropped. Focus mode has been deactivated.";
                                }
                            }
                        } else {
                            s.dnd_below_ticks = 0; below_ticks = 0;
                        }
                    } else if s.dnd_active {
                        // Feature was disabled while focus mode was active — clear it.
                        s.dnd_below_ticks  = 0;
                        below_ticks        = 0;
                        // NOTE: s.dnd_active stays true until post-lock OS call succeeds.
                        emit_active        = false;
                        set_dnd_to         = Some((false, String::new()));
                        send_exit_notification = exit_notif_cfg;
                        exit_body          = "Do Not Disturb automation was disabled. Focus mode deactivated.";
                    }

                    DndDecision {
                        dnd_enabled, threshold, exit_duration_secs, focus_lookback_secs,
                        window, exit_window, sample_count, avg_score,
                        emit_active, below_ticks, exit_held,
                        os_active: s.dnd_os_active,
                        set_dnd_to,
                        send_exit_notification,
                        exit_body,
                    }
                }; // lock released — set_dnd (file I/O) runs below

                // Perform OS DND change outside the lock.
                // Order: (1) exit system Focus first, (2) then notify the user.
                if let Some((enable, mode_id)) = d.set_dnd_to {
                    let ok = crate::dnd::set_dnd(enable, &mode_id);
                    if ok {
                        // Update app state only after the OS call succeeds.
                        // This prevents a state mismatch if the call fails and
                        // ensures the exit is retried on the next tick.
                        {
                            let sr = app.state::<Mutex<Box<AppState>>>();
                            let mut s = sr.lock_or_recover();
                            s.dnd_active        = enable;
                            s.dnd_below_ticks   = 0;
                            s.dnd_snr_low_ticks = 0;
                        }
                        let _ = app.emit("dnd-state-changed", enable);
                        app.state::<WsBroadcaster>().send("dnd-state-changed", &enable);
                        // (2) Notify the user AFTER system focus has been cleared.
                        if !enable && d.send_exit_notification {
                            send_toast(
                                app,
                                ToastLevel::Info,
                                "Focus mode exited",
                                d.exit_body,
                            );
                        }
                    }
                    // If !ok: s.dnd_active remains true, dnd_below_ticks is capped
                    // at exit_window, so the next tick will retry immediately.
                }

                let emit_active = d.emit_active;
                let exit_secs_remaining: f64 =
                    if emit_active && d.avg_score < d.threshold && !d.exit_held {
                        let remaining = d.exit_window.saturating_sub(d.below_ticks as usize);
                        remaining as f64 / 4.0
                    } else { 0.0 };

                // Write the latest band snapshot back so get_latest_bands
                // can read it without any lock contention from DSP.
                {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    sr.lock_or_recover().latest_bands = Some(snap.clone());
                }

                let eligibility = serde_json::json!({
                    "enabled":               d.dnd_enabled,
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
                // ── End Auto DND ─────────────────────────────────────────────

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
