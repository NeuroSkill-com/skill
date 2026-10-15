//! Cyton board — 8-channel EEG via USB serial dongle.
//!
//! The Cyton uses an FTDI USB-to-serial dongle at 115 200 baud.
//! Data arrives as 33-byte packets at 250 Hz.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{Board, ConfigurableBoard};
use crate::channel_config::{ChannelConfig, GainTracker, CHANNEL_LETTERS};
use crate::electrode::ElectrodeLayout;
use crate::error::{OpenBciError, Result};
use crate::packet::{decode_cyton, START_BYTE};
use crate::sample::{Sample, StreamHandle};

// ─────────────────────────────────────────────────────────────────────────────

/// Cyton 8-channel serial board.
///
/// # Example
/// ```rust,no_run
/// use openbci::board::cyton::CytonBoard;
/// use openbci::board::{Board, ConfigurableBoard};
/// use openbci::channel_config::{ChannelConfig, Gain};
/// use openbci::electrode::{ElectrodeLayout, positions};
///
/// let layout = ElectrodeLayout::from_labels(&[
///     positions::FP1, positions::FP2, positions::C3, positions::CZ,
///     positions::C4,  positions::P3,  positions::PZ, positions::P4,
/// ]);
///
/// let mut board = CytonBoard::new("/dev/ttyUSB0")
///     .with_electrode_layout(layout);
///
/// board.prepare().unwrap();
/// board.apply_all_channel_configs(&vec![ChannelConfig::default(); 8]).unwrap();
///
/// let stream = board.start_stream().unwrap();
/// for sample in stream.into_iter().take(250) {
///     println!("t={:.3}  eeg[0]={:.1} µV  label={}",
///         sample.timestamp, sample.eeg[0],
///         board.electrode_layout().label(0));
/// }
/// board.release().unwrap();
/// ```
pub struct CytonBoard {
    port_name:        String,
    electrode_layout: ElectrodeLayout,
    gains:            GainTracker,
    port:             Option<Box<dyn serialport::SerialPort>>,
    streaming:        bool,
    keep_alive:       Arc<AtomicBool>,
}

impl CytonBoard {
    /// Create a new Cyton board driver.
    ///
    /// `port` — serial port path, e.g. `"/dev/ttyUSB0"` or `"COM3"`.
    pub fn new(port: impl Into<String>) -> Self {
        Self {
            port_name:        port.into(),
            electrode_layout: ElectrodeLayout::new(8),
            gains:            GainTracker::new(vec![24.0; 8]),
            port:             None,
            streaming:        false,
            keep_alive:       Arc::new(AtomicBool::new(false)),
        }
    }

    /// Builder: attach an electrode layout.
    pub fn with_electrode_layout(mut self, layout: ElectrodeLayout) -> Self {
        self.electrode_layout = layout;
        self
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn port_mut(&mut self) -> Result<&mut Box<dyn serialport::SerialPort>> {
        self.port.as_mut().ok_or(OpenBciError::BoardNotPrepared)
    }

    /// Wait for three consecutive `$` characters that signal board readiness.
    fn wait_for_ready(port: &mut Box<dyn serialport::SerialPort>) -> Result<()> {
        let mut buf         = [0u8; 1];
        let mut dollar_cnt  = 0usize;
        let mut empty_seq   = 0usize;

        for _ in 0..500 {
            match port.read(&mut buf) {
                Ok(1) => {
                    empty_seq = 0;
                    if buf[0] == b'$' {
                        dollar_cnt += 1;
                        if dollar_cnt == 3 { return Ok(()); }
                    } else {
                        dollar_cnt = 0;
                    }
                }
                _ => {
                    empty_seq += 1;
                    if empty_seq >= 5 {
                        return Err(OpenBciError::BoardNotReady(
                            "Board did not send '$$$' ready marker".into(),
                        ));
                    }
                }
            }
        }
        Err(OpenBciError::Timeout)
    }

    /// Drain and return any bytes the board sends until a read timeout fires.
    fn drain_response(port: &mut Box<dyn serialport::SerialPort>) -> String {
        let mut resp = Vec::new();
        let mut b    = [0u8; 1];
        loop {
            match port.read(&mut b) {
                Ok(1) => resp.push(b[0]),
                _     => break,
            }
        }
        String::from_utf8_lossy(&resp).into_owned()
    }
}

// ─── Board trait ─────────────────────────────────────────────────────────────

impl Board for CytonBoard {
    fn prepare(&mut self) -> Result<()> {
        if self.port.is_some() { return Ok(()); }

        let mut port = serialport::new(&self.port_name, 115_200)
            .timeout(Duration::from_millis(1000))
            .open()?;

        // Soft-reset → board replies with firmware string then "$$$"
        port.write_all(b"v")?;
        Self::wait_for_ready(&mut port)?;

        // Apply default settings
        port.write_all(b"d")?;
        let resp = Self::drain_response(&mut port);
        if resp.starts_with("Failure") {
            return Err(OpenBciError::BoardNotReady(
                "Dongle connected but Cyton board is off or not responding".into(),
            ));
        }

        self.port = Some(port);
        Ok(())
    }

    fn start_stream(&mut self) -> Result<StreamHandle> {
        if self.streaming { return Err(OpenBciError::AlreadyStreaming); }

        let port = self.port.as_mut().ok_or(OpenBciError::BoardNotPrepared)?;
        port.write_all(b"b")?;

        // Clone the port handle for the background reader thread.
        let mut reader = port.try_clone().map_err(|e| {
            OpenBciError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        })?;

        let (sample_tx, sample_rx) = std::sync::mpsc::sync_channel::<Sample>(512);
        let (stop_tx, stop_rx)     = std::sync::mpsc::sync_channel::<()>(1);

        let keep_alive = self.keep_alive.clone();
        let gains      = self.gains.clone();

        keep_alive.store(true, Ordering::Release);

        std::thread::spawn(move || {
            let mut buf1  = [0u8; 1];
            let mut buf32 = [0u8; 32];

            'outer: loop {
                if stop_rx.try_recv().is_ok() || !keep_alive.load(Ordering::Acquire) {
                    break;
                }

                // Scan for start byte
                match reader.read(&mut buf1) {
                    Ok(1) if buf1[0] == START_BYTE => {}
                    _ => continue,
                }

                // Read the remaining 32 bytes of the packet
                let mut remaining = 32usize;
                let mut pos       = 0usize;
                while remaining > 0 {
                    if stop_rx.try_recv().is_ok() { break 'outer; }
                    match reader.read(&mut buf32[pos..pos + remaining]) {
                        Ok(n) if n > 0 => { pos += n; remaining -= n; }
                        _ => continue 'outer,
                    }
                }

                if let Some(sample) = decode_cyton(&buf32, &gains, 8) {
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
        if let Some(ref mut p) = self.port {
            let _ = p.write_all(b"s");
        }
        self.streaming = false;
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        if self.streaming { let _ = self.stop_stream(); }
        self.port = None;
        Ok(())
    }

    fn send_command(&mut self, cmd: &str) -> Result<String> {
        // Write the command — scope the borrow so we can access self.streaming after
        {
            let port = self.port_mut()?;
            port.write_all(cmd.as_bytes())?;
        }
        if self.streaming {
            // Don't try to read response while streaming — data bytes will interfere
            return Ok(String::new());
        }
        self.gains.apply_command(cmd);
        let port = self.port_mut()?;
        Ok(Self::drain_response(port))
    }

    fn electrode_layout(&self) -> &ElectrodeLayout   { &self.electrode_layout }
    fn set_electrode_layout(&mut self, l: ElectrodeLayout) { self.electrode_layout = l; }
    fn channel_count(&self) -> usize                  { 8 }
    fn sampling_rate(&self) -> u32                    { 250 }
}

// ─── ConfigurableBoard ────────────────────────────────────────────────────────

impl ConfigurableBoard for CytonBoard {
    fn apply_channel_config(&mut self, channel: usize, config: &ChannelConfig) -> Result<()> {
        if channel >= 8 { return Err(OpenBciError::ChannelOutOfRange(channel, 8)); }
        let cmd = config.to_command(CHANNEL_LETTERS[channel]);
        self.gains.apply_command(&cmd);
        let streaming = self.streaming;
        {
            let port = self.port_mut()?;
            port.write_all(cmd.as_bytes())?;
        }
        if !streaming {
            let port = self.port_mut()?;
            Self::drain_response(port);
        }
        Ok(())
    }
}
