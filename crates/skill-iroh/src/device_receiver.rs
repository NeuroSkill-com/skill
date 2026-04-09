//! Server-side device proxy receiver — accepts `skill/device-proxy/2` connections.
//!
//! Decodes all message types and forwards them as [`RemoteDeviceEvent`]s on a
//! tokio channel.  The session runner consumes these and translates them into
//! standard [`DeviceEvent`]s for the DSP / CSV / embedding pipeline.

use anyhow::Context as _;
use tokio::sync::mpsc;

use crate::device_proto::{self, *};

// ── Decoded event types ───────────────────────────────────────────────────────

/// A decoded device proxy event from a remote iOS client.
#[derive(Debug, Clone)]
pub enum RemoteDeviceEvent {
    /// 5-second sensor data chunk (EEG + PPG + IMU).
    SensorChunk {
        seq: u64,
        timestamp: i64,
        chunk: SensorChunk,
    },
    /// Device connected — JSON descriptor.
    DeviceConnected {
        seq: u64,
        timestamp: i64,
        descriptor_json: String,
    },
    /// Device disconnected.
    DeviceDisconnected { seq: u64, timestamp: i64 },
    /// Battery level update.
    Battery { seq: u64, timestamp: i64, level_pct: f32 },
    /// GPS location.
    Location {
        seq: u64,
        timestamp: i64,
        location: Location,
    },
    /// Opaque device metadata (JSON).
    Meta { seq: u64, timestamp: i64, json: String },
    /// Phone sensor data (accelerometer, gyroscope, magnetometer, barometer, light, proximity).
    /// Separate from the head-worn device's IMU — both are recorded in parallel.
    PhoneImu {
        seq: u64,
        timestamp: i64,
        samples: Vec<PhoneImuSample>,
    },
    /// Phone descriptor — model, OS, locale, app version, battery, etc.
    /// Sent once when the iroh tunnel connects, before any device data.
    /// Identifies which phone is streaming among multiple connected clients.
    PhoneInfo {
        seq: u64,
        timestamp: i64,
        info_json: String,
    },
}

/// Maximum payload we'll accept (4 MB).
const MAX_PAYLOAD: u32 = 4 * 1024 * 1024;
/// Maximum decompressed size (8 MB).
const MAX_DECOMPRESSED: usize = 8 * 1024 * 1024;

/// Channel capacity for the device proxy event channel.
/// Must be large enough to absorb bursts while the session runner processes
/// chunks.  At ~8 msgs/s (5s EEG chunks + phone IMU + PPG + battery +
/// location), 256 gives ~30s of buffer.
const CHANNEL_CAPACITY: usize = 256;

pub type RemoteEventTx = mpsc::Sender<RemoteDeviceEvent>;
pub type RemoteEventRx = mpsc::Receiver<RemoteDeviceEvent>;

/// Create a new event channel pair.
pub fn event_channel() -> (RemoteEventTx, RemoteEventRx) {
    mpsc::channel(CHANNEL_CAPACITY)
}

/// Handle one incoming `skill/device-proxy/2` connection.
///
/// Accepts `SharedDeviceEventTx` so it re-reads the current sender on every
/// incoming message.  This means a session can replace the tx (by calling
/// `connect_iroh_remote` which stores a fresh tx in the shared slot) and the
/// very next message from the phone is delivered to the new session's rx
/// without any tunnel restart.
pub async fn handle_device_proxy_connection(
    conn: iroh::endpoint::Connection,
    device_tx: std::sync::Arc<std::sync::Mutex<Option<RemoteEventTx>>>,
    peer_id: String,
) {
    eprintln!("[iroh-device] peer {peer_id} connected on device-proxy channel");

    loop {
        let (send, recv) = match conn.accept_bi().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("[iroh-device] peer {peer_id} accept_bi failed: {e}");
                break;
            }
        };

        // Re-read the current tx on every message — the session runner replaces
        // it with a fresh channel when connect_iroh_remote() is called.
        let maybe_tx = device_tx.lock().ok().and_then(|g| g.clone());

        match handle_one_message(send, recv, maybe_tx.as_ref()).await {
            Ok(_seq) => {
                // Logged at trace level — sensor chunks arrive every 5s
            }
            Err(e) => {
                eprintln!("[iroh-device] peer {peer_id} message error: {e}");
            }
        }
    }

    eprintln!("[iroh-device] peer {peer_id} disconnected — sending synthetic DeviceDisconnected");

    // The phone may not have had a chance to send MSG_DEVICE_DISCONNECTED
    // (e.g. app killed, phone out of range, iroh relay down).  Send a
    // synthetic disconnect so the session runner ends the recording
    // promptly instead of waiting for the data watchdog timeout.
    let maybe_tx = device_tx.lock().ok().and_then(|g| g.clone());
    if let Some(tx) = maybe_tx {
        let _ = tx.try_send(RemoteDeviceEvent::DeviceDisconnected { seq: 0, timestamp: 0 });
    }
}

async fn handle_one_message(
    mut send: iroh::endpoint::SendStream,
    mut recv: iroh::endpoint::RecvStream,
    tx: Option<&RemoteEventTx>,
) -> anyhow::Result<u64> {
    // 1. Read header
    let mut hdr_buf = [0u8; HEADER_SIZE];
    recv.read_exact(&mut hdr_buf).await.context("read header")?;

    let hdr = decode_header(&hdr_buf).ok_or_else(|| anyhow::anyhow!("invalid header version"))?;

    if hdr.payload_len > MAX_PAYLOAD {
        let ack = encode_ack(hdr.seq, ACK_ERR);
        let _ = send.write_all(&ack).await;
        return Err(anyhow::anyhow!("payload too large: {}", hdr.payload_len));
    }

    // 2. Read payload
    let mut payload = vec![0u8; hdr.payload_len as usize];
    if !payload.is_empty() {
        recv.read_exact(&mut payload).await.context("read payload")?;
    }

    // 3. Decompress if needed
    let raw = if hdr.is_compressed() {
        let decompressed = zstd::decode_all(std::io::Cursor::new(&payload)).context("zstd")?;
        if decompressed.len() > MAX_DECOMPRESSED {
            let ack = encode_ack(hdr.seq, ACK_ERR);
            let _ = send.write_all(&ack).await;
            return Err(anyhow::anyhow!("decompressed too large: {}", decompressed.len()));
        }
        decompressed
    } else {
        payload
    };

    // 4. Parse by message type
    let event = match hdr.msg_type {
        MSG_SENSOR_CHUNK => {
            let chunk = decode_sensor_chunk(&raw)?;
            RemoteDeviceEvent::SensorChunk {
                seq: hdr.seq,
                timestamp: hdr.timestamp,
                chunk,
            }
        }
        MSG_DEVICE_CONNECTED => {
            let json = String::from_utf8(raw).context("utf8")?;
            RemoteDeviceEvent::DeviceConnected {
                seq: hdr.seq,
                timestamp: hdr.timestamp,
                descriptor_json: json,
            }
        }
        MSG_DEVICE_DISCONNECTED => RemoteDeviceEvent::DeviceDisconnected {
            seq: hdr.seq,
            timestamp: hdr.timestamp,
        },
        MSG_BATTERY => {
            let level = decode_battery(&raw)?;
            RemoteDeviceEvent::Battery {
                seq: hdr.seq,
                timestamp: hdr.timestamp,
                level_pct: level,
            }
        }
        MSG_LOCATION => {
            let loc = decode_location(&raw)?;
            RemoteDeviceEvent::Location {
                seq: hdr.seq,
                timestamp: hdr.timestamp,
                location: loc,
            }
        }
        MSG_META => {
            let json = String::from_utf8(raw).context("utf8")?;
            RemoteDeviceEvent::Meta {
                seq: hdr.seq,
                timestamp: hdr.timestamp,
                json,
            }
        }
        MSG_PHONE_IMU => {
            let samples = device_proto::decode_phone_imu(&raw)?;
            RemoteDeviceEvent::PhoneImu {
                seq: hdr.seq,
                timestamp: hdr.timestamp,
                samples,
            }
        }
        MSG_PHONE_INFO => {
            let json = String::from_utf8(raw).context("utf8")?;
            RemoteDeviceEvent::PhoneInfo {
                seq: hdr.seq,
                timestamp: hdr.timestamp,
                info_json: json,
            }
        }
        other => {
            let ack = encode_ack(hdr.seq, ACK_ERR);
            let _ = send.write_all(&ack).await;
            anyhow::bail!("unknown msg_type: 0x{other:02x}");
        }
    };

    // 5. ACK
    let ack = encode_ack(hdr.seq, ACK_OK);
    send.write_all(&ack).await.context("write ack")?;

    // 6. Forward (non-blocking: prefer dropping a message over stalling
    //    the QUIC stream, which would block the phone's ACK and outbox).
    if let Some(tx) = tx {
        match tx.try_send(event) {
            Ok(_) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                eprintln!(
                    "[iroh-device] event channel full, seq={} dropped (capacity={})",
                    hdr.seq, CHANNEL_CAPACITY
                );
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                eprintln!("[iroh-device] event channel closed, seq={} dropped", hdr.seq);
            }
        }
    } else {
        eprintln!(
            "[iroh-device] no active session, seq={} dropped — start a session first",
            hdr.seq
        );
    }

    Ok(hdr.seq)
}
