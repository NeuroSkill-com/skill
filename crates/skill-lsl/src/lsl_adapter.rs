// SPDX-License-Identifier: GPL-3.0-only
//! Local-network LSL stream → [`DeviceAdapter`].

use async_trait::async_trait;
use rlsl::resolver;
use rlsl::stream_info::StreamInfo;
use tokio::sync::mpsc;

use skill_devices::session::{DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame};

/// Discover LSL EEG/EXG streams on the local network.
pub fn resolve_eeg_streams(timeout_secs: f64) -> Vec<StreamInfo> {
    resolver::resolve_all(timeout_secs)
        .into_iter()
        .filter(|s| {
            let t = s.type_().to_lowercase();
            t == "eeg" || t == "exg" || t == "biosignal"
        })
        .collect()
}

/// Lightweight description of a discovered LSL stream for UI display.
#[derive(Clone)]
pub struct LslStreamInfo {
    pub name: String,
    pub stream_type: String,
    pub channel_count: usize,
    pub sample_rate: f64,
    pub source_id: String,
    pub hostname: String,
    pub info: StreamInfo,
}

/// Resolve and return display-friendly stream info.
pub fn discover_streams(timeout_secs: f64) -> Vec<LslStreamInfo> {
    resolver::resolve_all(timeout_secs)
        .into_iter()
        .map(|s| LslStreamInfo {
            name: s.name().to_string(),
            stream_type: s.type_().to_string(),
            channel_count: s.channel_count() as usize,
            sample_rate: s.nominal_srate(),
            source_id: s.source_id().to_string(),
            hostname: s.hostname().to_string(),
            info: s,
        })
        .collect()
}

/// DeviceAdapter that pulls from a local LSL stream.
pub struct LslAdapter {
    rx: mpsc::Receiver<DeviceEvent>,
    desc: DeviceDescriptor,
    _shutdown: mpsc::Sender<()>,
}

impl LslAdapter {
    pub fn new(info: &StreamInfo) -> Self {
        let channel_count = info.channel_count() as usize;
        let sample_rate = info.nominal_srate();
        let name = info.name().to_string();
        let stream_type = info.type_().to_string();
        let source_id = info.source_id().to_string();

        // Read channel labels from LSL description XML if available
        let channel_names: Vec<String> = {
            let desc = info.desc();
            let channels_node = desc.child("channels");
            let mut names = Vec::with_capacity(channel_count);
            if !channels_node.is_empty() {
                let mut ch = channels_node.child("channel");
                while !ch.is_empty() {
                    let label = ch.child_value("label");
                    if label.is_empty() {
                        names.push(format!("Ch{}", names.len() + 1));
                    } else {
                        names.push(label);
                    }
                    ch = ch.next_sibling_named("channel");
                }
            }
            // Pad if XML didn't have all channels
            while names.len() < channel_count {
                names.push(format!("Ch{}", names.len() + 1));
            }
            names.truncate(channel_count);
            names
        };

        let desc = DeviceDescriptor {
            kind: "lsl",
            caps: DeviceCaps::EEG,
            eeg_channels: channel_count,
            eeg_sample_rate: sample_rate,
            channel_names,
            // DSP pipeline processes up to EEG_CHANNELS; all channels stored in CSV
            pipeline_channels: channel_count.min(skill_constants::EEG_CHANNELS),
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };

        let (tx, rx) = mpsc::channel(256);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let info_clone = info.clone();

        std::thread::Builder::new()
            .name(format!("lsl-inlet-{name}"))
            .spawn(move || {
                let inlet = rlsl::inlet::StreamInlet::new(&info_clone, 360, 0, true);

                let _ = tx.blocking_send(DeviceEvent::Connected(DeviceInfo {
                    name: name.clone(),
                    id: source_id,
                    serial_number: None,
                    firmware_version: None,
                    hardware_version: Some(stream_type),
                    bootloader_version: None,
                    mac_address: None,
                    headset_preset: None,
                }));

                // Pull samples in a tight loop, but check for shutdown periodically.
                // LSL clock may differ from system clock — apply time correction.
                let time_correction = inlet.time_correction(1.0);
                let mut buf = vec![0.0f64; channel_count];
                loop {
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }

                    let Ok(ts) = inlet.pull_sample_d(&mut buf, 0.1) else {
                        continue;
                    };
                    if ts <= 0.0 {
                        continue;
                    }

                    // Correct LSL timestamp to local system time.
                    let corrected_ts = ts + time_correction;

                    if tx
                        .blocking_send(DeviceEvent::Eeg(EegFrame {
                            channels: buf.to_vec(),
                            timestamp_s: corrected_ts,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .expect("failed to spawn LSL inlet thread");

        Self {
            rx,
            desc,
            _shutdown: shutdown_tx,
        }
    }
}

#[async_trait]
impl DeviceAdapter for LslAdapter {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    async fn next_event(&mut self) -> Option<DeviceEvent> {
        self.rx.recv().await
    }
    async fn disconnect(&mut self) {}
}
