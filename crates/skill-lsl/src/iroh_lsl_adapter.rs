// SPDX-License-Identifier: GPL-3.0-only
//! Remote LSL stream over iroh QUIC → [`DeviceAdapter`].
//!
//! Starts an `rlsl-iroh` sink that accepts incoming connections, re-publishes
//! streams as local LSL outlets, then pulls from those outlets as DeviceEvents.

use std::sync::Arc;

use async_trait::async_trait;
use iroh::Endpoint;
use iroh::protocol::ProtocolHandler;
use rlsl_iroh::sink::LslSinkHandler;
use tokio::sync::mpsc;

use skill_devices::session::{
    DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame,
};

pub struct IrohLslAdapter {
    rx: mpsc::Receiver<DeviceEvent>,
    desc: DeviceDescriptor,
    endpoint_id: String,
    _shutdown: mpsc::Sender<()>,
}

impl IrohLslAdapter {
    /// Start the iroh LSL sink.  Returns `(adapter, endpoint_id)`.
    ///
    /// Share the `endpoint_id` with the remote `rlsl-iroh source` so it
    /// can connect and stream LSL data through the relay.
    pub async fn start_sink() -> Result<(Self, String), String> {
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![rlsl_iroh::protocol::LSL_ALPN.to_vec()])
            .relay_mode(iroh::RelayMode::Default)
            .bind()
            .await
            .map_err(|e| format!("iroh bind: {e}"))?;

        endpoint.online().await;
        let endpoint_id = endpoint.id().to_string();
        let ep_arc = Arc::new(endpoint);

        let (tx, rx) = mpsc::channel(256);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Accept iroh connections → rlsl-iroh sink re-publishes as local LSL outlets
        let ep_accept = ep_arc.clone();
        tokio::spawn(async move {
            let handler = LslSinkHandler;
            loop {
                tokio::select! {
                    incoming = ep_accept.accept() => {
                        let Some(incoming) = incoming else { break };
                        let conn = match incoming.accept() {
                            Ok(a) => match a.await {
                                Ok(c) => c,
                                Err(e) => { log::error!("[rlsl-iroh] handshake: {e}"); continue; }
                            },
                            Err(e) => { log::error!("[rlsl-iroh] accept: {e}"); continue; }
                        };
                        log::info!("[rlsl-iroh] peer connected: {}", conn.remote_id());
                        let h = handler.clone();
                        tokio::spawn(async move {
                            if let Err(e) = h.accept(conn).await {
                                log::error!("[rlsl-iroh] handler: {e:?}");
                            }
                        });
                    }
                    _ = shutdown_rx.recv() => break,
                }
            }
        });

        // Background: resolve the re-published local outlet and pull samples
        let tx2 = tx;
        tokio::spawn(async move {
            // Give the sink a moment to republish
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            loop {
                let streams = tokio::task::spawn_blocking(|| {
                    rlsl::resolver::resolve_all(3.0)
                }).await.unwrap_or_default();

                let info = streams.iter().find(|s| {
                    let t = s.type_().to_lowercase();
                    t == "eeg" || t == "exg" || t == "biosignal"
                });
                let Some(info) = info.cloned() else {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                };

                let ch = info.channel_count() as usize;
                let name = info.name().to_string();
                let sid = info.source_id().to_string();
                let stype = info.type_().to_string();

                let _ = tx2.send(DeviceEvent::Connected(DeviceInfo {
                    name: format!("{name} (rlsl-iroh)"),
                    id: sid,
                    serial_number: None,
                    firmware_version: None,
                    hardware_version: Some(format!("{stype} via rlsl-iroh")),
                    bootloader_version: None,
                    mac_address: None,
                    headset_preset: None,
                })).await;

                // Pull loop on a dedicated thread (inlet blocks)
                let tx3 = tx2.clone();
                let info2 = info.clone();
                std::thread::Builder::new()
                    .name("rlsl-iroh-inlet".into())
                    .spawn(move || {
                        let inlet = rlsl::inlet::StreamInlet::new(&info2, 360, 0, true);
                        let mut buf = vec![0.0f64; ch];
                        loop {
                            let ts = match inlet.pull_sample_d(&mut buf, 0.2) {
                                Ok(t) if t > 0.0 => t,
                                _ => continue,
                            };
                            if tx3.blocking_send(DeviceEvent::Eeg(EegFrame {
                                channels: buf.to_vec(),
                                timestamp_s: ts,
                            })).is_err() {
                                break;
                            }
                        }
                    })
                    .ok();
                break;
            }
        });

        let desc = DeviceDescriptor {
            kind: "lsl-iroh",
            caps: DeviceCaps::EEG,
            eeg_channels: 4,
            eeg_sample_rate: 256.0,
            channel_names: vec!["Ch1".into(), "Ch2".into(), "Ch3".into(), "Ch4".into()],
            pipeline_channels: 4,
            ppg_channel_names: Vec::new(),
            imu_channel_names: Vec::new(),
            fnirs_channel_names: Vec::new(),
        };

        Ok((Self { rx, desc, endpoint_id: endpoint_id.clone(), _shutdown: shutdown_tx }, endpoint_id))
    }

    pub fn endpoint_id(&self) -> &str { &self.endpoint_id }
}

#[async_trait]
impl DeviceAdapter for IrohLslAdapter {
    fn descriptor(&self) -> &DeviceDescriptor { &self.desc }
    async fn next_event(&mut self) -> Option<DeviceEvent> { self.rx.recv().await }
    async fn disconnect(&mut self) {}
}
