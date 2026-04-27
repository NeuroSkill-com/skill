// SPDX-License-Identifier: GPL-3.0-only
//! BLE device connection functions — Muse, MW75, Hermes, IDUN, Mendi,
//! Ganglion, BrainBit, g.tec Unicorn.

use anyhow::Context as _;
use std::time::Duration;

use skill_devices::session::{DeviceAdapter, DeviceInfo};
use tracing::info;

use crate::state::AppState;

// ── Muse (BLE) ──────────────────────────────────────────────────────────────

pub(super) async fn connect_muse(target: &str, paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

pub(super) async fn connect_mw75(paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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
    handle.start().await.context("MW75 activation")?;

    #[cfg(feature = "mw75-rfcomm")]
    let addr = handle.peripheral_id();
    #[cfg(feature = "mw75-rfcomm")]
    handle.disconnect_ble().await.ok();

    let handle = std::sync::Arc::new(handle);

    #[cfg(feature = "mw75-rfcomm")]
    let rfcomm = skill_devices::mw75::rfcomm::start_rfcomm_stream(handle.clone(), &addr)
        .await
        .context("MW75 RFCOMM open")?;

    #[cfg_attr(not(feature = "mw75-rfcomm"), allow(unused_mut))]
    let mut adapter = Mw75Adapter::new(rx, handle, None);
    #[cfg(feature = "mw75-rfcomm")]
    adapter.set_rfcomm(rfcomm);
    Ok(Box::new(adapter))
}

// ── Hermes V1 (BLE) ─────────────────────────────────────────────────────────

pub(super) async fn connect_hermes(paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

pub(super) async fn connect_idun(
    state: &AppState,
    paired_name: Option<String>,
) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

// ── AWEAR EEG (BLE) ─────────────────────────────────────────────────────────

pub(super) async fn connect_awear(_paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
    use skill_devices::awear::prelude::*;
    use skill_devices::session::awear::AwearAdapter;

    let config = AwearClientConfig::default();
    info!("connecting to AWEAR EEG…");
    let client = AwearClient::new(config);
    let (rx, handle) = client.connect().await.context("AWEAR connect")?;
    // The device runs an HMAC-SHA256 challenge-response handshake after
    // connecting.  Wait for it to complete before requesting data.
    tokio::time::sleep(Duration::from_secs(2)).await;
    handle.start().await.context("AWEAR start streaming")?;
    Ok(Box::new(AwearAdapter::new(rx, handle)))
}

// ── Mendi fNIRS (BLE) ────────────────────────────────────────────────────────

pub(super) async fn connect_mendi(paired_name: Option<String>) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

pub(super) async fn connect_ganglion(state: &AppState) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

// ── BrainBit (BLE via NeuroSDK2) ───────────────────────────────────────────

pub(super) async fn connect_brainbit(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

pub(super) async fn connect_gtec(target: &str) -> anyhow::Result<Box<dyn DeviceAdapter>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use skill_devices::session::DeviceAdapter;

    #[tokio::test]
    async fn brainbit_adapter_emits_connected_first() {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "brainbit",
            eeg_channels: 4,
            eeg_sample_rate: 250.0,
            channel_names: vec!["O1".into(), "T3".into(), "T4".into(), "O2".into()],
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 4,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = BrainBitAdapter {
            name: "TestBrainBit".into(),
            desc,
            rx,
            stop_tx: None,
            keepalive_thread: None,
            connected_sent: false,
        };
        // First event should be Connected
        let ev = adapter.next_event().await;
        assert!(matches!(ev, Some(skill_devices::session::DeviceEvent::Connected(_))));
        assert!(adapter.connected_sent);
        // Drop tx to signal end
        drop(tx);
        let ev = adapter.next_event().await;
        assert!(ev.is_none());
    }

    #[tokio::test]
    async fn gtec_adapter_emits_connected_first() {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "gtec",
            eeg_channels: 8,
            eeg_sample_rate: 250.0,
            channel_names: (1..=8).map(|i| format!("Ch{i}")).collect(),
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 8,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = GtecAdapter {
            name: "TestGtec".into(),
            desc,
            rx,
            read_thread: None,
            connected_sent: false,
        };
        let ev = adapter.next_event().await;
        assert!(matches!(ev, Some(skill_devices::session::DeviceEvent::Connected(_))));
        drop(tx);
        let ev = adapter.next_event().await;
        assert!(ev.is_none());
    }

    #[tokio::test]
    async fn brainbit_adapter_descriptor_matches() {
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "brainbit",
            eeg_channels: 4,
            eeg_sample_rate: 250.0,
            channel_names: vec!["O1".into(), "T3".into(), "T4".into(), "O2".into()],
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 4,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let adapter = BrainBitAdapter {
            name: "TestBB".into(),
            desc,
            rx,
            stop_tx: None,
            keepalive_thread: None,
            connected_sent: false,
        };
        let d = adapter.descriptor();
        assert_eq!(d.kind, "brainbit");
        assert_eq!(d.eeg_channels, 4);
        assert_eq!(d.eeg_sample_rate, 250.0);
    }

    #[tokio::test]
    async fn brainbit_adapter_disconnect_is_safe_when_empty() {
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "brainbit",
            eeg_channels: 4,
            eeg_sample_rate: 250.0,
            channel_names: vec!["O1".into(), "T3".into(), "T4".into(), "O2".into()],
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 4,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = BrainBitAdapter {
            name: "TestBB".into(),
            desc,
            rx,
            stop_tx: None,
            keepalive_thread: None,
            connected_sent: false,
        };
        // Should not panic even with no stop_tx or thread
        adapter.disconnect().await;
    }

    #[tokio::test]
    async fn gtec_adapter_descriptor_has_correct_channels() {
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        let ch: Vec<String> = (1..=8).map(|i| format!("Ch{i}")).collect();
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "gtec",
            eeg_channels: 8,
            eeg_sample_rate: 250.0,
            channel_names: ch,
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 8,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let adapter = GtecAdapter {
            name: "TestGtec".into(),
            desc,
            rx,
            read_thread: None,
            connected_sent: false,
        };
        assert_eq!(adapter.descriptor().eeg_channels, 8);
        assert_eq!(adapter.descriptor().channel_names.len(), 8);
    }

    #[tokio::test]
    async fn gtec_adapter_disconnect_is_safe() {
        let (_tx, rx) = tokio::sync::mpsc::channel(16);
        let desc = skill_devices::session::DeviceDescriptor {
            kind: "gtec",
            eeg_channels: 8,
            eeg_sample_rate: 250.0,
            channel_names: (1..=8).map(|i| format!("Ch{i}")).collect(),
            caps: skill_devices::session::DeviceCaps::EEG,
            pipeline_channels: 8,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };
        let mut adapter = GtecAdapter {
            name: "TestGtec".into(),
            desc,
            rx,
            read_thread: None,
            connected_sent: false,
        };
        adapter.disconnect().await;
    }
}
