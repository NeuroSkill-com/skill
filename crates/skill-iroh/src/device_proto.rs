//! Device proxy protocol — streams all sensor data from a remote device
//! over an iroh QUIC channel.
//!
//! The iOS app acts as a thin client: it connects to the BLE device, collects
//! raw data, and forwards everything to the Skill desktop app for processing.
//! The protocol supports multiple message types so **any** device type can be
//! proxied — not just Muse.
//!
//! ## Wire format (per bi-stream: one message per open/close cycle)
//!
//! **Client → Server:**
//!
//! ```text
//! HEADER (24 bytes):
//! [1 byte]   version      = 0x02
//! [1 byte]   msg_type     (see MsgType constants)
//! [8 bytes]  seq          LE u64 — monotonic sequence number
//! [8 bytes]  timestamp    LE i64 — YYYYMMDDHHmmss UTC
//! [1 byte]   flags        bit0 = zstd compressed
//! [4 bytes]  payload_len  LE u32
//! [1 byte]   reserved     = 0x00
//!
//! PAYLOAD (payload_len bytes):
//!   if compressed: zstd frame → decompresses to type-specific raw layout
//!   else: type-specific raw layout (see below)
//! ```
//!
//! **Server → Client (ACK, 10 bytes):**
//!
//! ```text
//! [1 byte]   version = 0x02
//! [8 bytes]  acked_seq LE u64
//! [1 byte]   status: 0x00 = OK, 0x01 = error
//! ```
//!
//! ## Message types
//!
//! | Type | Name | Payload | Notes |
//! |------|------|---------|-------|
//! | 0x01 | SensorChunk | 5s of EEG+PPG+IMU | Compressed, outbox-persisted |
//! | 0x02 | DeviceConnected | JSON descriptor | Once at session start |
//! | 0x03 | DeviceDisconnected | (empty) | Session end |
//! | 0x04 | Battery | 4 bytes f32 LE | Periodic |
//! | 0x05 | Location | 36 bytes GPS | Periodic from iOS CLLocation |
//! | 0x06 | Meta | JSON blob | Device-specific metadata |
//! | 0x07 | PhoneImu | batch of phone sensor samples | Compressed |
//! | 0x08 | PhoneInfo | JSON phone descriptor | Once at tunnel connect |
//!
//! ## SensorChunk payload layout (after decompression)
//!
//! ```text
//! [4 bytes]  sample_rate       f32 LE (Hz)
//! [2 bytes]  eeg_channels      u16 LE (1–1024)
//! [2 bytes]  eeg_samples_per_ch u16 LE
//! [eeg_channels × eeg_samples_per_ch × 4 bytes]  f32 LE (µV)
//!
//! [2 bytes]  ppg_channels      u16 LE (0 or 3)
//! [2 bytes]  ppg_samples_per_ch u16 LE
//! [ppg_channels × ppg_samples_per_ch × 8 bytes]  f64 LE (ADC)
//!
//! [2 bytes]  imu_samples       u16 LE (0 if no IMU)
//! [imu_samples × 24 bytes]     6×f32 LE (accel_xyz + gyro_xyz)
//! ```
//!
//! ## PhoneInfo payload (JSON) — sent once at tunnel connect
//!
//! ```json
//! {
//!   "phone_model": "iPhone 15 Pro",
//!   "phone_name": "Mario's iPhone",
//!   "os": "iOS",
//!   "os_version": "17.4.1",
//!   "app_version": "1.2.0",
//!   "app_build": "42",
//!   "locale": "en_US",
//!   "timezone": "America/New_York",
//!   "carrier": "T-Mobile",
//!   "battery_level": 0.85,
//!   "battery_state": "charging",
//!   "screen_brightness": 0.7,
//!   "low_power_mode": false,
//!   "iroh_endpoint_id": "abc123..."
//! }
//! ```
//!
//! ## DeviceConnected payload (JSON) — sent when BLE device connects
//!
//! ```json
//! {
//!   "kind": "muse",
//!   "name": "Muse-1234",
//!   "id": "AA:BB:CC:DD:EE:FF",
//!   "sample_rate": 256.0,
//!   "eeg_channels": ["TP9","AF7","AF8","TP10"],
//!   "ppg_channels": ["Ambient","Infrared","Red"],
//!   "imu_channels": ["AccelX","AccelY","AccelZ","GyroX","GyroY","GyroZ"],
//!   "caps": ["eeg","ppg","imu","battery"],
//!   "firmware": "1.2.3",
//!   "serial": "...",
//!   "hardware": "..."
//! }
//! ```
//!
//! ## Location payload (36 bytes, fixed)
//!
//! ```text
//! [8 bytes]  latitude   f64 LE (degrees, WGS84)
//! [8 bytes]  longitude  f64 LE
//! [8 bytes]  altitude   f64 LE (meters above sea level)
//! [4 bytes]  accuracy   f32 LE (horizontal, meters)
//! [4 bytes]  speed      f32 LE (m/s, -1 if unavailable)
//! [4 bytes]  heading    f32 LE (degrees from true north, -1 if unavailable)
//! ```

pub const ALPN_DEVICE_PROXY: &[u8] = b"skill/device-proxy/2";
pub const PROTO_VERSION: u8 = 0x02;

// ── Message types ─────────────────────────────────────────────────────────────

pub const MSG_SENSOR_CHUNK: u8 = 0x01;
pub const MSG_DEVICE_CONNECTED: u8 = 0x02;
pub const MSG_DEVICE_DISCONNECTED: u8 = 0x03;
pub const MSG_BATTERY: u8 = 0x04;
pub const MSG_LOCATION: u8 = 0x05;
pub const MSG_META: u8 = 0x06;
pub const MSG_PHONE_IMU: u8 = 0x07;
/// Phone descriptor — model, OS, locale, app version, etc.
/// Sent once at connection start, before any device data.
pub const MSG_PHONE_INFO: u8 = 0x08;

// ── Flags ─────────────────────────────────────────────────────────────────────

pub const FLAG_ZSTD: u8 = 0x01;

// ── ACK ───────────────────────────────────────────────────────────────────────

pub const ACK_OK: u8 = 0x00;
pub const ACK_ERR: u8 = 0x01;

// ── Header ────────────────────────────────────────────────────────────────────

pub const HEADER_SIZE: usize = 24;

/// Encode a message header into a 24-byte buffer.
#[inline]
pub fn encode_header(
    msg_type: u8,
    seq: u64,
    timestamp: i64,
    flags: u8,
    payload_len: u32,
) -> [u8; HEADER_SIZE] {
    let mut h = [0u8; HEADER_SIZE];
    h[0] = PROTO_VERSION;
    h[1] = msg_type;
    h[2..10].copy_from_slice(&seq.to_le_bytes());
    h[10..18].copy_from_slice(&timestamp.to_le_bytes());
    h[18] = flags;
    h[19..23].copy_from_slice(&payload_len.to_le_bytes());
    h[23] = 0; // reserved
    h
}

/// Decoded message header.
#[derive(Debug, Clone)]
pub struct MsgHeader {
    pub msg_type: u8,
    pub seq: u64,
    pub timestamp: i64,
    pub flags: u8,
    pub payload_len: u32,
}

impl MsgHeader {
    pub fn is_compressed(&self) -> bool {
        self.flags & FLAG_ZSTD != 0
    }
}

/// Parse a 24-byte header buffer.  Returns `None` on version mismatch.
pub fn decode_header(h: &[u8; HEADER_SIZE]) -> Option<MsgHeader> {
    if h[0] != PROTO_VERSION {
        return None;
    }
    Some(MsgHeader {
        msg_type: h[1],
        seq: u64::from_le_bytes(h[2..10].try_into().unwrap()),
        timestamp: i64::from_le_bytes(h[10..18].try_into().unwrap()),
        flags: h[18],
        payload_len: u32::from_le_bytes(h[19..23].try_into().unwrap()),
    })
}

/// Encode an ACK response (10 bytes).
pub fn encode_ack(seq: u64, status: u8) -> [u8; 10] {
    let mut buf = [0u8; 10];
    buf[0] = PROTO_VERSION;
    buf[1..9].copy_from_slice(&seq.to_le_bytes());
    buf[9] = status;
    buf
}

/// Decode a 10-byte ACK. Returns `(seq, status)`.
pub fn decode_ack(buf: &[u8; 10]) -> Option<(u64, u8)> {
    if buf[0] != PROTO_VERSION {
        return None;
    }
    let seq = u64::from_le_bytes(buf[1..9].try_into().unwrap());
    Some((seq, buf[9]))
}

// ── SensorChunk encoding helpers ──────────────────────────────────────────────

/// Encode a SensorChunk payload from raw sensor arrays.
///
/// Layout: eeg header + eeg data + ppg header + ppg data + imu header + imu data
pub fn encode_sensor_chunk(
    sample_rate: f32,
    eeg_data: &[Vec<f32>],           // [channels][samples]
    ppg_data: &[Vec<f64>],           // [ppg_channels][samples]
    imu_data: &[(f32, f32, f32, f32, f32, f32)], // [(ax,ay,az,gx,gy,gz)]
) -> Vec<u8> {
    let eeg_ch = eeg_data.len() as u16;
    let eeg_spc = if eeg_ch > 0 { eeg_data[0].len() as u16 } else { 0 };
    let ppg_ch = ppg_data.len() as u16;
    let ppg_spc = if ppg_ch > 0 { ppg_data[0].len() as u16 } else { 0 };
    let imu_n = imu_data.len() as u16;

    let eeg_bytes = eeg_ch as usize * eeg_spc as usize * 4;
    let ppg_bytes = ppg_ch as usize * ppg_spc as usize * 8;
    let imu_bytes = imu_n as usize * 24;
    let total = 4 + 2 + 2 + eeg_bytes + 2 + 2 + ppg_bytes + 2 + imu_bytes;

    let mut buf = Vec::with_capacity(total);

    // EEG header
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&eeg_ch.to_le_bytes());
    buf.extend_from_slice(&eeg_spc.to_le_bytes());
    // EEG data
    for ch in eeg_data {
        for &v in ch {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    // PPG header
    buf.extend_from_slice(&ppg_ch.to_le_bytes());
    buf.extend_from_slice(&ppg_spc.to_le_bytes());
    // PPG data
    for ch in ppg_data {
        for &v in ch {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    // IMU header
    buf.extend_from_slice(&imu_n.to_le_bytes());
    // IMU data: 6 × f32 per sample
    for &(ax, ay, az, gx, gy, gz) in imu_data {
        buf.extend_from_slice(&ax.to_le_bytes());
        buf.extend_from_slice(&ay.to_le_bytes());
        buf.extend_from_slice(&az.to_le_bytes());
        buf.extend_from_slice(&gx.to_le_bytes());
        buf.extend_from_slice(&gy.to_le_bytes());
        buf.extend_from_slice(&gz.to_le_bytes());
    }

    buf
}

/// Decoded sensor chunk.
#[derive(Debug, Clone)]
pub struct SensorChunk {
    pub sample_rate: f32,
    pub eeg_data: Vec<Vec<f32>>,   // [channels][samples]
    pub ppg_data: Vec<Vec<f64>>,   // [ppg_channels][samples]
    pub imu_data: Vec<(f32, f32, f32, f32, f32, f32)>, // [(ax,ay,az,gx,gy,gz)]
}

/// Parse a SensorChunk from raw bytes.
pub fn decode_sensor_chunk(raw: &[u8]) -> Result<SensorChunk, String> {
    let mut off = 0usize;

    if raw.len() < 8 {
        return Err("sensor chunk too short".into());
    }

    let sample_rate = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap());
    off += 4;
    let eeg_ch = u16::from_le_bytes(raw[off..off + 2].try_into().unwrap()) as usize;
    off += 2;
    let eeg_spc = u16::from_le_bytes(raw[off..off + 2].try_into().unwrap()) as usize;
    off += 2;

    let eeg_bytes = eeg_ch * eeg_spc * 4;
    if off + eeg_bytes > raw.len() {
        return Err(format!("eeg data truncated: need {} have {}", off + eeg_bytes, raw.len()));
    }
    let mut eeg_data = Vec::with_capacity(eeg_ch);
    for _ in 0..eeg_ch {
        let mut ch = Vec::with_capacity(eeg_spc);
        for _ in 0..eeg_spc {
            ch.push(f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()));
            off += 4;
        }
        eeg_data.push(ch);
    }

    if off + 4 > raw.len() {
        return Err("ppg header truncated".into());
    }
    let ppg_ch = u16::from_le_bytes(raw[off..off + 2].try_into().unwrap()) as usize;
    off += 2;
    let ppg_spc = u16::from_le_bytes(raw[off..off + 2].try_into().unwrap()) as usize;
    off += 2;

    let ppg_bytes = ppg_ch * ppg_spc * 8;
    if off + ppg_bytes > raw.len() {
        return Err(format!("ppg data truncated: need {} have {}", off + ppg_bytes, raw.len()));
    }
    let mut ppg_data = Vec::with_capacity(ppg_ch);
    for _ in 0..ppg_ch {
        let mut ch = Vec::with_capacity(ppg_spc);
        for _ in 0..ppg_spc {
            ch.push(f64::from_le_bytes(raw[off..off + 8].try_into().unwrap()));
            off += 8;
        }
        ppg_data.push(ch);
    }

    if off + 2 > raw.len() {
        return Err("imu header truncated".into());
    }
    let imu_n = u16::from_le_bytes(raw[off..off + 2].try_into().unwrap()) as usize;
    off += 2;
    let imu_bytes = imu_n * 24;
    if off + imu_bytes > raw.len() {
        return Err(format!("imu data truncated: need {} have {}", off + imu_bytes, raw.len()));
    }
    let mut imu_data = Vec::with_capacity(imu_n);
    for _ in 0..imu_n {
        let ax = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()); off += 4;
        let ay = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()); off += 4;
        let az = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()); off += 4;
        let gx = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()); off += 4;
        let gy = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()); off += 4;
        let gz = f32::from_le_bytes(raw[off..off + 4].try_into().unwrap()); off += 4;
        imu_data.push((ax, ay, az, gx, gy, gz));
    }

    Ok(SensorChunk { sample_rate, eeg_data, ppg_data, imu_data })
}

// ── Location encoding ─────────────────────────────────────────────────────────

pub const LOCATION_PAYLOAD_SIZE: usize = 36;

#[derive(Debug, Clone, Copy)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub accuracy: f32,
    pub speed: f32,
    pub heading: f32,
}

pub fn encode_location(loc: &Location) -> [u8; LOCATION_PAYLOAD_SIZE] {
    let mut buf = [0u8; LOCATION_PAYLOAD_SIZE];
    buf[0..8].copy_from_slice(&loc.latitude.to_le_bytes());
    buf[8..16].copy_from_slice(&loc.longitude.to_le_bytes());
    buf[16..24].copy_from_slice(&loc.altitude.to_le_bytes());
    buf[24..28].copy_from_slice(&loc.accuracy.to_le_bytes());
    buf[28..32].copy_from_slice(&loc.speed.to_le_bytes());
    buf[32..36].copy_from_slice(&loc.heading.to_le_bytes());
    buf
}

pub fn decode_location(buf: &[u8]) -> Result<Location, String> {
    if buf.len() < LOCATION_PAYLOAD_SIZE {
        return Err("location payload too short".into());
    }
    Ok(Location {
        latitude: f64::from_le_bytes(buf[0..8].try_into().unwrap()),
        longitude: f64::from_le_bytes(buf[8..16].try_into().unwrap()),
        altitude: f64::from_le_bytes(buf[16..24].try_into().unwrap()),
        accuracy: f32::from_le_bytes(buf[24..28].try_into().unwrap()),
        speed: f32::from_le_bytes(buf[28..32].try_into().unwrap()),
        heading: f32::from_le_bytes(buf[32..36].try_into().unwrap()),
    })
}

// ── Battery encoding ──────────────────────────────────────────────────────────

pub fn encode_battery(level_pct: f32) -> [u8; 4] {
    level_pct.to_le_bytes()
}

pub fn decode_battery(buf: &[u8]) -> Result<f32, String> {
    if buf.len() < 4 {
        return Err("battery payload too short".into());
    }
    Ok(f32::from_le_bytes(buf[0..4].try_into().unwrap()))
}

// ── Phone IMU encoding ────────────────────────────────────────────────────────

/// Bytes per phone sensor sample:
/// dt(4) + raw_accel(12) + user_accel(12) + gravity(12) + gyro(12) + mag(12)
/// + attitude(12) + pressure(4) + rel_altitude(4) + ambient_light(4) + proximity(1) + pad(3) = 92
pub const PHONE_SENSOR_SAMPLE_SIZE: usize = 92;

// Keep old name as alias for backward compat in test code
pub const PHONE_IMU_SAMPLE_SIZE: usize = PHONE_SENSOR_SAMPLE_SIZE;

/// One phone sensor sample — full CoreMotion + environmental sensor output.
///
/// Captures **all** phone sensor data so the desktop can reconstruct the
/// user's body motion and environment independently of the head-worn device.
///
/// | Field | Source | Unit |
/// |-------|--------|------|
/// | `raw_accel` | `CMAccelerometerData.acceleration` | g |
/// | `user_accel` | `CMDeviceMotion.userAcceleration` | g (gravity removed) |
/// | `gravity` | `CMDeviceMotion.gravity` | g |
/// | `gyro` | `CMDeviceMotion.rotationRate` | rad/s |
/// | `mag` | `CMDeviceMotion.magneticField.field` | µT |
/// | `attitude` | `CMDeviceMotion.attitude` (roll, pitch, yaw) | rad |
/// | `pressure` | `CMAltimeter.pressure` | kPa (0 if unavailable) |
/// | `rel_altitude` | `CMAltimeter.relativeAltitude` | meters (0 if unavailable) |
/// | `ambient_light` | `UIScreen.main.brightness` (proxy) | 0.0–1.0 |
/// | `proximity` | `UIDevice.proximityState` | true/false |
#[derive(Debug, Clone, Copy)]
pub struct PhoneImuSample {
    /// Relative timestamp within the batch (seconds since batch start).
    pub dt: f32,
    /// Raw accelerometer (includes gravity).
    pub raw_accel: [f32; 3],
    /// User acceleration (gravity subtracted by sensor fusion).
    pub user_accel: [f32; 3],
    /// Gravity vector from sensor fusion.
    pub gravity: [f32; 3],
    /// Rotation rate (gyroscope).
    pub gyro: [f32; 3],
    /// Calibrated magnetometer (µT).  `[0,0,0]` if unavailable.
    pub mag: [f32; 3],
    /// Attitude: `[roll, pitch, yaw]` in radians.
    pub attitude: [f32; 3],
    /// Barometric pressure (kPa).  0 if unavailable.
    pub pressure: f32,
    /// Relative altitude change since recording start (meters).  0 if unavailable.
    pub rel_altitude: f32,
    /// Ambient light level (0.0–1.0, from screen brightness sensor).  -1 if unavailable.
    pub ambient_light: f32,
    /// Proximity sensor: 1.0 = near, 0.0 = far, -1.0 = unavailable.
    pub proximity: f32,
}

/// Encode a batch of phone sensor samples.
pub fn encode_phone_imu(samples: &[PhoneImuSample]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(2 + samples.len() * PHONE_SENSOR_SAMPLE_SIZE);
    buf.extend_from_slice(&(samples.len() as u16).to_le_bytes());
    for s in samples {
        buf.extend_from_slice(&s.dt.to_le_bytes());
        for &v in &s.raw_accel { buf.extend_from_slice(&v.to_le_bytes()); }
        for &v in &s.user_accel { buf.extend_from_slice(&v.to_le_bytes()); }
        for &v in &s.gravity { buf.extend_from_slice(&v.to_le_bytes()); }
        for &v in &s.gyro { buf.extend_from_slice(&v.to_le_bytes()); }
        for &v in &s.mag { buf.extend_from_slice(&v.to_le_bytes()); }
        for &v in &s.attitude { buf.extend_from_slice(&v.to_le_bytes()); }
        buf.extend_from_slice(&s.pressure.to_le_bytes());
        buf.extend_from_slice(&s.rel_altitude.to_le_bytes());
        buf.extend_from_slice(&s.ambient_light.to_le_bytes());
        buf.extend_from_slice(&s.proximity.to_le_bytes());
    }
    buf
}

/// Decode a batch of phone sensor samples.
pub fn decode_phone_imu(raw: &[u8]) -> Result<Vec<PhoneImuSample>, String> {
    if raw.len() < 2 { return Err("phone sensor too short".into()); }
    let count = u16::from_le_bytes(raw[0..2].try_into().unwrap()) as usize;
    let expected = 2 + count * PHONE_SENSOR_SAMPLE_SIZE;
    if raw.len() < expected {
        return Err(format!("phone sensor truncated: need {expected}, got {}", raw.len()));
    }
    let mut out = Vec::with_capacity(count);
    let mut off = 2;
    for _ in 0..count {
        let f = |o: &mut usize| -> f32 {
            let v = f32::from_le_bytes(raw[*o..*o + 4].try_into().unwrap());
            *o += 4; v
        };
        let dt = f(&mut off);
        let raw_accel = [f(&mut off), f(&mut off), f(&mut off)];
        let user_accel = [f(&mut off), f(&mut off), f(&mut off)];
        let gravity = [f(&mut off), f(&mut off), f(&mut off)];
        let gyro = [f(&mut off), f(&mut off), f(&mut off)];
        let mag = [f(&mut off), f(&mut off), f(&mut off)];
        let attitude = [f(&mut off), f(&mut off), f(&mut off)];
        let pressure = f(&mut off);
        let rel_altitude = f(&mut off);
        let ambient_light = f(&mut off);
        let proximity = f(&mut off);
        out.push(PhoneImuSample { dt, raw_accel, user_accel, gravity, gyro, mag, attitude,
                                   pressure, rel_altitude, ambient_light, proximity });
    }
    Ok(out)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let hdr = encode_header(MSG_SENSOR_CHUNK, 42, 20260315120000, FLAG_ZSTD, 1234);
        let dec = decode_header(&hdr).unwrap();
        assert_eq!(dec.msg_type, MSG_SENSOR_CHUNK);
        assert_eq!(dec.seq, 42);
        assert_eq!(dec.timestamp, 20260315120000);
        assert!(dec.is_compressed());
        assert_eq!(dec.payload_len, 1234);
    }

    #[test]
    fn ack_roundtrip() {
        let ack = encode_ack(99, ACK_OK);
        let (seq, status) = decode_ack(&ack).unwrap();
        assert_eq!(seq, 99);
        assert_eq!(status, ACK_OK);
    }

    #[test]
    fn sensor_chunk_roundtrip() {
        let eeg = vec![vec![1.0f32, 2.0, 3.0], vec![4.0, 5.0, 6.0]];
        let ppg = vec![vec![100.0f64, 200.0]];
        let imu = vec![(0.1, 0.2, 9.8, 0.01, 0.02, 0.03)];

        let raw = encode_sensor_chunk(256.0, &eeg, &ppg, &imu);
        let dec = decode_sensor_chunk(&raw).unwrap();

        assert_eq!(dec.sample_rate, 256.0);
        assert_eq!(dec.eeg_data.len(), 2);
        assert_eq!(dec.eeg_data[0], vec![1.0, 2.0, 3.0]);
        assert_eq!(dec.ppg_data.len(), 1);
        assert_eq!(dec.ppg_data[0], vec![100.0, 200.0]);
        assert_eq!(dec.imu_data.len(), 1);
        assert!((dec.imu_data[0].0 - 0.1).abs() < 1e-6);
    }

    #[test]
    fn sensor_chunk_no_ppg_no_imu() {
        let eeg = vec![vec![1.0f32; 1280]; 4];
        let raw = encode_sensor_chunk(256.0, &eeg, &[], &[]);
        let dec = decode_sensor_chunk(&raw).unwrap();
        assert_eq!(dec.eeg_data.len(), 4);
        assert_eq!(dec.eeg_data[0].len(), 1280);
        assert!(dec.ppg_data.is_empty());
        assert!(dec.imu_data.is_empty());
    }

    #[test]
    fn phone_sensor_roundtrip() {
        let samples = vec![
            PhoneImuSample {
                dt: 0.01, raw_accel: [0.1, 0.2, -0.7], user_accel: [0.1, 0.2, 0.3],
                gyro: [0.01, 0.02, 0.03], gravity: [0.0, 0.0, -1.0],
                mag: [25.0, -10.0, 42.0], attitude: [0.1, 0.2, 0.3],
                pressure: 101.3, rel_altitude: 2.5, ambient_light: 0.7, proximity: 0.0,
            },
            PhoneImuSample {
                dt: 0.02, raw_accel: [0.4, 0.5, -0.4], user_accel: [0.4, 0.5, 0.6],
                gyro: [0.04, 0.05, 0.06], gravity: [0.0, 0.0, -1.0],
                mag: [25.1, -10.1, 42.1], attitude: [0.4, 0.5, 0.6],
                pressure: 101.2, rel_altitude: 2.7, ambient_light: 0.3, proximity: 1.0,
            },
        ];
        let raw = encode_phone_imu(&samples);
        let dec = decode_phone_imu(&raw).unwrap();
        assert_eq!(dec.len(), 2);
        assert!((dec[0].dt - 0.01).abs() < 1e-6);
        assert!((dec[0].raw_accel[0] - 0.1).abs() < 1e-6);
        assert!((dec[1].user_accel[0] - 0.4).abs() < 1e-6);
        assert!((dec[0].gravity[2] - (-1.0)).abs() < 1e-6);
        assert!((dec[0].mag[0] - 25.0).abs() < 1e-4);
        assert!((dec[0].pressure - 101.3).abs() < 1e-4);
        assert!((dec[0].rel_altitude - 2.5).abs() < 1e-4);
        assert!((dec[0].ambient_light - 0.7).abs() < 1e-4);
        assert!((dec[1].proximity - 1.0).abs() < 1e-4);
    }

    #[test]
    fn location_roundtrip() {
        let loc = Location {
            latitude: 37.7749,
            longitude: -122.4194,
            altitude: 10.5,
            accuracy: 5.0,
            speed: 1.5,
            heading: 90.0,
        };
        let raw = encode_location(&loc);
        let dec = decode_location(&raw).unwrap();
        assert!((dec.latitude - 37.7749).abs() < 1e-10);
        assert!((dec.longitude - (-122.4194)).abs() < 1e-10);
        assert!((dec.speed - 1.5).abs() < 1e-6);
    }
}
