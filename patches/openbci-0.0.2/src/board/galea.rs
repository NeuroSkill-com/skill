//! Galea research headset — 24-channel EEG+EMG over UDP.
//!
//! [Galea](https://galea.co/) is OpenBCI's high-channel-count research headset.
//! Data arrives over UDP (port 2390) in fixed-size packets.
//!
//! ## Channel assignment
//!
//! | Channels | Signal | Default gain |
//! |---|---|---|
//! | 0–7   | Upper-face EMG (frontalis, corrugator, …) | 4× |
//! | 8–17  | EEG (10-20 montage) | 12× |
//! | 18–21 | Auxiliary EMG | 4× |
//! | 22–23 | Reserved | 12× |
//!
//! ## EXG packet layout (96 bytes)
//!
//! ```text
//! Byte   0     : packet number (rolling 0–255)
//! Bytes  1–4   : EDA (skin conductance), float32 big-endian, volts
//! Bytes  5–76  : 24 EEG/EMG channels × 3 bytes each (24-bit signed big-endian)
//! Byte   77    : battery level (%)
//! Bytes  78–79 : temperature × 100, uint16 big-endian (divide by 100 → °C)
//! Bytes  80–83 : PPG red, int32 big-endian
//! Bytes  84–87 : PPG infrared, int32 big-endian
//! Bytes  88–95 : device timestamp, uint64 big-endian (µs since boot)
//! ```
//!
//! IMU data optionally follows from byte 96 onward (same UDP datagram).
//! See [`GaleaSample`] for the full decoded structure.

use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{Board, ConfigurableBoard};
use crate::channel_config::{ChannelConfig, GainTracker, CHANNEL_LETTERS};
use crate::electrode::ElectrodeLayout;
use crate::error::{OpenBciError, Result};
use crate::packet::{cast_24bit_to_i32, cast_16bit_to_i32};
use crate::sample::{Sample, StreamHandle, now};

// ─────────────────────────────────────────────────────────────────────────────

/// UDP port on which the Galea streams EXG data.
const GALEA_PORT: u16        = 2390;
/// Fixed size of one EXG data packet in bytes (see module-level layout table).
const EXG_PACKET_SIZE: usize = 96;
/// Maximum UDP datagram size to receive; the Galea may batch several EXG
/// packets into one datagram.
const MAX_UDP_RECV: usize    = 4096;

/// Default gains for Galea channels:
/// - Channels 0-7   (EMG):     4×
/// - Channels 8-17  (EEG):    12×
/// - Channels 18-21 (AUX EMG): 4×
/// - Channels 22-23 (reserved): 12×
fn default_galea_gains() -> Vec<f64> {
    let mut g = vec![0.0f64; 24];
    for i in 0..8  { g[i] = 4.0; }
    for i in 8..18 { g[i] = 12.0; }
    for i in 18..22 { g[i] = 4.0; }
    for i in 22..24 { g[i] = 12.0; }
    g
}

// ─────────────────────────────────────────────────────────────────────────────

/// An extended Galea sample including IMU and biosensor data.
#[derive(Debug, Clone)]
pub struct GaleaSample {
    /// Base EEG/EMG sample (channels 0-7 = EMG, 8-17 = EEG, 18-21 = AUX EMG).
    pub base: Sample,
    /// EDA (skin conductance) in volts.
    pub eda_volts: f32,
    /// Battery level 0-100%.
    pub battery: u8,
    /// Temperature in °C.
    pub temperature_c: f32,
    /// PPG red channel count.
    pub ppg_red: i32,
    /// PPG infrared channel count.
    pub ppg_ir: i32,
    /// Device timestamp in seconds (from the board's internal clock).
    pub device_timestamp: f64,
    /// Accelerometer [X, Y, Z] in g (present every 5th packet).
    pub accel: Option<[f64; 3]>,
    /// Gyroscope [X, Y, Z] in °/s.
    pub gyro: Option<[f64; 3]>,
    /// Magnetometer [X, Y, Z] in µT.
    pub magnetometer: Option<[f64; 3]>,
}

// ─────────────────────────────────────────────────────────────────────────────

/// Galea 24-channel EEG+EMG board.
///
/// # Channel layout
/// - Channels 0-7   → 8 EMG channels (gain 4×)
/// - Channels 8-17  → 10 EEG channels (gain 12×)
/// - Channels 18-21 → 4 auxiliary EMG channels (gain 4×)
/// - Channels 22-23 → 2 reserved channels
pub struct GaleaBoard {
    ip_address:       String,
    electrode_layout: ElectrodeLayout,
    gains:            GainTracker,
    socket:           Option<UdpSocket>,
    streaming:        bool,
    keep_alive:       Arc<AtomicBool>,
    /// Estimated half round-trip time for timestamp correction (seconds).
    half_rtt:         f64,
}

impl GaleaBoard {
    /// Create a new Galea board driver.
    ///
    /// `ip_address` — the board's IP (empty = auto-discover via SSDP).
    pub fn new(ip_address: impl Into<String>) -> Self {
        Self {
            ip_address:       ip_address.into(),
            electrode_layout: Self::default_layout(),
            gains:            GainTracker::new(default_galea_gains()),
            socket:           None,
            streaming:        false,
            keep_alive:       Arc::new(AtomicBool::new(false)),
            half_rtt:         0.0,
        }
    }

    /// Builder: attach a custom electrode layout.
    ///
    /// The default layout already annotates channels 0–7 as EMG and 8–17 as
    /// EEG with generated labels (`"EMG1"`, `"EEG1"`, …).  Use this method to
    /// override with named 10-20 positions or custom labels.
    pub fn with_electrode_layout(mut self, layout: ElectrodeLayout) -> Self {
        self.electrode_layout = layout;
        self
    }

    /// Build the default electrode layout with signal-type annotations for all
    /// 24 Galea channels.
    fn default_layout() -> ElectrodeLayout {
        use crate::electrode::{Electrode, SignalType};
        let mut layout = ElectrodeLayout::new(24);
        for i in 0..8 {
            layout.set_electrode(i, Electrode {
                label: format!("EMG{}", i + 1),
                signal_type: SignalType::Emg,
                note: None,
            });
        }
        for i in 8..18 {
            layout.set_electrode(i, Electrode {
                label: format!("EEG{}", i - 7),
                signal_type: SignalType::Eeg,
                note: None,
            });
        }
        for i in 18..22 {
            layout.set_electrode(i, Electrode {
                label: format!("AUX{}", i - 17),
                signal_type: SignalType::Emg,
                note: None,
            });
        }
        layout
    }


    /// Measure half round-trip time by sending a timestamp request and reading
    /// back the device's echo.  Repeating this 3 times averages the RTT.
    fn calibrate_time(&mut self) -> Result<()> {
        let sock = self.socket.as_ref().ok_or(OpenBciError::BoardNotPrepared)?;
        let mut rtt_sum = 0.0f64;
        let time_cmd = b"F4444444"; // 8 bytes: 'F' + 7 bytes padding

        for _ in 0..3 {
            let t_start = now();
            sock.send(time_cmd)?;

            let mut resp = [0u8; 8];
            sock.recv(&mut resp)?;
            let t_done = now();

            let rtt = t_done - t_start;
            rtt_sum += rtt;
        }
        self.half_rtt = rtt_sum / 6.0; // sum of 3 RTTs / 2 / 3
        log::info!("Galea estimated half-RTT: {:.3} ms", self.half_rtt * 1000.0);
        Ok(())
    }

    /// Decode one 96-byte EXG packet from `buf` starting at `offset`, together
    /// with any IMU data appended at byte 96 of the datagram.
    ///
    /// # Parameters
    /// - `buf` — raw UDP datagram bytes.
    /// - `offset` — byte offset of the start of this EXG packet within `buf`.
    /// - `gains` — per-channel gain multipliers for µV scaling.
    /// - `half_rtt` — estimated half round-trip time (seconds), used to
    ///   correct the board's device timestamp to host-clock time.
    /// - `pc_timestamp` — host UNIX timestamp when the datagram was received.
    fn decode_packet(
        buf: &[u8],
        offset: usize,
        gains: &GainTracker,
        half_rtt: f64,
        pc_timestamp: f64,
    ) -> GaleaSample {
        let o = offset;

        let packet_num = buf[o];

        // EDA float32 at bytes 1-4
        let mut eda_bytes = [0u8; 4];
        eda_bytes.copy_from_slice(&buf[o + 1..o + 5]);
        let eda_volts = f32::from_le_bytes(eda_bytes);

        // EEG/EMG: 24 channels × 3 bytes (big-endian 24-bit signed) at bytes 5-76
        const EEG_SCALE: f64 = 4.5 / 8_388_607.0 * 1_000_000.0;
        let mut eeg = vec![0.0f64; 24];
        for ch in 0..24 {
            let byte_off = o + 5 + ch * 3;
            let raw = cast_24bit_to_i32(&buf[byte_off..byte_off + 3]);
            eeg[ch] = EEG_SCALE / gains.gain_for(ch) * raw as f64;
        }

        let battery = buf[o + 77];

        // Temperature uint16 at bytes 78-79 (value × 100)
        let temp_raw = u16::from_le_bytes([buf[o + 78], buf[o + 79]]);
        let temperature_c = temp_raw as f32 / 100.0;

        // PPG at bytes 80-87
        let mut ppg_red_bytes = [0u8; 4];
        ppg_red_bytes.copy_from_slice(&buf[o + 80..o + 84]);
        let ppg_red = i32::from_le_bytes(ppg_red_bytes);

        let mut ppg_ir_bytes = [0u8; 4];
        ppg_ir_bytes.copy_from_slice(&buf[o + 84..o + 88]);
        let ppg_ir = i32::from_le_bytes(ppg_ir_bytes);

        // Device timestamp uint64 at bytes 88-95 (microseconds)
        let mut ts_bytes = [0u8; 8];
        ts_bytes.copy_from_slice(&buf[o + 88..o + 96]);
        let device_ts_us = u64::from_le_bytes(ts_bytes);
        let device_timestamp = device_ts_us as f64 / 1_000_000.0;

        // Corrected host timestamp
        let corrected_ts = device_timestamp + (pc_timestamp - device_timestamp) - half_rtt;

        let mut base = Sample::zeroed(24);
        base.sample_num = packet_num;
        base.eeg        = eeg;
        base.timestamp  = corrected_ts;

        // IMU data appears every 5th packet, appended after all EXG packets
        // at offset 96 from the start of the datagram.
        let imu_offset = 96; // fixed offset in the datagram (after first EXG packet)
        let (accel, gyro, mag) = if packet_num % 5 == 0 && buf.len() > imu_offset + 18 {
            let io = imu_offset;

            const ACCEL_SCALE: f64 = 8.0 / 65535.0;
            const GYRO_SCALE:  f64 = 1000.0 / 65535.0;
            const MAG_SCALE_XY: f64 = 2.6 / 8191.0;
            const MAG_SCALE_Z:  f64 = 5.0 / 32767.0;

            let ax = cast_16bit_to_i32(&buf[io..]) as f64 * ACCEL_SCALE;
            let ay = cast_16bit_to_i32(&buf[io+2..]) as f64 * ACCEL_SCALE;
            let az = cast_16bit_to_i32(&buf[io+4..]) as f64 * ACCEL_SCALE;

            let gx = cast_16bit_to_i32(&buf[io+6..])  as f64 * GYRO_SCALE;
            let gy = cast_16bit_to_i32(&buf[io+8..])  as f64 * GYRO_SCALE;
            let gz = cast_16bit_to_i32(&buf[io+10..]) as f64 * GYRO_SCALE;

            let mx = cast_16bit_to_i32(&buf[io+12..]) as f64 * MAG_SCALE_XY;
            let my = cast_16bit_to_i32(&buf[io+14..]) as f64 * MAG_SCALE_XY;
            let mz = cast_16bit_to_i32(&buf[io+16..]) as f64 * MAG_SCALE_Z;

            (Some([ax, ay, az]), Some([gx, gy, gz]), Some([mx, my, mz]))
        } else {
            (None, None, None)
        };

        GaleaSample {
            base,
            eda_volts,
            battery,
            temperature_c,
            ppg_red,
            ppg_ir,
            device_timestamp,
            accel,
            gyro,
            magnetometer: mag,
        }
    }
}

// ─── Board trait ─────────────────────────────────────────────────────────────

impl Board for GaleaBoard {
    fn prepare(&mut self) -> Result<()> {
        if self.socket.is_some() { return Ok(()); }

        // Auto-discover if IP not specified
        if self.ip_address.is_empty() {
            use super::wifi_shield::discover_wifi_shield;
            self.ip_address = discover_wifi_shield(Duration::from_secs(10));
            log::info!("Galea discovered at {}", self.ip_address);
        }

        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.connect(format!("{}:{}", self.ip_address, GALEA_PORT))?;
        sock.set_read_timeout(Some(Duration::from_secs(5)))?;

        self.socket = Some(sock);

        // Apply default settings and sampling rate
        self.send_command("d")?;
        self.send_command("~6")?; // 250 Hz

        self.calibrate_time()?;
        Ok(())
    }

    fn start_stream(&mut self) -> Result<StreamHandle> {
        if self.streaming { return Err(OpenBciError::AlreadyStreaming); }
        let sock = self.socket.as_ref().ok_or(OpenBciError::BoardNotPrepared)?;
        sock.send(b"b")?;

        let reader = sock.try_clone()?;
        let (sample_tx, sample_rx) = std::sync::mpsc::sync_channel::<Sample>(512);
        let (stop_tx, stop_rx)     = std::sync::mpsc::sync_channel::<()>(1);
        let keep_alive = self.keep_alive.clone();
        let gains      = self.gains.clone();
        let half_rtt   = self.half_rtt;

        keep_alive.store(true, Ordering::Release);

        std::thread::spawn(move || {
            let mut buf = vec![0u8; MAX_UDP_RECV];

            loop {
                if stop_rx.try_recv().is_ok() || !keep_alive.load(Ordering::Acquire) { break; }

                let n = match reader.recv(&mut buf) {
                    Ok(n) => n,
                    Err(_) => continue,
                };

                // Validate: must be a multiple of EXG_PACKET_SIZE
                if n < EXG_PACKET_SIZE || (n % EXG_PACKET_SIZE != 0 && n < EXG_PACKET_SIZE + 18) {
                    // Might be a text response — log and skip
                    let preview = &buf[..n.min(64)];
                    log::debug!("Non-packet data received: {:?}", preview);
                    continue;
                }

                let pc_ts = now();
                let num_pkts = n / EXG_PACKET_SIZE;

                for pkt_i in 0..num_pkts {
                    let offset = pkt_i * EXG_PACKET_SIZE;
                    let galea_sample = GaleaBoard::decode_packet(
                        &buf[..n], offset, &gains, half_rtt, pc_ts
                    );

                    if sample_tx.send(galea_sample.base).is_err() { return; }
                }
            }
        });

        self.streaming = true;
        Ok(StreamHandle { receiver: sample_rx, stop_tx: Some(stop_tx) })
    }

    fn stop_stream(&mut self) -> Result<()> {
        if !self.streaming { return Err(OpenBciError::NotStreaming); }
        self.keep_alive.store(false, Ordering::Release);
        if let Some(ref sock) = self.socket {
            let _ = sock.send(b"s");
        }
        self.streaming = false;
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        if self.streaming { let _ = self.stop_stream(); }
        self.socket = None;
        Ok(())
    }

    fn send_command(&mut self, cmd: &str) -> Result<String> {
        let sock = self.socket.as_ref().ok_or(OpenBciError::BoardNotPrepared)?;
        sock.send(cmd.as_bytes())?;

        if self.streaming {
            return Ok(String::new());
        }
        self.gains.apply_command(cmd);

        // Read text response (ends when recv returns non-packet data)
        let mut resp_bytes = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            match sock.recv(&mut buf) {
                Ok(n) if n > 0 && n % EXG_PACKET_SIZE != 0 => {
                    resp_bytes.extend_from_slice(&buf[..n]);
                    break;
                }
                _ => break,
            }
        }
        Ok(String::from_utf8_lossy(&resp_bytes).into_owned())
    }

    fn electrode_layout(&self) -> &ElectrodeLayout        { &self.electrode_layout }
    fn set_electrode_layout(&mut self, l: ElectrodeLayout) { self.electrode_layout = l; }
    fn channel_count(&self) -> usize                       { 24 }
    fn sampling_rate(&self) -> u32                         { 250 }
}

// ─── ConfigurableBoard ────────────────────────────────────────────────────────

impl ConfigurableBoard for GaleaBoard {
    fn apply_channel_config(&mut self, channel: usize, config: &ChannelConfig) -> Result<()> {
        if channel >= 24 { return Err(OpenBciError::ChannelOutOfRange(channel, 24)); }
        // Galea uses extended channel letters (A,S,D,...) for channels > 15
        let letter = if channel < 16 {
            CHANNEL_LETTERS[channel]
        } else {
            // Additional Galea channel letters: A S D G H J K L (channels 16-23)
            const GALEA_EXTRA: [char; 8] = ['A','S','D','G','H','J','K','L'];
            GALEA_EXTRA[channel - 16]
        };
        let cmd = config.to_command(letter);
        self.gains.apply_command(&cmd);
        self.send_command(&cmd)?;
        Ok(())
    }
}
