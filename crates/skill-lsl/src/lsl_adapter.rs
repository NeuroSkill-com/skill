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

/// Resolve a single LSL stream by name.  Returns as soon as a match is found
/// (typically < 500 ms for local streams) rather than waiting the full timeout.
pub fn resolve_stream_by_name(name: &str, timeout_secs: f64) -> Option<StreamInfo> {
    let query = format!("name='{name}'");
    let mut results = resolver::resolve_query(&query, 1, timeout_secs);
    if results.is_empty() {
        None
    } else {
        Some(results.swap_remove(0))
    }
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
    /// Connect to an LSL stream and return an adapter that produces
    /// [`DeviceEvent`]s.  Returns an error if the TCP data connection
    /// cannot be established (outlet gone, firewall, timeout).
    pub fn connect(info: &StreamInfo) -> Result<Self, String> {
        let channel_count = info.channel_count() as usize;
        let sample_rate = info.nominal_srate();
        let name = info.name().to_string();
        let stream_type = info.type_().to_string();
        let source_id = info.source_id().to_string();

        // Read channel labels from the LSL stream's XML description.
        //
        // Path 1 (in-process): when the caller passes the outlet's own
        // StreamInfo (e.g. unit tests), `info.desc()` already contains
        // the full `<channels>` XML — read labels directly.
        //
        // Path 2 (network-resolved): `resolve_query` returns a minimal
        // StreamInfo whose desc is `<desc></desc>`.  `rlsl` 0.0.4's
        // `get_fullinfo()` is a no-op stub, so we must fetch the full
        // XML ourselves via a TCP `LSL:fullinfo` request to the outlet's
        // data port and parse the `<label>` tags from the response.
        let channel_names: Vec<String> = {
            let mut names = read_labels_from_desc(info, channel_count);

            if names.is_empty() {
                // desc was empty (network-resolved) — fetch via TCP.
                names = fetch_labels_via_fullinfo(info, channel_count);
            }

            // Pad / truncate to exact channel count.
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
        let thread_channel_names = desc.channel_names.clone();
        let name_for_timeout = name.clone();

        // Channel for the inlet thread to report open_stream result back.
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        std::thread::Builder::new()
            .name(format!("lsl-inlet-{name}"))
            .spawn(move || {
                let inlet = rlsl::inlet::StreamInlet::new(&info_clone, 360, 0, true);
                inlet.set_postprocessing(rlsl::types::PROC_ALL);

                // Open the TCP data connection.  Signal the result back so
                // connect() can return an error to the caller rather than
                // silently entering a broken session.
                if let Err(e) = inlet.open_stream(10.0) {
                    let _ = ready_tx.send(Err(format!("LSL stream '{name}' found but TCP connection failed: {e}")));
                    return;
                }
                let _ = ready_tx.send(Ok(()));

                eprintln!(
                    "[lsl] connected to '{}' — {} ch @ {} Hz | channels: [{}]",
                    name,
                    channel_count,
                    sample_rate,
                    thread_channel_names.join(", "),
                );

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

                // Pull samples in batches to reduce per-sample overhead.
                loop {
                    if shutdown_rx.try_recv().is_ok() {
                        break;
                    }

                    let Ok((timestamps, data)) = inlet.pull_chunk_d(256, 0.1) else {
                        continue;
                    };
                    if timestamps.is_empty() {
                        continue;
                    }

                    let n_ch = channel_count;
                    for (i, &ts) in timestamps.iter().enumerate() {
                        let offset = i * n_ch;
                        let channels = data[offset..offset + n_ch].to_vec();
                        if tx
                            .blocking_send(DeviceEvent::Eeg(EegFrame {
                                channels,
                                timestamp_s: ts,
                            }))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            })
            .expect("failed to spawn LSL inlet thread");

        // Wait for the inlet thread to report open_stream result.
        // Timeout matches open_stream's 10 s + 2 s grace.
        match ready_rx.recv_timeout(std::time::Duration::from_secs(12)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(format!("LSL stream '{}' connection timed out", name_for_timeout)),
        }

        Ok(Self {
            rx,
            desc,
            _shutdown: shutdown_tx,
        })
    }

    /// Backwards-compatible constructor that panics on failure.
    /// Prefer [`connect`] for new code.
    pub fn new(info: &StreamInfo) -> Self {
        Self::connect(info).expect("LSL connect failed")
    }
}

// ── Channel label helpers ────────────────────────────────────────────────────

/// Read channel labels from `info.desc()` (works when the StreamInfo was
/// created in this process, e.g. tests sharing the outlet's StreamInfo).
fn read_labels_from_desc(info: &StreamInfo, channel_count: usize) -> Vec<String> {
    let desc = info.desc();
    let channels_node = desc.child("channels");
    if channels_node.is_empty() {
        return Vec::new();
    }
    let mut names = Vec::with_capacity(channel_count);
    let mut ch = channels_node.child("channel");
    while !ch.is_empty() {
        let label = ch.child_value("label");
        names.push(if label.is_empty() {
            format!("Ch{}", names.len() + 1)
        } else {
            label
        });
        ch = ch.next_sibling_named("channel");
    }
    names
}

/// Fetch channel labels by sending `LSL:fullinfo` over TCP to the outlet's
/// service port.  `rlsl` 0.0.4's `get_fullinfo()` is a stub, so we must
/// implement the TCP request ourselves.
fn fetch_labels_via_fullinfo(info: &StreamInfo, channel_count: usize) -> Vec<String> {
    use std::io::{BufRead, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    // Determine the outlet's TCP address.  The `LSL:fullinfo` handler runs
    // on the **data port** (TCP server), not the service port (UDP time sync).
    let addr = {
        let v4 = info.v4address();
        let v4port = info.v4data_port();
        if !v4.is_empty() && v4port > 0 {
            format!("{v4}:{v4port}")
        } else {
            let v6 = info.v6address();
            let v6port = info.v6data_port();
            if !v6.is_empty() && v6port > 0 {
                format!("[{v6}]:{v6port}")
            } else {
                return Vec::new();
            }
        }
    };

    let stream = match TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| "127.0.0.1:0".parse().unwrap()),
        Duration::from_secs(3),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[lsl] fullinfo TCP connect to {addr} failed: {e}");
            return Vec::new();
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok();
    let mut writer = stream.try_clone().unwrap_or_else(|_| {
        // Should not happen on any supported platform.
        panic!("TcpStream::try_clone failed");
    });

    // Send request.
    if writer.write_all(b"LSL:fullinfo\r\n").is_err() {
        return Vec::new();
    }
    let _ = writer.flush();

    // Read response (full-info XML, terminated by `</info>`).
    let reader = std::io::BufReader::new(stream);
    let mut xml = String::with_capacity(4096);
    for line in reader.lines() {
        match line {
            Ok(l) => {
                xml.push_str(&l);
                xml.push('\n');
                if l.contains("</info>") {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    if xml.is_empty() {
        return Vec::new();
    }

    // Parse `<label>` tags from the XML.
    parse_labels_from_xml(&xml, channel_count)
}

/// Extract channel labels from a full-info XML string by simple tag parsing.
fn parse_labels_from_xml(xml: &str, channel_count: usize) -> Vec<String> {
    let mut names = Vec::with_capacity(channel_count);
    // Find all <label>...</label> pairs inside the <channels> section.
    let channels_start = xml.find("<channels>");
    let channels_end = xml.find("</channels>");
    if let (Some(start), Some(end)) = (channels_start, channels_end) {
        let section = &xml[start..end];
        let mut pos = 0;
        while let Some(open) = section[pos..].find("<label>") {
            let label_start = pos + open + 7; // length of "<label>"
            if let Some(close) = section[label_start..].find("</label>") {
                let label = &section[label_start..label_start + close];
                names.push(if label.is_empty() {
                    format!("Ch{}", names.len() + 1)
                } else {
                    label.to_string()
                });
                pos = label_start + close + 8; // length of "</label>"
            } else {
                break;
            }
        }
    }
    names
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
