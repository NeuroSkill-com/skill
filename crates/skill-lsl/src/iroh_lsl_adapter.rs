// SPDX-License-Identifier: GPL-3.0-only
//! Remote LSL stream over iroh QUIC → [`DeviceAdapter`].
//!
//! Starts an `rlsl-iroh` sink that accepts incoming connections, re-publishes
//! streams as local LSL outlets, then pulls from those outlets as DeviceEvents.

use std::sync::Arc;

use async_trait::async_trait;
use iroh::protocol::ProtocolHandler;
use iroh::Endpoint;
use tokio::sync::mpsc;

use skill_devices::session::{DeviceAdapter, DeviceCaps, DeviceDescriptor, DeviceEvent, DeviceInfo, EegFrame};

/// Maximum time to wait for a remote LSL source to connect (seconds).
const RESOLVE_TIMEOUT_SECS: u64 = 120;

pub struct IrohLslAdapter {
    rx: mpsc::Receiver<DeviceEvent>,
    desc: DeviceDescriptor,
    endpoint_id: String,
    _shutdown: mpsc::Sender<()>,
}

impl IrohLslAdapter {
    /// Start the iroh LSL sink and wait for a remote source to connect.
    ///
    /// Blocks (async) until a remote LSL source connects and the
    /// re-published local outlet is resolved, so the returned adapter
    /// has the correct channel count and sample rate.
    ///
    /// Returns `(adapter, endpoint_id)` or an error on timeout (120 s).
    pub async fn start_sink() -> Result<(Self, String), String> {
        let (endpoint_id, adapter) = Self::start_sink_two_phase().await?;
        let adapter = adapter.await.map_err(|e| format!("sink resolve: {e}"))?;
        Ok((adapter?, endpoint_id))
    }

    /// Two-phase start: returns the endpoint ID immediately and a future
    /// that resolves to the adapter once a remote source connects.
    ///
    /// Use this when you need the endpoint ID before the remote connects
    /// (e.g. to display a QR code or share the ID with the remote client).
    ///
    /// ```ignore
    /// let (endpoint_id, adapter_fut) = IrohLslAdapter::start_sink_two_phase().await?;
    /// // Show endpoint_id to user...
    /// let (adapter, _) = adapter_fut.await??;
    /// ```
    pub async fn start_sink_two_phase() -> Result<(String, tokio::task::JoinHandle<Result<Self, String>>), String> {
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .alpns(vec![rlsl_iroh::protocol::LSL_ALPN.to_vec()])
            .relay_mode(iroh::RelayMode::Default)
            .bind()
            .await
            .map_err(|e| format!("iroh bind: {e}"))?;

        endpoint.online().await;
        let endpoint_id = endpoint.id().to_string();
        let ep_arc = Arc::new(endpoint);

        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Accept iroh connections → rlsl-iroh sink re-publishes as local LSL outlets
        let ep_accept = ep_arc.clone();
        tokio::spawn(async move {
            let handler = rlsl_iroh::sink::LslSinkHandler;
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

        let eid = endpoint_id.clone();
        let handle = tokio::spawn(async move {
            log::info!("[rlsl-iroh] waiting for remote LSL source...");
            let info = resolve_eeg_stream(RESOLVE_TIMEOUT_SECS).await?;

            let ch = info.channel_count() as usize;
            let sr = info.nominal_srate();
            let name = info.name().to_string();
            let sid = info.source_id().to_string();
            let stype = info.type_().to_string();

            let channel_names = read_channel_labels(&info, ch);

            log::info!("[rlsl-iroh] resolved '{}' ({}ch @ {}Hz, source={})", name, ch, sr, sid);

            let desc = DeviceDescriptor {
                kind: "lsl-iroh",
                caps: DeviceCaps::EEG,
                eeg_channels: ch,
                eeg_sample_rate: sr,
                channel_names,
                pipeline_channels: ch.min(skill_constants::EEG_CHANNELS),
                ppg_channel_names: Vec::new(),
                imu_channel_names: Vec::new(),
                fnirs_channel_names: Vec::new(),
            };

            let (tx, rx) = mpsc::channel(256);

            let _ = tx
                .send(DeviceEvent::Connected(DeviceInfo {
                    name: format!("{name} (rlsl-iroh)"),
                    id: sid,
                    serial_number: None,
                    firmware_version: None,
                    hardware_version: Some(format!("{stype} via rlsl-iroh")),
                    bootloader_version: None,
                    mac_address: None,
                    headset_preset: None,
                }))
                .await;

            // Pull loop on a dedicated thread (inlet blocks)
            let tx2 = tx;
            let info2 = info.clone();
            std::thread::Builder::new()
                .name("rlsl-iroh-inlet".into())
                .spawn(move || {
                    let inlet = rlsl::inlet::StreamInlet::new(&info2, 360, 0, true);
                    let time_correction = inlet.time_correction(1.0);
                    let mut buf = vec![0.0f64; ch];
                    loop {
                        let ts = match inlet.pull_sample_d(&mut buf, 0.2) {
                            Ok(t) if t > 0.0 => t + time_correction,
                            _ => continue,
                        };
                        if tx2
                            .blocking_send(DeviceEvent::Eeg(EegFrame {
                                channels: buf.to_vec(),
                                timestamp_s: ts,
                            }))
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .ok();

            Ok(Self {
                rx,
                desc,
                endpoint_id: eid,
                _shutdown: shutdown_tx,
            })
        });

        Ok((endpoint_id, handle))
    }

    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }
}

/// Resolve a re-published local LSL EEG outlet with timeout.
async fn resolve_eeg_stream(timeout_secs: u64) -> Result<rlsl::stream_info::StreamInfo, String> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(format!("Timed out waiting for remote LSL source ({timeout_secs}s)"));
        }
        let streams = tokio::task::spawn_blocking(|| rlsl::resolver::resolve_all(3.0))
            .await
            .unwrap_or_default();

        let found = streams.into_iter().find(|s| {
            let t = s.type_().to_lowercase();
            t == "eeg" || t == "exg" || t == "biosignal"
        });
        if let Some(info) = found {
            return Ok(info);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Read channel labels from an LSL stream's XML description.
fn read_channel_labels(info: &rlsl::stream_info::StreamInfo, ch: usize) -> Vec<String> {
    let xml = info.desc();
    let channels_node = xml.child("channels");
    let mut names = Vec::with_capacity(ch);
    if !channels_node.is_empty() {
        let mut node = channels_node.child("channel");
        while !node.is_empty() {
            let label = node.child_value("label");
            if label.is_empty() {
                names.push(format!("Ch{}", names.len() + 1));
            } else {
                names.push(label);
            }
            node = node.next_sibling_named("channel");
        }
    }
    while names.len() < ch {
        names.push(format!("Ch{}", names.len() + 1));
    }
    names.truncate(ch);
    names
}

#[async_trait]
impl DeviceAdapter for IrohLslAdapter {
    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    async fn next_event(&mut self) -> Option<DeviceEvent> {
        self.rx.recv().await
    }
    async fn disconnect(&mut self) {}
}
