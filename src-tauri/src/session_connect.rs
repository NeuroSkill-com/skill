// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Per-device scan / connect factories.
//!
//! Each factory performs the device-specific BLE scanning, pairing, and
//! activation dance, then returns a `Box<dyn DeviceAdapter>` ready for
//! [`session_runner::run_device_session`].
//!
//! All Tauri state interactions (reading paired devices, updating status)
//! happen here.  The adapters themselves are Tauri-free.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use skill_devices::session::*;

use crate::AppStateExt;
use crate::{
    AppState, MutexExt, StreamHandle,
    emit_status,
};
use crate::ble_scanner::{bluetooth_ok, classify_bt_error};
use crate::session_csv::new_csv_path;

// ── Error type ────────────────────────────────────────────────────────────────

pub(crate) enum ConnectError {
    /// A Bluetooth / transport error with a user-facing message.
    Bluetooth(String),
    /// User pressed cancel.
    Cancelled,
    /// Any other error.
    Other(String),
}

// ── Muse ──────────────────────────────────────────────────────────────────────

pub(crate) async fn connect_muse(
    app:       &AppHandle,
    cancel: &tokio_util::sync::CancellationToken,
    preferred_id: Option<String>,
) -> Result<Box<dyn DeviceAdapter>, ConnectError> {
    use skill_devices::muse_rs::prelude::*;
    use skill_devices::session::muse::MuseAdapter;

    // BT check
    if let Err((msg, _)) = bluetooth_ok().await {
        return Err(ConnectError::Bluetooth(msg));
    }

    // Scan
    let config = MuseClientConfig {
        scan_timeout_secs: 10,
        enable_ppg: true,
        ..Default::default()
    };
    let client = MuseClient::new(config);
    let all_devices = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = client.scan_all() => match r {
            Err(e) => {
                let (m, _) = classify_bt_error(&e.to_string());
                return Err(ConnectError::Bluetooth(m));
            }
            Ok(d) => d,
        }
    };

    // Pick device
    let paired_ids: Vec<String> = {
        let r = app.app_state();
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
        None => return Err(ConnectError::Other("NO_MUSE_NEARBY".into())),
    };

    // Pin real BLE ID into status.
    {
        let sr = app.app_state();
        let mut g = sr.lock_or_recover();
        g.status.device_id = Some(device.id.clone());
        g.retry_attempt    = 0;
    }

    // Connect
    let (rx, handle) = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = client.connect_to(device) => match r {
            Err(e) => {
                let (m, _) = classify_bt_error(&e.to_string());
                return Err(ConnectError::Bluetooth(m));
            }
            Ok(v) => v,
        }
    };

    // Start streaming
    tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = handle.disconnect().await;
            return Err(ConnectError::Cancelled);
        }
        r = handle.start(false, false) => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[muse] start: {e}");
            }
        }
    }

    Ok(Box::new(MuseAdapter::new(rx, handle)))
}

// ── MW75 ──────────────────────────────────────────────────────────────────────

pub(crate) async fn connect_mw75(
    app:          &AppHandle,
    cancel:       &tokio_util::sync::CancellationToken,
    preferred_id: Option<String>,
) -> Result<Box<dyn DeviceAdapter>, ConnectError> {
    use skill_devices::mw75::prelude::*;
    use skill_devices::session::mw75::Mw75Adapter;

    // BT check
    if let Err((msg, _)) = bluetooth_ok().await {
        return Err(ConnectError::Bluetooth(msg));
    }

    // BLE discover + connect
    let config = Mw75ClientConfig {
        scan_timeout_secs: 10,
        ..Default::default()
    };
    let client = Mw75Client::new(config);
    app_log!(app, "bluetooth", "[mw75] connecting (preferred={preferred_id:?})…");

    // Scan, then select preferred device (or first found).
    let connect_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = async {
            let devices = client.scan_all().await.map_err(|e| format!("{e}"))?;
            if devices.is_empty() {
                return Err("No MW75 devices found during scan.".into());
            }
            let device = if let Some(ref pref) = preferred_id {
                devices.into_iter().find(|d| &d.id == pref)
                    .ok_or_else(|| format!("Preferred MW75 ({pref}) not found; try re-pairing."))?
            } else {
                devices.into_iter().next().unwrap()
            };
            client.connect_to(device).await.map_err(|e| format!("{e}"))
        } => r,
    };

    let (mut rx, handle) = match connect_result {
        Ok(v) => v,
        Err(msg) => {
            app_log!(app, "bluetooth", "[mw75] connect failed: {msg}");
            let (m, _) = classify_bt_error(&msg);
            return Err(ConnectError::Bluetooth(format!(
                "{m}\n\nTo pair MW75: hold the power button for 4+ seconds,\n\
                 then pair in System Bluetooth Settings."
            )));
        }
    };

    app_log!(app, "bluetooth", "[mw75] BLE connected, starting activation…");

    // BLE activation
    tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = handle.disconnect().await;
            return Err(ConnectError::Cancelled);
        }
        r = handle.start() => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[mw75] BLE activation failed: {e}");
                let _ = handle.disconnect().await;
                return Err(ConnectError::Other(format!("MW75 activation failed: {e}")));
            }
        }
    }
    app_log!(app, "bluetooth", "[mw75] activation complete");

    // Disconnect BLE before RFCOMM (required on macOS).
    let bt_address = handle.peripheral_id();
    app_log!(app, "bluetooth", "[mw75] disconnecting BLE (addr={bt_address})…");
    if let Err(e) = handle.disconnect_ble().await {
        app_log!(app, "bluetooth", "[mw75] BLE disconnect warning: {e}");
    }

    let handle = Arc::new(handle);

    // RFCOMM transport (if feature enabled).
    #[cfg(feature = "mw75-rfcomm")]
    {
        app_log!(app, "bluetooth", "[mw75] starting RFCOMM stream…");
        let rfcomm = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
            r = skill_devices::mw75::rfcomm::start_rfcomm_stream(handle.clone(), &bt_address) => match r {
                Err(e) => {
                    app_log!(app, "bluetooth", "[mw75] RFCOMM failed: {e}");
                    return Err(ConnectError::Other(format!(
                        "MW75 RFCOMM failed: {e}\n\n\
                         Make sure the headphones are paired in System Bluetooth Settings.\n\
                         To pair: hold the power button for 4+ seconds to enter pairing mode."
                    )));
                }
                Ok(r) => r,
            }
        };
        app_log!(app, "bluetooth", "[mw75] RFCOMM connected — streaming EEG at {} Hz",
            skill_constants::MW75_SAMPLE_RATE);

        // Drain stale BLE events.
        let mut drained = 0u32;
        while rx.try_recv().is_ok() { drained += 1; }
        if drained > 0 {
            app_log!(app, "bluetooth", "[mw75] drained {drained} stale BLE events");
        }

        // Inject synthetic Connected + attach RFCOMM guard.
        let info = DeviceInfo {
            name: handle.device_name().to_string(),
            id:   handle.peripheral_id(),
            ..Default::default()
        };
        let mut adapter = Mw75Adapter::new(rx, handle.clone(), Some(info));
        adapter.set_rfcomm(rfcomm);
        Ok(Box::new(adapter))
    }

    #[cfg(not(feature = "mw75-rfcomm"))]
    {
        app_log!(app, "bluetooth",
            "[mw75] RFCOMM feature disabled — receiving EEG via BLE notifications");
        let adapter = Mw75Adapter::new(rx, handle.clone(), None);
        Ok(Box::new(adapter))
    }
}

// ── Hermes ────────────────────────────────────────────────────────────────────

pub(crate) async fn connect_hermes(
    app:          &AppHandle,
    cancel:       &tokio_util::sync::CancellationToken,
    preferred_id: Option<String>,
) -> Result<Box<dyn DeviceAdapter>, ConnectError> {
    use skill_devices::hermes_ble::prelude::*;
    use skill_devices::session::hermes::HermesAdapter;

    // BT check
    if let Err((msg, _)) = bluetooth_ok().await {
        return Err(ConnectError::Bluetooth(msg));
    }

    let config = HermesClientConfig {
        scan_timeout_secs: 15,
        ..Default::default()
    };
    let client = HermesClient::new(config);
    app_log!(app, "bluetooth", "[hermes] connecting (preferred={preferred_id:?})…");

    // Scan, then select preferred device (or first found).
    let connect_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = async {
            let devices = client.scan_all().await.map_err(|e| format!("{e}"))?;
            if devices.is_empty() {
                return Err("No Hermes devices found during scan.".into());
            }
            let device = if let Some(ref pref) = preferred_id {
                devices.into_iter().find(|d| &d.id == pref)
                    .ok_or_else(|| format!("Preferred Hermes ({pref}) not found; try re-pairing."))?
            } else {
                devices.into_iter().next().unwrap()
            };
            client.connect_to(device).await.map_err(|e| format!("{e}"))
        } => r,
    };

    let (rx, handle) = match connect_result {
        Ok(v) => v,
        Err(msg) => {
            app_log!(app, "bluetooth", "[hermes] connect failed: {msg}");
            let (m, _) = classify_bt_error(&msg);
            return Err(ConnectError::Bluetooth(m));
        }
    };

    app_log!(app, "bluetooth", "[hermes] BLE connected, starting streaming…");

    // Start streaming
    tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = handle.disconnect().await;
            return Err(ConnectError::Cancelled);
        }
        r = handle.start() => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[hermes] start failed: {e}");
                let _ = handle.disconnect().await;
                return Err(ConnectError::Other(format!("Hermes start failed: {e}")));
            }
        }
    }
    app_log!(app, "bluetooth", "[hermes] streaming started — 8ch EEG at {} Hz",
        skill_constants::HERMES_SAMPLE_RATE);

    Ok(Box::new(HermesAdapter::new(rx, handle)))
}

// ── OpenBCI Ganglion BLE ──────────────────────────────────────────────────────

pub(crate) async fn connect_ganglion(
    app:          &AppHandle,
    cancel:       &tokio_util::sync::CancellationToken,
    preferred_id: Option<String>,
) -> Result<Box<dyn DeviceAdapter>, ConnectError> {
    use skill_devices::openbci::board::Board as _;
    use skill_devices::openbci::board::ganglion::{GanglionBoard, GanglionConfig, GanglionFilter};
    use skill_devices::session::openbci::OpenBciAdapter;

    // BT check
    if let Err((msg, _)) = bluetooth_ok().await {
        return Err(ConnectError::Bluetooth(msg));
    }

    let preferred_mac = preferred_id.clone();
    let scan_timeout_secs = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.openbci_config.scan_timeout_secs
    };

    let board_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = tokio::task::spawn_blocking(move || {
            let filter = GanglionFilter { mac_address: preferred_mac, device_name: None };
            let cfg = GanglionConfig {
                scan_timeout: Duration::from_secs(scan_timeout_secs.into()),
                filter,
                ..Default::default()
            };
            let mut board = GanglionBoard::new(cfg);
            board.prepare().map(|_| board)
        }) => r,
    };

    let mut board = match board_result {
        Ok(Ok(b))  => b,
        Ok(Err(e)) => {
            let (m, _) = classify_bt_error(&e.to_string());
            return Err(ConnectError::Bluetooth(m));
        }
        Err(_) => return Err(ConnectError::Other("Ganglion scan task panicked".into())),
    };

    // Derive device name/id
    let dev_name = preferred_id.as_ref()
        .and_then(|id| {
            let r = app.app_state();
            let s = r.lock_or_recover();
            s.status.paired_devices.iter()
                .find(|d| &d.id == id).map(|d| d.name.clone())
                .or_else(|| s.discovered.iter().find(|d| &d.id == id).map(|d| d.name.clone()))
        })
        .unwrap_or_else(|| "Ganglion".into());
    let dev_id = preferred_id.clone().unwrap_or_else(|| dev_name.clone());

    // Build channel labels
    let ch_labels: Vec<String> = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        let cfg_labels = &s.openbci_config.channel_labels;
        (0..4).map(|i| {
            cfg_labels.get(i)
                .filter(|l| !l.is_empty()).cloned()
                .unwrap_or_else(|| crate::constants::GANGLION_CHANNEL_NAMES[i].to_string())
        }).collect()
    };

    // Start stream
    let stream_handle = match board.start_stream() {
        Ok(h)  => h,
        Err(e) => return Err(ConnectError::Other(format!("Ganglion start_stream: {e}"))),
    };

    let desc = OpenBciAdapter::make_descriptor(
        "ganglion", 4,
        skill_constants::GANGLION_SAMPLE_RATE,
        ch_labels,
    );
    let info = DeviceInfo {
        name: dev_name,
        id: dev_id,
        ..Default::default()
    };

    Ok(Box::new(OpenBciAdapter::start(stream_handle, desc, info)))
}

// ── OpenBCI Generic Board (WiFi, Cyton serial, Galea) ─────────────────────────

pub(crate) async fn connect_openbci_board(
    app:       &AppHandle,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(Box<dyn DeviceAdapter>, String), ConnectError> {
    use crate::settings::OpenBciBoard as Brd;
    use skill_devices::openbci::board::cyton::CytonBoard;
    use skill_devices::openbci::board::cyton_daisy::CytonDaisyBoard;
    use skill_devices::openbci::board::cyton_wifi::{CytonWifiBoard, CytonWifiConfig};
    use skill_devices::openbci::board::cyton_daisy_wifi::{CytonDaisyWifiBoard, CytonDaisyWifiConfig};
    use skill_devices::openbci::board::ganglion_wifi::{GanglionWifiBoard, GanglionWifiConfig};
    use skill_devices::openbci::board::galea::GaleaBoard;
    use skill_devices::openbci::board::Board as OpenBciBoard;
    use skill_devices::session::openbci::OpenBciAdapter;

    let (board_kind, cfg) = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        (s.openbci_config.board.clone(), s.openbci_config.clone())
    };

    if board_kind.is_ble() {
        return Err(ConnectError::Other("Use the main Connect button for Ganglion BLE.".into()));
    }

    let board: Box<dyn OpenBciBoard> = match board_kind.clone() {
        Brd::GanglionWifi => Box::new(GanglionWifiBoard::new(GanglionWifiConfig {
            shield_ip: cfg.wifi_shield_ip.clone(), local_port: cfg.wifi_local_port, http_timeout: 10,
        })),
        Brd::Cyton => {
            let port = if cfg.serial_port.is_empty() {
                serialport::available_ports().unwrap_or_default().into_iter().next()
                    .map(|p| p.port_name)
                    .ok_or_else(|| ConnectError::Other(
                        "No serial ports found. Connect the USB dongle and try again.".into()
                    ))?
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
                    .ok_or_else(|| ConnectError::Other(
                        "No serial ports found. Connect the USB dongle and try again.".into()
                    ))?
            } else { cfg.serial_port.clone() };
            Box::new(CytonDaisyBoard::new(port))
        }
        Brd::CytonDaisyWifi => Box::new(CytonDaisyWifiBoard::new(CytonDaisyWifiConfig {
            shield_ip: cfg.wifi_shield_ip.clone(), local_port: cfg.wifi_local_port, http_timeout: 10,
        })),
        Brd::Galea => Box::new(GaleaBoard::new(cfg.galea_ip.clone())),
        Brd::Ganglion => unreachable!(),
    };

    let ch_count    = board_kind.channel_count();
    let sample_rate = board_kind.sample_rate();
    let kind_str    = format!("openbci_{}", serde_json::to_string(&board_kind)
                              .unwrap_or_default().trim_matches('"'));

    // Connect (blocking)
    let connect_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = tokio::task::spawn_blocking(move || {
            let mut b = board; b.prepare().map(|_| b)
        }) => r,
    };

    let mut board = match connect_result {
        Ok(Ok(b))  => b,
        Ok(Err(e)) => return Err(ConnectError::Other(format!("OpenBCI connect error: {e}"))),
        Err(e)     => return Err(ConnectError::Other(format!("OpenBCI thread error: {e}"))),
    };

    // Build channel labels
    let ch_labels: Vec<String> = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        let cfg_labels = &s.openbci_config.channel_labels;
        (0..ch_count).map(|i| {
            cfg_labels.get(i).filter(|l| !l.is_empty()).cloned()
                .unwrap_or_else(|| format!("Ch{}", i + 1))
        }).collect()
    };

    // Start stream
    let stream_handle = match board.start_stream() {
        Ok(h) => h,
        Err(e) => return Err(ConnectError::Other(format!("OpenBCI start_stream: {e}"))),
    };

    let desc = OpenBciAdapter::make_descriptor(
        // Leak a &'static str for the kind_str (lives for the process lifetime, one per session).
        Box::leak(kind_str.clone().into_boxed_str()),
        ch_count, sample_rate, ch_labels,
    );
    let info = DeviceInfo {
        name: kind_str.clone(),
        id:   kind_str.clone(),
        ..Default::default()
    };

    Ok((Box::new(OpenBciAdapter::start(stream_handle, desc, info)), kind_str))
}

// ── Emotiv (Cortex WebSocket API) ──────────────────────────────────────────────

pub(crate) async fn connect_emotiv(
    app:    &AppHandle,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Box<dyn DeviceAdapter>, ConnectError> {
    use skill_devices::emotiv::prelude::*;
    use skill_devices::session::emotiv::EmotivAdapter;

    app_log!(app, "bluetooth", "[emotiv] connecting via Cortex API…");

    let device_api = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.device_api_config.clone()
    };

    let client_id = if device_api.emotiv_client_id.trim().is_empty() {
        std::env::var("EMOTIV_CLIENT_ID").unwrap_or_default()
    } else {
        device_api.emotiv_client_id
    };

    let client_secret = if device_api.emotiv_client_secret.trim().is_empty() {
        std::env::var("EMOTIV_CLIENT_SECRET").unwrap_or_default()
    } else {
        device_api.emotiv_client_secret
    };

    let config = CortexClientConfig {
        client_id,
        client_secret,
        ..Default::default()
    };

    if config.client_id.is_empty() || config.client_secret.is_empty() {
        return Err(ConnectError::Other(
            "Emotiv credentials not configured.\n\n\
             Set Emotiv credentials in Settings → Devices → Device API, or set\n\
             EMOTIV_CLIENT_ID and EMOTIV_CLIENT_SECRET environment variables\n\
             (from https://www.emotiv.com/my-account/cortex-apps/)."
            .into(),
        ));
    }

    let client = CortexClient::new(config);

    let connect_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = client.connect() => r.map_err(|e| format!("{e}")),
    };

    let (rx, handle) = match connect_result {
        Ok(v) => v,
        Err(msg) => {
            app_log!(app, "bluetooth", "[emotiv] connect failed: {msg}");
            return Err(ConnectError::Other(format!(
                "Emotiv Cortex connection failed: {msg}\n\n\
                 Make sure the EMOTIV Launcher is running and a headset is connected."
            )));
        }
    };

    // Subscribe to EEG, motion, and device (battery) streams.
    tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = handle.subscribe(&[STREAM_EEG, STREAM_MOT, STREAM_DEV]) => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[emotiv] subscribe failed: {e}");
                return Err(ConnectError::Other(format!("Emotiv subscribe failed: {e}")));
            }
        }
    }

    app_log!(app, "bluetooth", "[emotiv] connected — streaming EEG at {} Hz",
        skill_constants::EMOTIV_SAMPLE_RATE);

    Ok(Box::new(EmotivAdapter::new_epoc(rx, handle)))
}

// ── IDUN Guardian (BLE) ───────────────────────────────────────────────────────

pub(crate) async fn connect_idun(
    app:    &AppHandle,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Box<dyn DeviceAdapter>, ConnectError> {
    use skill_devices::idun::prelude::*;
    use skill_devices::session::idun::IdunAdapter;

    // BT check
    if let Err((msg, _)) = bluetooth_ok().await {
        return Err(ConnectError::Bluetooth(msg));
    }

    let idun_token = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        s.device_api_config.idun_api_token.clone()
    };

    let use_60hz = {
        let r = app.app_state();
        let s = r.lock_or_recover();
        // Match the user's notch filter setting; default to 60 Hz if unset.
        match s.status.filter_config.notch {
            Some(skill_eeg::eeg_filter::PowerlineFreq::Hz50) => false,
            _ => true,
        }
    };

    let config = GuardianClientConfig {
        api_token: if idun_token.trim().is_empty() { None } else { Some(idun_token) },
        use_60hz,
        ..GuardianClientConfig::default()
    };
    let client = GuardianClient::new(config);
    app_log!(app, "bluetooth", "[idun] connecting…");

    let connect_result = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(ConnectError::Cancelled),
        r = client.connect() => r.map_err(|e| format!("{e}")),
    };

    let (rx, handle) = match connect_result {
        Ok(v) => v,
        Err(msg) => {
            app_log!(app, "bluetooth", "[idun] connect failed: {msg}");
            let (m, _) = classify_bt_error(&msg);
            return Err(ConnectError::Bluetooth(m));
        }
    };

    app_log!(app, "bluetooth", "[idun] BLE connected, starting recording…");

    // Start EEG + IMU streaming.
    tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            let _ = handle.disconnect().await;
            return Err(ConnectError::Cancelled);
        }
        r = handle.start_recording() => {
            if let Err(e) = r {
                app_log!(app, "bluetooth", "[idun] start_recording failed: {e}");
                let _ = handle.disconnect().await;
                return Err(ConnectError::Other(format!("IDUN start_recording failed: {e}")));
            }
        }
    }

    app_log!(app, "bluetooth", "[idun] streaming started — 1ch EEG at {} Hz",
        skill_constants::IDUN_SAMPLE_RATE);

    Ok(Box::new(IdunAdapter::new(rx, handle)))
}

// ── Tauri command: connect_openbci ────────────────────────────────────────────

/// Connect to any non-BLE OpenBCI board using the current `openbci_config`
/// and run the EEG session loop via the generic session runner.
#[tauri::command]
pub(crate) async fn connect_openbci(app: AppHandle) -> Result<(), String> {
    {
        let r = app.app_state();
        if r.lock_or_recover().stream.is_some() {
            return Err("Already connected or connecting.".into());
        }
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let csv_path = {
        let r = app.app_state();
        let mut s = r.lock_or_recover();
        s.stream       = Some(StreamHandle { cancel_tx: tx });
        s.status.state = "scanning".into();
        new_csv_path(&app)
    };
    emit_status(&app);

    let app2 = app.clone();
    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel2 = cancel.clone();
    tokio::spawn(async move {
        let _ = rx.await;
        cancel2.cancel();
    });

    tokio::spawn(async move {
        let connect_result = connect_openbci_board(&app2, &cancel).await;

        match connect_result {
            Ok((adapter, kind_str)) => {
                {
                    let r = app2.state::<Mutex<Box<AppState>>>();
                    r.lock_or_recover().status.device_kind = kind_str;
                }
                crate::session_runner::run_device_session(app2, cancel, csv_path, adapter).await;
            }
            Err(ConnectError::Cancelled) => {
                crate::go_disconnected(&app2, None, false);
            }
            Err(ConnectError::Bluetooth(msg)) => {
                crate::go_disconnected(&app2, Some(msg), true);
            }
            Err(ConnectError::Other(msg)) => {
                crate::go_disconnected(&app2, Some(msg), false);
            }
        }
    });
    Ok(())
}
