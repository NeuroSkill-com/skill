// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// OpenBCI device sessions: Ganglion BLE and the generic board-agnostic path
// (Ganglion WiFi, Cyton serial/WiFi, Galea).

use std::{path::PathBuf, sync::Mutex};

use openbci::board::ganglion::{GanglionBoard, GanglionConfig};
use openbci::board::cyton::CytonBoard;
use openbci::board::cyton_daisy::CytonDaisyBoard;
use openbci::board::cyton_wifi::{CytonWifiBoard, CytonWifiConfig};
use openbci::board::cyton_daisy_wifi::{CytonDaisyWifiBoard, CytonDaisyWifiConfig};
use openbci::board::ganglion_wifi::{GanglionWifiBoard, GanglionWifiConfig};
use openbci::board::galea::GaleaBoard;
use openbci::board::Board as OpenBciBoard;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    AppState, EegPacket, MutexExt, SessionDsp, StreamHandle, ToastLevel,
    emit_status, emit_devices, refresh_tray, send_toast, upsert_paired, unix_secs,
};
use crate::ble_scanner::{bluetooth_ok, classify_bt_error};
use crate::eeg_bands::BandSnapshot;
use crate::session_csv::{CsvState, EEG_SAMPLE_RATE, new_csv_path, write_session_meta};
use crate::ws_server::WsBroadcaster;
use crate::constants::EEG_CHANNELS;

// ── Ganglion BLE session ──────────────────────────────────────────────────────

pub(crate) async fn run_openbci_ganglion_session(
    app:          AppHandle,
    cancel_rx:    tokio::sync::oneshot::Receiver<()>,
    csv_path:     PathBuf,
    preferred_id: Option<String>,
) {
    use openbci::board::ganglion::GanglionFilter;
    tokio::pin!(cancel_rx);

    // 0. BT check (same as Muse path)
    if let Err((msg, is_bt)) = bluetooth_ok().await {
        crate::go_disconnected(&app, Some(msg), is_bt); return;
    }

    // 1. → "scanning"
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.session_start_utc = Some(unix_secs());
        s.status.reset_for_scanning("ganglion", &csv_path, preferred_id.as_deref());
    }
    refresh_tray(&app); emit_status(&app);

    // 2. Prepare (connect BLE via openbci crate, blocking)
    let preferred_mac = preferred_id.clone();
    let scan_timeout_secs = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        s.openbci_config.scan_timeout_secs
    };

    let board_result = tokio::select! {
        biased;
        _ = &mut cancel_rx => { crate::go_disconnected(&app, None, false); return; }
        r = tokio::task::spawn_blocking(move || {
            let filter = GanglionFilter { mac_address: preferred_mac, device_name: None };
            let cfg = GanglionConfig {
                scan_timeout: std::time::Duration::from_secs(scan_timeout_secs.into()),
                filter,
                ..Default::default()
            };
            let mut board = GanglionBoard::new(cfg);
            board.prepare().map(|_| board)
        }) => r,
    };

    let mut board = match board_result {
        Ok(Ok(b))  => b,
        Ok(Err(e)) => { let (m, bt) = classify_bt_error(&e.to_string()); crate::go_disconnected(&app, Some(m), bt); return; }
        Err(_)     => { crate::go_disconnected(&app, Some("Ganglion scan task panicked".into()), false); return; }
    };

    // 3. Derive device name/id
    let dev_name = preferred_id.as_ref()
        .and_then(|id| {
            let r = app.state::<Mutex<Box<AppState>>>();
            let s = r.lock_or_recover();
            s.status.paired_devices.iter()
                .find(|d| &d.id == id).map(|d| d.name.clone())
                .or_else(|| s.discovered.iter().find(|d| &d.id == id).map(|d| d.name.clone()))
        })
        .unwrap_or_else(|| "Ganglion".into());
    let dev_id = preferred_id.clone().unwrap_or_else(|| dev_name.clone());

    // Session-local DSP — created early so update_device can be called on connect.
    let mut dsp = SessionDsp::new(&app);

    // 4. → "connected"
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.status.state                = "connected".into();
        s.status.device_name          = Some(dev_name.clone());
        s.status.device_id            = Some(dev_id.clone());
        s.status.bt_error             = None;
        s.status.target_name          = None;
        s.retry_attempt               = 0;
        s.status.retry_attempt        = 0;
        s.status.retry_countdown_secs = 0;
    }
    dsp.accumulator.update_device(Some(dev_id.clone()), Some(dev_name.clone()));
    app_log!(app, "bluetooth", "[ganglion] connected: {dev_name} (id={dev_id})");
    upsert_paired(&app, &dev_id, &dev_name);
    refresh_tray(&app); emit_status(&app); emit_devices(&app);
    write_session_meta(&app, &csv_path);

    let connect_payload = serde_json::json!({
        "device_name": dev_name, "device_id": dev_id, "timestamp": unix_secs(),
    });
    let _ = app.emit("device-connected", &connect_payload);
    app.state::<WsBroadcaster>().send("device-connected", &connect_payload);
    send_toast(&app, ToastLevel::Success, "Connected",
        &format!("{dev_name} is now streaming EEG data."));

    // 5. Open CSV with configured channel labels
    let ch_labels: Vec<String> = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        let cfg_labels = &s.openbci_config.channel_labels;
        (0..4).map(|i| {
            cfg_labels.get(i)
                .filter(|l| !l.is_empty()).cloned()
                .unwrap_or_else(|| crate::constants::GANGLION_CHANNEL_NAMES[i].to_string())
        }).collect()
    };
    let label_refs: Vec<&str> = ch_labels.iter().map(|s| s.as_str()).collect();
    let mut csv = match CsvState::open_with_labels(&csv_path, &label_refs) {
        Ok(c)  => c,
        Err(e) => {
            write_session_meta(&app, &csv_path);
            crate::go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };
    write_session_meta(&app, &csv_path);

    // 6. Start streaming
    let stream_handle = match board.start_stream() {
        Ok(h)  => h,
        Err(e) => { crate::go_disconnected(&app, Some(format!("Ganglion start_stream: {e}")), false); return; }
    };

    // 7. Bridge blocking mpsc → async
    let (sample_tx, mut sample_rx) = tokio::sync::mpsc::channel::<openbci::sample::Sample>(256);
    let bridge_handle = tokio::task::spawn_blocking(move || {
        while let Some(s) = stream_handle.recv() {
            if sample_tx.blocking_send(s).is_err() { break; }
        }
    });

    // 8. Event loop
    let mut user_cancelled = false;
    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => { user_cancelled = true; break; }
            maybe_sample = sample_rx.recv() => {
                let Some(sample) = maybe_sample else {
                    app_log!(app, "bluetooth", "[ganglion] sample bridge closed");
                    break;
                };
                let ts_ms = sample.timestamp * 1000.0;

                dsp.sync_config(&app);

                // Brief lock: status write-back + IPC channel only.
                let ipc_ch = {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    let mut s = sr.lock_or_recover();
                    for (ch, &uv) in sample.eeg.iter().enumerate().take(EEG_CHANNELS) {
                        if ch < EEG_CHANNELS { s.status.eeg[ch] = uv; }
                    }
                    if let Some(accel) = sample.accel {
                        let a = [accel[0] as f32, accel[1] as f32, accel[2] as f32];
                        s.status.accel = a;
                    }
                    s.status.sample_count += 1;
                    s.eeg_channel.clone()
                }; // lock released — all DSP below is lock-free

                let mut filter_fired = false;
                let mut band_fired   = false;
                for (ch, &uv) in sample.eeg.iter().enumerate().take(EEG_CHANNELS) {
                    let one = [uv];
                    csv.push_eeg(ch, &one, sample.timestamp, EEG_SAMPLE_RATE);
                    if dsp.filter.push(ch, &one)        { filter_fired = true; }
                    if dsp.band_analyzer.push(ch, &one) { band_fired   = true; }
                    dsp.quality.push(ch, &one);
                    dsp.artifact_detector.push(ch, &one);
                    dsp.accumulator.push(ch, &[uv as f32]);
                }
                if let Some(accel) = sample.accel {
                    let a = [accel[0] as f32, accel[1] as f32, accel[2] as f32];
                    dsp.head_pose.update(a, [0.0f32; 3]);
                }

                let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                    (0..EEG_CHANNELS).map(|ch| (ch, dsp.filter.drain(ch)))
                        .filter(|(_, v)| !v.is_empty()).collect()
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
                if let Some(col) = spec_col { let _ = app.emit("eeg-spectrogram", &col); }
                if let Some(snap) = band_snap {
                    // Write back so get_latest_bands can read without DSP contention.
                    app.state::<Mutex<Box<AppState>>>().lock_or_recover().latest_bands = Some(snap.clone());
                    let _ = app.emit("eeg-bands", &snap);
                    app.state::<WsBroadcaster>().send("eeg-bands", &snap);
                }
            }
        }
    }

    // 9. Clean up
    let _ = board.stop_stream();
    let _ = bridge_handle.await;
    let _ = board.release();
    csv.flush();
    write_session_meta(&app, &csv_path);

    if !user_cancelled {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        if s.status.sample_count > 0 { s.pending_reconnect = true; }
    }
    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        r.lock_or_recover().status.device_kind = "unknown".into();
    }
    crate::go_disconnected(&app, error_msg, false);
}

// ── Generic board session (WiFi, Cyton serial, Galea) ─────────────────────────

/// Connect to any non-BLE OpenBCI board using the current `openbci_config`
/// and run the EEG session loop.
///
/// The first `min(ch_count, EEG_CHANNELS)` channels are routed through the
/// existing filter / band / embedding pipeline.  All channels are written to
/// the session CSV.
#[tauri::command]
pub(crate) async fn connect_openbci(app: AppHandle) -> Result<(), String> {
    use crate::settings::OpenBciBoard as Brd;

    let (board_kind, cfg) = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let s = r.lock_or_recover();
        (s.openbci_config.board.clone(), s.openbci_config.clone())
    };

    if board_kind.is_ble() {
        return Err("Use the main Connect button for Ganglion BLE.".into());
    }

    {
        let r = app.state::<Mutex<Box<AppState>>>();
        if r.lock_or_recover().stream.is_some() {
            return Err("Already connected or connecting.".into());
        }
    }

    let board: Box<dyn openbci::board::Board> = match board_kind.clone() {
        Brd::GanglionWifi => Box::new(GanglionWifiBoard::new(GanglionWifiConfig {
            shield_ip: cfg.wifi_shield_ip.clone(), local_port: cfg.wifi_local_port, http_timeout: 10,
        })),
        Brd::Cyton => {
            let port = if cfg.serial_port.is_empty() {
                serialport::available_ports().unwrap_or_default().into_iter().next()
                    .map(|p| p.port_name)
                    .ok_or("No serial ports found. Connect the USB dongle and try again.")?
            } else { cfg.serial_port.clone() };
            Box::new(CytonBoard::new(port))
        }
        Brd::CytonWifi => Box::new(CytonWifiBoard::new(CytonWifiConfig {
            shield_ip: cfg.wifi_shield_ip.clone(), local_port: cfg.wifi_local_port, http_timeout: 10,
        })),
        Brd::CytonDaisy => {
            let port = if cfg.serial_port.is_empty() {
                serialport::available_ports().unwrap_or_default().into_iter().next()
                    .map(|p| p.port_name)
                    .ok_or("No serial ports found. Connect the USB dongle and try again.")?
            } else { cfg.serial_port.clone() };
            Box::new(CytonDaisyBoard::new(port))
        }
        Brd::CytonDaisyWifi => Box::new(CytonDaisyWifiBoard::new(CytonDaisyWifiConfig {
            shield_ip: cfg.wifi_shield_ip.clone(), local_port: cfg.wifi_local_port, http_timeout: 10,
        })),
        Brd::Galea    => Box::new(GaleaBoard::new(cfg.galea_ip.clone())),
        Brd::Ganglion => unreachable!(),
    };

    let ch_count    = board_kind.channel_count();
    let sample_rate = board_kind.sample_rate();
    let kind_str    = format!("openbci_{}", serde_json::to_string(&board_kind)
                              .unwrap_or_default().trim_matches('"'));

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let csv_path = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.stream             = Some(StreamHandle { cancel_tx: tx });
        s.status.state       = "scanning".into();
        s.status.device_kind = kind_str;
        new_csv_path(&app)
    };
    emit_status(&app);

    let app2 = app.clone();
    tokio::spawn(async move {
        run_openbci_board_session(app2, rx, csv_path, board, ch_count, sample_rate).await;
    });
    Ok(())
}

async fn run_openbci_board_session(
    app:         AppHandle,
    cancel_rx:   tokio::sync::oneshot::Receiver<()>,
    csv_path:    PathBuf,
    board:       Box<dyn openbci::board::Board>,
    ch_count:    usize,
    sample_rate: f64,
) {
    tokio::pin!(cancel_rx);

    // 1. Connect (blocking)
    let connect_result = tokio::select! {
        biased;
        _ = &mut cancel_rx => { crate::go_disconnected(&app, None, false); return; }
        r = tokio::task::spawn_blocking(move || {
            let mut b = board; b.prepare().map(|_| b)
        }) => r,
    };

    let mut board = match connect_result {
        Ok(Ok(b))  => b,
        Ok(Err(e)) => { crate::go_disconnected(&app, Some(format!("OpenBCI connect error: {e}")), false); return; }
        Err(e)     => { crate::go_disconnected(&app, Some(format!("OpenBCI thread error: {e}")), false); return; }
    };

    // 2. Mark connected; build channel labels
    let ch_labels: Vec<String> = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let mut s = r.lock_or_recover();
        s.status.state = "connected".into();
        let cfg_labels = &s.openbci_config.channel_labels;
        (0..ch_count).map(|i| {
            cfg_labels.get(i).filter(|l| !l.is_empty()).cloned()
                .unwrap_or_else(|| format!("Ch{}", i + 1))
        }).collect()
    };
    emit_status(&app);

    // 3. Open CSV
    let label_refs: Vec<&str> = ch_labels.iter().map(|s| s.as_str()).collect();
    let mut csv = match CsvState::open_with_labels(&csv_path, &label_refs) {
        Ok(c)  => c,
        Err(e) => {
            write_session_meta(&app, &csv_path);
            crate::go_disconnected(&app, Some(format!("CSV error: {e}")), false);
            return;
        }
    };
    write_session_meta(&app, &csv_path);

    // 4. Start streaming
    let stream_handle = match board.start_stream() {
        Ok(h)  => h,
        Err(e) => { crate::go_disconnected(&app, Some(format!("OpenBCI start_stream: {e}")), false); return; }
    };

    // 5. Bridge blocking mpsc → async
    let (sample_tx, mut sample_rx) = tokio::sync::mpsc::channel::<openbci::sample::Sample>(256);
    let bridge = tokio::task::spawn_blocking(move || {
        while let Some(s) = stream_handle.recv() {
            if sample_tx.blocking_send(s).is_err() { break; }
        }
    });

    // Session-local DSP — lock-free during sample processing.
    let mut dsp = SessionDsp::new(&app);

    // 6. Event loop
    let pipeline_ch = ch_count.min(EEG_CHANNELS);
    let mut user_cancelled = false;

    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => { user_cancelled = true; break; }
            maybe_sample = sample_rx.recv() => {
                let Some(sample) = maybe_sample else { break; };
                let ts_ms = sample.timestamp * 1000.0;

                dsp.sync_config(&app);

                // Brief lock: status write-back + IPC clone only.
                let ipc_ch = {
                    let sr = app.state::<Mutex<Box<AppState>>>();
                    let mut s = sr.lock_or_recover();
                    for (ch, &uv) in sample.eeg.iter().enumerate() {
                        if ch < pipeline_ch && ch < EEG_CHANNELS { s.status.eeg[ch] = uv; }
                    }
                    s.status.sample_count += 1;
                    s.eeg_channel.clone()
                }; // lock released — all DSP below is lock-free

                let mut filter_fired = false;
                let mut band_fired   = false;
                for (ch, &uv) in sample.eeg.iter().enumerate() {
                    let one = [uv];
                    csv.push_eeg(ch, &one, sample.timestamp, sample_rate);
                    if ch < pipeline_ch {
                        if dsp.filter.push(ch, &one)        { filter_fired = true; }
                        if dsp.band_analyzer.push(ch, &one) { band_fired   = true; }
                        dsp.quality.push(ch, &one);
                        dsp.artifact_detector.push(ch, &one);
                        dsp.accumulator.push(ch, &[uv as f32]);
                    }
                }

                let drained: Vec<(usize, Vec<f64>)> = if filter_fired {
                    (0..pipeline_ch).map(|ch| (ch, dsp.filter.drain(ch)))
                        .filter(|(_, v)| !v.is_empty()).collect()
                } else { Vec::new() };
                let spec_col  = dsp.filter.take_spec_col();
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
                if let Some(col) = spec_col { let _ = app.emit("eeg-spectrogram", &col); }
                if let Some(snap) = band_snap {
                    app.state::<Mutex<Box<AppState>>>().lock_or_recover().latest_bands = Some(snap.clone());
                    let _ = app.emit("eeg-bands", &snap);
                    app.state::<WsBroadcaster>().send("eeg-bands", &snap);
                }
            }
        }
    }

    // 7. Clean up
    let _ = board.stop_stream();
    let _ = bridge.await;
    let _ = board.release();
    csv.flush();
    write_session_meta(&app, &csv_path);
    {
        let r = app.state::<Mutex<Box<AppState>>>();
        r.lock_or_recover().status.device_kind = "unknown".into();
    }
    let error_msg = if user_cancelled { None } else { Some("DEVICE_DISCONNECTED".into()) };
    crate::go_disconnected(&app, error_msg, false);
}
