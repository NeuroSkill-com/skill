//! Server-side device proxy receiver — accepts `skill/device-proxy/2` connections.
//!
//! Decodes all message types and forwards them as [`RemoteDeviceEvent`]s on a
//! tokio channel.  The session runner consumes these and translates them into
//! standard [`DeviceEvent`]s for the DSP / CSV / embedding pipeline.

use anyhow::Context as _;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Mutex, OnceLock};
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

/// Small per-peer pre-session cache for messages that may arrive *before*
/// the daemon installs an active session tx (common when clients send
/// phone-info/device-connected immediately after tunnel connect).
const PRESESSION_MAX_PER_PEER: usize = 64;

type PresessionQueue = VecDeque<(u64, RemoteDeviceEvent)>;
type PresessionCache = HashMap<String, PresessionQueue>;

fn presession_cache() -> &'static Mutex<PresessionCache> {
    static CACHE: OnceLock<Mutex<PresessionCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Default)]
pub struct PeerActivityView {
    pub peer_id: String,
    pub tunnel_connected: bool,
    pub remote_device_connected: bool,
    pub streaming_active: bool,
    pub eeg_streaming_active: bool,
    pub last_seen_unix: u64,
}

#[derive(Debug, Clone, Default)]
struct PeerActivityState {
    tunnel_connected: bool,
    remote_device_connected: bool,
    last_seen_unix: u64,
    last_sensor_chunk_unix: u64,
    last_eeg_chunk_unix: u64,
}

fn active_peers() -> &'static Mutex<HashSet<String>> {
    static ACTIVE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn peer_activity() -> &'static Mutex<HashMap<String, PeerActivityState>> {
    static ACTIVITY: OnceLock<Mutex<HashMap<String, PeerActivityState>>> = OnceLock::new();
    ACTIVITY.get_or_init(|| Mutex::new(HashMap::new()))
}

const STREAM_ACTIVE_WINDOW_SECS: u64 = 3;

fn mark_peer_connected(peer_id: &str) {
    let now = crate::unix_secs();
    if let Ok(mut g) = peer_activity().lock() {
        let st = g.entry(peer_id.to_string()).or_default();
        st.tunnel_connected = true;
        st.last_seen_unix = now;
    }
}

fn mark_peer_disconnected(peer_id: &str) {
    if let Ok(mut g) = peer_activity().lock() {
        if let Some(st) = g.get_mut(peer_id) {
            st.tunnel_connected = false;
        }
    }
}

fn note_peer_event(peer_id: &str, event: &RemoteDeviceEvent) {
    let now = crate::unix_secs();
    if let Ok(mut g) = peer_activity().lock() {
        let st = g.entry(peer_id.to_string()).or_default();
        st.last_seen_unix = now;
        match event {
            RemoteDeviceEvent::DeviceConnected { .. } => st.remote_device_connected = true,
            RemoteDeviceEvent::DeviceDisconnected { .. } => st.remote_device_connected = false,
            RemoteDeviceEvent::SensorChunk { chunk, .. } => {
                st.last_sensor_chunk_unix = now;
                let has_eeg = chunk.eeg_data.iter().any(|ch| !ch.is_empty());
                if has_eeg {
                    st.last_eeg_chunk_unix = now;
                }
            }
            _ => {}
        }
    }
}

fn cache_presession_event(peer_id: &str, event: RemoteDeviceEvent) {
    if let Ok(mut g) = presession_cache().lock() {
        let q = g.entry(peer_id.to_string()).or_default();
        if q.len() >= PRESESSION_MAX_PER_PEER {
            q.pop_front();
        }
        q.push_back((crate::unix_secs(), event));
    }
}

const PRESESSION_MAX_AGE_SECS: u64 = 20;

fn flush_presession_events(peer_id: &str, tx: &RemoteEventTx) {
    if let Ok(mut g) = presession_cache().lock() {
        if let Some(mut q) = g.remove(peer_id) {
            let now = crate::unix_secs();
            while let Some((ts, ev)) = q.pop_front() {
                // Avoid replaying stale buffered events from long-dead sessions.
                if now.saturating_sub(ts) > PRESESSION_MAX_AGE_SECS {
                    continue;
                }
                if tx.try_send(ev).is_err() {
                    break;
                }
            }
        }
    }
}

/// Snapshot peer IDs that currently have *active* device-proxy connections.
pub fn connected_peer_ids() -> Vec<String> {
    active_peers()
        .lock()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default()
}

/// Snapshot per-peer iroh device-proxy activity for session routing + UI.
pub fn peer_activity_snapshot() -> Vec<PeerActivityView> {
    let now = crate::unix_secs();
    peer_activity()
        .lock()
        .map(|g| {
            g.iter()
                .map(|(peer, st)| PeerActivityView {
                    peer_id: peer.clone(),
                    tunnel_connected: st.tunnel_connected,
                    remote_device_connected: st.remote_device_connected,
                    streaming_active: st.last_sensor_chunk_unix > 0
                        && now.saturating_sub(st.last_sensor_chunk_unix) <= STREAM_ACTIVE_WINDOW_SECS,
                    eeg_streaming_active: st.last_eeg_chunk_unix > 0
                        && now.saturating_sub(st.last_eeg_chunk_unix) <= STREAM_ACTIVE_WINDOW_SECS,
                    last_seen_unix: st.last_seen_unix,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Snapshot peer IDs that currently have *recent* pre-session cached events.
pub fn cached_peer_ids_recent(max_age_secs: u64) -> Vec<String> {
    let now = crate::unix_secs();
    presession_cache()
        .lock()
        .map(|g| {
            g.iter()
                .filter_map(|(peer, q)| {
                    let fresh = q
                        .back()
                        .map(|(ts, _)| now.saturating_sub(*ts) <= max_age_secs)
                        .unwrap_or(false);
                    if fresh {
                        Some(peer.clone())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Flush cached pre-session events for one peer into an active tx.
/// Safe to call repeatedly.
pub fn flush_presession_for_peer(peer_id: &str, tx: &RemoteEventTx) {
    flush_presession_events(peer_id, tx);
}

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
    if let Ok(mut g) = active_peers().lock() {
        g.insert(peer_id.clone());
    }
    mark_peer_connected(&peer_id);

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

        match handle_one_message(send, recv, maybe_tx.as_ref(), &peer_id).await {
            Ok(_seq) => {
                // Logged at trace level — sensor chunks arrive every 5s
            }
            Err(e) => {
                eprintln!("[iroh-device] peer {peer_id} message error: {e}");
            }
        }
    }

    // QUIC connections are transient — the phone may reconnect and a new
    // handle_device_proxy_connection task will be spawned for it.  We do NOT
    // send a synthetic DeviceDisconnected here because doing so would
    // immediately terminate the active session.  Instead, the session ends via:
    //   • an explicit MSG_DEVICE_DISCONNECTED from iOS, or
    //   • the IrohRemoteAdapter watchdog timeout (no data for 60 s).
    if let Ok(mut g) = active_peers().lock() {
        g.remove(&peer_id);
    }
    mark_peer_disconnected(&peer_id);
    eprintln!("[iroh-device] peer {peer_id} QUIC connection closed (transient — not ending session)");
}

async fn handle_one_message(
    mut send: iroh::endpoint::SendStream,
    mut recv: iroh::endpoint::RecvStream,
    tx: Option<&RemoteEventTx>,
    peer_id: &str,
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
    note_peer_event(peer_id, &event);

    if let Some(tx) = tx {
        // If this is the first event after session start, replay any queued
        // pre-session messages (device descriptor / phone info / initial data).
        flush_presession_events(peer_id, tx);

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
        // Cache early events until a session is started, instead of dropping.
        cache_presession_event(peer_id, event);
        eprintln!(
            "[iroh-device] no active session, seq={} cached (peer={})",
            hdr.seq, peer_id
        );
    }

    Ok(hdr.seq)
}
