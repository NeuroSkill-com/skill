//! Cyton WiFi board — 8-channel EEG via the OpenBCI WiFi Shield.
//!
//! The board sends 33-byte Cyton-format packets to a TCP server on the host PC.

use std::io::Read;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{Board, ConfigurableBoard};
use super::wifi_shield::{
    connect_wifi_shield, send_wifi_command, wifi_start_stream, wifi_stop_stream, WifiShieldConfig,
};
use crate::channel_config::{ChannelConfig, GainTracker, CHANNEL_LETTERS};
use crate::electrode::ElectrodeLayout;
use crate::error::{OpenBciError, Result};
use crate::packet::{decode_cyton, START_BYTE};
use crate::sample::{Sample, StreamHandle};

// ─────────────────────────────────────────────────────────────────────────────

/// Cyton 8-channel EEG board connected via the OpenBCI WiFi Shield.
///
/// # Example
/// ```rust,no_run
/// use openbci::board::cyton_wifi::{CytonWifiBoard, CytonWifiConfig};
/// use openbci::board::Board;
/// use openbci::electrode::ElectrodeLayout;
///
/// let cfg = CytonWifiConfig {
///     shield_ip: "192.168.4.1".to_string(),
///     local_port: 3000,
///     ..Default::default()
/// };
///
/// let mut board = CytonWifiBoard::new(cfg)
///     .with_electrode_layout(ElectrodeLayout::from_labels(&[
///         "Fp1","Fp2","C3","Cz","C4","P3","Pz","P4"
///     ]));
///
/// board.prepare().unwrap();
/// let stream = board.start_stream().unwrap();
/// for sample in stream.into_iter().take(250) {
///     println!("{:?}", sample.eeg);
/// }
/// ```
pub struct CytonWifiBoard {
    wifi_cfg:         CytonWifiConfig,
    electrode_layout: ElectrodeLayout,
    gains:            GainTracker,
    stream:           Option<TcpStream>,
    streaming:        bool,
    keep_alive:       Arc<AtomicBool>,
}

/// Configuration for a WiFi-connected Cyton board.
#[derive(Debug, Clone)]
pub struct CytonWifiConfig {
    /// IP address of the OpenBCI WiFi Shield.  An empty string triggers
    /// automatic SSDP discovery on the local network.
    pub shield_ip: String,
    /// Local TCP port on which this driver listens for incoming stream packets
    /// from the shield.  Default: `3000`.
    pub local_port: u16,
    /// Timeout in seconds for HTTP requests sent to the shield's REST API
    /// and for the initial TCP `accept()`.  Default: `10`.
    pub http_timeout: u64,
}

impl Default for CytonWifiConfig {
    fn default() -> Self {
        Self { shield_ip: String::new(), local_port: 3000, http_timeout: 10 }
    }
}

impl CytonWifiBoard {
    /// Create a new Cyton WiFi board driver with the given configuration.
    pub fn new(cfg: CytonWifiConfig) -> Self {
        Self {
            wifi_cfg:         cfg,
            electrode_layout: ElectrodeLayout::new(8),
            gains:            GainTracker::new(vec![24.0; 8]),
            stream:           None,
            streaming:        false,
            keep_alive:       Arc::new(AtomicBool::new(false)),
        }
    }

    /// Builder: attach an electrode layout describing all 8 channel sites.
    pub fn with_electrode_layout(mut self, layout: ElectrodeLayout) -> Self {
        self.electrode_layout = layout;
        self
    }

    /// Resolved shield IP address (after optional SSDP discovery).
    fn shield_ip(&self) -> &str { &self.wifi_cfg.shield_ip }
    /// HTTP request timeout in seconds.
    fn http_timeout(&self) -> u64 { self.wifi_cfg.http_timeout }
}

// ─── Board trait ─────────────────────────────────────────────────────────────

impl Board for CytonWifiBoard {
    fn prepare(&mut self) -> Result<()> {
        if self.stream.is_some() { return Ok(()); }

        let mut shield_cfg = WifiShieldConfig {
            shield_ip:    self.wifi_cfg.shield_ip.clone(),
            local_port:   self.wifi_cfg.local_port,
            http_timeout: self.wifi_cfg.http_timeout,
        };

        let tcp_stream = connect_wifi_shield(&mut shield_cfg)?;

        // Persist the resolved IP
        self.wifi_cfg.shield_ip = shield_cfg.shield_ip;

        // Lock the sampling rate to 1 000 Hz (Cyton max via WiFi)
        send_wifi_command(self.shield_ip(), "~4", self.http_timeout())?;
        // Apply default channel settings
        send_wifi_command(self.shield_ip(), "d", self.http_timeout())?;

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

                // Read exactly 33 bytes per packet
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

                // buf[1..33] is the 32-byte packet body
                let body: [u8; 32] = buf[1..33].try_into().unwrap();
                if let Some(sample) = decode_cyton(&body, &gains, 8) {
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
        self.gains.apply_command(cmd);
        Ok(String::new())
    }

    fn electrode_layout(&self) -> &ElectrodeLayout        { &self.electrode_layout }
    fn set_electrode_layout(&mut self, l: ElectrodeLayout) { self.electrode_layout = l; }
    fn channel_count(&self) -> usize                       { 8 }
    fn sampling_rate(&self) -> u32                         { 1000 }
}

// ─── ConfigurableBoard ────────────────────────────────────────────────────────

impl ConfigurableBoard for CytonWifiBoard {
    fn apply_channel_config(&mut self, channel: usize, config: &ChannelConfig) -> Result<()> {
        if channel >= 8 { return Err(OpenBciError::ChannelOutOfRange(channel, 8)); }
        let cmd = config.to_command(CHANNEL_LETTERS[channel]);
        self.gains.apply_command(&cmd);
        send_wifi_command(self.shield_ip(), &cmd, self.http_timeout())?;
        Ok(())
    }
}
