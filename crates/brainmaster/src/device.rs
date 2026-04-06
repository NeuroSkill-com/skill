//! High-level BrainMaster device API.

use std::io::Read;
use std::time::Duration;

use crate::error::{BrainMasterError, Result};
use crate::protocol::{self, SYNC};

/// BrainMaster device model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DeviceModel {
    /// Atlantis I/II — 2-channel neurofeedback amplifier.
    Atlantis2,
    /// Atlantis 4×4 — 4-channel neurofeedback amplifier.
    Atlantis4,
    /// Discovery — 24-channel clinical EEG system.
    Discovery,
    /// Freedom — 24-channel wireless EEG system.
    Freedom,
}

impl DeviceModel {
    /// Number of EEG channels for this model.
    pub fn channel_count(self) -> usize {
        match self {
            Self::Atlantis2 => ATLANTIS2_CHANNELS,
            Self::Atlantis4 => ATLANTIS4_CHANNELS,
            Self::Discovery | Self::Freedom => DISCOVERY_CHANNELS,
        }
    }

    /// Sampling rate in Hz.
    pub fn sample_rate(self) -> u32 {
        match self {
            Self::Atlantis2 | Self::Atlantis4 => 256,
            Self::Discovery | Self::Freedom => 256,
        }
    }

    /// ADC scale factor (µV per LSB).
    pub fn uv_per_lsb(self) -> f64 {
        match self {
            Self::Atlantis2 | Self::Atlantis4 => protocol::ATLANTIS_UV_PER_LSB,
            Self::Discovery | Self::Freedom => protocol::DISCOVERY_UV_PER_LSB,
        }
    }

    /// Channel labels for this model.
    pub fn channel_names(self) -> Vec<&'static str> {
        match self {
            Self::Atlantis2 => ATLANTIS2_CHANNEL_NAMES.to_vec(),
            Self::Atlantis4 => ATLANTIS4_CHANNEL_NAMES.to_vec(),
            Self::Discovery | Self::Freedom => DISCOVERY_CHANNEL_NAMES.to_vec(),
        }
    }

    /// Baud rate for serial communication.
    pub fn baud_rate(self) -> u32 {
        57600
    }
}

// ── Constants ────────────────────────────────────────────────────────────────

pub const ATLANTIS2_CHANNELS: usize = 2;
pub const ATLANTIS4_CHANNELS: usize = 4;
pub const DISCOVERY_CHANNELS: usize = 24;
pub const FREEDOM_CHANNELS: usize = 24;

pub const ATLANTIS2_CHANNEL_NAMES: [&str; 2] = ["EEG1", "EEG2"];
pub const ATLANTIS4_CHANNEL_NAMES: [&str; 4] = ["EEG1", "EEG2", "EEG3", "EEG4"];
pub const DISCOVERY_CHANNEL_NAMES: [&str; 24] = [
    "Fp1", "Fp2", "F7", "F3", "Fz", "F4", "F8", "T3", "C3", "Cz", "C4", "T4", "T5", "P3", "Pz", "P4", "T6", "O1", "O2",
    "A1", "A2", "EMG1", "EMG2", "AUX",
];

// ── EEG sample ──────────────────────────────────────────────────────────────

/// A single EEG sample from a BrainMaster device.
#[derive(Debug, Clone)]
pub struct EegSample {
    /// Packet sequence counter (0–255).
    pub counter: u8,
    /// Channel values in **microvolts**.
    pub channels: Vec<f64>,
}

// ── Device ──────────────────────────────────────────────────────────────────

/// A connected BrainMaster device.
pub struct BrainMasterDevice {
    port: Box<dyn serialport::SerialPort>,
    model: DeviceModel,
    streaming: bool,
    buf: Vec<u8>,
}

impl BrainMasterDevice {
    /// Scan for BrainMaster-compatible serial ports.
    ///
    /// Returns port names that are likely FTDI/CDC serial adapters (COM3+ on
    /// Windows, /dev/ttyUSB* on Linux, /dev/cu.usbserial* on macOS).
    pub fn scan() -> Result<Vec<String>> {
        let ports = serialport::available_ports().map_err(BrainMasterError::Serial)?;
        let mut results = Vec::new();
        for port in ports {
            let dominated = match &port.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    // FTDI chips commonly used by BrainMaster
                    usb.vid == 0x0403
                        || usb
                            .manufacturer
                            .as_deref()
                            .map(|m| {
                                let ml = m.to_lowercase();
                                ml.contains("ftdi") || ml.contains("brainmaster")
                            })
                            .unwrap_or(false)
                }
                _ => {
                    let lower = port.port_name.to_lowercase();
                    lower.contains("ttyusb") || lower.contains("usbserial")
                }
            };
            if dominated {
                results.push(port.port_name);
            }
        }
        Ok(results)
    }

    /// Open a connection to a BrainMaster device on the given serial port.
    pub fn open(port_name: &str, model: DeviceModel) -> Result<Self> {
        let port = serialport::new(port_name, model.baud_rate())
            .timeout(Duration::from_millis(1000))
            .open()
            .map_err(BrainMasterError::Serial)?;

        Ok(Self {
            port,
            model,
            streaming: false,
            buf: Vec::with_capacity(256),
        })
    }

    /// Device model.
    pub fn model(&self) -> DeviceModel {
        self.model
    }

    /// Number of EEG channels.
    pub fn channel_count(&self) -> usize {
        self.model.channel_count()
    }

    /// Start EEG data streaming.
    ///
    /// Sends the start command byte (`0x53` = 'S') to the amplifier.
    pub fn start_streaming(&mut self) -> Result<()> {
        use std::io::Write;
        self.port.write_all(&[0x53])?; // 'S' = Start
        self.streaming = true;
        self.buf.clear();
        Ok(())
    }

    /// Stop EEG data streaming.
    ///
    /// Sends the stop command byte (`0x51` = 'Q') to the amplifier.
    pub fn stop_streaming(&mut self) -> Result<()> {
        use std::io::Write;
        self.port.write_all(&[0x51])?; // 'Q' = Quit
        self.streaming = false;
        Ok(())
    }

    /// Read a single EEG sample (blocks until data arrives or timeout).
    pub fn read_sample(&mut self) -> Result<EegSample> {
        if !self.streaming {
            return Err(BrainMasterError::NotStreaming);
        }

        let n_ch = self.model.channel_count();
        let packet_len = 2 + 1 + n_ch * 2 + 1; // sync(2) + counter(1) + data + checksum(1)
        let scale = self.model.uv_per_lsb();

        // Read bytes until we find a sync header and have a complete packet.
        let mut tmp = [0u8; 1];
        let mut attempts = 0;
        loop {
            attempts += 1;
            if attempts > packet_len * 100 {
                return Err(BrainMasterError::SyncLost);
            }

            match self.port.read(&mut tmp) {
                Ok(1) => self.buf.push(tmp[0]),
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    return Err(BrainMasterError::Timeout);
                }
                Err(e) => return Err(BrainMasterError::Io(e)),
            }

            // Look for sync in buffer.
            if self.buf.len() < packet_len {
                continue;
            }

            // Find sync position.
            let sync_pos = self.buf.windows(2).position(|w| w == SYNC);
            let Some(pos) = sync_pos else {
                // No sync found — keep last byte in case it's first sync byte.
                let last = self.buf.last().copied().unwrap_or(0);
                self.buf.clear();
                self.buf.push(last);
                continue;
            };

            // Check if we have enough bytes after sync.
            let payload_start = pos + 2;
            let payload_len = 1 + n_ch * 2 + 1;
            if self.buf.len() < payload_start + payload_len {
                continue;
            }

            // Parse the packet.
            let payload = &self.buf[payload_start..payload_start + payload_len];
            if let Some((counter, raw_channels, checksum_ok)) = protocol::parse_packet(payload, n_ch) {
                // Consume the packet from buffer.
                let consumed = payload_start + payload_len;
                self.buf.drain(..consumed);

                if !checksum_ok {
                    log::debug!("checksum mismatch — skipping packet");
                    continue;
                }

                let channels: Vec<f64> = raw_channels
                    .iter()
                    .map(|&raw| protocol::adc_to_uv(raw, scale))
                    .collect();

                return Ok(EegSample { counter, channels });
            }

            // Parse failed — advance past this sync.
            self.buf.drain(..pos + 2);
        }
    }

    /// Capture N samples (convenience wrapper).
    pub fn capture(&mut self, n_samples: usize) -> Result<Vec<EegSample>> {
        if !self.streaming {
            self.start_streaming()?;
        }
        let mut samples = Vec::with_capacity(n_samples);
        for _ in 0..n_samples {
            samples.push(self.read_sample()?);
        }
        Ok(samples)
    }

    /// Release the serial port.
    pub fn close(mut self) {
        let _ = self.stop_streaming();
        // port dropped automatically
    }
}

impl Drop for BrainMasterDevice {
    fn drop(&mut self) {
        if self.streaming {
            let _ = self.stop_streaming();
        }
    }
}
