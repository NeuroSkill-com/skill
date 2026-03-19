// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Generic device session runner.
//!
//! Consumes a `Box<dyn DeviceAdapter>` and drives the shared DSP / CSV / DND /
//! emit pipeline.  All device-specific logic lives in the adapter; this module
//! only knows about [`DeviceEvent`].

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};

use skill_devices::session::*;
use skill_devices::{self, BatteryEma, BatteryAlert};

use crate::{
    EegPacket, ImuPacket, MutexExt, PpgPacket, SessionDsp, ToastLevel,
    emit_status, refresh_tray, send_toast, upsert_paired, unix_secs,
};
use skill_eeg::artifact_detection::ArtifactDetector;
use skill_eeg::eeg_bands::{BandAnalyzer, BandSnapshot};
use skill_eeg::eeg_quality::QualityMonitor;
use crate::session_csv::write_session_meta;
use skill_data::session_writer::{SessionWriter, StorageFormat};
use crate::ws_server::WsBroadcaster;
use crate::AppStateExt;

// ── Data watchdog ─────────────────────────────────────────────────────────────

/// If no [`DeviceEvent`] arrives for this duration, treat the connection as
/// silently lost and break out of the event loop.  BLE devices can sometimes
/// maintain the L2CAP link while GATT notifications stop flowing (radio
/// interference, firmware hang, headset entered sleep mode, …).  Without a
/// watchdog the session runner would block on `next_event()` forever.
///
/// 15 seconds is generous enough to tolerate short radio glitches (BLE
/// supervision timeouts are typically 2–6 s) while still recovering promptly
/// when data truly stops.
const DATA_WATCHDOG_TIMEOUT: Duration = Duration::from_secs(15);

// ── Public entry point ────────────────────────────────────────────────────────

/// Run a device session using any [`DeviceAdapter`].
///
/// This single function replaces the four former session-specific event loops
/// (`run_muse_session`, `run_mw75_session`, `run_openbci_ganglion_session`,
/// `run_hermes_session`).
///
/// The caller is responsible for:
///   1. Performing BLE scanning / connecting (device-specific).
///   2. Constructing the adapter and passing it here.
///   3. Wiring a `cancel_rx` oneshot for user-initiated disconnect.
pub(crate) async fn run_device_session(
    app:        AppHandle,
    cancel:     tokio_util::sync::CancellationToken,
    csv_path:   PathBuf,
    mut adapter: Box<dyn DeviceAdapter>,
) {

    let desc = adapter.descriptor().clone();
    let has_ppg = desc.caps.contains(DeviceCaps::PPG);
    let has_imu = desc.caps.contains(DeviceCaps::IMU);
    let has_battery = desc.caps.contains(DeviceCaps::BATTERY);
    let kind = desc.kind;
    let mut pipeline_ch = desc.pipeline_channels;
    // Whether the adapter may still update its descriptor (e.g. Emotiv
    // auto-detecting channel count).  Cleared after the first match or
    // after a few EEG frames to avoid checking on every frame forever.
    let mut desc_may_change = kind == "emotiv";
    let sample_rate = desc.eeg_sample_rate;

    // CSV is opened lazily on the first EEG frame so that adapters like Emotiv
    // can auto-detect the actual channel count (via DataLabels) before the
    // header is written.
    let storage_format = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        StorageFormat::from_str(&s.settings_storage_format)
    };
    let mut csv: Option<SessionWriter> = None;
    write_session_meta(&app, &csv_path);

    // ── Set device sample rate and channel info in AppState ─────────────────
    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.status.filter_config.sample_rate = sample_rate as f32;
        s.status.channel_names     = desc.channel_names.clone();
        s.status.eeg_channel_count = desc.eeg_channels;
        s.status.eeg_sample_rate_hz = sample_rate;
    }

    // ── Session-local DSP (lock-free during sample processing) ───────────────
    let ch_name_refs: Vec<&str> = desc.channel_names.iter().map(|s| s.as_str()).collect();
    let mut dsp = SessionDsp::new(&app, &ch_name_refs);
    dsp.accumulator.set_device_channels(desc.channel_names.clone(), sample_rate as f32);

    // ── Battery EMA (from skill-devices — replaces inline smoothing) ─────────
    let mut battery_ema = BatteryEma::new(0.1);

    // ── Event loop ───────────────────────────────────────────────────────────
    let mut user_cancelled = false;
    let mut last_event_at = Instant::now();
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                adapter.disconnect().await;
                user_cancelled = true;
                break;
            }
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(last_event_at + DATA_WATCHDOG_TIMEOUT)) => {
                // No event received for DATA_WATCHDOG_TIMEOUT — treat as
                // silent disconnect.  This catches scenarios where the BLE
                // link stays up but GATT notifications stop (radio
                // interference, device sleep, firmware hang).
                let elapsed = last_event_at.elapsed();
                let watchdog_msg = format!(
                    "[{kind}] Data watchdog: no events for {:.1}s — treating as disconnected",
                    elapsed.as_secs_f64());
                app_log!(app, "bluetooth", "{watchdog_msg}");
                crate::device_scanner::device_log("session", &watchdog_msg);
                on_disconnected(&app, kind);
                adapter.disconnect().await;
                break;
            }
            ev = adapter.next_event() => {
                let Some(ev) = ev else {
                    app_log!(app, "bluetooth", "[{kind}] event channel closed");
                    on_disconnected(&app, kind);
                    adapter.disconnect().await;
                    break;
                };
                last_event_at = Instant::now();
                match ev {
                    DeviceEvent::Connected(info) => {
                        on_connected(&app, &mut dsp, &csv_path, &info, kind);
                    }
                    DeviceEvent::Disconnected => {
                        on_disconnected(&app, kind);
                        adapter.disconnect().await;
                        break;
                    }
                    DeviceEvent::Eeg(frame) => {
                        // Log the first few EEG frames for debugging.
                        if csv.is_none() {
                            let n = frame.channels.len();
                            let preview: Vec<String> = frame.channels.iter()
                                .take(4).map(|v| format!("{v:.1}")).collect();
                            app_log!(app, "bluetooth",
                                "[{kind}] first EEG frame: {n} channels, preview={preview:?}, \
                                 pipeline_ch={pipeline_ch}");
                        }
                        // Re-check pipeline_channels for adapters that
                        // auto-detect (Emotiv DataLabels / first packet).
                        if desc_may_change {
                            let fresh = adapter.descriptor();
                            if fresh.pipeline_channels != pipeline_ch {
                                pipeline_ch = fresh.pipeline_channels;
                                app_log!(app, "bluetooth",
                                    "[{kind}] updated to {} pipeline channels", pipeline_ch);
                                // Reset quality, bands, and artifact detector for
                                // the new channel count — old samples from the
                                // pre-DataLabels phase had garbage (COUNTER,
                                // INTERPOLATED, etc.) that corrupted the quality
                                // window.  We do NOT rebuild the EegFilter because
                                // it initialises the cubecl GPU runtime (a global
                                // singleton that panics on second init).  Instead
                                // we just reset the filter's internal buffers.
                                let ch_refs: Vec<&str> = fresh.channel_names
                                    .iter().map(|s| s.as_str()).collect();
                                dsp.filter.reset();
                                dsp.quality = QualityMonitor::with_window(
                                    crate::constants::EEG_CHANNELS,
                                    fresh.eeg_sample_rate as usize,
                                );
                                dsp.band_analyzer = BandAnalyzer::new_with_rate(
                                    fresh.eeg_sample_rate as f32,
                                );
                                dsp.artifact_detector = ArtifactDetector::with_channels(
                                    fresh.eeg_sample_rate, &ch_refs,
                                );
                                dsp.accumulator.set_device_channels(
                                    fresh.channel_names.clone(),
                                    fresh.eeg_sample_rate as f32,
                                );
                                // Update status with correct channel info.
                                {
                                    let r = app.app_state();
                                    let mut s = r.lock_or_recover();
                                    s.status.channel_names     = fresh.channel_names.clone();
                                    s.status.eeg_channel_count = fresh.eeg_channels;
                                    s.status.filter_config.sample_rate = fresh.eeg_sample_rate as f32;
                                }
                            }
                            // For Emotiv, keep checking until DataLabels has
                            // been processed (pipeline_channels matches the
                            // frame channel count — meaning electrode_indices
                            // are set).
                            if kind != "emotiv" || pipeline_ch == frame.channels.len() {
                                desc_may_change = false;
                            }
                        }

                        // Lazy-open recording file on first EEG frame (after auto-detection).
                        if csv.is_none() {
                            let fresh = adapter.descriptor();
                            let labels: Vec<&str> = fresh.channel_names.iter()
                                .map(|s| s.as_str()).collect();
                            match SessionWriter::open(&csv_path, &labels, storage_format) {
                                Ok(c)  => { csv = Some(c); }
                                Err(e) => {
                                    adapter.disconnect().await;
                                    write_session_meta(&app, &csv_path);
                                    crate::go_disconnected(&app, Some(format!("Recording open error: {e}")), false);
                                    return;
                                }
                            }
                            // Update status with final channel info.
                            {
                                let r = app.app_state();
                                let mut s = r.lock_or_recover();
                                s.status.channel_names     = fresh.channel_names.clone();
                                s.status.eeg_channel_count = fresh.eeg_channels;
                            }
                            write_session_meta(&app, &csv_path);
                        }

                        let temperature_raw = {
                            let sr = app.app_state();
                            let val = sr.lock_or_recover().status.temperature_raw;
                            val
                        };
                        if let Some(ref mut c) = csv {
                            process_eeg(
                                &app, &mut dsp, c, &csv_path,
                                &frame, sample_rate, pipeline_ch, has_ppg,
                                temperature_raw,
                            );
                        }
                    }
                    DeviceEvent::Ppg(frame) if has_ppg => {
                        if let Some(ref mut c) = csv {
                            process_ppg(&app, &mut dsp, c, &csv_path, &frame);
                        }
                    }
                    DeviceEvent::Imu(frame) if has_imu => {
                        if let Some(ref mut c) = csv {
                            process_imu(&app, &mut dsp, c, &csv_path, &frame);
                        } else {
                            process_imu_no_csv(&app, &mut dsp, &frame);
                        }
                    }
                    DeviceEvent::Battery(frame) if has_battery => {
                        process_battery(&app, &mut battery_ema, &csv_path, &frame);
                    }
                    DeviceEvent::Meta(val) => {
                        process_meta(&app, &csv_path, &val);
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Post-drain sleep (let CoreBluetooth delegate callbacks drain) ─────────
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // ── Finalise ─────────────────────────────────────────────────────────────
    if let Some(ref mut c) = csv {
        finalize_session(&app, c, &csv_path, user_cancelled);
    } else {
        // CSV was never opened (disconnect before first EEG frame, or
        // adapter never emitted Eeg events).  Still need to clean up
        // the session state so the UI transitions to disconnected and
        // reconnect logic can fire.
        app_log!(app, "bluetooth",
            "[{kind}] session ended before any EEG data was recorded");
        crate::device_scanner::device_log("session",
            &format!("[{kind}] Session ended (no data recorded)"));
        let error_msg = if user_cancelled { None }
                        else { Some("DEVICE_DISCONNECTED".into()) };
        crate::go_disconnected(&app, error_msg, false);
    }
}

// ── Event handlers ────────────────────────────────────────────────────────────

fn on_connected(
    app:      &AppHandle,
    dsp:      &mut SessionDsp,
    csv_path: &Path,
    info:     &DeviceInfo,
    kind:     &str,
) {
    let dev_id = {
        let sr = app.app_state();
        let g = sr.lock_or_recover();
        g.status.device_id.clone().unwrap_or_else(|| info.id.clone())
    };

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.status.state       = "connected".into();
        s.status.device_name = Some(info.name.clone());
        if s.status.device_id.is_none() {
            s.status.device_id = Some(info.id.clone());
        }
        // Populate device identity fields from the adapter's DeviceInfo.
        if let Some(ref v) = info.serial_number      { s.status.serial_number      = Some(v.clone()); }
        if let Some(ref v) = info.firmware_version    { s.status.firmware_version   = Some(v.clone()); }
        if let Some(ref v) = info.hardware_version    { s.status.hardware_version   = Some(v.clone()); }
        if let Some(ref v) = info.bootloader_version  { s.status.bootloader_version = Some(v.clone()); }
        if let Some(ref v) = info.mac_address         { s.status.mac_address        = Some(v.clone()); }
        if let Some(ref v) = info.headset_preset      { s.status.headset_preset     = Some(v.clone()); }
        s.status.device_error    = None;
        s.status.target_name = None;
        s.retry_attempt               = 0;
        s.status.retry_attempt        = 0;
        s.status.retry_countdown_secs = 0;
    }

    dsp.accumulator.update_device(Some(dev_id.clone()), Some(info.name.clone()));
    app_log!(app, "bluetooth", "[{kind}] connected: {} (id={dev_id})", info.name);
    crate::device_scanner::device_log("session",
        &format!("[{kind}] Connected: {} (id={dev_id})", info.name));
    // Auto-pair ONLY on first launch (no paired devices at all) so the user
    // can test immediately.  Otherwise, only update existing paired entries
    // (e.g. refresh last_seen timestamp, name).  The user must explicitly
    // click "Pair" for new devices.
    {
        let is_already_paired = {
            let r = app.app_state();
            let g = r.lock_or_recover();
            g.status.paired_devices.iter().any(|d| d.id == dev_id)
        };
        let first_time = {
            let r = app.app_state();
            let g = r.lock_or_recover();
            g.status.paired_devices.is_empty()
        };

        if is_already_paired {
            // Just refresh the existing entry (last_seen, name).
            upsert_paired(app, &dev_id, &info.name);
        } else if first_time {
            // First-time onboarding: auto-pair the first device.
            app_log!(app, "bluetooth",
                "[{kind}] first-time auto-pair: {dev_id}");
            upsert_paired(app, &dev_id, &info.name);
        }
        // else: device not paired and not first-time → don't auto-pair.
    }

    // Migrate legacy "cortex:emotiv" → "cortex:<headset_id>" so paired and
    // discovered lists match by exact ID.  This is a one-time migration for
    // users who paired before individual headset IDs were tracked.
    if kind == "emotiv" && dev_id == "cortex:emotiv" && !info.name.is_empty() {
        let real_id = format!("cortex:{}", info.name);
        if real_id != dev_id {
            app_log!(app, "bluetooth",
                "[{kind}] migrating paired ID: {dev_id} → {real_id}");
            upsert_paired(app, &real_id, &info.name);
            {
                let r = app.app_state();
                let mut s = r.lock_or_recover();
                s.status.paired_devices.retain(|d| d.id != "cortex:emotiv");
                s.discovered.retain(|d| d.id != "cortex:emotiv");
                if s.preferred_id.as_deref() == Some("cortex:emotiv") {
                    s.preferred_id = Some(real_id.clone());
                }
                s.status.device_id = Some(real_id);
            }
            crate::helpers::save_settings(app);
        }
    }

    refresh_tray(app);
    emit_status(app);
    crate::emit_devices(app);
    write_session_meta(app, csv_path);

    let payload = serde_json::json!({
        "device_name": info.name,
        "device_id":   dev_id,
        "timestamp":   unix_secs(),
    });
    let _ = app.emit("device-connected", &payload);
    app.state::<WsBroadcaster>().send("device-connected", &payload);
    send_toast(app, ToastLevel::Success, "Connected",
        &format!("{} is now streaming EEG data.", info.name));
}

fn on_disconnected(app: &AppHandle, kind: &str) {
    let (name, device_id) = {
        let sr = app.app_state();
        let g = sr.lock_or_recover();
        (
            g.status.device_name.clone().unwrap_or_else(|| "unknown".into()),
            g.status.device_id.clone(),
        )
    };
    app_log!(app, "bluetooth", "[{kind}] disconnected: {name}");
    crate::device_scanner::device_log("session",
        &format!("[{kind}] Disconnected: {name}"));
    let payload = serde_json::json!({
        "device_name": name,
        "device_id":   device_id,
        "timestamp":   unix_secs(),
        "reason":      "device_disconnected",
    });
    let _ = app.emit("device-disconnected", &payload);
    app.state::<WsBroadcaster>().send("device-disconnected", &payload);
    send_toast(app, ToastLevel::Warning, "Connection Lost",
        &format!("{name} disconnected."));
}

// ── EEG processing ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn process_eeg(
    app:            &AppHandle,
    dsp:            &mut SessionDsp,
    csv: &mut SessionWriter,
    csv_path:       &Path,
    frame:          &EegFrame,
    sample_rate:    f64,
    pipeline_ch:    usize,
    has_ppg:        bool,
    temperature_raw: u16,
) {
    let ts_s = frame.timestamp_s;
    let ts_ms = ts_s * 1000.0;

    // ── Sync config changes from UI ──────────────────────────────────────────
    dsp.sync_config(app);

    // ── Status write-back (brief lock) ───────────────────────────────────────
    let (ipc_ch, count) = {
        let sr = app.app_state();
        let mut s = sr.lock_or_recover();
        for (ch, &uv) in frame.channels.iter().enumerate() {
            if ch < s.status.eeg.len() {
                s.status.eeg[ch] = uv;
            }
        }
        s.status.sample_count += 1;
        (s.eeg_channel.clone(), s.status.sample_count)
    }; // lock released — all DSP below is lock-free

    // ── Per-channel: CSV write + DSP pipeline ────────────────────────────────
    let mut filter_fired = false;
    let mut band_fired   = false;
    for (ch, &uv) in frame.channels.iter().enumerate() {
        let one = [uv];
        csv.push_eeg(ch, &one, ts_s, sample_rate);
        if ch < pipeline_ch {
            if dsp.filter.push(ch, &one)        { filter_fired = true; }
            if dsp.band_analyzer.push(ch, &one) { band_fired   = true; }
            dsp.quality.push(ch, &one);
            dsp.artifact_detector.push(ch, &one);
            dsp.accumulator.push(ch, &[uv as f32]);
        }
    }

    // ── Drain filtered data → emit IPC packets ──────────────────────────────
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

    // ── Write quality back (brief lock) ──────────────────────────────────────
    if filter_fired {
        let qualities = dsp.quality.all_qualities();
        let sr = app.app_state();
        sr.lock_or_recover().status.channel_quality = qualities;
    }

    // ── Emit filtered EEG packets via IPC ────────────────────────────────────
    if !drained.is_empty() {
        for (ch, samples) in drained {
            let pkt = EegPacket { electrode: ch, samples, timestamp: ts_ms };
            if let Some(ref ipc_ch) = ipc_ch {
                let _ = ipc_ch.send(pkt);
            }
        }
    }

    // ── Emit spectrogram ─────────────────────────────────────────────────────
    if let Some(col) = spec_col {
        let _ = app.emit("eeg-spectrogram", &col);
    }

    // ── Band snapshot: enrich, DND, emit ─────────────────────────────────────
    if let Some(mut snap) = band_snap {
        let ppg = if has_ppg {
            dsp.accumulator.latest_ppg().cloned()
        } else {
            None
        };
        let enrich_ctx = skill_devices::SnapshotContext {
            ppg,
            artifacts:       Some(dsp.artifact_detector.metrics()),
            head_pose:       Some(dsp.head_pose.metrics()),
            temperature_raw,
            gpu:             skill_data::gpu_stats::read(),
        };
        skill_devices::enrich_band_snapshot(&mut snap, &enrich_ctx);

        csv.push_metrics(csv_path, &snap);

        // ── DND tick (all devices get this now) ──────────────────────────────
        run_dnd_tick(app, &snap);

        // ── Write back & emit ────────────────────────────────────────────────
        {
            let sr = app.app_state();
            sr.lock_or_recover().latest_bands = Some(snap.clone());
        }
        let _ = app.emit("eeg-bands", &snap);
        app.state::<WsBroadcaster>().send("eeg-bands", &snap);
    }

    // ── Periodic full status emit ────────────────────────────────────────────
    if count % 256 == 0 {
        emit_status(app);
    }
}

// ── DND tick ──────────────────────────────────────────────────────────────────

fn run_dnd_tick(app: &AppHandle, snap: &BandSnapshot) {
    let engage_raw = skill_devices::compute_engagement_raw(snap);
    let focus_score = skill_devices::focus_score(engage_raw);
    let snr_db = snap.snr;

    // Brief lock: read DND config + state, run pure dnd_tick, write state back.
    let d = {
        let sr = app.app_state();
        let mut s = sr.lock_or_recover();
        let cfg = skill_devices::DndConfig {
            enabled:               s.dnd_config.enabled,
            focus_threshold:        s.dnd_config.focus_threshold as f64,
            duration_secs:          s.dnd_config.duration_secs,
            exit_duration_secs:     s.dnd_config.exit_duration_secs,
            focus_lookback_secs:    s.dnd_config.focus_lookback_secs,
            exit_notification:      s.dnd_config.exit_notification,
            focus_mode_identifier:  s.dnd_config.focus_mode_identifier.clone(),
            snr_exit_db:            s.dnd_config.snr_exit_db,
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
    }; // lock released

    // Perform OS DND change outside the lock.
    if let Some((enable, mode_id)) = d.set_dnd_to {
        let ok = skill_data::dnd::set_dnd(enable, &mode_id);
        if ok {
            {
                let sr = app.app_state();
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
        } else {
            0.0
        };

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
}

// ── PPG processing ────────────────────────────────────────────────────────────

fn process_ppg(
    app:      &AppHandle,
    dsp:      &mut SessionDsp,
    csv: &mut SessionWriter,
    csv_path: &Path,
    frame:    &PpgFrame,
) {
    let samples_f64 = &frame.samples;

    // Brief lock: status write-back + IPC channel clone.
    let ipc = {
        let sr = app.app_state();
        let mut s = sr.lock_or_recover();
        if frame.channel < 3 {
            if let Some(last) = samples_f64.last() {
                s.status.ppg[frame.channel] = *last;
            }
        }
        s.status.ppg_sample_count += samples_f64.len() as u64;
        s.ppg_channel.clone()
    }; // lock released

    dsp.accumulator.push_ppg(frame.channel, samples_f64);
    let ppg_vitals = dsp.accumulator.latest_ppg().cloned();

    csv.push_ppg(csv_path, frame.channel, samples_f64, frame.timestamp_s, ppg_vitals.as_ref());

    if let Some(ch) = ipc {
        let _ = ch.send(PpgPacket {
            channel:   frame.channel,
            samples:   samples_f64.clone(),
            timestamp: frame.timestamp_s * 1000.0,
        });
    }
}

// ── IMU processing ────────────────────────────────────────────────────────────

fn process_imu(
    app:      &AppHandle,
    dsp:      &mut SessionDsp,
    csv:      &mut SessionWriter,
    csv_path: &Path,
    frame:    &ImuFrame,
) {
    let accel = frame.accel;
    let gyro = frame.gyro.unwrap_or([0.0; 3]);

    {
        let sr = app.app_state();
        let mut s = sr.lock_or_recover();
        s.status.accel = accel;
        if frame.gyro.is_some() {
            s.status.gyro = gyro;
        }
    }

    // Head-pose tracker (session-local, lock-free).
    dsp.head_pose.update(accel, gyro);

    // Record IMU data.
    let now_s = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    csv.push_imu(csv_path, now_s, accel, frame.gyro, frame.mag);

    // Emit IMU IPC events.
    let now_ms = now_s * 1000.0;

    let ipc = {
        let sr = app.app_state();
        let ch = sr.lock_or_recover().imu_channel.clone();
        ch
    };
    if let Some(ch) = ipc {
        let _ = ch.send(ImuPacket {
            sensor:    "accel".into(),
            samples:   [accel, accel, accel],
            timestamp: now_ms,
        });
        if frame.gyro.is_some() {
            let _ = ch.send(ImuPacket {
                sensor:    "gyro".into(),
                samples:   [gyro, gyro, gyro],
                timestamp: now_ms,
            });
        }
    }
}

/// Process IMU when CSV writer is not yet available (before first EEG frame).
fn process_imu_no_csv(
    app:   &AppHandle,
    dsp:   &mut SessionDsp,
    frame: &ImuFrame,
) {
    let accel = frame.accel;
    let gyro = frame.gyro.unwrap_or([0.0; 3]);

    {
        let sr = app.app_state();
        let mut s = sr.lock_or_recover();
        s.status.accel = accel;
        if frame.gyro.is_some() {
            s.status.gyro = gyro;
        }
    }

    dsp.head_pose.update(accel, gyro);

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64() * 1000.0;

    let ipc = {
        let sr = app.app_state();
        let ch = sr.lock_or_recover().imu_channel.clone();
        ch
    };
    if let Some(ch) = ipc {
        let _ = ch.send(ImuPacket {
            sensor:    "accel".into(),
            samples:   [accel, accel, accel],
            timestamp: now_ms,
        });
        if frame.gyro.is_some() {
            let _ = ch.send(ImuPacket {
                sensor:    "gyro".into(),
                samples:   [gyro, gyro, gyro],
                timestamp: now_ms,
            });
        }
    }
}

// ── Battery processing ────────────────────────────────────────────────────────

fn process_battery(
    app:          &AppHandle,
    battery_ema:  &mut BatteryEma,
    csv_path:     &Path,
    frame:        &BatteryFrame,
) {
    let first_reading = battery_ema.is_first_reading();
    let (smoothed, alert) = battery_ema.update(frame.level_pct);

    {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.status.battery = smoothed;
        if let Some(mv) = frame.voltage_mv {
            s.status.fuel_gauge_mv = mv;
        }
        if let Some(temp) = frame.temperature_raw {
            s.status.temperature_raw = temp;
        }
    }
    emit_status(app);

    if first_reading {
        write_session_meta(app, csv_path);
    }

    match alert {
        BatteryAlert::Critical(_) => {
            send_toast(app, ToastLevel::Error, "Battery Critical",
                &format!("Battery at {smoothed:.0}% \u{2014} charge soon."));
        }
        BatteryAlert::Low(_) => {
            send_toast(app, ToastLevel::Warning, "Low Battery",
                &format!("Battery at {smoothed:.0}% \u{2014} consider charging."));
        }
        BatteryAlert::None => {}
    }
}

// ── Meta processing ───────────────────────────────────────────────────────────

fn process_meta(app: &AppHandle, csv_path: &Path, val: &serde_json::Value) {
    // Extract device identity fields from Meta events.
    // Supports both Muse Control short keys (sn, ma, fw, hw, bl, tp) and
    // long-form keys used by other adapters (serial_number, mac_address, …).
    let obj = match val.as_object() {
        Some(o) => o,
        None => return,
    };

    let str_key = |short: &str, long: &str| -> Option<String> {
        obj.get(short).and_then(|v| v.as_str()).map(str::to_owned)
            .or_else(|| obj.get(long).and_then(|v| v.as_str()).map(str::to_owned))
    };

    let sn = str_key("sn", "serial_number");
    let ma = str_key("ma", "mac_address");
    let fw = str_key("fw", "firmware_version");
    let hw = str_key("hw", "hardware_version");
    let bl = str_key("bl", "bootloader_version");
    let tp = str_key("tp", "headset_preset");

    if sn.is_some() || ma.is_some() || fw.is_some() || hw.is_some() {
        let r = app.app_state();
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

// ── Session finalisation ──────────────────────────────────────────────────────

fn finalize_session(
    app:            &AppHandle,
    csv: &mut SessionWriter,
    csv_path:       &Path,
    user_cancelled: bool,
) {
    csv.flush();
    write_session_meta(app, csv_path);

    if !user_cancelled {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        if s.status.sample_count > 0 {
            s.pending_reconnect = true;
        }
    }
    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };
    crate::go_disconnected(app, error_msg, false);
}
