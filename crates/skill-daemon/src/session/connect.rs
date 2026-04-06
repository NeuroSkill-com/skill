// SPDX-License-Identifier: GPL-3.0-only
//! Per-device connection logic — transport-specific setup (BLE, serial,
//! Cortex WS, PCAN) → `Box<dyn DeviceAdapter>` for the generic runner.

use std::time::Duration;

use skill_devices::session::{DeviceAdapter, DeviceInfo};
use tokio::sync::oneshot;
use tracing::{error, info};

use super::runner::run_adapter_session;
use crate::session_runner::SessionHandle;
use crate::state::AppState;

/// Spawn a device session for the given target.  Returns a cancel handle.
pub fn spawn_device_session(state: AppState, target: String) -> Option<SessionHandle> {
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let state2 = state.clone();

    tokio::task::spawn(async move {
        if let Ok(mut s) = state2.status.lock() {
            s.state = "connecting".into();
            s.target_name = Some(target.clone());
            s.device_error = None;
        }

        match connect_device(&state2, &target).await {
            Ok(adapter) => {
                run_adapter_session(state2.clone(), cancel_rx, adapter).await;
            }
            Err(e) => {
                error!(%e, %target, "device connect failed");
                if let Ok(mut s) = state2.status.lock() {
                    s.state = "disconnected".into();
                    s.device_error = Some(e);
                }
            }
        }
        if let Ok(mut slot) = state2.session_handle.lock() {
            *slot = None;
        }
    });

    Some(SessionHandle { cancel_tx })
}

async fn connect_device(state: &AppState, target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    let lower = target.to_lowercase();

    if lower == "openbci" || lower.starts_with("usb:") {
        return connect_openbci(state, target).await;
    }
    if lower.starts_with("cgx:") {
        return connect_cognionics(target).await;
    }
    if lower.starts_with("cortex:") {
        return connect_emotiv(state).await;
    }
    if lower == "ganglion" {
        return connect_ganglion(state).await;
    }
    if lower.starts_with("brainbit:") || lower.contains("brainbit") {
        return connect_brainbit(target).await;
    }
    if lower.starts_with("gtec:") || lower.contains("unicorn") {
        return connect_gtec(target).await;
    }
    if lower.contains("mw75") || lower.contains("neurable") {
        return connect_mw75().await;
    }
    if lower.contains("hermes") {
        return connect_hermes().await;
    }
    if lower.contains("idun") || lower.contains("guardian") {
        return connect_idun(state).await;
    }
    if lower.contains("mendi") {
        return connect_mendi().await;
    }

    // Default: Muse
    connect_muse().await
}

// ── Muse (BLE) ──────────────────────────────────────────────────────────────

async fn connect_muse() -> Result<Box<dyn DeviceAdapter>, String> {
    use skill_devices::muse_rs::prelude::*;
    use skill_devices::session::muse::MuseAdapter;

    info!("scanning for Muse headband…");
    let config = MuseClientConfig {
        scan_timeout_secs: 10,
        enable_ppg: true,
        ..Default::default()
    };
    let client = MuseClient::new(config);
    let devices = client.scan_all().await.map_err(|e| format!("Muse scan: {e}"))?;
    let device = devices.into_iter().next().ok_or("No Muse device found nearby")?;
    info!(name = %device.name, "connecting to Muse");
    let (rx, handle) = client
        .connect_to(device)
        .await
        .map_err(|e| format!("Muse connect: {e}"))?;
    Ok(Box::new(MuseAdapter::new(rx, handle)))
}

// ── MW75 Neuro (BLE) ────────────────────────────────────────────────────────

async fn connect_mw75() -> Result<Box<dyn DeviceAdapter>, String> {
    use skill_devices::mw75::prelude::*;
    use skill_devices::session::mw75::Mw75Adapter;

    info!("scanning for MW75 Neuro…");
    let config = Mw75ClientConfig {
        scan_timeout_secs: 15,
        ..Default::default()
    };
    let client = Mw75Client::new(config);
    let devices = client.scan_all().await.map_err(|e| format!("MW75 scan: {e}"))?;
    let device = devices.into_iter().next().ok_or("No MW75 device found")?;
    info!(name = %device.name, "connecting to MW75");
    let (rx, handle) = client
        .connect_to(device)
        .await
        .map_err(|e| format!("MW75 connect: {e}"))?;
    let handle = std::sync::Arc::new(handle);
    Ok(Box::new(Mw75Adapter::new(rx, handle, None)))
}

// ── Hermes V1 (BLE) ─────────────────────────────────────────────────────────

async fn connect_hermes() -> Result<Box<dyn DeviceAdapter>, String> {
    use skill_devices::hermes_ble::prelude::*;
    use skill_devices::session::hermes::HermesAdapter;

    info!("scanning for Hermes…");
    let config = HermesClientConfig {
        scan_timeout_secs: 15,
        ..Default::default()
    };
    let client = HermesClient::new(config);
    let devices = client.scan_all().await.map_err(|e| format!("Hermes scan: {e}"))?;
    let device = devices.into_iter().next().ok_or("No Hermes device found")?;
    info!(name = %device.name, "connecting to Hermes");
    let (rx, handle) = client
        .connect_to(device)
        .await
        .map_err(|e| format!("Hermes connect: {e}"))?;
    Ok(Box::new(HermesAdapter::new(rx, handle)))
}

// ── IDUN Guardian (BLE) ──────────────────────────────────────────────────────

async fn connect_idun(state: &AppState) -> Result<Box<dyn DeviceAdapter>, String> {
    use skill_devices::idun::prelude::*;
    use skill_devices::session::idun::IdunAdapter;

    let api_token = {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        skill_settings::load_settings(&skill_dir)
            .device_api
            .idun_api_token
            .clone()
    };

    info!("connecting to IDUN Guardian…");
    let config = GuardianClientConfig {
        api_token: if api_token.is_empty() { None } else { Some(api_token) },
        ..Default::default()
    };
    let client = GuardianClient::new(config);
    let (rx, handle) = client.connect().await.map_err(|e| format!("IDUN connect: {e}"))?;
    Ok(Box::new(IdunAdapter::new(rx, handle)))
}

// ── Mendi fNIRS (BLE) ────────────────────────────────────────────────────────

async fn connect_mendi() -> Result<Box<dyn DeviceAdapter>, String> {
    use skill_devices::mendi::prelude::*;
    use skill_devices::session::mendi::MendiAdapter;

    info!("scanning for Mendi…");
    let client = MendiClient::new(MendiClientConfig::default());
    let devices = client.scan().await.map_err(|e| format!("Mendi scan: {e}"))?;
    let device = devices.into_iter().next().ok_or("No Mendi device found")?;
    info!(name = %device.name, "connecting to Mendi");
    let (rx, handle) = client
        .connect_to(device)
        .await
        .map_err(|e| format!("Mendi connect: {e}"))?;
    Ok(Box::new(MendiAdapter::new(rx, handle)))
}

// ── OpenBCI Ganglion (BLE) ───────────────────────────────────────────────────

async fn connect_ganglion(state: &AppState) -> Result<Box<dyn DeviceAdapter>, String> {
    use skill_devices::openbci::board::ganglion::{GanglionBoard, GanglionConfig};
    use skill_devices::openbci::board::Board;
    use skill_devices::session::openbci::OpenBciAdapter;

    let config = {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        skill_settings::load_settings(&skill_dir).openbci
    };

    info!("connecting to Ganglion (BLE)…");
    let ganglion_config = GanglionConfig {
        scan_timeout: Duration::from_secs(config.scan_timeout_secs as u64),
        ..Default::default()
    };

    let adapter = tokio::task::spawn_blocking(move || -> Result<Box<dyn DeviceAdapter>, String> {
        let mut board = GanglionBoard::new(ganglion_config);
        board.prepare().map_err(|e| format!("Ganglion prepare: {e}"))?;
        let stream = board.start_stream().map_err(|e| format!("Ganglion stream: {e}"))?;
        let ch: Vec<String> = (1..=4).map(|i| format!("Ch{i}")).collect();
        let desc = OpenBciAdapter::make_descriptor("ganglion", 4, 200.0, ch);
        let info = DeviceInfo {
            name: "Ganglion".into(),
            ..Default::default()
        };
        Ok(Box::new(OpenBciAdapter::start(stream, desc, info)) as Box<dyn DeviceAdapter>)
    })
    .await
    .map_err(|e| format!("spawn: {e}"))??;

    Ok(adapter)
}

// ── OpenBCI Cyton/Daisy (USB serial) ─────────────────────────────────────────

async fn connect_openbci(state: &AppState, target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    let config = {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let mut settings = skill_settings::load_settings(&skill_dir);
        if let Some(port) = target.strip_prefix("usb:") {
            settings.openbci.serial_port = port.to_string();
            let path = skill_settings::settings_path(&skill_dir);
            if let Ok(json) = serde_json::to_string_pretty(&settings) {
                let _ = std::fs::write(path, json);
            }
        }
        settings.openbci
    };

    info!(board = ?config.board, port = %config.serial_port, "connecting to OpenBCI…");
    let adapter = tokio::task::spawn_blocking(move || -> Result<Box<dyn DeviceAdapter>, String> {
        let (adapter, _board) = crate::session_runner::create_and_start_board(&config)?;
        Ok(Box::new(adapter) as Box<dyn DeviceAdapter>)
    })
    .await
    .map_err(|e| format!("spawn: {e}"))??;

    Ok(adapter)
}

// ── Cognionics CGX (USB serial) ──────────────────────────────────────────────

async fn connect_cognionics(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use cognionics::prelude::*;
    use skill_devices::session::cognionics::CognionicsAdapter;

    let port = target.strip_prefix("cgx:").unwrap_or(target).to_string();
    info!(port = %port, "connecting to Cognionics CGX…");

    let config = CgxClientConfig {
        port: Some(port),
        ..Default::default()
    };
    let client = CgxClient::new(config);
    let (rx, handle) = client.start().await.map_err(|e| format!("CGX start: {e}"))?;
    let adapter: Box<dyn DeviceAdapter> = Box::new(CognionicsAdapter::new(rx, handle));

    Ok(adapter)
}

// ── BrainBit (BLE via NeuroSDK2) ───────────────────────────────────────────

async fn connect_brainbit(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use brainbit::prelude::*;

    info!("scanning for BrainBit…");
    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<Vec<brainbit::device::EegSample>>(64);

    let target_addr = target.strip_prefix("brainbit:").unwrap_or("").to_string();

    let (device_name, device_addr) = tokio::task::spawn_blocking(move || -> Result<(String, String), String> {
        let scanner = Scanner::new(&[SensorFamily::LEBrainBit]).map_err(|e| format!("BrainBit scanner: {e}"))?;
        scanner.start().map_err(|e| format!("BrainBit scan start: {e}"))?;
        std::thread::sleep(std::time::Duration::from_secs(5));
        scanner.stop().map_err(|e| format!("BrainBit scan stop: {e}"))?;
        let devices = scanner.devices().map_err(|e| format!("BrainBit devices: {e}"))?;
        if devices.is_empty() {
            return Err("No BrainBit device found nearby".into());
        }
        // Pick matching device or first.
        let info = if !target_addr.is_empty() {
            devices
                .iter()
                .find(|d| d.address_str() == target_addr)
                .or(devices.first())
        } else {
            devices.first()
        }
        .ok_or("No matching BrainBit device")?;

        let mut device = BrainBitDevice::connect(&scanner, info).map_err(|e| format!("BrainBit connect: {e}"))?;
        let name = device.name().unwrap_or_else(|_| "BrainBit".into());
        let addr = device.address().unwrap_or_default();

        // Set up streaming callback.
        let tx = sample_tx;
        device
            .on_signal(move |samples| {
                let _ = tx.blocking_send(samples.to_vec());
            })
            .map_err(|e| format!("BrainBit on_signal: {e}"))?;
        device
            .start_signal()
            .map_err(|e| format!("BrainBit start_signal: {e}"))?;

        // Leak the device to keep it alive for the session lifetime.
        // It will be cleaned up when the process exits.
        // TODO: proper Drop-based lifetime management.
        std::mem::forget(device);
        std::mem::forget(scanner);

        Ok((name, addr))
    })
    .await
    .map_err(|e| format!("spawn: {e}"))??;

    info!(name = %device_name, addr = %device_addr, "BrainBit connected");

    use skill_devices::session::{DeviceCaps, DeviceDescriptor};
    let desc = DeviceDescriptor {
        kind: "brainbit",
        eeg_channels: 4,
        eeg_sample_rate: 250.0,
        channel_names: vec!["O1".into(), "O2".into(), "T3".into(), "T4".into()],
        caps: DeviceCaps::EEG,
        pipeline_channels: 4,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };
    Ok(Box::new(BrainBitAdapter {
        name: device_name,
        desc,
        rx: sample_rx,
        connected_sent: false,
    }))
}

/// Minimal DeviceAdapter for BrainBit.
struct BrainBitAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<Vec<brainbit::device::EegSample>>,
    connected_sent: bool,
}

#[async_trait::async_trait]
impl skill_devices::session::DeviceAdapter for BrainBitAdapter {
    fn descriptor(&self) -> &skill_devices::session::DeviceDescriptor {
        &self.desc
    }

    async fn next_event(&mut self) -> Option<skill_devices::session::DeviceEvent> {
        use skill_devices::session::*;
        if !self.connected_sent {
            self.connected_sent = true;
            return Some(DeviceEvent::Connected(DeviceInfo {
                name: self.name.clone(),
                ..Default::default()
            }));
        }
        let samples = self.rx.recv().await?;
        // BrainBit sends in Volts; convert to µV.
        let s = samples.first()?;
        let channels: Vec<f64> = s.channels.iter().map(|&v| v * 1e6).collect();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        Some(DeviceEvent::Eeg(EegFrame {
            channels,
            timestamp_s: ts,
        }))
    }

    async fn disconnect(&mut self) {
        self.rx.close();
    }
}

// ── g.tec Unicorn Hybrid Black (BLE) ──────────────────────────────────────

async fn connect_gtec(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use gtec::prelude::*;

    let serial = target.strip_prefix("gtec:").unwrap_or("").to_string();
    info!(serial = %serial, "connecting to g.tec Unicorn");

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<gtec::device::Scan>(512);

    let device_serial = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let serial = if serial.is_empty() {
            let serials = UnicornDevice::scan(true).map_err(|e| format!("scan: {e}"))?;
            serials.into_iter().next().ok_or("No g.tec Unicorn found")?
        } else {
            serial
        };

        let mut device = UnicornDevice::open(&serial).map_err(|e| format!("open: {e}"))?;
        device.start_acquisition(false).map_err(|e| format!("start: {e}"))?;

        let dev_serial = serial.clone();
        let tx = sample_tx;
        // Blocking reader thread.
        std::thread::Builder::new()
            .name("gtec-read".to_string())
            .spawn(move || {
                while let Ok(scan) = device.get_single_scan() {
                    if tx.blocking_send(scan).is_err() {
                        break;
                    }
                }
            })
            .expect("gtec reader thread");

        Ok(dev_serial)
    })
    .await
    .map_err(|e| format!("spawn: {e}"))??;

    info!(serial = %device_serial, "g.tec Unicorn connected");

    use skill_devices::session::{DeviceCaps, DeviceDescriptor};
    let desc = DeviceDescriptor {
        kind: "gtec",
        eeg_channels: 8,
        eeg_sample_rate: 250.0,
        channel_names: gtec::types::EEG_CHANNEL_NAMES.iter().map(ToString::to_string).collect(),
        caps: DeviceCaps::EEG,
        pipeline_channels: 8,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };
    Ok(Box::new(GtecAdapter {
        name: format!("g.tec Unicorn ({device_serial})"),
        desc,
        rx: sample_rx,
        connected_sent: false,
    }))
}

struct GtecAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<gtec::device::Scan>,
    connected_sent: bool,
}

#[async_trait::async_trait]
impl skill_devices::session::DeviceAdapter for GtecAdapter {
    fn descriptor(&self) -> &skill_devices::session::DeviceDescriptor {
        &self.desc
    }

    async fn next_event(&mut self) -> Option<skill_devices::session::DeviceEvent> {
        use skill_devices::session::*;
        if !self.connected_sent {
            self.connected_sent = true;
            return Some(DeviceEvent::Connected(DeviceInfo {
                name: self.name.clone(),
                ..Default::default()
            }));
        }
        let scan = self.rx.recv().await?;
        let eeg = scan.eeg();
        let channels: Vec<f64> = eeg.iter().map(|&v| v as f64).collect();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        Some(DeviceEvent::Eeg(EegFrame {
            channels,
            timestamp_s: ts,
        }))
    }

    async fn disconnect(&mut self) {
        self.rx.close();
    }
}

// ── Emotiv (Cortex WebSocket API) ────────────────────────────────────────────

async fn connect_emotiv(state: &AppState) -> Result<Box<dyn DeviceAdapter>, String> {
    use skill_devices::emotiv::prelude::*;
    use skill_devices::session::emotiv::EmotivAdapter;

    let (client_id, client_secret) = {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let settings = skill_settings::load_settings(&skill_dir);
        (
            settings.device_api.emotiv_client_id.clone(),
            settings.device_api.emotiv_client_secret.clone(),
        )
    };

    if client_id.trim().is_empty() || client_secret.trim().is_empty() {
        return Err("Emotiv client_id/client_secret not configured in Settings → Device API".into());
    }

    info!("connecting to Emotiv via Cortex API…");
    let config = CortexClientConfig {
        client_id,
        client_secret,
        ..Default::default()
    };
    let client = CortexClient::new(config);
    let (rx, handle) = client.connect().await.map_err(|e| format!("Emotiv connect: {e}"))?;
    // Emotiv auto-detects channels from the headset type after DataLabels arrives.
    // Start with defaults; the adapter updates dynamically.
    Ok(Box::new(EmotivAdapter::new(rx, handle, 14, vec![], String::new())))
}
