// SPDX-License-Identifier: GPL-3.0-only
//! Per-device connection logic — transport-specific setup (BLE, serial,
//! Cortex WS, PCAN) → `Box<dyn DeviceAdapter>` for the generic runner.

use anyhow::Context as _;
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
            let target_id = if target.contains(':') {
                Some(target.clone())
            } else {
                s.paired_devices.iter().find(|d| d.name == target).map(|d| d.id.clone())
            };
            let target_display_name = if target.contains(':') {
                s.paired_devices
                    .iter()
                    .find(|d| d.id == target)
                    .map(|d| d.name.clone())
                    .or_else(|| Some(target.clone()))
            } else {
                Some(target.clone())
            };
            s.state = "connecting".into();
            s.target_name = Some(target.clone());
            s.target_id = target_id;
            s.target_display_name = target_display_name;
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
                push_device_log_static(
                    &state2,
                    "session",
                    &format!("connect failed: target={target:?} err={e}"),
                );
                if let Ok(mut s) = state2.status.lock() {
                    s.state = "disconnected".into();
                    s.device_error = Some(e.to_string());
                }
            }
        }
        if let Ok(mut slot) = state2.session_handle.lock() {
            *slot = None;
        }
    });

    Some(SessionHandle { cancel_tx })
}

fn requires_pairing(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    // LSL streams are logical network sources, not pairable hardware.
    // Iroh remote peers are pre-authenticated by the iroh tunnel (TOTP-paired).
    !(lower == "lsl" || lower.starts_with("lsl:") || lower.starts_with("peer:"))
}

fn is_paired(state: &AppState, target: &str) -> bool {
    state
        .status
        .lock()
        .ok()
        .map(|s| s.paired_devices.iter().any(|d| d.id == target || d.name == target))
        .unwrap_or(false)
}

async fn connect_device(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    let lower = target.to_lowercase();

    // Defense-in-depth: session control endpoints already enforce pairing for
    // scanner/device targets. Keep the same invariant here in case connect
    // paths are called from future internal entry points.
    if requires_pairing(target) && !is_paired(state, target) {
        anyhow::bail!("Target device is not paired. Pair it first in Settings → Devices.");
    }

    // Devices that use their own BLE scanner (btleplug CBCentralManager) need
    // the background BLE listener scan to be stopped first.  On macOS, two
    // concurrent CBCentralManager.scanForPeripherals() calls suppress the
    // centralManager(_:didConnect:) delegate callback, so peripheral.connect()
    // hangs forever.  We pause here once for every BLE-scanning connect path
    // rather than duplicating the logic in each individual function.
    let needs_ble_pause = lower == "ganglion"
        || lower.contains("mw75")
        || lower.contains("neurable")
        || lower.contains("hermes")
        || lower.contains("mendi")
        || lower.contains("idun")
        || lower.contains("guardian")
        || lower.starts_with("ige")
        || lower.starts_with("ble:")
        // catch generic Muse targets (device name used as target)
        || lower.starts_with("muse");

    if needs_ble_pause {
        state.ble_scan_paused.store(true, std::sync::atomic::Ordering::Relaxed);
        // Allow up to 400 ms for the listener task to detect the flag and
        // call stop_scan().  The event loop now has a 300 ms timeout so the
        // listener notices the flag within 300 ms; stop_scan() is near-instant.
        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    let result = connect_device_inner(state, target, &lower).await;

    if needs_ble_pause {
        state.ble_scan_paused.store(false, std::sync::atomic::Ordering::Relaxed);
    }

    result
}

/// Look up the human-readable name for a paired device ID from the daemon's
/// in-memory paired list.  Used to give BLE clients a specific name prefix so
/// they can use the fast event-driven `connect()` path (~250 ms) instead of
/// the fixed-sleep `scan_all()` path (3-5 s).
fn paired_name_for(state: &AppState, target: &str) -> Option<String> {
    state
        .status
        .lock()
        .ok()
        .and_then(|s| s.paired_devices.iter().find(|d| d.id == target).map(|d| d.name.clone()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectRoute {
    OpenBci,
    Cognionics,
    Emotiv,
    Ganglion,
    Lsl,
    Brainmaster,
    Brainbit,
    Neurosky,
    Neurosity,
    Brainvision,
    Neurofield,
    Gtec,
    Mw75,
    Hermes,
    Idun,
    Mendi,
    IrohRemote,
    Muse,
}

type ConnectPredicate = fn(&str) -> bool;

fn is_openbci(s: &str) -> bool {
    s == "openbci" || s.starts_with("usb:")
}
fn is_cognionics(s: &str) -> bool {
    s.starts_with("cgx:")
}
fn is_emotiv(s: &str) -> bool {
    s.starts_with("cortex:")
}
fn is_ganglion(s: &str) -> bool {
    s == "ganglion"
}
fn is_lsl(s: &str) -> bool {
    s.starts_with("lsl:") || s == "lsl"
}
fn is_brainmaster(s: &str) -> bool {
    s.starts_with("brainmaster:") || s.contains("brainmaster")
}
fn is_brainbit(s: &str) -> bool {
    s.starts_with("brainbit:") || s.contains("brainbit")
}
fn is_neurosky(s: &str) -> bool {
    s.starts_with("neurosky:") || s == "neurosky" || s.contains("mindwave")
}
fn is_neurosity(s: &str) -> bool {
    s.starts_with("neurosity:") || s == "neurosity" || s.contains("crown") || s.contains("notion")
}
fn is_brainvision(s: &str) -> bool {
    s.starts_with("brainvision:") || s == "brainvision" || s.starts_with("rda:")
}
fn is_neurofield(s: &str) -> bool {
    s.starts_with("neurofield:") || s.contains("neurofield")
}
fn is_gtec(s: &str) -> bool {
    s.starts_with("gtec:") || s.contains("unicorn")
}
fn is_mw75(s: &str) -> bool {
    s.contains("mw75") || s.contains("neurable")
}
fn is_hermes(s: &str) -> bool {
    s.contains("hermes")
}
fn is_idun(s: &str) -> bool {
    s.contains("idun") || s.contains("guardian")
}
fn is_mendi(s: &str) -> bool {
    s.contains("mendi")
}
fn is_iroh_remote(s: &str) -> bool {
    s.starts_with("peer:")
}

const CONNECT_ROUTE_RULES: &[(ConnectPredicate, ConnectRoute)] = &[
    (is_openbci, ConnectRoute::OpenBci),
    (is_cognionics, ConnectRoute::Cognionics),
    (is_emotiv, ConnectRoute::Emotiv),
    (is_ganglion, ConnectRoute::Ganglion),
    (is_lsl, ConnectRoute::Lsl),
    (is_brainmaster, ConnectRoute::Brainmaster),
    (is_brainbit, ConnectRoute::Brainbit),
    (is_neurosky, ConnectRoute::Neurosky),
    (is_neurosity, ConnectRoute::Neurosity),
    (is_brainvision, ConnectRoute::Brainvision),
    (is_neurofield, ConnectRoute::Neurofield),
    (is_gtec, ConnectRoute::Gtec),
    (is_mw75, ConnectRoute::Mw75),
    (is_hermes, ConnectRoute::Hermes),
    (is_idun, ConnectRoute::Idun),
    (is_mendi, ConnectRoute::Mendi),
    (is_iroh_remote, ConnectRoute::IrohRemote),
];

fn matching_connect_routes(lower: &str) -> Vec<ConnectRoute> {
    CONNECT_ROUTE_RULES
        .iter()
        .filter_map(|(pred, route)| pred(lower).then_some(*route))
        .collect()
}

fn select_connect_route(lower: &str) -> ConnectRoute {
    matching_connect_routes(lower)
        .into_iter()
        .next()
        .unwrap_or(ConnectRoute::Muse)
}

async fn connect_device_inner(state: &AppState, target: &str, lower: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    match select_connect_route(lower) {
        ConnectRoute::OpenBci => connect_openbci(state, target).await,
        ConnectRoute::Cognionics => connect_cognionics(target).await,
        ConnectRoute::Emotiv => connect_emotiv(state).await,
        ConnectRoute::Ganglion => connect_ganglion(state).await,
        ConnectRoute::Lsl => connect_lsl(target).await,
        ConnectRoute::Brainmaster => connect_brainmaster(state, target).await,
        ConnectRoute::Brainbit => connect_brainbit(target).await,
        ConnectRoute::Neurosky => connect_neurosky(target).await,
        ConnectRoute::Neurosity => connect_neurosity(state, target).await,
        ConnectRoute::Brainvision => connect_brainvision(target).await,
        ConnectRoute::Neurofield => connect_neurofield(target).await,
        ConnectRoute::Gtec => connect_gtec(target).await,
        ConnectRoute::Mw75 => connect_mw75(paired_name_for(state, target)).await,
        ConnectRoute::Hermes => connect_hermes(paired_name_for(state, target)).await,
        ConnectRoute::Idun => connect_idun(state, paired_name_for(state, target)).await,
        ConnectRoute::Mendi => connect_mendi(paired_name_for(state, target)).await,
        ConnectRoute::IrohRemote => connect_iroh_remote(state, target).await,
        ConnectRoute::Muse => connect_muse(target, paired_name_for(state, target)).await,
    }
}

// ── Muse (BLE) ──────────────────────────────────────────────────────────────

async fn connect_muse(target: &str, paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use skill_devices::muse_rs::prelude::*;
    use skill_devices::session::muse::MuseAdapter;

    // Fast path: if we know the device's name from the paired list, use
    // connect() which polls every 250 ms and exits as soon as the device
    // is found (~250 ms).  Fall back to scan_all() only when the name is
    // unknown (first-time unpaired connect).
    if let Some(name) = paired_name {
        info!(name = %name, "connecting to Muse (fast path)");
        let config = MuseClientConfig {
            name_prefix: name.clone(),
            enable_ppg: true,
            scan_timeout_secs: 5,
            ..Default::default()
        };
        let client = MuseClient::new(config);
        let (rx, handle) = client.connect().await.context("Muse connect")?;
        handle.start(true, false).await.context("Muse start")?;
        let _ = handle.request_device_info().await;
        return Ok(Box::new(MuseAdapter::new(rx, handle)));
    }

    // Slow path: scan for 5 s then filter by UUID.
    info!("scanning for Muse headband (slow path)…");
    let client = MuseClient::new(MuseClientConfig {
        scan_timeout_secs: 5,
        enable_ppg: true,
        ..Default::default()
    });
    let devices = client.scan_all().await.context("Muse scan")?;
    let target_ble_id = target.strip_prefix("ble:").unwrap_or("");
    let device = if !target_ble_id.is_empty() {
        devices
            .into_iter()
            .find(|d| d.id.eq_ignore_ascii_case(target_ble_id))
            .ok_or_else(|| anyhow::anyhow!("Muse {target} not found nearby"))?
    } else {
        devices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No Muse device found nearby"))?
    };
    info!(name = %device.name, id = %device.id, "connecting to Muse");
    let (rx, handle) = client.connect_to(device).await.context("Muse connect")?;
    handle.start(true, false).await.context("Muse start")?;
    let _ = handle.request_device_info().await;
    Ok(Box::new(MuseAdapter::new(rx, handle)))
}

// ── MW75 Neuro (BLE) ────────────────────────────────────────────────────────

async fn connect_mw75(paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use skill_devices::mw75::prelude::*;
    use skill_devices::session::mw75::Mw75Adapter;

    let config = Mw75ClientConfig {
        name_pattern: paired_name.unwrap_or_else(|| "MW75".into()),
        scan_timeout_secs: 5,
        ..Default::default()
    };
    info!(name_pattern = %config.name_pattern, "connecting to MW75 Neuro");
    let client = Mw75Client::new(config);
    let (rx, handle) = client.connect().await.context("MW75 connect")?;
    let handle = std::sync::Arc::new(handle);
    Ok(Box::new(Mw75Adapter::new(rx, handle, None)))
}

// ── Hermes V1 (BLE) ─────────────────────────────────────────────────────────

async fn connect_hermes(paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use skill_devices::hermes_ble::prelude::*;
    use skill_devices::session::hermes::HermesAdapter;

    let config = HermesClientConfig {
        name_prefix: paired_name.unwrap_or_else(|| "Hermes".into()),
        scan_timeout_secs: 5,
    };
    info!(name_prefix = %config.name_prefix, "connecting to Hermes");
    let client = HermesClient::new(config);
    let (rx, handle) = client.connect().await.context("Hermes connect")?;
    Ok(Box::new(HermesAdapter::new(rx, handle)))
}

// ── IDUN Guardian (BLE) ──────────────────────────────────────────────────────

async fn connect_idun(state: &AppState, paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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
        name_prefix: paired_name.unwrap_or_else(|| "IGE".into()),
        scan_timeout_secs: 5,
        ..Default::default()
    };
    info!(name_prefix = %config.name_prefix, "connecting to IDUN Guardian");
    let client = GuardianClient::new(config);
    let (rx, handle) = client.connect().await.context("IDUN connect")?;
    Ok(Box::new(IdunAdapter::new(rx, handle)))
}

// ── Mendi fNIRS (BLE) ────────────────────────────────────────────────────────

async fn connect_mendi(paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use skill_devices::mendi::prelude::*;
    use skill_devices::session::mendi::MendiAdapter;

    let config = MendiClientConfig {
        name_prefix: paired_name.unwrap_or_else(|| "Mendi".into()),
        scan_timeout_secs: 5,
        ..Default::default()
    };
    info!(name_prefix = %config.name_prefix, "connecting to Mendi");
    let client = MendiClient::new(config);
    let (rx, handle) = client.connect().await.context("Mendi connect")?;
    Ok(Box::new(MendiAdapter::new(rx, handle)))
}

// ── OpenBCI Ganglion (BLE) ───────────────────────────────────────────────────

async fn connect_ganglion(state: &AppState) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

    let adapter = tokio::task::spawn_blocking(move || -> anyhow::Result<Box<dyn DeviceAdapter>> {
        let mut board = GanglionBoard::new(ganglion_config);
        board.prepare().context("Ganglion prepare")?;
        let stream = board.start_stream().context("Ganglion stream")?;
        let ch: Vec<String> = (1..=4).map(|i| format!("Ch{i}")).collect();
        let desc = OpenBciAdapter::make_descriptor("ganglion", 4, 200.0, ch);
        let info = DeviceInfo {
            name: "Ganglion".into(),
            ..Default::default()
        };
        Ok(Box::new(OpenBciAdapter::start(stream, desc, info)) as Box<dyn DeviceAdapter>)
    })
    .await
    .context("spawn")??;

    Ok(adapter)
}

// ── OpenBCI Cyton/Daisy (USB serial) ─────────────────────────────────────────

async fn connect_openbci(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

async fn connect_cognionics(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

async fn connect_neurofield(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

async fn connect_brainmaster(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

fn lsl_query_from_target(target: &str) -> String {
    if target.len() >= 4 && target[..4].eq_ignore_ascii_case("lsl:") {
        return target[4..].to_string();
    }
    "".to_string()
}

// ── Iroh remote (iOS / phone streaming over iroh QUIC tunnel) ────────────────

async fn connect_iroh_remote(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use skill_devices::session::iroh_remote::IrohRemoteAdapter;

    let peer_id = target.strip_prefix("peer:").unwrap_or(target).to_string();
    info!(peer_id = %peer_id, "connecting iroh remote adapter");

    // Create a fresh channel pair and install the sender in the shared slot.
    // The tunnel's per-message tx re-read will pick this up immediately so
    // events from the phone start flowing into this session's rx.
    let (tx, rx) = skill_iroh::event_channel();
    if let Ok(mut g) = state.iroh_device_tx.lock() {
        *g = Some(tx);
    }

    Ok(Box::new(IrohRemoteAdapter::new(rx, peer_id)))
}

async fn connect_lsl(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

async fn connect_neurosky(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

async fn connect_neurosity(state: &AppState, target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

async fn connect_brainvision(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

// ── BrainBit (BLE via NeuroSDK2) ───────────────────────────────────────────

async fn connect_brainbit(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use brainbit::prelude::*;

    info!("scanning for BrainBit…");
    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<Vec<brainbit::device::EegSample>>(64);
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let target_addr = target.strip_prefix("brainbit:").unwrap_or("").to_string();

    let (device_name, device_addr, keepalive_thread) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let scanner = Scanner::new(&[SensorFamily::LEBrainBit]).context("BrainBit scanner")?;
        scanner.start().context("BrainBit scan start")?;
        std::thread::sleep(std::time::Duration::from_secs(5));
        scanner.stop().context("BrainBit scan stop")?;
        let devices = scanner.devices().context("BrainBit devices")?;
        if devices.is_empty() {
            anyhow::bail!("No BrainBit device found nearby");
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
        .ok_or_else(|| anyhow::anyhow!("No matching BrainBit device"))?;

        let mut device = BrainBitDevice::connect(&scanner, info).context("BrainBit connect")?;
        let name = device.name().unwrap_or_else(|_| "BrainBit".into());
        let addr = device.address().unwrap_or_default();

        // Set up streaming callback.
        let tx = sample_tx;
        device
            .on_signal(move |samples| {
                let _ = tx.blocking_send(samples.to_vec());
            })
            .context("BrainBit on_signal")?;
        device.start_signal().context("BrainBit start_signal")?;

        // Keep scanner/device alive until adapter disconnects.
        let keepalive_thread = std::thread::Builder::new()
            .name("brainbit-keepalive".to_string())
            .spawn(move || {
                let _ = stop_rx.recv();
                drop(device);
                drop(scanner);
            })
            .context("spawn keepalive")?;

        Ok((name, addr, keepalive_thread))
    })
    .await
    .context("spawn")??;

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

async fn connect_gtec(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use gtec::prelude::*;

    let serial = target.strip_prefix("gtec:").unwrap_or("").to_string();
    info!(serial = %serial, "connecting to g.tec Unicorn");

    let (sample_tx, sample_rx) = tokio::sync::mpsc::channel::<gtec::device::Scan>(512);

    let (device_serial, read_thread) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let serial = if serial.is_empty() {
            let serials = UnicornDevice::scan(true).context("scan")?;
            serials
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No g.tec Unicorn found"))?
        } else {
            serial
        };

        let mut device = UnicornDevice::open(&serial).context("open")?;
        device.start_acquisition(false).context("start")?;

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
            .context("spawn reader")?;

        Ok((dev_serial, read_thread))
    })
    .await
    .context("spawn")??;

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

async fn connect_emotiv(state: &AppState) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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
    if lower.starts_with("neurosky:") || lower == "neurosky" {
        return "neurosky";
    }
    if lower.starts_with("neurosity:") {
        return "neurosity";
    }
    if lower.starts_with("brainvision:") {
        return "brainvision";
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
    if lower.starts_with("peer:") {
        return "iroh-remote";
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_kind_covers_supported_device_targets() {
        let cases = [
            ("muse", "muse"),
            ("MW75-ABCD", "mw75"),
            ("Hermes-001", "hermes"),
            ("Idun-Guardian", "idun"),
            ("Mendi-XY", "mendi"),
            ("ganglion", "ganglion"),
            ("openbci", "openbci"),
            ("usb:COM3", "openbci/cyton"),
            ("lsl", "lsl"),
            ("lsl:SkillVirtualEEG", "lsl"),
            ("neurofield:USB1:5", "neurofield"),
            ("brainbit:AA:BB", "brainbit"),
            ("gtec:UN-123", "gtec"),
            ("brainmaster:/dev/ttyUSB0", "brainmaster"),
            ("cortex:emotiv", "emotiv"),
            ("cgx:/dev/ttyUSB1", "cognionics"),
            ("neurosky:/dev/ttyUSB0", "neurosky"),
            ("neurosity:device123", "neurosity"),
            ("brainvision:127.0.0.1:51244", "brainvision"),
        ];

        for (target, expected) in cases {
            assert_eq!(infer_kind_from_target(target), expected, "target={target}");
        }
    }

    #[test]
    fn lsl_target_query_parsing_configurations() {
        assert_eq!(lsl_query_from_target("lsl"), "");
        assert_eq!(lsl_query_from_target("lsl:"), "");
        assert_eq!(lsl_query_from_target("lsl:SkillVirtualEEG"), "SkillVirtualEEG");
        assert_eq!(lsl_query_from_target("lsl:EEG-32ch@1kHz"), "EEG-32ch@1kHz");
        assert_eq!(lsl_query_from_target("LSL:SkillVirtualEEG"), "SkillVirtualEEG");
        assert_eq!(lsl_query_from_target("LsL:MixedCase"), "MixedCase");
        assert_eq!(lsl_query_from_target("not-lsl"), "");
    }

    #[tokio::test]
    async fn connect_lsl_missing_named_stream_returns_error() {
        let t0 = std::time::Instant::now();
        let res = connect_lsl("lsl:THIS_STREAM_SHOULD_NOT_EXIST_987654321").await;
        let elapsed = t0.elapsed();
        assert!(res.is_err(), "missing LSL stream should error");
        let msg = res.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("No LSL stream matching"), "unexpected error: {msg}");
        assert!(
            elapsed < std::time::Duration::from_secs(8),
            "LSL missing-stream failure too slow: {elapsed:?}"
        );
    }

    #[test]
    fn infer_kind_unknown_defaults_to_muse() {
        assert_eq!(infer_kind_from_target("totally-unknown-device"), "muse");
    }

    #[test]
    fn infer_kind_prefixes_are_case_insensitive() {
        assert_eq!(infer_kind_from_target("NEUROFIELD:USB1:1"), "neurofield");
        assert_eq!(infer_kind_from_target("BRAINBIT:AA:BB"), "brainbit");
        assert_eq!(infer_kind_from_target("GTEC:UN-1"), "gtec");
        assert_eq!(infer_kind_from_target("BRAINMASTER:COM3"), "brainmaster");
        assert_eq!(infer_kind_from_target("CORTEX:EMOTIV"), "emotiv");
        assert_eq!(infer_kind_from_target("CGX:/dev/ttyUSB0"), "cognionics");
        assert_eq!(infer_kind_from_target("LSL:MyStream"), "lsl");
        assert_eq!(infer_kind_from_target("USB:COM4"), "openbci/cyton");
    }

    #[test]
    fn select_connect_route_covers_aliases_and_prefixes() {
        let cases = [
            ("openbci", ConnectRoute::OpenBci),
            ("usb:COM3", ConnectRoute::OpenBci),
            ("cgx:/dev/ttyUSB1", ConnectRoute::Cognionics),
            ("cortex:emotiv", ConnectRoute::Emotiv),
            ("ganglion", ConnectRoute::Ganglion),
            ("lsl", ConnectRoute::Lsl),
            ("lsl:SkillVirtualEEG", ConnectRoute::Lsl),
            ("brainmaster:/dev/ttyUSB0", ConnectRoute::Brainmaster),
            ("brainbit:AA:BB", ConnectRoute::Brainbit),
            ("neurosky:/dev/ttyUSB0", ConnectRoute::Neurosky),
            ("neurosity:device123", ConnectRoute::Neurosity),
            ("brainvision:127.0.0.1:51244", ConnectRoute::Brainvision),
            ("neurofield:USB1:5", ConnectRoute::Neurofield),
            ("gtec:UN-123", ConnectRoute::Gtec),
            ("MW75-ABCD", ConnectRoute::Mw75),
            ("Hermes-001", ConnectRoute::Hermes),
            ("Idun-Guardian", ConnectRoute::Idun),
            ("Mendi-XY", ConnectRoute::Mendi),
            ("totally-unknown-device", ConnectRoute::Muse),
        ];

        for (target, expected) in cases {
            let lower = target.to_ascii_lowercase();
            assert_eq!(select_connect_route(&lower), expected, "target={target}");
        }
    }

    #[test]
    fn connect_route_rules_do_not_overlap_for_known_targets() {
        let targets = [
            "openbci",
            "usb:COM3",
            "cgx:/dev/ttyUSB1",
            "cortex:emotiv",
            "ganglion",
            "lsl:SkillVirtualEEG",
            "brainmaster:/dev/ttyUSB0",
            "brainbit:AA:BB",
            "neurosky:/dev/ttyUSB0",
            "neurosity:device123",
            "brainvision:127.0.0.1:51244",
            "neurofield:USB1:5",
            "gtec:UN-123",
            "MW75-ABCD",
            "Hermes-001",
            "Idun-Guardian",
            "Mendi-XY",
        ];

        for target in targets {
            let lower = target.to_ascii_lowercase();
            let matches = matching_connect_routes(&lower);
            assert_eq!(
                matches.len(),
                1,
                "ambiguous route rules for target={target}: {matches:?}"
            );
        }
    }

    #[test]
    fn select_connect_route_is_deterministic_for_random_targets() {
        use rand::{Rng, SeedableRng};

        let mut rng = rand::rngs::StdRng::seed_from_u64(0x5EED_BAAD_F00D);
        for _ in 0..512 {
            let len = rng.random_range(0..64);
            let s: String = (0..len)
                .map(|_| {
                    let c = rng.random_range(0x20u8..0x7Eu8);
                    c as char
                })
                .collect();
            let lower = s.to_ascii_lowercase();
            let a = select_connect_route(&lower);
            let b = select_connect_route(&lower);
            assert_eq!(a, b, "non-deterministic route for input={s:?}");
        }
    }

    #[test]
    fn paired_name_lookup_uses_status_paired_devices() {
        let td = tempfile::tempdir().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        if let Ok(mut s) = state.status.lock() {
            s.paired_devices.push(skill_daemon_common::PairedDeviceResponse {
                id: "ble:abc".into(),
                name: "Muse S Alice".into(),
                last_seen: 0,
            });
        }

        assert_eq!(paired_name_for(&state, "ble:abc").as_deref(), Some("Muse S Alice"));
        assert_eq!(paired_name_for(&state, "ble:missing"), None);
    }

    #[test]
    fn push_device_log_static_caps_to_256() {
        let td = tempfile::tempdir().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        for i in 0..300 {
            push_device_log_static(&state, "session", &format!("m{i}"));
        }

        let log = state.device_log.lock().unwrap();
        assert_eq!(log.len(), 256);
        assert_eq!(log.front().map(|e| e.msg.clone()), Some("m44".into()));
        assert_eq!(log.back().map(|e| e.msg.clone()), Some("m299".into()));
    }
}
