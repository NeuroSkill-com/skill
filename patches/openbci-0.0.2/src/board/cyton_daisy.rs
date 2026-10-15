//! Cyton + Daisy 16-channel EEG board over USB serial.
//!
//! The [Daisy](https://docs.openbci.com/AddOns/Headwear/DaisyModule/) module
//! adds a second ADS1299 to the Cyton.  Both chips share a single FTDI
//! USB dongle running at 115 200 baud.
//!
//! ## Packet interleaving
//!
//! The combined firmware transmits two alternating 33-byte packets per
//! physical sample cycle:
//!
//! 1. **Daisy packet** (even `sample_num`): carries Daisy channels 8–15
//!    → buffered by the driver.
//! 2. **Cyton packet** (odd `sample_num`): carries Cyton channels 0–7
//!    → driver merges with the buffered Daisy data and emits a 16-channel
//!    [`crate::sample::Sample`].
//!
//! The effective output rate is therefore **~125 Hz** (half of the 250 Hz
//! hardware rate).
//!
//! ## Channel mapping
//!
//! | Channel indices | Board | ADS1299 |
//! |---|---|---|
//! | 0–7  | Cyton  | IC #1 |
//! | 8–15 | Daisy  | IC #2 |

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{Board, ConfigurableBoard};
use crate::channel_config::{ChannelConfig, GainTracker, CHANNEL_LETTERS};
use crate::electrode::ElectrodeLayout;
use crate::error::{OpenBciError, Result};
use crate::packet::{decode_cyton_daisy, START_BYTE};
use crate::sample::{Sample, StreamHandle};

// ─────────────────────────────────────────────────────────────────────────────

/// Cyton + Daisy 16-channel serial board.
///
/// Channels 0–7  → Cyton board (ADS1299 #1)
/// Channels 8–15 → Daisy board  (ADS1299 #2)
///
/// # Electrode layout note
/// Construct an [`ElectrodeLayout`] with 16 entries for a complete mapping.
pub struct CytonDaisyBoard {
    port_name:        String,
    electrode_layout: ElectrodeLayout,
    /// Gains for all 16 channels (0-7 = Cyton, 8-15 = Daisy).
    gains:            GainTracker,
    port:             Option<Box<dyn serialport::SerialPort>>,
    streaming:        bool,
    keep_alive:       Arc<AtomicBool>,
}

impl CytonDaisyBoard {
    /// Create a new Cyton+Daisy board driver.
    pub fn new(port: impl Into<String>) -> Self {
        Self {
            port_name:        port.into(),
            electrode_layout: ElectrodeLayout::new(16),
            gains:            GainTracker::new(vec![24.0; 16]),
            port:             None,
            streaming:        false,
            keep_alive:       Arc::new(AtomicBool::new(false)),
        }
    }

    /// Builder: attach an electrode layout describing the 16 channel sites.
    pub fn with_electrode_layout(mut self, layout: ElectrodeLayout) -> Self {
        self.electrode_layout = layout;
        self
    }

    /// Return a mutable reference to the open serial port, or
    /// [`OpenBciError::BoardNotPrepared`] if [`prepare`](super::Board::prepare)
    /// has not been called yet.
    fn port_mut(&mut self) -> Result<&mut Box<dyn serialport::SerialPort>> {
        self.port.as_mut().ok_or(OpenBciError::BoardNotPrepared)
    }

    /// Block until three consecutive `$` characters arrive, indicating that
    /// the board has completed its reset and is ready for commands.
    ///
    /// Aborts early with [`OpenBciError::BoardNotReady`] if five consecutive
    /// reads return nothing, or [`OpenBciError::Timeout`] after 500 attempts.
    fn wait_for_ready(port: &mut Box<dyn serialport::SerialPort>) -> Result<()> {
        let mut buf        = [0u8; 1];
        let mut dollar_cnt = 0usize;
        let mut empty_seq  = 0usize;

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
                            "Board did not send '$$$'".into(),
                        ));
                    }
                }
            }
        }
        Err(OpenBciError::Timeout)
    }

    /// Read and discard bytes until the port read-timeout fires, returning
    /// whatever the board sent as a UTF-8 string (best-effort).
    ///
    /// Used after sending a command (e.g. `"d"` for defaults) to consume the
    /// board's text reply so it doesn't pollute the sample stream.
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

impl Board for CytonDaisyBoard {
    fn prepare(&mut self) -> Result<()> {
        if self.port.is_some() { return Ok(()); }

        let mut port = serialport::new(&self.port_name, 115_200)
            .timeout(Duration::from_millis(1000))
            .open()?;

        port.write_all(b"v")?;
        Self::wait_for_ready(&mut port)?;

        port.write_all(b"d")?;
        let resp = Self::drain_response(&mut port);
        if resp.starts_with("Failure") {
            return Err(OpenBciError::BoardNotReady(
                "Dongle connected but board is off or not responding".into(),
            ));
        }

        self.port = Some(port);
        Ok(())
    }

    fn start_stream(&mut self) -> Result<StreamHandle> {
        if self.streaming { return Err(OpenBciError::AlreadyStreaming); }

        let port = self.port.as_mut().ok_or(OpenBciError::BoardNotPrepared)?;
        port.write_all(b"b")?;

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
            let mut daisy_buf: Option<Sample> = None; // partial assembled sample

            'outer: loop {
                if stop_rx.try_recv().is_ok() || !keep_alive.load(Ordering::Acquire) { break; }

                // Find start byte
                match reader.read(&mut buf1) {
                    Ok(1) if buf1[0] == START_BYTE => {}
                    _ => continue,
                }

                // Read remaining 32 bytes
                let mut remaining = 32usize;
                let mut pos       = 0usize;
                while remaining > 0 {
                    if stop_rx.try_recv().is_ok() { break 'outer; }
                    match reader.read(&mut buf32[pos..pos + remaining]) {
                        Ok(n) if n > 0 => { pos += n; remaining -= n; }
                        _ => continue 'outer,
                    }
                }

                if let Some(sample) = decode_cyton_daisy(&buf32, &gains, &mut daisy_buf) {
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
        if let Some(ref mut p) = self.port { let _ = p.write_all(b"s"); }
        self.streaming = false;
        Ok(())
    }

    fn release(&mut self) -> Result<()> {
        if self.streaming { let _ = self.stop_stream(); }
        self.port = None;
        Ok(())
    }

    fn send_command(&mut self, cmd: &str) -> Result<String> {
        {
            let port = self.port_mut()?;
            port.write_all(cmd.as_bytes())?;
        }
        if self.streaming { return Ok(String::new()); }
        self.gains.apply_command(cmd);
        let port = self.port_mut()?;
        Ok(Self::drain_response(port))
    }

    fn electrode_layout(&self) -> &ElectrodeLayout        { &self.electrode_layout }
    fn set_electrode_layout(&mut self, l: ElectrodeLayout) { self.electrode_layout = l; }
    fn channel_count(&self) -> usize                       { 16 }
    fn sampling_rate(&self) -> u32                         { 125 } // effective after interleaving
}

// ─── ConfigurableBoard ────────────────────────────────────────────────────────

impl ConfigurableBoard for CytonDaisyBoard {
    fn apply_channel_config(&mut self, channel: usize, config: &ChannelConfig) -> Result<()> {
        if channel >= 16 { return Err(OpenBciError::ChannelOutOfRange(channel, 16)); }
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
