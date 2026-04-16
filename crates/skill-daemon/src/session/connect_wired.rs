// SPDX-License-Identifier: GPL-3.0-only
//! Serial, network, and cloud-API device connection functions — OpenBCI,
//! Cognionics, NeuroField, BrainMaster, LSL, Iroh remote, NeuroSky,
//! Neurosity, BrainVision, Emotiv.

use anyhow::Context as _;
use std::time::Duration;

use skill_devices::session::DeviceAdapter;
use tracing::info;

use crate::state::AppState;

// ── OpenBCI Cyton/Daisy (USB serial) ─────────────────────────────────────────

pub(super) async fn connect_openbci(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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
    let adapter = tokio::task::spawn_blocking(move || -> anyhow::Result<Box<dyn DeviceAdapter>> {
        let (adapter, _board) = crate::session_runner::create_and_start_board(&config)?;
        Ok(Box::new(adapter) as Box<dyn DeviceAdapter>)
    })
    .await
    .context("spawn")??;

    Ok(adapter)
}

// ── Cognionics CGX (USB serial) ──────────────────────────────────────────────

pub(super) async fn connect_cognionics(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use cognionics::prelude::*;
    use skill_devices::session::cognionics::CognionicsAdapter;

    let port = target.strip_prefix("cgx:").unwrap_or(target).to_string();
    info!(port = %port, "connecting to Cognionics CGX…");

    let config = CgxClientConfig {
        port: Some(port),
        ..Default::default()
    };
    let client = CgxClient::new(config);
    let (rx, handle) = client.start().await.context("CGX start")?;
    let adapter: Box<dyn DeviceAdapter> = Box::new(CognionicsAdapter::new(rx, handle));

    Ok(adapter)
}

// ── NeuroField Q21 (PCAN-USB) ────────────────────────────────────────────

pub(super) async fn connect_neurofield(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use crate::session_runner::parse_neurofield_bus;
    use neurofield::prelude::*;

    let bus = parse_neurofield_bus(target);
    info!(?bus, %target, "connecting to NeuroField Q21");

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<neurofield::q21_api::EegSample>(512);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, read_thread) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let mut api = Q21Api::new(bus).context("Q21 connect")?;
        let name = format!(
            "NeuroField Q21 ({:?} #{})",
            api.eeg_device_type(),
            api.eeg_device_serial()
        );
        api.start_receiving_eeg().context("start")?;

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
            .context("spawn")?;

        Ok((name, read_thread))
    })
    .await
    .context("spawn_blocking")??;

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

pub(super) async fn connect_brainmaster(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

    let (device_name, read_thread) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let port = if port.is_empty() {
            BrainMasterDevice::scan()
                .context("scan")?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No BrainMaster device found"))?
        } else {
            port
        };
        let mut device = BrainMasterDevice::open(&port, model).context("open")?;
        device.start_streaming().context("start")?;
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
            .context("spawn reader")?;
        Ok((name, read_thread))
    })
    .await
    .context("spawn")??;

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

pub(super) fn lsl_query_from_target(target: &str) -> String {
    if target.len() >= 4 && target[..4].eq_ignore_ascii_case("lsl:") {
        return target[4..].to_string();
    }
    "".to_string()
}

// ── Iroh remote (iOS / phone streaming over iroh QUIC tunnel) ────────────────

pub(super) async fn connect_iroh_remote(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use skill_devices::session::iroh_remote::IrohRemoteAdapter;

    let peer_id = target.strip_prefix("peer:").unwrap_or(target).to_string();
    info!(peer_id = %peer_id, "connecting iroh remote adapter");

    // Surface paired client identity immediately in status/UI.
    if let Ok(auth) = state.iroh_auth.lock() {
        let client_name = auth.client_name_for_endpoint(&peer_id);
        drop(auth);
        if let Ok(mut s) = state.status.lock() {
            s.iroh_client_name = client_name;
        }
    }

    // Create a fresh channel pair and install the sender in the shared slot.
    // The tunnel's per-message tx re-read will pick this up immediately so
    // events from the phone start flowing into this session's rx.
    let (tx, rx) = skill_iroh::event_channel();
    if let Ok(mut g) = state.iroh_device_tx.lock() {
        *g = Some(tx.clone());
    }
    // Immediately replay any cached pre-session messages (device_connected /
    // phone_info / first chunks) so the session doesn't depend on new traffic
    // to hydrate metadata.
    skill_iroh::flush_presession_for_peer(&peer_id, &tx);

    // Pass the shared slot so the adapter clears it on drop, turning
    // post-session iroh messages into "no active session" rather than
    // "event channel closed".
    Ok(Box::new(IrohRemoteAdapter::new(
        rx,
        peer_id,
        state.iroh_device_tx.clone(),
    )))
}

pub(super) async fn connect_lsl(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    let query = lsl_query_from_target(target);
    info!(query = %query, "connecting to LSL stream");

    let adapter = tokio::task::spawn_blocking(move || -> anyhow::Result<Box<dyn DeviceAdapter>> {
        // Fast path: when we know the stream name, use a targeted query with
        // minimum=1 so it returns as soon as the stream is found (typically
        // < 500 ms for local streams) instead of waiting the full timeout.
        let info = if !query.is_empty() {
            skill_lsl::resolve_stream_by_name(&query, 5.0)
                .ok_or_else(|| anyhow::anyhow!("No LSL stream matching '{query}'"))?
        } else {
            // No name given — discover all EEG streams and take the first.
            let streams = skill_lsl::resolve_eeg_streams(5.0);
            streams
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No LSL EEG streams found on the network"))?
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
    .context("spawn")??;

    Ok(adapter)
}

// ── NeuroSky MindWave (serial ThinkGear) ───────────────────────────────────

pub(super) async fn connect_neurosky(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use neurosky::prelude::*;

    let requested = target.strip_prefix("neurosky:").unwrap_or("").trim().to_string();
    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<i16>(1024);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, read_thread) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let port = if requested.is_empty() {
            MindWaveDevice::find()
                .context("MindWave find")?
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No NeuroSky MindWave serial port found"))?
        } else {
            requested
        };

        let mut device = MindWaveDevice::open(&port)
            .or_else(|_| MindWaveDevice::open_bluetooth(&port))
            .context("MindWave open")?;
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
                                Packet::RawValue(v) if tx.blocking_send(v).is_err() => {
                                    return;
                                }
                                Packet::HeadsetDisconnected => return,
                                _ => {}
                            }
                        }
                    }
                    Err(_) => break,
                }
            })
            .context("spawn reader")?;

        Ok((format!("NeuroSky MindWave ({port})"), read_thread))
    })
    .await
    .context("spawn")??;

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

pub(super) async fn connect_neurosity(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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
        anyhow::bail!("Neurosity device_id missing (set in Device API settings or use neurosity:<device_id>)");
    }
    if email.trim().is_empty() {
        anyhow::bail!("Neurosity email missing (Device API settings or SKILL_NEUROSITY_EMAIL)");
    }
    if password.trim().is_empty() {
        anyhow::bail!("Neurosity password missing (Device API settings or SKILL_NEUROSITY_PASSWORD)");
    }

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<Vec<f64>>(512);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, eeg_channels, sample_rate, channel_names, read_thread) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let mut client = NeurosityClient::new(&device_id);
            client
                .login(&Credentials { email, password })
                .context("Neurosity login")?;

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
                .context("spawn reader")?;

            Ok((display_name, eeg_channels, sample_rate, channel_names, read_thread))
        })
        .await
        .context("spawn")??;

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

pub(super) async fn connect_brainvision(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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
            .with_context(|| format!("invalid BrainVision port '{p}'"))?;
        (h.to_string(), parsed)
    } else {
        (spec, brainvision::types::RDA_PORT_I16)
    };

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<Vec<f64>>(1024);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let (device_name, eeg_channels, sample_rate, channel_names, read_thread) =
        tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let mut device = BrainVisionDevice::connect(&host, port).context("RDA connect")?;
            let header = device.wait_for_start().context("RDA start")?;
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
                .context("spawn reader")?;

            Ok((
                format!("BrainVision RDA ({host}:{port})"),
                eeg_channels,
                if sample_rate > 0.0 { sample_rate } else { 500.0 },
                channel_names,
                read_thread,
            ))
        })
        .await
        .context("spawn")??;

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

// ── Emotiv (Cortex WebSocket API) ────────────────────────────────────────────

pub(super) async fn connect_emotiv(state: &AppState) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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
        anyhow::bail!("Emotiv client_id/client_secret not configured in Settings → Device API");
    }

    info!("connecting to Emotiv via Cortex API…");
    let config = CortexClientConfig {
        client_id,
        client_secret,
        ..Default::default()
    };
    let client = CortexClient::new(config);
    let (rx, handle) = client.connect().await.context("Emotiv connect")?;
    // Emotiv auto-detects channels from the headset type after DataLabels arrives.
    // Start with defaults; the adapter updates dynamically.
    Ok(Box::new(EmotivAdapter::new(rx, handle, 14, vec![], String::new())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill_devices::session::DeviceAdapter;

    #[test]
    fn lsl_query_strips_prefix() {
        assert_eq!(lsl_query_from_target("lsl:name='Muse'"), "name='Muse'");
        assert_eq!(lsl_query_from_target("LSL:type='EEG'"), "type='EEG'");
        assert_eq!(lsl_query_from_target("lsl"), "");
        assert_eq!(lsl_query_from_target("other"), "");
        assert_eq!(lsl_query_from_target("lsl:"), "");
    }

    #[tokio::test]
    async fn neurosky_adapter_emits_connected_then_eeg() {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "neurosky",
            eeg_channels: 1,
            eeg_sample_rate: 512.0,
            channel_names: vec!["Fp1".into()],
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 1,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = NeuroSkyAdapter {
            name: "TestNeuroSky".into(),
            desc,
            rx,
            stop_tx: None,
            read_thread: None,
            connected_sent: false,
        };
        // First call: Connected
        let ev = adapter.next_event().await;
        assert!(matches!(ev, Some(skill_devices::session::DeviceEvent::Connected(_))));
        // Send a raw sample
        tx.send(42i16).await.unwrap();
        let ev = adapter.next_event().await;
        match ev {
            Some(skill_devices::session::DeviceEvent::Eeg(frame)) => {
                assert_eq!(frame.channels.len(), 1);
                assert_eq!(frame.channels[0], 42.0);
            }
            other => panic!("expected Eeg, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn brainmaster_adapter_emits_connected_first() {
        let (_tx, rx) = tokio::sync::mpsc::channel::<brainmaster::device::EegSample>(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "brainmaster",
            eeg_channels: 2,
            eeg_sample_rate: 256.0,
            channel_names: vec!["Ch1".into(), "Ch2".into()],
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 2,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = BrainMasterAdapter {
            name: "TestBM".into(),
            desc,
            rx,
            read_thread: None,
            connected_sent: false,
        };
        let ev = adapter.next_event().await;
        assert!(matches!(ev, Some(skill_devices::session::DeviceEvent::Connected(_))));
    }

    #[test]
    fn lsl_query_empty_and_edge_cases() {
        assert_eq!(lsl_query_from_target(""), "");
        assert_eq!(lsl_query_from_target("ls"), "");
        assert_eq!(lsl_query_from_target("lslx"), "");
        // Exactly 4 chars with prefix
        assert_eq!(lsl_query_from_target("lsl:"), "");
        // With spaces
        assert_eq!(
            lsl_query_from_target("lsl: name='Test Device' "),
            " name='Test Device' "
        );
    }

    #[tokio::test]
    async fn neurofield_adapter_descriptor_correct() {
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "neurofield",
            eeg_channels: 21,
            eeg_sample_rate: 500.0,
            channel_names: (1..=21).map(|i| format!("Ch{i}")).collect(),
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 21,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let adapter = NeuroFieldAdapter {
            name: "TestNF".into(),
            desc,
            rx,
            stop_tx: None,
            read_thread: None,
            connected_sent: false,
        };
        assert_eq!(adapter.descriptor().kind, "neurofield");
        assert_eq!(adapter.descriptor().eeg_channels, 21);
    }

    #[tokio::test]
    async fn neurofield_adapter_disconnect_is_safe() {
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "neurofield",
            eeg_channels: 21,
            eeg_sample_rate: 500.0,
            channel_names: (1..=21).map(|i| format!("Ch{i}")).collect(),
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 21,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = NeuroFieldAdapter {
            name: "TestNF".into(),
            desc,
            rx,
            stop_tx: None,
            read_thread: None,
            connected_sent: false,
        };
        adapter.disconnect().await;
    }

    #[tokio::test]
    async fn neurosity_adapter_emits_connected_first() {
        let (_tx, rx) = tokio::sync::mpsc::channel::<Vec<f64>>(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "neurosity",
            eeg_channels: 8,
            eeg_sample_rate: 256.0,
            channel_names: (1..=8).map(|i| format!("Ch{i}")).collect(),
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 8,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = NeurosityAdapter {
            name: "TestNeurosity".into(),
            desc,
            rx,
            stop_tx: None,
            read_thread: None,
            connected_sent: false,
        };
        let ev = adapter.next_event().await;
        assert!(matches!(ev, Some(skill_devices::session::DeviceEvent::Connected(_))));
    }

    #[tokio::test]
    async fn brainvision_adapter_emits_connected_first() {
        let (_tx, rx) = tokio::sync::mpsc::channel::<Vec<f64>>(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "brainvision",
            eeg_channels: 32,
            eeg_sample_rate: 500.0,
            channel_names: (1..=32).map(|i| format!("Ch{i}")).collect(),
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 32,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = BrainVisionAdapter {
            name: "TestBV".into(),
            desc,
            rx,
            stop_tx: None,
            read_thread: None,
            connected_sent: false,
        };
        let ev = adapter.next_event().await;
        assert!(matches!(ev, Some(skill_devices::session::DeviceEvent::Connected(_))));
    }
}
