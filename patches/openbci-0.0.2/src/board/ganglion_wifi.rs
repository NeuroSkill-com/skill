//! Ganglion WiFi board — 4-channel EEG via the OpenBCI WiFi Shield.
//!
//! The WiFi Shield reformats the Ganglion's 20-byte BLE packets into the
//! standard 33-byte OpenBCI format before streaming them over TCP.  Only the
//! first 4 EEG channel slots in the packet carry valid data.

use std::io::Read;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::Board;
use super::wifi_shield::{
    connect_wifi_shield, send_wifi_command, wifi_start_stream, wifi_stop_stream, WifiShieldConfig,
};
use crate::electrode::ElectrodeLayout;
use crate::error::{OpenBciError, Result};
use crate::packet::{decode_cyton, START_BYTE};
use crate::channel_config::GainTracker;
use crate::sample::{Sample, StreamHandle};

// ─────────────────────────────────────────────────────────────────────────────

/// Ganglion 4-channel EEG board connected via the OpenBCI WiFi Shield.
///
/// The WiFi Shield forwards the Ganglion's 20-byte BLE packets as standard
/// 33-byte OpenBCI packets over a TCP connection.  Only the first four channel
/// slots carry valid EEG data; the remaining slots (channels 4–7) are zero-padded
/// by the firmware and are discarded by this driver.
///
/// The Ganglion's amplifier gain is hardware-fixed (51×), so the gain tracker
/// is set to `1.0` and the EEG µV scale is baked into [`crate::packet::GANGLION_EEG_SCALE`].
pub struct GanglionWifiBoard {
    wifi_cfg:         GanglionWifiConfig,
    electrode_layout: ElectrodeLayout,
    /// Gain multipliers held at `1.0` — actual scaling is done by the Ganglion-specific
    /// constant in `packet::GANGLION_EEG_SCALE`.
    gains:            GainTracker,
    stream:           Option<TcpStream>,
    streaming:        bool,
    keep_alive:       Arc<AtomicBool>,
}

/// Configuration for a WiFi-connected Ganglion board.
#[derive(Debug, Clone)]
pub struct GanglionWifiConfig {
    /// IP address of the OpenBCI WiFi Shield.  An empty string triggers
    /// automatic SSDP discovery on the local network.
    pub shield_ip:    String,
    /// Local TCP port on which this driver listens for incoming stream packets
    /// from the shield.  Default: `3001` (different from Cyton to avoid conflicts).
    pub local_port:   u16,
    /// Timeout in seconds for HTTP requests to the shield's REST API and for
    /// the initial TCP `accept()`.  Default: `10`.
    pub http_timeout: u64,
}

impl Default for GanglionWifiConfig {
    fn default() -> Self {
        Self { shield_ip: String::new(), local_port: 3001, http_timeout: 10 }
    }
}

impl GanglionWifiBoard {
    /// Create a new Ganglion WiFi board driver with the given configuration.
    pub fn new(cfg: GanglionWifiConfig) -> Self {
        // Ganglion gain is hardware-fixed; we use 51.0 to match the EEG scale
        // derived from the Ganglion's internal voltage reference (see packet.rs).
        // decode_cyton will apply gains.gain_for(i) — we set 1.0 and rely on
        // the firmware's internal scaling.
        Self {
            wifi_cfg:         cfg,
            electrode_layout: ElectrodeLayout::new(4),
            gains:            GainTracker::new(vec![1.0; 4]),
            stream:           None,
            streaming:        false,
            keep_alive:       Arc::new(AtomicBool::new(false)),
        }
    }

    /// Builder: attach an electrode layout describing the 4 channel sites.
    pub fn with_electrode_layout(mut self, layout: ElectrodeLayout) -> Self {
        self.electrode_layout = layout;
        self
    }

    /// Resolved shield IP address (after optional SSDP discovery).
    fn shield_ip(&self) -> &str   { &self.wifi_cfg.shield_ip }
    /// HTTP request timeout in seconds.
    fn http_timeout(&self) -> u64 { self.wifi_cfg.http_timeout }
}

// ─── Board trait ─────────────────────────────────────────────────────────────

impl Board for GanglionWifiBoard {
    fn prepare(&mut self) -> Result<()> {
        if self.stream.is_some() { return Ok(()); }

        let mut shield_cfg = WifiShieldConfig {
            shield_ip:    self.wifi_cfg.shield_ip.clone(),
            local_port:   self.wifi_cfg.local_port,
            http_timeout: self.wifi_cfg.http_timeout,
        };
        let tcp_stream = connect_wifi_shield(&mut shield_cfg)?;
        self.wifi_cfg.shield_ip = shield_cfg.shield_ip;

        // Ganglion default sampling rate via WiFi shield: 1600 Hz raw
        send_wifi_command(self.shield_ip(), "~4", self.http_timeout())?;

        self.stream = Some(tcp_stream);
        Ok(())
    }

    fn start_stream(&mut self) -> Result<StreamHandle> {
        if self.streaming { return Err(OpenBciError::AlreadyStreaming); }
        let tcp = self.stream.as_ref().ok_or(OpenBciError::BoardNotPrepared)?;

        wifi_start_stream(self.shield_ip(), self.http_timeout())?;

        let mut reader = tcp.try_clone()?;
        let (sample_tx, sample_rx) = std::sync::mpsc::sync_channel::<Sample>(512);
        let (stop_tx, stop_rx)     = std::sync::mpsc::sync_channel::<()>(1);
        let keep_alive = self.keep_alive.clone();
        let gains      = self.gains.clone();

        keep_alive.store(true, Ordering::Release);

        std::thread::spawn(move || {
            let mut buf = [0u8; 33];

            'outer: loop {
                if stop_rx.try_recv().is_ok() || !keep_alive.load(Ordering::Acquire) { break; }

                let mut remaining = 33usize;
                let mut pos       = 0usize;
                while remaining > 0 {
                    if stop_rx.try_recv().is_ok() { break 'outer; }
                    match reader.read(&mut buf[pos..pos + remaining]) {
                        Ok(n) if n > 0 => { pos += n; remaining -= n; }
                        _ => continue 'outer,
                    }
                }

                if buf[0] != START_BYTE { continue; }

                let body: [u8; 32] = buf[1..33].try_into().unwrap();
                // Decode only the first 4 channels; channels 4-7 are unused padding
                if let Some(mut sample) = decode_cyton(&body, &gains, 4) {
                    sample.eeg.truncate(4);
                    if sample_tx.send(sample).is_err() { break; }
                }
            }
        });

        self.streaming = true;
        Ok(StreamHandle { receiver: sample_rx, stop_tx: Some(stop_tx) })
    }

    fn stop_stream(&mut self) -> Result<()> {
        if !self.streaming { return Err(OpenBciError::NotStreaming); }
        self.keep_alive.store(false, Ordering::Release);
        wifi_stop_stream(self.shield_ip(), self.http_timeout())?;
        self.streaming = false;
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        if self.streaming { let _ = self.stop_stream(); }
        self.stream = None;
        Ok(())
    }

    fn send_command(&mut self, cmd: &str) -> Result<String> {
        send_wifi_command(self.shield_ip(), cmd, self.http_timeout())?;
        Ok(String::new())
    }

    fn electrode_layout(&self) -> &ElectrodeLayout        { &self.electrode_layout }
    fn set_electrode_layout(&mut self, l: ElectrodeLayout) { self.electrode_layout = l; }
    fn channel_count(&self) -> usize                       { 4 }
    fn sampling_rate(&self) -> u32                         { 200 }
}
