// SPDX-License-Identifier: GPL-3.0-only
//! Per-device connection logic — transport-specific setup (BLE, serial,
//! Cortex WS, PCAN) → `Box<dyn DeviceAdapter>` for the generic runner.

use std::time::Duration;

use skill_daemon_common::DeviceLogEntry;
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

        // ── Routing log ──────────────────────────────────────────────────
        // Produces: [devices] [session] routing: target=… kind=…
        // Visible in the device log and tracing output so connection
        // failures are easy to diagnose.
        let routed_kind = infer_kind_from_target(&target);
        push_device_log_static(
            &state2,
            "session",
            &format!("routing: target={target:?} kind={routed_kind}"),
        );
        info!(target = %target, kind = %routed_kind, "session routing");

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
    if lower.starts_with("lsl:") || lower == "lsl" {
        return connect_lsl(target).await;
    }
    if lower.starts_with("brainmaster:") || lower.contains("brainmaster") {
        return connect_brainmaster(state, target).await;
    }
    if lower.starts_with("brainbit:") || lower.contains("brainbit") {
        return connect_brainbit(target).await;
    }
    if lower.starts_with("neurosky:") || lower == "neurosky" || lower.contains("mindwave") {
        return connect_neurosky(target).await;
    }
    if lower.starts_with("neurosity:") || lower == "neurosity" || lower.contains("crown") || lower.contains("notion") {
        return connect_neurosity(state, target).await;
    }
    if lower.starts_with("brainvision:") || lower == "brainvision" || lower.starts_with("rda:") {
        return connect_brainvision(target).await;
    }
    if lower.starts_with("neurofield:") || lower.contains("neurofield") {
        return connect_neurofield(target).await;
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

    // Safety-net: if the target carries an explicit serial port but the
    // persisted board type is not a serial board (Ganglion/BLE,
    // GanglionWifi, CytonWifi, CytonDaisyWifi, Galea/UDP), the port would
    // be silently ignored and the wrong transport would start.
    // Keep the board as-is when it is already a serial board (Cyton or
    // CytonDaisy) so the user's 8-ch / 16-ch choice is respected.
    let config = if target.to_lowercase().starts_with("usb:") && !config.board.is_serial() {
        skill_settings::OpenBciConfig {
            board: skill_settings::OpenBciBoard::Cyton,
            ..config
        }
    } else {
        config
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

// ── NeuroField Q21 (PCAN-USB) ────────────────────────────────────────────

async fn connect_neurofield(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use crate::session_runner::parse_neurofield_bus;
    use neurofield::prelude::*;

    let bus = parse_neurofield_bus(target);
    info!(?bus, %target, "connecting to NeuroField Q21");

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<neurofield::q21_api::EegSample>(512);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, read_thread) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let mut api = Q21Api::new(bus).map_err(|e| format!("Q21 connect: {e}"))?;
        let name = format!(
            "NeuroField Q21 ({:?} #{})",
            api.eeg_device_type(),
            api.eeg_device_serial()
        );
        api.start_receiving_eeg().map_err(|e| format!("start: {e}"))?;

        let tx = sample_tx;
        let read_thread = std::thread::Builder::new()
            .name("neurofield-read".to_string())
            .spawn(move || {
                loop {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }
                    match api.get_single_sample() {
                        Ok(s) => {
                            if tx.blocking_send(s).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let _ = api.abort_receiving_eeg();
                api.release();
            })
            .map_err(|e| format!("spawn: {e}"))?;

        Ok((name, read_thread))
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))??;

    info!(name = %device_name, "NeuroField Q21 connected");

    use skill_devices::session::{DeviceCaps, DeviceDescriptor};
    let desc = DeviceDescriptor {
        kind: "neurofield",
        eeg_channels: neurofield::q21_api::NUM_CHANNELS,
        eeg_sample_rate: neurofield::q21_api::SAMPLING_RATE,
        channel_names: neurofield::q21_api::EEG_CHANNEL_NAMES
            .iter()
            .map(ToString::to_string)
            .collect(),
        caps: DeviceCaps::EEG,
        pipeline_channels: neurofield::q21_api::NUM_CHANNELS,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };
    Ok(Box::new(NeuroFieldAdapter {
        name: device_name,
        desc,
        rx: sample_rx,
        stop_tx: Some(stop_tx),
        read_thread: Some(read_thread),
        connected_sent: false,
    }))
}

struct NeuroFieldAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<neurofield::q21_api::EegSample>,
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    read_thread: Option<std::thread::JoinHandle<()>>,
    connected_sent: bool,
}

#[async_trait::async_trait]
impl skill_devices::session::DeviceAdapter for NeuroFieldAdapter {
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
        let s = self.rx.recv().await?;
        let channels: Vec<f64> = s.data.to_vec();
        let ts = s.timestamp_us as f64 / 1_000_000.0;
        Some(DeviceEvent::Eeg(EegFrame {
            channels,
            timestamp_s: ts,
        }))
    }

    async fn disconnect(&mut self) {
        self.rx.close();
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.read_thread.take() {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = handle.join();
            })
            .await;
        }
    }
}

// ── BrainMaster (USB serial) ────────────────────────────────────────────

async fn connect_brainmaster(state: &AppState, target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use brainmaster::prelude::*;

    let port = target.strip_prefix("brainmaster:").unwrap_or("").to_string();

    // Read model from settings (device_api.brainmaster_model) or default Atlantis4.
    let model_str = {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let settings = skill_settings::load_settings(&skill_dir);
        settings.device_api.brainmaster_model.clone()
    };
    let model = match model_str.to_lowercase().as_str() {
        "atlantis2" | "a2" => DeviceModel::Atlantis2,
        "discovery" | "d24" => DeviceModel::Discovery,
        "freedom" | "f24" => DeviceModel::Freedom,
        _ => DeviceModel::Atlantis4,
    };

    info!(port = %port, model = ?model, "connecting to BrainMaster");

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<brainmaster::device::EegSample>(512);

    let (device_name, read_thread) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let port = if port.is_empty() {
            BrainMasterDevice::scan()
                .map_err(|e| format!("scan: {e}"))?
                .into_iter()
                .next()
                .ok_or("No BrainMaster device found")?
        } else {
            port
        };
        let mut device = BrainMasterDevice::open(&port, model).map_err(|e| format!("open: {e}"))?;
        device.start_streaming().map_err(|e| format!("start: {e}"))?;
        let name = format!("BrainMaster {:?} ({port})", model);
        let tx = sample_tx;
        let read_thread = std::thread::Builder::new()
            .name("brainmaster-read".to_string())
            .spawn(move || {
                while let Ok(sample) = device.read_sample() {
                    if tx.blocking_send(sample).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("spawn reader: {e}"))?;
        Ok((name, read_thread))
    })
    .await
    .map_err(|e| format!("spawn: {e}"))??;

    info!(name = %device_name, "BrainMaster connected");

    use skill_devices::session::{DeviceCaps, DeviceDescriptor};
    let ch_names: Vec<String> = model.channel_names().iter().map(ToString::to_string).collect();
    let desc = DeviceDescriptor {
        kind: "brainmaster",
        eeg_channels: model.channel_count(),
        eeg_sample_rate: model.sample_rate() as f64,
        channel_names: ch_names,
        caps: DeviceCaps::EEG,
        pipeline_channels: model.channel_count(),
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };
    Ok(Box::new(BrainMasterAdapter {
        name: device_name,
        desc,
        rx: sample_rx,
        read_thread: Some(read_thread),
        connected_sent: false,
    }))
}

struct BrainMasterAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<brainmaster::device::EegSample>,
    read_thread: Option<std::thread::JoinHandle<()>>,
    connected_sent: bool,
}

#[async_trait::async_trait]
impl skill_devices::session::DeviceAdapter for BrainMasterAdapter {
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
        let sample = self.rx.recv().await?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        Some(DeviceEvent::Eeg(EegFrame {
            channels: sample.channels,
            timestamp_s: ts,
        }))
    }

    async fn disconnect(&mut self) {
        self.rx.close();
        if let Some(handle) = self.read_thread.take() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || {
                    let _ = handle.join();
                }),
            )
            .await;
        }
    }
}

// ── LSL (Lab Streaming Layer) ──────────────────────────────────────────────

async fn connect_lsl(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    let query = target.strip_prefix("lsl:").unwrap_or("").to_string();
    info!(query = %query, "connecting to LSL stream");

    let adapter = tokio::task::spawn_blocking(move || -> Result<Box<dyn DeviceAdapter>, String> {
        // Fast path: when we know the stream name, use a targeted query with
        // minimum=1 so it returns as soon as the stream is found (typically
        // < 500 ms for local streams) instead of waiting the full timeout.
        let info = if !query.is_empty() {
            skill_lsl::resolve_stream_by_name(&query, 5.0).ok_or_else(|| format!("No LSL stream matching '{query}'"))?
        } else {
            // No name given — discover all EEG streams and take the first.
            let streams = skill_lsl::resolve_eeg_streams(5.0);
            streams
                .into_iter()
                .next()
                .ok_or_else(|| "No LSL EEG streams found on the network".to_string())?
        };

        info!(
            name = %info.name(),
            channels = info.channel_count(),
            rate = info.nominal_srate(),
            "LSL stream resolved"
        );
        let adapter = skill_lsl::LslAdapter::new(&info);
        Ok(Box::new(adapter) as Box<dyn DeviceAdapter>)
    })
    .await
    .map_err(|e| format!("spawn: {e}"))??;

    Ok(adapter)
}

// ── NeuroSky MindWave (serial ThinkGear) ───────────────────────────────────

async fn connect_neurosky(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use neurosky::prelude::*;

    let requested = target.strip_prefix("neurosky:").unwrap_or("").trim().to_string();
    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<i16>(1024);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, read_thread) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let port = if requested.is_empty() {
            MindWaveDevice::find()
                .map_err(|e| format!("MindWave find: {e}"))?
                .into_iter()
                .next()
                .ok_or("No NeuroSky MindWave serial port found")?
        } else {
            requested
        };

        let mut device = MindWaveDevice::open(&port)
            .or_else(|_| MindWaveDevice::open_bluetooth(&port))
            .map_err(|e| format!("MindWave open: {e}"))?;
        let _ = device.auto_connect();

        let tx = sample_tx;
        let read_thread = std::thread::Builder::new()
            .name("neurosky-read".to_string())
            .spawn(move || loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                match device.read() {
                    Ok(packets) => {
                        if packets.is_empty() {
                            std::thread::sleep(Duration::from_millis(5));
                            continue;
                        }
                        for p in packets {
                            match p {
                                Packet::RawValue(v) => {
                                    if tx.blocking_send(v).is_err() {
                                        return;
                                    }
                                }
                                Packet::HeadsetDisconnected => return,
                                _ => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            })
            .map_err(|e| format!("spawn reader: {e}"))?;

        Ok((format!("NeuroSky MindWave ({port})"), read_thread))
    })
    .await
    .map_err(|e| format!("spawn: {e}"))??;

    use skill_devices::session::{DeviceCaps, DeviceDescriptor};
    let desc = DeviceDescriptor {
        kind: "neurosky",
        eeg_channels: 1,
        eeg_sample_rate: neurosky::types::RAW_SAMPLING_RATE as f64,
        channel_names: vec!["Fp1".into()],
        caps: DeviceCaps::EEG,
        pipeline_channels: 1,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };

    Ok(Box::new(NeuroSkyAdapter {
        name: device_name,
        desc,
        rx: sample_rx,
        stop_tx: Some(stop_tx),
        read_thread: Some(read_thread),
        connected_sent: false,
    }))
}

struct NeuroSkyAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<i16>,
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    read_thread: Option<std::thread::JoinHandle<()>>,
    connected_sent: bool,
}

#[async_trait::async_trait]
impl skill_devices::session::DeviceAdapter for NeuroSkyAdapter {
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

        let sample = self.rx.recv().await?;
        Some(DeviceEvent::Eeg(EegFrame {
            channels: vec![sample as f64],
            timestamp_s: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0),
        }))
    }

    async fn disconnect(&mut self) {
        self.rx.close();
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.read_thread.take() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || {
                    let _ = handle.join();
                }),
            )
            .await;
        }
    }
}

// ── Neurosity Crown/Notion (Cloud API) ─────────────────────────────────────

async fn connect_neurosity(state: &AppState, target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use neurosity::prelude::*;

    let requested_device_id = target.strip_prefix("neurosity:").unwrap_or("").trim().to_string();

    let (device_id, email, password) = {
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let settings = skill_settings::load_settings(&skill_dir);

        let device_id = if requested_device_id.is_empty() {
            settings.device_api.neurosity_device_id.clone()
        } else {
            requested_device_id
        };
        let email = if settings.device_api.neurosity_email.trim().is_empty() {
            std::env::var("SKILL_NEUROSITY_EMAIL")
                .or_else(|_| std::env::var("NEUROSITY_EMAIL"))
                .unwrap_or_default()
        } else {
            settings.device_api.neurosity_email.clone()
        };
        let password = if settings.device_api.neurosity_password.trim().is_empty() {
            std::env::var("SKILL_NEUROSITY_PASSWORD")
                .or_else(|_| std::env::var("NEUROSITY_PASSWORD"))
                .unwrap_or_default()
        } else {
            settings.device_api.neurosity_password.clone()
        };

        (device_id, email, password)
    };

    if device_id.is_empty() {
        return Err("Neurosity device_id missing (set in Device API settings or use neurosity:<device_id>)".into());
    }
    if email.trim().is_empty() {
        return Err("Neurosity email missing (Device API settings or SKILL_NEUROSITY_EMAIL)".into());
    }
    if password.trim().is_empty() {
        return Err("Neurosity password missing (Device API settings or SKILL_NEUROSITY_PASSWORD)".into());
    }

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<Vec<f64>>(512);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, eeg_channels, sample_rate, channel_names, read_thread) =
        tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut client = NeurosityClient::new(&device_id);
            client
                .login(&Credentials { email, password })
                .map_err(|e| format!("Neurosity login: {e}"))?;

            let info = client.get_info().unwrap_or_default();
            let model = info.model.to_lowercase();
            let default_names: Vec<String> = if model.contains("notion") {
                neurosity::types::NOTION_CHANNEL_NAMES
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            } else {
                neurosity::types::CROWN_CHANNEL_NAMES
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            };
            let eeg_channels = if info.channels > 0 {
                info.channels as usize
            } else {
                default_names.len()
            };
            let channel_names = if default_names.len() == eeg_channels {
                default_names
            } else {
                (1..=eeg_channels).map(|i| format!("Ch{i}")).collect()
            };
            let sample_rate = if info.sampling_rate > 0 {
                info.sampling_rate as f64
            } else {
                neurosity::types::CROWN_SAMPLING_RATE as f64
            };
            let display_name = if info.device_nickname.trim().is_empty() {
                format!("Neurosity {device_id}")
            } else {
                format!("Neurosity {}", info.device_nickname)
            };

            let tx = sample_tx;
            let read_thread = std::thread::Builder::new()
                .name("neurosity-read".to_string())
                .spawn(move || loop {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }
                    match client.brainwaves_raw() {
                        Ok(raw) => {
                            let channels: Vec<f64> =
                                raw.data.iter().map(|c| c.last().copied().unwrap_or(0.0)).collect();
                            if !channels.is_empty() && tx.blocking_send(channels).is_err() {
                                break;
                            }
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(250)),
                    }
                    std::thread::sleep(Duration::from_millis(25));
                })
                .map_err(|e| format!("spawn reader: {e}"))?;

            Ok((display_name, eeg_channels, sample_rate, channel_names, read_thread))
        })
        .await
        .map_err(|e| format!("spawn: {e}"))??;

    use skill_devices::session::{DeviceCaps, DeviceDescriptor};
    let desc = DeviceDescriptor {
        kind: "neurosity",
        eeg_channels,
        eeg_sample_rate: sample_rate,
        channel_names,
        caps: DeviceCaps::EEG,
        pipeline_channels: eeg_channels,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };

    Ok(Box::new(NeurosityAdapter {
        name: device_name,
        desc,
        rx: sample_rx,
        stop_tx: Some(stop_tx),
        read_thread: Some(read_thread),
        connected_sent: false,
    }))
}

struct NeurosityAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<Vec<f64>>,
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    read_thread: Option<std::thread::JoinHandle<()>>,
    connected_sent: bool,
}

#[async_trait::async_trait]
impl skill_devices::session::DeviceAdapter for NeurosityAdapter {
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

        let channels = self.rx.recv().await?;
        Some(DeviceEvent::Eeg(EegFrame {
            channels,
            timestamp_s: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0),
        }))
    }

    async fn disconnect(&mut self) {
        self.rx.close();
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.read_thread.take() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || {
                    let _ = handle.join();
                }),
            )
            .await;
        }
    }
}

// ── BrainVision RDA (TCP/IP) ────────────────────────────────────────────────

async fn connect_brainvision(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use brainvision::prelude::*;

    let spec = target
        .strip_prefix("brainvision:")
        .or_else(|| target.strip_prefix("rda:"))
        .unwrap_or("")
        .trim()
        .to_string();

    let (host, port) = if spec.is_empty() {
        ("127.0.0.1".to_string(), brainvision::types::RDA_PORT_I16)
    } else if let Some((h, p)) = spec.rsplit_once(':') {
        let parsed = p
            .parse::<u16>()
            .map_err(|e| format!("invalid BrainVision port '{p}': {e}"))?;
        (h.to_string(), parsed)
    } else {
        (spec, brainvision::types::RDA_PORT_I16)
    };

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<Vec<f64>>(1024);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, eeg_channels, sample_rate, channel_names, read_thread) =
        tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut device = BrainVisionDevice::connect(&host, port).map_err(|e| format!("RDA connect: {e}"))?;
            let header = device.wait_for_start().map_err(|e| format!("RDA start: {e}"))?;
            let eeg_channels = header.channel_count as usize;
            let sample_rate = header.sampling_rate_hz();
            let channel_names = if header.channel_names.is_empty() {
                (1..=eeg_channels).map(|i| format!("Ch{i}")).collect()
            } else {
                header.channel_names.clone()
            };

            let tx = sample_tx;
            let read_thread = std::thread::Builder::new()
                .name("brainvision-read".to_string())
                .spawn(move || {
                    loop {
                        if stop_rx.try_recv().is_ok() {
                            break;
                        }
                        match device.next_scan() {
                            Ok(Some(scan)) => {
                                if tx.blocking_send(scan.data).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => break,
                            Err(_) => break,
                        }
                    }
                    device.close();
                })
                .map_err(|e| format!("spawn reader: {e}"))?;

            Ok((
                format!("BrainVision RDA ({host}:{port})"),
                eeg_channels,
                if sample_rate > 0.0 { sample_rate } else { 500.0 },
                channel_names,
                read_thread,
            ))
        })
        .await
        .map_err(|e| format!("spawn: {e}"))??;

    use skill_devices::session::{DeviceCaps, DeviceDescriptor};
    let desc = DeviceDescriptor {
        kind: "brainvision",
        eeg_channels,
        eeg_sample_rate: sample_rate,
        channel_names,
        caps: DeviceCaps::EEG,
        pipeline_channels: eeg_channels,
        ppg_channel_names: Vec::new(),
        imu_channel_names: Vec::new(),
        fnirs_channel_names: Vec::new(),
    };

    Ok(Box::new(BrainVisionAdapter {
        name: device_name,
        desc,
        rx: sample_rx,
        stop_tx: Some(stop_tx),
        read_thread: Some(read_thread),
        connected_sent: false,
    }))
}

struct BrainVisionAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<Vec<f64>>,
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    read_thread: Option<std::thread::JoinHandle<()>>,
    connected_sent: bool,
}

#[async_trait::async_trait]
impl skill_devices::session::DeviceAdapter for BrainVisionAdapter {
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

        let channels = self.rx.recv().await?;
        Some(DeviceEvent::Eeg(EegFrame {
            channels,
            timestamp_s: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0),
        }))
    }

    async fn disconnect(&mut self) {
        self.rx.close();
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.read_thread.take() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || {
                    let _ = handle.join();
                }),
            )
            .await;
        }
    }
}

// ── BrainBit (BLE via NeuroSDK2) ───────────────────────────────────────────

async fn connect_brainbit(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use brainbit::prelude::*;

    info!("scanning for BrainBit…");
    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<Vec<brainbit::device::EegSample>>(64);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let target_addr = target.strip_prefix("brainbit:").unwrap_or("").to_string();

    let (device_name, device_addr, keepalive_thread) = tokio::task::spawn_blocking(move || -> Result<_, String> {
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

        // Keep scanner/device alive until adapter disconnects.
        let keepalive_thread = std::thread::Builder::new()
            .name("brainbit-keepalive".to_string())
            .spawn(move || {
                let _ = stop_rx.recv();
                drop(device);
                drop(scanner);
            })
            .map_err(|e| format!("spawn keepalive: {e}"))?;

        Ok((name, addr, keepalive_thread))
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
        stop_tx: Some(stop_tx),
        keepalive_thread: Some(keepalive_thread),
        connected_sent: false,
    }))
}

/// Minimal DeviceAdapter for BrainBit.
struct BrainBitAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<Vec<brainbit::device::EegSample>>,
    stop_tx: Option<std::sync::mpsc::Sender<()>>,
    keepalive_thread: Option<std::thread::JoinHandle<()>>,
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
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.keepalive_thread.take() {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = handle.join();
            })
            .await;
        }
    }
}

// ── g.tec Unicorn Hybrid Black (BLE) ──────────────────────────────────────

async fn connect_gtec(target: &str) -> Result<Box<dyn DeviceAdapter>, String> {
    use gtec::prelude::*;

    let serial = target.strip_prefix("gtec:").unwrap_or("").to_string();
    info!(serial = %serial, "connecting to g.tec Unicorn");

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<gtec::device::Scan>(512);

    let (device_serial, read_thread) = tokio::task::spawn_blocking(move || -> Result<_, String> {
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
        let read_thread = std::thread::Builder::new()
            .name("gtec-read".to_string())
            .spawn(move || {
                while let Ok(scan) = device.get_single_scan() {
                    if tx.blocking_send(scan).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("spawn reader: {e}"))?;

        Ok((dev_serial, read_thread))
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
        read_thread: Some(read_thread),
        connected_sent: false,
    }))
}

struct GtecAdapter {
    name: String,
    desc: skill_devices::session::DeviceDescriptor,
    rx: tokio::sync::mpsc::Receiver<gtec::device::Scan>,
    read_thread: Option<std::thread::JoinHandle<()>>,
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
        if let Some(handle) = self.read_thread.take() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                tokio::task::spawn_blocking(move || {
                    let _ = handle.join();
                }),
            )
            .await;
        }
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

// ── Routing helpers ────────────────────────────────────────────────────────────────

/// Infer a human-readable device kind string from the raw target identifier.
///
/// Used only for diagnostic logging in [`spawn_device_session`]; not used for
/// actual routing decisions (that remains in [`connect_device`]).
fn infer_kind_from_target(target: &str) -> &'static str {
    let lower = target.to_lowercase();
    if lower.starts_with("neurofield:") {
        return "neurofield";
    }
    if lower.starts_with("brainbit:") {
        return "brainbit";
    }
    if lower.starts_with("gtec:") {
        return "gtec";
    }
    if lower.starts_with("brainmaster:") {
        return "brainmaster";
    }
    if lower.starts_with("cortex:") {
        return "emotiv";
    }
    if lower.starts_with("cgx:") {
        return "cognionics";
    }
    if lower.starts_with("lsl:") || lower == "lsl" {
        return "lsl";
    }
    if lower.starts_with("usb:") {
        return "openbci/cyton";
    } // serial → Cyton, not Ganglion
    if lower == "ganglion" {
        return "ganglion";
    }
    if lower == "openbci" {
        return "openbci";
    }
    if lower.contains("mw75") || lower.contains("neurable") {
        return "mw75";
    }
    if lower.contains("hermes") {
        return "hermes";
    }
    if lower.contains("idun") || lower.contains("guardian") {
        return "idun";
    }
    if lower.contains("mendi") {
        return "mendi";
    }
    "muse"
}

/// Append an entry to the state device log (used by `spawn_device_session`).
///
/// Mirrors the `push_device_log` helper in `main.rs` without requiring a
/// shared reference to it across the module boundary.
fn push_device_log_static(state: &AppState, tag: &str, msg: &str) {
    let entry = DeviceLogEntry {
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        tag: tag.to_string(),
        msg: msg.to_string(),
    };
    if let Ok(mut guard) = state.device_log.lock() {
        if guard.len() >= 256 {
            guard.pop_front();
        }
        guard.push_back(entry);
    }
}
